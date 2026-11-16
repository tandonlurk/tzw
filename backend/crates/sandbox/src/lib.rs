//! Runs a user's JavaScript `decide(ctx)` in a QuickJS guest (built once with
//! Javy, `guest/runner.js`) inside Wasmtime, and turns its output into
//! strictly validated `aen_core` intents (D-11, D-40, D-41).
//!
//! Limits per run: fuel (CPU), an epoch deadline (wall clock), a memory cap,
//! a 512 KiB wasm stack, and caps on code, input and output size. Every run
//! gets a fresh `Store` and instance, so nothing survives between runs.
//! `Date` and `Math.random` are deterministic, from the caller's `now_ms` and
//! `seed`.
//!
//! The result is only a *proposal*: every intent must still go through
//! `aen_core::authorize::authorize`. The sandbox never sees keys, addresses
//! or policy limits.

mod output;
mod wasi;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use aen_core::authorize::Intent;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimitsBuilder, Trap};

pub use output::validate_output;
use wasi::{HostState, Limiter};

/// Environment variable that overrides the guest module's path.
pub const MODULE_ENV: &str = "AEN_SANDBOX_MODULE";
/// Default number of runs executing at once through `run_blocking`.
pub const DEFAULT_MAX_CONCURRENCY: usize = 4;
/// Length of one epoch tick; `Limits::epoch_ticks` counts these.
pub const TICK: Duration = Duration::from_millis(10);

/// `$AEN_SANDBOX_MODULE` if set, else this crate's `guest/runner.wasm`
/// (built by `build-guest.sh`).
pub fn default_module_path() -> PathBuf {
    std::env::var_os(MODULE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/guest/runner.wasm")))
}

/// Per-run limits. `Default` gives the D-41 values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Wasm fuel; 50M is about 7 ms of CPU, a simple `decide` uses about 1M.
    pub fuel: u64,
    /// Wall-clock deadline in epoch ticks of `TICK` (50 = 500 ms).
    pub epoch_ticks: u64,
    pub memory_bytes: usize,
    pub max_code_bytes: usize,
    /// Size of the serialised guest input (code, ctx, now, seed).
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_intents: usize,
    pub max_logs: usize,
    pub max_log_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            fuel: 50_000_000,
            epoch_ticks: 50,
            memory_bytes: 32 << 20,
            max_code_bytes: 64 << 10,
            max_input_bytes: 256 << 10,
            max_output_bytes: 64 << 10,
            max_intents: 10,
            max_logs: 20,
            max_log_chars: 200,
        }
    }
}

/// A successful run: validated intents (proposals only), the strategy's
/// captured `console.*` lines, and the fuel it used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    pub intents: Vec<Intent>,
    pub logs: Vec<String>,
    pub fuel_used: u64,
}

/// Why a run produced no intents. `Display` is written for the audit trail.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxError {
    #[error("could not load the sandbox module: {0}")]
    Load(String),
    #[error("strategy code is larger than the limit")]
    CodeTooLarge,
    #[error("strategy input (code plus context) is larger than the limit")]
    InputTooLarge,
    #[error("strategy used too much CPU")]
    OutOfFuel,
    #[error("strategy ran past its time limit")]
    Timeout,
    #[error("strategy used too much memory")]
    MemoryLimit,
    /// Wasm stack exhausted (QuickJS's own stack check does not fire in wasm).
    #[error("strategy recursed too deeply")]
    StackOverflow,
    #[error("strategy output is larger than the limit")]
    OutputTooLarge,
    /// `decide` threw, was missing, or the code did not parse; message from the guest.
    #[error("strategy error: {0}")]
    Guest(String),
    /// Output was not valid JSON or not a valid list of intents.
    #[error("strategy returned invalid intents: {0}")]
    BadOutput(String),
    /// The guest aborted with a wasm `unreachable`: Javy's response to
    /// features it does not support, such as `import()`.
    #[error(
        "strategy used a JavaScript feature the sandbox does not support (for example import())"
    )]
    Unsupported,
    /// Any other trap, including a call to an import the host does not provide.
    #[error("sandbox trap: {0}")]
    Trap(String),
    #[error("sandbox host error: {0}")]
    Host(String),
}

/// Point-in-time counters, for metrics and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxStats {
    pub in_flight: usize,
    /// Highest `in_flight` seen since the sandbox was created.
    pub peak_in_flight: usize,
    pub max_concurrency: usize,
}

#[derive(Serialize)]
struct GuestInput<'a> {
    code: &'a str,
    ctx: &'a serde_json::Value,
    now_ms: i64,
    seed: u32,
}

/// Increments the engine's epoch every `TICK` until dropped.
struct Ticker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Ticker {
    fn start(engine: Engine) -> Ticker {
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("aen-sandbox-epoch".into())
                .spawn(move || {
                    // Tick from elapsed wall time, so oversleeping does not stretch deadlines.
                    let start = Instant::now();
                    let mut ticks: u128 = 0;
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(TICK);
                        let due = start.elapsed().as_millis() / TICK.as_millis();
                        while ticks < due {
                            engine.increment_epoch();
                            ticks += 1;
                        }
                    }
                })
                .expect("spawn epoch ticker thread")
        };
        Ticker {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Decrements `in_flight` even if the run panics.
struct InFlight<'a>(&'a AtomicUsize);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The compiled guest plus its engine, linker and epoch ticker. Create one at
/// boot (compiling takes about 0.6 s) and share it, e.g. in an `Arc`.
pub struct Sandbox {
    engine: Engine,
    module: Module,
    linker: Linker<HostState>,
    module_sha256: String,
    limits: Limits,
    semaphore: Arc<Semaphore>,
    max_concurrency: usize,
    in_flight: AtomicUsize,
    peak_in_flight: AtomicUsize,
    _ticker: Ticker,
}

impl std::fmt::Debug for Sandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sandbox")
            .field("module_sha256", &self.module_sha256)
            .field("limits", &self.limits)
            .field("max_concurrency", &self.max_concurrency)
            .finish_non_exhaustive()
    }
}

impl Sandbox {
    /// Reads and compiles the guest module at `module_path` (see
    /// `default_module_path`) and records its SHA-256.
    pub fn new(module_path: impl AsRef<Path>) -> Result<Sandbox, SandboxError> {
        let path = module_path.as_ref();
        let wasm = std::fs::read(path)
            .map_err(|e| SandboxError::Load(format!("{}: {e}", path.display())))?;
        Sandbox::from_bytes(&wasm)
    }

    /// Compiles `wasm` (binary, or text for tests) that uses the runner's ABI.
    pub fn from_bytes(wasm: &[u8]) -> Result<Sandbox, SandboxError> {
        let load = |e: wasmtime::Error| SandboxError::Load(format!("{e:#}"));
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.max_wasm_stack(512 << 10);
        // Guest backtraces are noise for users and cost time on every trap.
        config.wasm_backtrace_max_frames(None);
        let engine = Engine::new(&config).map_err(load)?;
        let module = Module::new(&engine, wasm).map_err(load)?;
        let mut linker = Linker::new(&engine);
        wasi::link(&mut linker).map_err(load)?;
        // Anything else the module imports (fs, sockets, random...) traps when called.
        linker
            .define_unknown_imports_as_traps(&module)
            .map_err(load)?;

        let module_sha256 = Sha256::digest(wasm)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Sandbox {
            _ticker: Ticker::start(engine.clone()),
            engine,
            module,
            linker,
            module_sha256,
            limits: Limits::default(),
            semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENCY)),
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            in_flight: AtomicUsize::new(0),
            peak_in_flight: AtomicUsize::new(0),
        })
    }

    /// Replaces the default limits used by `run` and `run_blocking`.
    pub fn with_limits(mut self, limits: Limits) -> Sandbox {
        self.limits = limits;
        self
    }

    /// Sets how many `run_blocking` calls execute at once (at least 1).
    pub fn with_max_concurrency(mut self, n: usize) -> Sandbox {
        let n = n.max(1);
        self.semaphore = Arc::new(Semaphore::new(n));
        self.max_concurrency = n;
        self
    }

    /// Lowercase hex SHA-256 of the module bytes, for the audit trail (D-40).
    pub fn module_sha256(&self) -> &str {
        &self.module_sha256
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn stats(&self) -> SandboxStats {
        SandboxStats {
            in_flight: self.in_flight.load(Ordering::SeqCst),
            peak_in_flight: self.peak_in_flight.load(Ordering::SeqCst),
            max_concurrency: self.max_concurrency,
        }
    }

    /// Runs `decide(ctx)` from `code` with the sandbox's limits. Synchronous
    /// and CPU-bound: from async code use `run_blocking`.
    pub fn run(
        &self,
        code: &str,
        ctx: &serde_json::Value,
        now_ms: i64,
        seed: u32,
    ) -> Result<RunOutput, SandboxError> {
        self.run_with_limits(code, ctx, now_ms, seed, &self.limits)
    }

    /// `run` with explicit limits.
    pub fn run_with_limits(
        &self,
        code: &str,
        ctx: &serde_json::Value,
        now_ms: i64,
        seed: u32,
        limits: &Limits,
    ) -> Result<RunOutput, SandboxError> {
        if code.len() > limits.max_code_bytes {
            return Err(SandboxError::CodeTooLarge);
        }
        let stdin = serde_json::to_vec(&GuestInput {
            code,
            ctx,
            now_ms,
            seed,
        })
        .map_err(|e| SandboxError::Host(e.to_string()))?;
        if stdin.len() > limits.max_input_bytes {
            return Err(SandboxError::InputTooLarge);
        }
        let (stdout, fuel_used) = self.run_raw(stdin, now_ms, limits)?;
        let mut out = validate_output(&stdout, limits)?;
        out.fuel_used = fuel_used;
        Ok(out)
    }

    /// `run` on tokio's blocking pool, at most `max_concurrency` at once.
    /// Waits for a permit rather than failing when the sandbox is busy.
    pub async fn run_blocking(
        self: &Arc<Self>,
        code: String,
        ctx: serde_json::Value,
        now_ms: i64,
        seed: u32,
    ) -> Result<RunOutput, SandboxError> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| SandboxError::Host(e.to_string()))?;
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            this.run(&code, &ctx, now_ms, seed)
        })
        .await
        .map_err(|e| SandboxError::Host(format!("sandbox task failed: {e}")))?
    }

    /// Low level: instantiates the module, runs `_start` with `stdin`, and
    /// returns raw stdout and fuel used. Output is NOT validated.
    pub fn run_raw(
        &self,
        stdin: Vec<u8>,
        now_ms: i64,
        limits: &Limits,
    ) -> Result<(Vec<u8>, u64), SandboxError> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        let _guard = InFlight(&self.in_flight);
        self.peak_in_flight.fetch_max(now, Ordering::SeqCst);

        let state = HostState {
            stdin,
            stdin_pos: 0,
            stdout: Vec::new(),
            stdout_cap: limits.max_output_bytes,
            stdout_overflow: false,
            stderr: Vec::new(),
            now_ns: u64::try_from(now_ms).unwrap_or(0).saturating_mul(1_000_000),
            limiter: Limiter {
                inner: StoreLimitsBuilder::new()
                    .memory_size(limits.memory_bytes)
                    .instances(1)
                    .memories(1)
                    .tables(4)
                    .table_elements(20_000)
                    .trap_on_grow_failure(true)
                    .build(),
                memory_hit: false,
            },
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limiter);
        store
            .set_fuel(limits.fuel)
            .map_err(|e| SandboxError::Host(e.to_string()))?;
        store.set_epoch_deadline(limits.epoch_ticks);
        store.epoch_deadline_trap();

        let result = self
            .linker
            .instantiate(&mut store, &self.module)
            .and_then(|i| i.get_typed_func::<(), ()>(&mut store, "_start"))
            .and_then(|f| f.call(&mut store, ()));
        let fuel_used = limits.fuel.saturating_sub(store.get_fuel().unwrap_or(0));
        let st = store.data();
        if let Err(e) = result {
            return Err(if st.stdout_overflow {
                SandboxError::OutputTooLarge
            } else if st.limiter.memory_hit {
                SandboxError::MemoryLimit
            } else {
                match e.downcast_ref::<Trap>() {
                    Some(Trap::OutOfFuel) => SandboxError::OutOfFuel,
                    Some(Trap::Interrupt) => SandboxError::Timeout,
                    Some(Trap::StackOverflow) => SandboxError::StackOverflow,
                    Some(Trap::UnreachableCodeReached) => SandboxError::Unsupported,
                    _ => SandboxError::Trap(format!("{e:#}").chars().take(500).collect()),
                }
            });
        }
        Ok((store.into_data().stdout, fuel_used))
    }
}

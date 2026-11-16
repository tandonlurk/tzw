//! SPIKE for D-11: run a user's JavaScript `decide(ctx)` in a QuickJS guest
//! (built once with Javy) inside Wasmtime, and turn its output into strictly
//! validated intents.
//!
//! The guest gets exactly nine `wasi_snapshot_preview1` imports, all
//! implemented here: stdin/stdout/stderr backed by in-memory buffers, an empty
//! environment, a fixed clock and `proc_exit`. There is no filesystem, socket,
//! random or real-clock capability to grant. Any other import traps when called.
//!
//! Nothing the guest writes is trusted: output is size-capped, parsed and
//! deserialised into `Intent` with unknown fields rejected. The result is a
//! *proposal*; it must still go through `core::authorize`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer, Serialize};
use wasmtime::{
    Caller, Config, Engine, Extern, Linker, Memory, Module, ResourceLimiter, Store, StoreLimits,
    StoreLimitsBuilder, Trap,
};

/// Per-run limits. `Default` gives the D-11 starting values.
#[derive(Debug, Clone)]
pub struct Limits {
    pub fuel: u64,
    /// Wall-clock deadline in epoch ticks (one tick = `Host::TICK`).
    pub epoch_ticks: u64,
    pub memory_bytes: usize,
    pub max_code_bytes: usize,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_intents: usize,
    pub max_logs: usize,
    pub max_log_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            fuel: 50_000_000, // a simple decide() uses ~1.2M
            epoch_ticks: 50,  // 500 ms
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

/// Mirror of `aen_core::authorize::Intent` (kept local so the spike stays
/// isolated). Money is a string of integer minor units, as in D-13.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Intent {
    ProposeChallenge {
        stake: Money,
    },
    AcceptChallenge {
        stake: Money,
    },
    CounterOffer {
        stake: Money,
    },
    Reject,
    CommitFunds {
        interaction_type: InteractionType,
        stake: Money,
    },
    ProposeCollaboration {
        contribution: Money,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    Challenge,
    Collaboration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Money(pub i64);

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.is_empty() || s.len() > 18 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(serde::de::Error::custom(
                "money must be a string of 1-18 digits (minor units)",
            ));
        }
        s.parse().map(Money).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    intents: Option<Vec<Intent>>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    logs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    pub intents: Vec<Intent>,
    pub logs: Vec<String>,
    pub fuel_used: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SandboxError {
    CodeTooLarge,
    InputTooLarge,
    OutOfFuel,
    Timeout,
    MemoryLimit,
    /// Wasm stack exhausted (QuickJS's own stack check does not fire in wasm).
    StackOverflow,
    OutputTooLarge,
    /// `decide` threw, or was missing; message comes from the guest.
    Guest(String),
    /// Output was not valid JSON or not the expected shape.
    BadOutput(String),
    /// Any other trap (including a call to an import we do not provide).
    Trap(String),
    Host(String),
}

#[derive(Serialize)]
struct GuestInput<'a> {
    code: &'a str,
    ctx: &'a serde_json::Value,
    now_ms: i64,
    seed: u32,
}

struct Limiter {
    inner: StoreLimits,
    memory_hit: bool,
}

impl ResourceLimiter for Limiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let r = self.inner.memory_growing(current, desired, maximum);
        if !matches!(r, Ok(true)) {
            self.memory_hit = true;
        }
        r
    }
    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.inner.table_growing(current, desired, maximum)
    }
    fn instances(&self) -> usize {
        self.inner.instances()
    }
    fn tables(&self) -> usize {
        self.inner.tables()
    }
    fn memories(&self) -> usize {
        self.inner.memories()
    }
}

struct HostState {
    stdin: Vec<u8>,
    stdin_pos: usize,
    stdout: Vec<u8>,
    stdout_cap: usize,
    stdout_overflow: bool,
    stderr: Vec<u8>,
    now_ns: u64,
    limiter: Limiter,
}

const ERRNO_SUCCESS: i32 = 0;
const ERRNO_BADF: i32 = 8;
const ERRNO_FAULT: i32 = 21;
const ERRNO_SPIPE: i32 = 70;
const STDERR_CAP: usize = 4 << 10;

fn memory(caller: &mut Caller<'_, HostState>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

fn read_u32(mem: &[u8], at: u32) -> Option<u32> {
    let at = at as usize;
    mem.get(at..at.checked_add(4)?)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

fn write_bytes(mem: &mut [u8], at: u32, bytes: &[u8]) -> bool {
    let at = at as usize;
    match mem.get_mut(at..at.saturating_add(bytes.len())) {
        Some(dst) => {
            dst.copy_from_slice(bytes);
            true
        }
        None => false,
    }
}

/// The only host functions the guest can call.
fn link(linker: &mut Linker<HostState>) -> wasmtime::Result<()> {
    const W: &str = "wasi_snapshot_preview1";

    linker.func_wrap(
        W,
        "environ_sizes_get",
        |mut c: Caller<'_, HostState>, n: u32, sz: u32| {
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            let d = m.data_mut(&mut c);
            if write_bytes(d, n, &0u32.to_le_bytes()) && write_bytes(d, sz, &0u32.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;
    linker.func_wrap(
        W,
        "environ_get",
        |_: Caller<'_, HostState>, _: u32, _: u32| ERRNO_SUCCESS,
    )?;

    // A fixed clock: the guest sees only the `now` the host chose.
    linker.func_wrap(
        W,
        "clock_time_get",
        |mut c: Caller<'_, HostState>, _id: u32, _prec: u64, out: u32| {
            let now = c.data().now_ns;
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            if write_bytes(m.data_mut(&mut c), out, &now.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;

    linker.func_wrap(W, "fd_close", |_: Caller<'_, HostState>, fd: u32| {
        if fd <= 2 { ERRNO_SUCCESS } else { ERRNO_BADF }
    })?;
    linker.func_wrap(
        W,
        "fd_seek",
        |_: Caller<'_, HostState>, _: u32, _: i64, _: u32, _: u32| ERRNO_SPIPE,
    )?;
    linker.func_wrap(
        W,
        "fd_fdstat_get",
        |mut c: Caller<'_, HostState>, fd: u32, out: u32| {
            if fd > 2 {
                return ERRNO_BADF;
            }
            // filetype=CHARACTER_DEVICE(2), flags=0, rights: fd_read(1<<1) | fd_write(1<<6)
            let mut stat = [0u8; 24];
            stat[0] = 2;
            stat[8..16].copy_from_slice(&((1u64 << 1) | (1u64 << 6)).to_le_bytes());
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            if write_bytes(m.data_mut(&mut c), out, &stat) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;

    linker.func_wrap(
        W,
        "fd_read",
        |mut c: Caller<'_, HostState>, fd: u32, iovs: u32, n_iovs: u32, nread: u32| {
            if fd != 0 {
                return ERRNO_BADF;
            }
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            let (mem, st) = m.data_and_store_mut(&mut c);
            let mut total = 0u32;
            for i in 0..n_iovs {
                let base = iovs.wrapping_add(i.wrapping_mul(8));
                let (Some(ptr), Some(len)) =
                    (read_u32(mem, base), read_u32(mem, base.wrapping_add(4)))
                else {
                    return ERRNO_FAULT;
                };
                let rest = &st.stdin[st.stdin_pos..];
                let k = rest.len().min(len as usize);
                if !write_bytes(mem, ptr, &rest[..k]) {
                    return ERRNO_FAULT;
                }
                st.stdin_pos += k;
                total += k as u32;
                if k < len as usize {
                    break;
                }
            }
            if write_bytes(mem, nread, &total.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;

    linker.func_wrap(
        W,
        "fd_write",
        |mut c: Caller<'_, HostState>,
         fd: u32,
         iovs: u32,
         n_iovs: u32,
         nwritten: u32|
         -> wasmtime::Result<i32> {
            if fd != 1 && fd != 2 {
                return Ok(ERRNO_BADF);
            }
            let Some(m) = memory(&mut c) else {
                return Ok(ERRNO_FAULT);
            };
            let (mem, st) = m.data_and_store_mut(&mut c);
            let mut total = 0u32;
            for i in 0..n_iovs {
                let base = iovs.wrapping_add(i.wrapping_mul(8));
                let (Some(ptr), Some(len)) =
                    (read_u32(mem, base), read_u32(mem, base.wrapping_add(4)))
                else {
                    return Ok(ERRNO_FAULT);
                };
                let Some(src) = mem.get(ptr as usize..(ptr as usize).saturating_add(len as usize))
                else {
                    return Ok(ERRNO_FAULT);
                };
                if fd == 1 {
                    if st.stdout.len() + src.len() > st.stdout_cap {
                        st.stdout_overflow = true;
                        wasmtime::bail!("guest output exceeds cap");
                    }
                    st.stdout.extend_from_slice(src);
                } else {
                    let room = STDERR_CAP.saturating_sub(st.stderr.len());
                    st.stderr.extend_from_slice(&src[..src.len().min(room)]);
                }
                total = total.saturating_add(len);
            }
            Ok(if write_bytes(mem, nwritten, &total.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            })
        },
    )?;

    linker.func_wrap(
        W,
        "proc_exit",
        |_: Caller<'_, HostState>, code: i32| -> wasmtime::Result<()> {
            wasmtime::bail!("guest called proc_exit({code})")
        },
    )?;
    Ok(())
}

struct Ticker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// One per process: shared engine, compiled guest and linker. Cheap to share
/// across threads; each `run` gets a fresh `Store` and instance.
pub struct Host {
    engine: Engine,
    module: Module,
    linker: Linker<HostState>,
    _ticker: Ticker,
}

impl Host {
    pub const TICK: Duration = Duration::from_millis(10);

    /// Compiles `wasm` (the Javy runner, or any module using the same ABI).
    pub fn new(wasm: &[u8]) -> wasmtime::Result<Host> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.max_wasm_stack(512 << 10);
        // Guest backtraces are noise for users and cost time on every trap.
        config.wasm_backtrace_max_frames(None);
        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, wasm)?;
        let mut linker = Linker::new(&engine);
        link(&mut linker)?;
        // Anything else the module imports (fs, sockets, random...) traps.
        linker.define_unknown_imports_as_traps(&module)?;

        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let engine = engine.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                // Tick from elapsed wall time, so oversleeping does not stretch deadlines.
                let start = Instant::now();
                let mut ticks: u128 = 0;
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Self::TICK);
                    let due = start.elapsed().as_millis() / Self::TICK.as_millis();
                    while ticks < due {
                        engine.increment_epoch();
                        ticks += 1;
                    }
                }
            })
        };
        Ok(Host {
            engine,
            module,
            linker,
            _ticker: Ticker {
                stop,
                handle: Some(handle),
            },
        })
    }

    /// Runs `decide(ctx)` from `code` and returns validated intents.
    pub fn run(
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

    /// Instantiates and runs `_start` with `stdin`; returns raw stdout and fuel used.
    pub fn run_raw(
        &self,
        stdin: Vec<u8>,
        now_ms: i64,
        limits: &Limits,
    ) -> Result<(Vec<u8>, u64), SandboxError> {
        let state = HostState {
            stdin,
            stdin_pos: 0,
            stdout: Vec::new(),
            stdout_cap: limits.max_output_bytes,
            stdout_overflow: false,
            stderr: Vec::new(),
            now_ns: (now_ms.max(0) as u64).saturating_mul(1_000_000),
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
        let fuel_used = limits.fuel - store.get_fuel().unwrap_or(0);
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
                    _ => SandboxError::Trap(format!("{e:#}").chars().take(500).collect()),
                }
            });
        }
        Ok((store.into_data().stdout, fuel_used))
    }
}

/// Parses and strictly validates the guest's stdout.
pub fn validate_output(bytes: &[u8], limits: &Limits) -> Result<RunOutput, SandboxError> {
    if bytes.len() > limits.max_output_bytes {
        return Err(SandboxError::OutputTooLarge);
    }
    let env: Envelope =
        serde_json::from_slice(bytes).map_err(|e| SandboxError::BadOutput(e.to_string()))?;
    let logs: Vec<String> = env
        .logs
        .into_iter()
        .take(limits.max_logs)
        .map(|l| l.chars().take(limits.max_log_chars).collect())
        .collect();
    if !env.ok {
        let msg = env.error.unwrap_or_default().chars().take(500).collect();
        return Err(SandboxError::Guest(msg));
    }
    let intents = env
        .intents
        .ok_or_else(|| SandboxError::BadOutput("decide must return an array of intents".into()))?;
    if intents.len() > limits.max_intents {
        return Err(SandboxError::BadOutput(format!(
            "too many intents: {} > {}",
            intents.len(),
            limits.max_intents
        )));
    }
    Ok(RunOutput {
        intents,
        logs,
        fuel_used: 0,
    })
}

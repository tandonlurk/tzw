//! Escape and resource-abuse tests for the sandbox (M5 QA item).
//!
//! Tests that need the real guest skip with a message if `guest/runner.wasm`
//! (or `$AEN_SANDBOX_MODULE`) is missing: build it with `./build-guest.sh`.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use aen_core::authorize::Intent;
use aen_core::money::Money;
use aen_core::policy::InteractionType;
use aen_sandbox::{Limits, RunOutput, Sandbox, SandboxError, default_module_path, validate_output};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const NOW_MS: i64 = 1_763_251_200_000; // 2025-11-16T00:00:00Z

fn sandbox() -> Option<&'static Arc<Sandbox>> {
    static SB: OnceLock<Option<Arc<Sandbox>>> = OnceLock::new();
    SB.get_or_init(|| {
        let path = default_module_path();
        if !path.exists() {
            eprintln!(
                "SKIP: {} missing; run backend/crates/sandbox/build-guest.sh",
                path.display()
            );
            return None;
        }
        Some(Arc::new(Sandbox::new(&path).expect("compile runner")))
    })
    .as_ref()
}

/// Returns the shared sandbox or skips the test.
macro_rules! sb {
    () => {
        match sandbox() {
            Some(sb) => sb,
            None => return,
        }
    };
}

fn ctx() -> Value {
    json!({
        "agent_id": "agent-1",
        "market": { "event": "BTC-100K-NOV", "price": "0.62", "threshold": "0.55" },
        "balances": { "available": "10000000" }
    })
}

fn run(sb: &Sandbox, code: &str) -> Result<RunOutput, SandboxError> {
    sb.run(code, &ctx(), NOW_MS, 42)
}

fn run_with(sb: &Sandbox, code: &str, limits: Limits) -> Result<RunOutput, SandboxError> {
    sb.run_with_limits(code, &ctx(), NOW_MS, 42, &limits)
}

fn money(n: i64) -> Money {
    Money::from_minor(n).unwrap()
}

// ---------- setup ----------

#[test]
fn module_hash_is_recorded() {
    let sb = sb!();
    let bytes = std::fs::read(default_module_path()).unwrap();
    let want: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(sb.module_sha256(), want);
    assert_eq!(sb.limits(), &Limits::default());
}

#[test]
fn missing_module_is_a_load_error() {
    let err = Sandbox::new("/nonexistent/runner.wasm").unwrap_err();
    assert!(matches!(err, SandboxError::Load(_)), "got {err:?}");
    assert!(matches!(
        Sandbox::from_bytes(b"not wasm"),
        Err(SandboxError::Load(_))
    ));
}

// ---------- happy path ----------

#[test]
fn normal_decide_returns_core_intents() {
    let sb = sb!();
    let code = r#"
        function decide(ctx) {
          const p = Number(ctx.market.price), t = Number(ctx.market.threshold);
          console.log("price", p, "threshold", t);
          if (p <= t) return [{ type: "reject" }];
          return [
            { type: "propose_challenge", stake: "2500000" },
            { type: "commit_funds", interaction_type: "challenge", stake: "2500000" },
          ];
        }"#;
    let out = run(sb, code).expect("run");
    assert_eq!(
        out.intents,
        vec![
            Intent::ProposeChallenge {
                stake: money(2_500_000)
            },
            Intent::CommitFunds {
                interaction_type: InteractionType::Challenge,
                stake: money(2_500_000)
            },
        ]
    );
    assert_eq!(out.logs, vec!["price 0.62 threshold 0.55"]);
    assert!(out.fuel_used > 0 && out.fuel_used < Limits::default().fuel);
}

#[test]
fn state_does_not_leak_between_runs() {
    let sb = sb!();
    run(sb, "globalThis.leak = 1; function decide(){ return []; }").unwrap();
    let out = run(
        sb,
        "function decide(){ console.log(typeof leak); return []; }",
    )
    .unwrap();
    assert_eq!(out.logs, vec!["undefined"]);
}

#[test]
fn oversized_intents_convert_faithfully() {
    // Authorization is not the sandbox's job: stakes far above any policy
    // limit still come back as exact core intents, for `authorize` to deny.
    let sb = sb!();
    let code = r#"
        function decide() {
          const big = "999999999999999999"; // 18 digits, the D-41 maximum
          return [
            { type: "propose_challenge", stake: big },
            { type: "accept_challenge", stake: big },
            { type: "counter_offer", stake: big },
            { type: "commit_funds", interaction_type: "challenge", stake: big },
            { type: "commit_funds", interaction_type: "collaboration", stake: big },
            { type: "propose_collaboration", contribution: big },
            { type: "reject" },
            { type: "propose_challenge", stake: "0" },
            { type: "propose_challenge", stake: "1" },
            { type: "propose_challenge", stake: big },
          ];
        }"#;
    let out = run(sb, code).expect("run");
    let big = money(999_999_999_999_999_999);
    assert_eq!(out.intents.len(), 10);
    assert_eq!(out.intents[0], Intent::ProposeChallenge { stake: big });
    assert_eq!(out.intents[1], Intent::AcceptChallenge { stake: big });
    assert_eq!(out.intents[2], Intent::CounterOffer { stake: big });
    assert_eq!(
        out.intents[3],
        Intent::CommitFunds {
            interaction_type: InteractionType::Challenge,
            stake: big
        }
    );
    assert_eq!(
        out.intents[4],
        Intent::CommitFunds {
            interaction_type: InteractionType::Collaboration,
            stake: big
        }
    );
    assert_eq!(
        out.intents[5],
        Intent::ProposeCollaboration { contribution: big }
    );
    assert_eq!(out.intents[6], Intent::Reject);
    assert_eq!(out.intents[7], Intent::ProposeChallenge { stake: money(0) });
    assert_eq!(out.intents[8], Intent::ProposeChallenge { stake: money(1) });
    // The values are the core types `authorize` takes.
    assert_eq!(out.intents[9].stake(), Some(big));
}

// ---------- CPU ----------

#[test]
fn infinite_loop_stopped_by_fuel() {
    let sb = sb!();
    let limits = Limits {
        epoch_ticks: 100_000, // ~17 min: fuel must win
        ..Limits::default()
    };
    let t = Instant::now();
    let err = run_with(sb, "function decide(){ for(;;){} }", limits).unwrap_err();
    assert_eq!(err, SandboxError::OutOfFuel);
    eprintln!("fuel stop after {:?}", t.elapsed());
}

#[test]
fn infinite_loop_stopped_by_epoch_deadline() {
    let sb = sb!();
    let limits = Limits {
        fuel: u64::MAX / 2,
        epoch_ticks: 50,
        ..Limits::default()
    };
    let t = Instant::now();
    let err = run_with(sb, "function decide(){ for(;;){} }", limits).unwrap_err();
    let took = t.elapsed();
    assert_eq!(err, SandboxError::Timeout);
    assert!(took < Duration::from_millis(1500), "took {took:?}");
    eprintln!("epoch stop after {took:?}");
}

#[test]
fn deep_recursion_is_an_error_not_a_crash() {
    let sb = sb!();
    let err = run(
        sb,
        "function f(n){ return f(n+1)+1; } function decide(){ return [f(0)]; }",
    )
    .unwrap_err();
    assert_eq!(err, SandboxError::StackOverflow);
}

// ---------- memory ----------

fn generous() -> Limits {
    Limits {
        fuel: 20_000_000_000,
        epoch_ticks: 1_000,
        ..Limits::default()
    }
}

#[test]
fn memory_bomb_array_is_stopped() {
    let sb = sb!();
    let err = run_with(
        sb,
        "function decide(){ const a=[]; for(;;) a.push(new Array(100000).fill(1.5)); }",
        generous(),
    )
    .unwrap_err();
    assert_eq!(err, SandboxError::MemoryLimit);
}

#[test]
fn memory_bomb_string_is_stopped() {
    let sb = sb!();
    let err = run_with(
        sb,
        "function decide(){ let s='x'; for(;;) s = s + s; }",
        generous(),
    )
    .unwrap_err();
    // QuickJS may refuse the string itself ("string too long") before memory runs out.
    assert!(
        matches!(err, SandboxError::MemoryLimit | SandboxError::Guest(_)),
        "got {err:?}"
    );
}

// ---------- capabilities ----------

#[test]
fn no_ambient_capabilities_in_js() {
    let sb = sb!();
    let out = run(
        sb,
        r#"
        function decide() {
          const names = ["require","process","fetch","XMLHttpRequest","WebSocket","Javy",
                         "std","os","Deno","Bun","setTimeout","setInterval","WebAssembly"];
          for (const n of names) console.log(n + "=" + typeof globalThis[n]);
          return [];
        }"#,
    )
    .unwrap();
    for line in &out.logs {
        assert!(line.ends_with("=undefined"), "capability exposed: {line}");
    }
    assert_eq!(out.logs.len(), 13);
}

#[test]
fn dynamic_import_is_unsupported() {
    let sb = sb!();
    let res = run(
        sb,
        r#"
        function decide() {
          let r = "none";
          try { const p = import("os"); r = typeof p; } catch (e) { r = "threw:" + e.message; }
          console.log(r);
          return [{ type: "propose_challenge", stake: "1" }];
        }"#,
    );
    // Javy aborts with a wasm `unreachable`: the run fails closed with a friendly error.
    assert_eq!(res, Err(SandboxError::Unsupported));
    assert!(
        SandboxError::Unsupported
            .to_string()
            .contains("not support")
    );
}

#[test]
fn time_and_randomness_are_deterministic() {
    let sb = sb!();
    let code = r#"
        function decide() {
          console.log(String(Date.now()), String(new Date().getTime()));
          console.log(String(Math.random()), String(Math.random()));
          return [];
        }"#;
    let a = sb.run(code, &ctx(), NOW_MS, 7).unwrap();
    let b = sb.run(code, &ctx(), NOW_MS, 7).unwrap();
    let c = sb.run(code, &ctx(), NOW_MS, 8).unwrap();
    assert_eq!(a.logs[0], format!("{NOW_MS} {NOW_MS}"));
    assert_eq!(a.logs, b.logs, "same seed must give same output");
    assert_ne!(a.logs[1], c.logs[1], "seed must change the PRNG stream");
}

#[test]
fn unlinked_wasi_imports_trap() {
    // A module asking for capabilities the host never provides (fs, sockets, random).
    let wat = r#"(module
        (import "wasi_snapshot_preview1" "path_open"
          (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "sock_accept" (func $acc (param i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "random_get" (func $rnd (param i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "_start")
          (drop (call $open (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0)
                            (i32.const 0) (i64.const 0) (i64.const 0) (i32.const 0) (i32.const 0)))))"#;
    let h = Sandbox::from_bytes(wat.as_bytes()).unwrap();
    let err = h.run_raw(Vec::new(), 0, &Limits::default()).unwrap_err();
    assert!(
        matches!(err, SandboxError::Trap(ref m) if m.contains("path_open")),
        "got {err:?}"
    );
}

#[test]
fn env_is_empty_and_extra_fds_are_bad() {
    // Raw ABI check: environ_sizes_get reports 0 vars, fd_read on fd 3 is EBADF (8).
    let wat = r#"(module
        (import "wasi_snapshot_preview1" "environ_sizes_get" (func $es (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_read" (func $rd (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_write" (func $wr (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "_start")
          (i32.store (i32.const 100) (i32.const 999))
          (drop (call $es (i32.const 100) (i32.const 104)))
          ;; byte 0 = '0' + env count, byte 1 = '0' + errno of fd_read(3)
          (i32.store8 (i32.const 0) (i32.add (i32.const 48) (i32.load (i32.const 100))))
          (i32.store8 (i32.const 1) (i32.add (i32.const 48)
            (call $rd (i32.const 3) (i32.const 200) (i32.const 0) (i32.const 208))))
          (i32.store (i32.const 16) (i32.const 0))
          (i32.store (i32.const 20) (i32.const 2))
          (drop (call $wr (i32.const 1) (i32.const 16) (i32.const 1) (i32.const 24)))))"#;
    let h = Sandbox::from_bytes(wat.as_bytes()).unwrap();
    let (out, _) = h.run_raw(Vec::new(), 0, &Limits::default()).unwrap();
    assert_eq!(out, b"08"); // 0 env vars, EBADF
}

#[test]
fn raw_unreachable_maps_to_unsupported() {
    let wat = r#"(module (memory (export "memory") 1) (func (export "_start") unreachable))"#;
    let h = Sandbox::from_bytes(wat.as_bytes()).unwrap();
    assert_eq!(
        h.run_raw(Vec::new(), 0, &Limits::default()).unwrap_err(),
        SandboxError::Unsupported
    );
}

// ---------- bad output ----------

fn expect_bad(sb: &Sandbox, code: &str) {
    match run(sb, code) {
        Err(SandboxError::BadOutput(m)) => eprintln!("rejected: {m}"),
        other => panic!("expected BadOutput for {code}, got {other:?}"),
    }
}

#[test]
fn wrong_shapes_are_rejected() {
    let sb = sb!();
    for code in [
        r#"function decide(){ return [{type:"steal_funds", stake:"1"}]; }"#,
        r#"function decide(){ return [{type:"propose_challenge", stake:"100", recipient:"0xdead"}]; }"#,
        r#"function decide(){ return [{type:"propose_challenge", stake:100}]; }"#,
        r#"function decide(){ return [{type:"propose_challenge", stake:"-5"}]; }"#,
        r#"function decide(){ return [{type:"propose_challenge", stake:"1e9"}]; }"#,
        r#"function decide(){ return [{type:"propose_challenge", stake:"0100"}]; }"#,
        r#"function decide(){ return [{type:"propose_challenge", stake:"1000000000000000000"}]; }"#,
        r#"function decide(){ return [{type:"propose_challenge"}]; }"#,
        r#"function decide(){ return [{type:"commit_funds", interaction_type:"loan", stake:"1"}]; }"#,
        r#"function decide(){ return [{type:"reject", stake:"1"}]; }"#,
        r#"function decide(){ return {type:"reject"}; }"#,
        r#"function decide(){ return undefined; }"#,
        r#"function decide(){ return Array(11).fill({type:"reject"}); }"#,
    ] {
        expect_bad(sb, code);
    }
}

#[test]
fn huge_output_is_rejected() {
    let sb = sb!();
    // ~85 KB > 64 KiB
    let err = run(
        sb,
        r#"function decide(){ return Array(5000).fill({type:"reject"}); }"#,
    )
    .unwrap_err();
    assert_eq!(err, SandboxError::OutputTooLarge);
}

#[test]
fn guest_errors_are_reported() {
    let sb = sb!();
    assert_eq!(
        run(sb, "function decide(){ throw new Error('nope'); }"),
        Err(SandboxError::Guest("nope".into()))
    );
    assert!(
        matches!(run(sb, "const x = 1;"), Err(SandboxError::Guest(ref m)) if m.contains("not defined"))
    );
    assert!(matches!(
        run(sb, "function decide( {"),
        Err(SandboxError::Guest(_))
    ));
}

#[test]
fn non_json_stdout_is_rejected() {
    let wat = r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $w (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 16) "not json at all")
        (func (export "_start")
          (i32.store (i32.const 0) (i32.const 16))
          (i32.store (i32.const 4) (i32.const 15))
          (drop (call $w (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#;
    let h = Sandbox::from_bytes(wat.as_bytes()).unwrap();
    let (out, _) = h.run_raw(Vec::new(), 0, &Limits::default()).unwrap();
    assert!(matches!(
        validate_output(&out, &Limits::default()),
        Err(SandboxError::BadOutput(_))
    ));
    let extra = br#"{"ok":true,"intents":[],"logs":[],"admin":true}"#;
    assert!(matches!(
        validate_output(extra, &Limits::default()),
        Err(SandboxError::BadOutput(_))
    ));
}

#[test]
fn oversize_code_and_ctx_rejected_before_execution() {
    // A module that would trap if it ever ran: proves the checks come first.
    let wat = r#"(module (memory (export "memory") 1) (func (export "_start") unreachable))"#;
    let h = Sandbox::from_bytes(wat.as_bytes()).unwrap();
    let big_code = format!("function decide(){{ return []; }} //{}", "x".repeat(70_000));
    assert_eq!(
        h.run(&big_code, &ctx(), NOW_MS, 1).unwrap_err(),
        SandboxError::CodeTooLarge
    );
    let big_ctx = json!({ "blob": "y".repeat(300_000) });
    assert_eq!(
        h.run("function decide(){ return []; }", &big_ctx, NOW_MS, 1)
            .unwrap_err(),
        SandboxError::InputTooLarge
    );
    assert_eq!(h.stats().peak_in_flight, 0, "guest must not have run");
}

// ---------- robustness and concurrency ----------

#[test]
fn runs_after_traps_still_work() {
    let sb = sb!();
    let ok = "function decide(){ return [{type:\"reject\"}]; }";
    let traps = [
        ("function decide(){ for(;;){} }", SandboxError::OutOfFuel),
        (
            "function f(n){ return f(n+1)+1; } function decide(){ return [f(0)]; }",
            SandboxError::StackOverflow,
        ),
        (
            "function decide(){ import('os'); return []; }",
            SandboxError::Unsupported,
        ),
        (
            "function decide(){ return Array(5000).fill({type:\"reject\"}); }",
            SandboxError::OutputTooLarge,
        ),
    ];
    for (code, want) in traps {
        assert_eq!(run(sb, code).unwrap_err(), want);
        assert_eq!(run(sb, ok).unwrap().intents, vec![Intent::Reject]);
    }
    let err = run_with(
        sb,
        "function decide(){ const a=[]; for(;;) a.push(new Array(100000).fill(1.5)); }",
        generous(),
    )
    .unwrap_err();
    assert_eq!(err, SandboxError::MemoryLimit);
    assert_eq!(run(sb, ok).unwrap().intents, vec![Intent::Reject]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_runs_respect_the_semaphore() {
    // Own sandbox so other tests running in parallel do not affect the counters.
    let path = default_module_path();
    if !path.exists() {
        eprintln!("SKIP: {} missing", path.display());
        return;
    }
    // Busy enough (tens of ms each) that runs overlap; fuel raised to allow it.
    let sb = Arc::new(Sandbox::new(&path).unwrap().with_limits(Limits {
        fuel: 2_000_000_000,
        ..Limits::default()
    }));
    assert_eq!(sb.stats().max_concurrency, 4);
    let code = r#"function decide(ctx){
        let x = 0; for (let i = 0; i < 200000; i++) x = (x + i * ctx.seed) % 1000003;
        return [{ type: "propose_challenge", stake: String(ctx.seed) }]; }"#;
    let tasks: Vec<_> = (1..=8u32)
        .map(|i| {
            let sb = sb.clone();
            tokio::spawn(async move {
                sb.run_blocking(code.to_string(), json!({ "seed": i }), NOW_MS, i)
                    .await
            })
        })
        .collect();
    for (i, t) in tasks.into_iter().enumerate() {
        let out = t.await.unwrap().expect("run");
        assert_eq!(
            out.intents,
            vec![Intent::ProposeChallenge {
                stake: money(i as i64 + 1)
            }]
        );
    }
    let stats = sb.stats();
    eprintln!("{stats:?}");
    assert_eq!(stats.in_flight, 0);
    assert!(stats.peak_in_flight <= 4, "semaphore exceeded: {stats:?}");
    assert!(
        stats.peak_in_flight >= 2,
        "runs never overlapped: {stats:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_blocking_reports_errors() {
    let sb = sb!();
    let err = sb
        .run_blocking(
            "function decide(){ throw new Error('x'); }".into(),
            ctx(),
            NOW_MS,
            1,
        )
        .await
        .unwrap_err();
    assert_eq!(err, SandboxError::Guest("x".into()));
}

// ---------- latency ----------

/// Run alone for stable numbers (release recommended):
/// cargo test -p aen-sandbox --test sandbox latency -- --ignored --nocapture --test-threads=1
#[test]
#[ignore]
fn latency() {
    let path = default_module_path();
    let wasm = std::fs::read(&path).expect("runner.wasm: run build-guest.sh");
    let t = Instant::now();
    let h = Sandbox::from_bytes(&wasm).unwrap();
    let compile = t.elapsed();

    let mut inst = Vec::new();
    for _ in 0..20 {
        let t = Instant::now();
        let _ = h.run_raw(b"{".to_vec(), NOW_MS, &Limits::default()); // fails fast after instantiate
        inst.push(t.elapsed());
    }

    let code = r#"function decide(ctx){
        return Number(ctx.market.price) > Number(ctx.market.threshold)
          ? [{type:"propose_challenge", stake:"2500000"}] : [{type:"reject"}]; }"#;
    let mut calls = Vec::new();
    let mut fuel = 0;
    for _ in 0..100 {
        let t = Instant::now();
        let out = h.run(code, &ctx(), NOW_MS, 1).unwrap();
        calls.push(t.elapsed());
        fuel = out.fuel_used;
    }
    let pct = |v: &mut Vec<Duration>, p: f64| {
        v.sort();
        v[((v.len() as f64 - 1.0) * p).round() as usize]
    };
    println!("wasm size: {} bytes", wasm.len());
    println!("cold compile (Engine+Module::new): {compile:?}");
    println!(
        "instantiate+bad-input exit p50: {:?} p95: {:?}",
        pct(&mut inst, 0.5),
        pct(&mut inst, 0.95)
    );
    println!(
        "full run p50: {:?} p95: {:?} max: {:?}",
        pct(&mut calls, 0.5),
        pct(&mut calls, 0.95),
        pct(&mut calls, 1.0)
    );
    println!("fuel per normal call: {fuel}");
}

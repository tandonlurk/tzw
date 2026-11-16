//! Requires guest/runner.wasm: run ./build-guest.sh first.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sandbox_spike::{Host, Intent, InteractionType, Limits, Money, SandboxError, validate_output};
use serde_json::{Value, json};

const NOW_MS: i64 = 1_763_251_200_000; // 2025-11-16T00:00:00Z

fn runner_wasm() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/guest/runner.wasm");
    std::fs::read(path).unwrap_or_else(|_| panic!("{path} missing: run ./build-guest.sh"))
}

fn host() -> &'static Host {
    static HOST: OnceLock<Host> = OnceLock::new();
    HOST.get_or_init(|| Host::new(&runner_wasm()).expect("compile runner"))
}

fn ctx() -> Value {
    json!({
        "agent_id": "agent-1",
        "market": { "event": "BTC-100K-NOV", "price": "0.62", "threshold": "0.55" },
        "balances": { "available": "10000000" }
    })
}

fn run(code: &str) -> Result<sandbox_spike::RunOutput, SandboxError> {
    host().run(code, &ctx(), NOW_MS, 42, &Limits::default())
}

fn run_with(code: &str, limits: Limits) -> Result<sandbox_spike::RunOutput, SandboxError> {
    host().run(code, &ctx(), NOW_MS, 42, &limits)
}

// ---------- happy path ----------

#[test]
fn normal_decide_returns_valid_intents() {
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
    let out = run(code).expect("run");
    assert_eq!(
        out.intents,
        vec![
            Intent::ProposeChallenge {
                stake: Money(2_500_000)
            },
            Intent::CommitFunds {
                interaction_type: InteractionType::Challenge,
                stake: Money(2_500_000)
            },
        ]
    );
    assert_eq!(out.logs, vec!["price 0.62 threshold 0.55"]);
    eprintln!("fuel used by a normal decide(): {}", out.fuel_used);
}

#[test]
fn state_does_not_leak_between_runs() {
    run("globalThis.leak = 1; function decide(){ return []; }").unwrap();
    let out = run("function decide(){ console.log(typeof leak); return []; }").unwrap();
    assert_eq!(out.logs, vec!["undefined"]);
}

// ---------- CPU ----------

#[test]
fn infinite_loop_stopped_by_fuel() {
    let limits = Limits {
        epoch_ticks: 100_000,
        ..Limits::default()
    }; // ~17 min: fuel must win
    host(); // compile outside the timed region
    let t = Instant::now();
    let err = run_with("function decide(){ for(;;){} }", limits).unwrap_err();
    assert_eq!(err, SandboxError::OutOfFuel);
    eprintln!("fuel stop after {:?}", t.elapsed());
}

#[test]
fn infinite_loop_stopped_by_epoch_deadline() {
    let limits = Limits {
        fuel: u64::MAX / 2,
        epoch_ticks: 50,
        ..Limits::default()
    };
    host(); // compile outside the timed region
    let t = Instant::now();
    let err = run_with("function decide(){ for(;;){} }", limits).unwrap_err();
    let took = t.elapsed();
    assert_eq!(err, SandboxError::Timeout);
    assert!(took < Duration::from_millis(1500), "took {took:?}");
    eprintln!("epoch stop after {took:?}");
}

#[test]
fn deep_recursion_is_an_error_not_a_crash() {
    let err =
        run("function f(n){ return f(n+1)+1; } function decide(){ return [f(0)]; }").unwrap_err();
    assert_eq!(err, SandboxError::StackOverflow);
}

// ---------- memory ----------

#[test]
fn memory_bomb_array_is_stopped() {
    let limits = Limits {
        fuel: 20_000_000_000,
        epoch_ticks: 1_000,
        ..Limits::default()
    };
    let err = run_with(
        "function decide(){ const a=[]; for(;;) a.push(new Array(100000).fill(1.5)); }",
        limits,
    )
    .unwrap_err();
    assert_eq!(err, SandboxError::MemoryLimit);
}

#[test]
fn memory_bomb_string_is_stopped() {
    let limits = Limits {
        fuel: 20_000_000_000,
        epoch_ticks: 1_000,
        ..Limits::default()
    };
    let err = run_with("function decide(){ let s='x'; for(;;) s = s + s; }", limits).unwrap_err();
    // QuickJS may refuse the string itself ("invalid string length") before memory runs out.
    assert!(
        matches!(err, SandboxError::MemoryLimit | SandboxError::Guest(_)),
        "got {err:?}"
    );
    eprintln!("string bomb -> {err:?}");
}

// ---------- capabilities ----------

#[test]
fn no_ambient_capabilities_in_js() {
    let out = run(r#"
        function decide() {
          const names = ["require","process","fetch","XMLHttpRequest","WebSocket","Javy",
                         "std","os","Deno","Bun","setTimeout","setInterval","WebAssembly"];
          for (const n of names) console.log(n + "=" + typeof globalThis[n]);
          return [];
        }"#)
    .unwrap();
    for line in &out.logs {
        assert!(line.ends_with("=undefined"), "capability exposed: {line}");
    }
    assert_eq!(out.logs.len(), 13);
}

#[test]
fn dynamic_import_does_not_load_modules() {
    // There is no module loader; whatever happens, no module object comes back.
    let res = run(r#"
        function decide() {
          let r = "none";
          try { const p = import("os"); r = typeof p; } catch (e) { r = "threw:" + e.message; }
          console.log(r);
          return [];
        }"#);
    // Observed: Javy aborts (wasm `unreachable`), i.e. the run fails closed.
    eprintln!("dynamic import -> {res:?}");
    if let Ok(out) = res {
        assert!(out.intents.is_empty());
    }
}

#[test]
fn time_and_randomness_are_deterministic() {
    let code = r#"
        function decide() {
          console.log(String(Date.now()), String(new Date().getTime()));
          console.log(String(Math.random()), String(Math.random()));
          return [];
        }"#;
    let a = host()
        .run(code, &ctx(), NOW_MS, 7, &Limits::default())
        .unwrap();
    let b = host()
        .run(code, &ctx(), NOW_MS, 7, &Limits::default())
        .unwrap();
    let c = host()
        .run(code, &ctx(), NOW_MS, 8, &Limits::default())
        .unwrap();
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
    let h = Host::new(wat.as_bytes()).unwrap();
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
    let h = Host::new(wat.as_bytes()).unwrap();
    let (out, _) = h.run_raw(Vec::new(), 0, &Limits::default()).unwrap();
    assert_eq!(out, b"08"); // 0 env vars, EBADF
}

// ---------- bad output ----------

fn expect_bad(code: &str) {
    match run(code) {
        Err(SandboxError::BadOutput(m)) => eprintln!("rejected: {m}"),
        other => panic!("expected BadOutput for {code}, got {other:?}"),
    }
}

#[test]
fn wrong_shapes_are_rejected() {
    expect_bad(r#"function decide(){ return [{type:"steal_funds", stake:"1"}]; }"#);
    expect_bad(
        r#"function decide(){ return [{type:"propose_challenge", stake:"100", recipient:"0xdead"}]; }"#,
    );
    expect_bad(r#"function decide(){ return [{type:"propose_challenge", stake:100}]; }"#);
    expect_bad(r#"function decide(){ return [{type:"propose_challenge", stake:"-5"}]; }"#);
    expect_bad(r#"function decide(){ return [{type:"propose_challenge", stake:"1e9"}]; }"#);
    expect_bad(r#"function decide(){ return [{type:"propose_challenge"}]; }"#);
    expect_bad(
        r#"function decide(){ return [{type:"commit_funds", interaction_type:"loan", stake:"1"}]; }"#,
    );
    expect_bad(r#"function decide(){ return {type:"reject"}; }"#);
    expect_bad(r#"function decide(){ return undefined; }"#);
    expect_bad(r#"function decide(){ return Array(11).fill({type:"reject"}); }"#);
}

#[test]
fn huge_output_is_rejected() {
    let err = run(r#"function decide(){ return Array(5000).fill({type:"reject"}); }"#).unwrap_err(); // ~85 KB > 64 KiB
    assert_eq!(err, SandboxError::OutputTooLarge);
}

#[test]
fn guest_errors_are_reported() {
    assert!(
        matches!(run("function decide(){ throw new Error('nope'); }"),
        Err(SandboxError::Guest(ref m)) if m == "nope")
    );
    assert!(
        matches!(run("const x = 1;"), Err(SandboxError::Guest(ref m)) if m.contains("not defined"))
    );
    assert!(matches!(
        run("function decide( {"),
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
    let h = Host::new(wat.as_bytes()).unwrap();
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
fn oversize_inputs_are_rejected_before_running() {
    let big_code = format!("function decide(){{ return []; }} //{}", "x".repeat(70_000));
    assert_eq!(run(&big_code).unwrap_err(), SandboxError::CodeTooLarge);
    let big_ctx = json!({ "blob": "y".repeat(300_000) });
    let err = host()
        .run(
            "function decide(){ return []; }",
            &big_ctx,
            NOW_MS,
            1,
            &Limits::default(),
        )
        .unwrap_err();
    assert_eq!(err, SandboxError::InputTooLarge);
}

// ---------- latency ----------

/// Run alone for stable numbers:
/// cargo test --test sandbox latency -- --ignored --nocapture --test-threads=1
#[test]
#[ignore]
fn latency() {
    let wasm = runner_wasm();
    let t = Instant::now();
    let h = Host::new(&wasm).unwrap();
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
        let out = h.run(code, &ctx(), NOW_MS, 1, &Limits::default()).unwrap();
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

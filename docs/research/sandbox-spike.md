# Sandbox spike: user JS `decide(ctx)` in QuickJS inside Wasmtime (D-11), 2026-11-16

**Verdict: GO for D-11.** Every required limit works, all tests pass, and a call takes about 0.16 ms. The Rhai fallback was not needed.

> Superseded 2026-11-16: the spike crate was ported to `backend/crates/sandbox` (`aen-sandbox`, 27 tests) and `spikes/sandbox/` was removed; it remains in git history at commit c1d3f79. Build the guest with `backend/crates/sandbox/build-guest.sh`.

## What was built (`spikes/sandbox/`, a standalone Cargo workspace that is not part of `backend/`)
- **Guest:** `guest/runner.js` is compiled **once** with Javy into `runner.wasm` (1.36 MB) by `build-guest.sh`. The runner reads `{code, ctx, now_ms, seed}` from stdin and turns the user's source into a function with `new Function`. It replaces `Date` with the host's `now_ms` and `Math.random` with a PRNG seeded by `seed`. It captures `console.*` into `logs`, deletes the `Javy` I/O global, and writes `{ok, intents, logs}` or `{ok:false, error}`.
- **Host (`src/lib.rs`):** wasmtime `=50.0.1` with `default-features = false` and the `cranelift`, `runtime`, `std`, `parallel-compilation` and `wat` features. It enables fuel (default 50M) and an epoch deadline (a 10 ms wall-clock ticker, default 50 ticks = 500 ms). Memory is capped at 32 MiB through `StoreLimits` wrapped in a `ResourceLimiter`, and the wasm stack is 512 KiB. Each run gets a fresh `Store` and instance.
- **No WASI crate.** The Javy module imports exactly 9 `wasi_snapshot_preview1` functions, and the host implements each one by hand:
  - stdin and stdout are memory buffers, with stdout capped at 64 KiB.
  - stderr is capped at 4 KiB and then discarded.
  - The environment is empty and the clock is fixed.
  - Seek returns `ESPIPE`, and file descriptors 3 and up return `EBADF`.
  - `proc_exit` traps.
  - Any other import, such as `path_open`, `sock_*` or `random_get`, is linked with `define_unknown_imports_as_traps`.
- **Output validation:** output is parsed as JSON into an envelope with `deny_unknown_fields`, then into an `Intent` that mirrors `aen_core::authorize::Intent` (internally tagged with `deny_unknown_fields`). Money must be a string of 1 to 18 digits. At most 10 intents and 20 logs of 200 characters each are accepted. Code is capped at 64 KiB and input at 256 KiB, and both are checked before the guest runs.

## Versions and checksums
- Javy CLI v9.1.0 (the latest release, 2026-07-30), from `github.com/bytecodealliance/javy/releases`. Asset `javy-arm-macos-v9.1.0.gz`, SHA-256 `99e9ec6a8e8c98e119d137c08a921d2443d3b873c675a5571e1800f4451e6294`. This matches the release's published `.sha256` file, and `build-guest.sh` re-checks it on every download.
- Build flags: `-J javy-stream-io=y -J text-encoding=y -J event-loop=n -C deterministic=y -C source=omitted`. The resulting `runner.wasm` had SHA-256 `aa8487b5d03c8a65b5a708053dd5343dd69a88c2583f7677de7622ca02db2eab`.
- Binaries go in the repo-root `tools/` directory and `runner.wasm` stays in the spike directory. Both are gitignored and neither is committed.
- Toolchain: rustc 1.100.0. The dependency graph has about 110 crates. A clean build takes about 80 s with dependencies at `opt-level=3`.

## Test results: `cd spikes/sandbox && ./build-guest.sh && cargo test`, 17 passed and 1 ignored (latency)

| Area | Result |
|---|---|
| Normal `decide` | Returns valid `propose_challenge` and `commit_funds` intents plus logs. |
| State isolation | Globals set in one run are not visible in the next. |
| Infinite loop, fuel | `OutOfFuel` after about 7 ms, because 50M fuel is roughly 7 ms of CPU. |
| Infinite loop, epoch | With fuel effectively unlimited, `Timeout` at 506 ms. |
| Memory bomb (array) | `MemoryLimit` at 32 MiB. The string-doubling bomb stops earlier, with QuickJS's own `"string too long"` error. |
| Deep recursion | `StackOverflow` from the wasm stack guard. QuickJS's own stack check does not fire inside wasm. |
| Capabilities | `require`, `process`, `fetch`, `XMLHttpRequest`, `WebSocket`, `Javy`, `std`, `os`, `Deno`, `Bun`, `setTimeout`, `setInterval` and `WebAssembly` are all `undefined`. `Date` and `Math.random` are deterministic for a given seed. Raw-ABI checks: the environment has 0 variables and fd 3 returns `EBADF`. A module calling `path_open` traps. `import("os")` aborts the guest, so the run fails closed. |
| Bad output | Rejected cases: an unknown intent type, an extra field, a numeric, negative or `1e9` stake, a missing field, a bad enum value, a non-array result, `undefined`, 11 intents, and an unknown envelope field. Non-JSON stdout (from a raw wasm module) is rejected as `BadOutput`. About 85 KB of output is rejected as `OutputTooLarge`. A throw, a missing `decide` or a syntax error is returned as `Guest(msg)`. |

## Latency (M-series Mac, one test thread; `cargo test --test sandbox latency -- --ignored --nocapture --test-threads=1`)

| Step | Run 1 | Run 2 |
|---|---|---|
| Cold compile (`Engine` + `Module::new`, 1.36 MB) | 574 ms | 713 ms |
| Instantiate, `_start`, bad-input exit (p50 / p95, n=20) | 135 / 187 µs | 174 / 278 µs |
| Full `run()` including JSON validation (p50 / p95 / max, n=100) | 156 / 163 / 756 µs | 181 / 405 / 810 µs |
| Fuel used by a simple `decide` | 1.05M (about 2% of the 50M budget) | |

## Risks and open points
- Compile costs about 0.6 s, so do it once at boot (or use `Module::serialize`/`deserialize`) and share the `Host`. `run` is synchronous: call it through `spawn_blocking` behind a `Semaphore`. The tokio path itself was not exercised.
- The 50M fuel budget is about 7 ms of CPU, roughly 50 times a trivial strategy. Tune it per plan. The epoch deadline is the backstop for time spent in host calls.
- Javy's own failures, such as `import()`, appear as a bare `unreachable` trap. They should be mapped to a friendly "unsupported feature" message.
- If a Javy upgrade adds a new WASI import, that import traps when called, so the sandbox fails closed. The tests would catch it.
- Wasmtime CVEs are the residual risk, so keep the pin current. For more defence in depth, run the host in a worker process that holds no secrets.
- Not verified: Linux and x86 behaviour, the pooling allocator, and load under concurrency.
- Owner decision: build `runner.wasm` in CI and the dev setup with the checksum-verified Javy, or commit it with a recorded hash.
- `aen_core::authorize::Intent` has no `deny_unknown_fields`. The integration should either add it or deserialize into a strict mirror type and convert.

## Recommended integration for a `custom.code` block
Add a new `backend/crates/sandbox` crate (`aen-sandbox`) that contains this host.
1. The runtime builds `ctx` from the agent's state: market snapshot, balances as minor-unit strings, `now_ms`, and `seed = hash(agent_id, tick)`.
2. It calls `Host::run` through `spawn_blocking` behind a `Semaphore(4)`, which returns `Vec<Intent>`.
3. Each intent goes through `core::authorize(policy, state, intent)`.
4. Authorized intents are executed. Rejections and `SandboxError`s are written to the audit trail as decision summaries.

`custom.code` stores at most 64 KiB of source in the agent config, and saving a config edit performs a dry run against a sample `ctx`. The sandbox never sees keys, addresses or limits, and its output is only ever a proposal.

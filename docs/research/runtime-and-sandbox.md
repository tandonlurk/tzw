# Runtime, sandbox, builder and Rust stack research (2026-11-14)

Versions come from the crates.io / npm registry APIs on 2026-11-14 (https://crates.io/api/v1/crates/<name>, https://registry.npmjs.org/<pkg>/latest). Anything not confirmed in a doc is marked UNVERIFIED.

## 1. Custom-code sandbox

### 1.1 Comparison

| Option | Version | CPU limit | Memory limit | Net / FS | Isolation per user | Maturity | Tokio embed effort |
|---|---|---|---|---|---|---|---|
| Wasmtime core module | wasmtime 50.0.1 (MSRV 1.97) | `Config::consume_fuel(true)` + `Store::set_fuel`; `epoch_interruption(true)` + `Engine::increment_epoch` + `Store::set_epoch_deadline`. Epoch is faster but non-deterministic. https://docs.rs/wasmtime/latest/wasmtime/struct.Config.html | `StoreLimitsBuilder::memory_size/instances/tables/table_elements` via `ResourceLimiter`. https://docs.rs/wasmtime/latest/wasmtime/struct.StoreLimitsBuilder.html. `max_wasm_stack` defaults to 512 KiB. | Deny by default. A module gets only the imports you link, and WASI is opt-in. | A fresh `Store` and `Instance` per run. Share one `Engine` and a pre-compiled `Module`. | Bytecode Alliance, production use | Low. Use `spawn_blocking` for sync, or the async API. |
| Wasmtime component model (WIT) | same crate, `wasmtime::component` | same | same | same; typed WIT interface | same | Stable, but guest toolchains (`wit-bindgen`, `cargo-component`, `jco`) add friction | Medium. Needs bindgen and a component-producing guest toolchain. |
| Wasmer | wasmer 7.5.0 | Metering middleware. UNVERIFIED in detail. | `Tunables` / limiting tunables. UNVERIFIED. | WASIX adds sockets. Not needed here. | Per `Store` | Mature, but fewer sandbox-focused docs than Wasmtime | Low to medium. No advantage over Wasmtime for this use. |
| Extism | extism 1.41.0 (Wasmtime-based host SDK) | `timeout_ms` in the manifest. UNVERIFIED on the docs page. | `memory.max_pages` (64 KiB pages), `max_http_response_bytes`, `max_var_bytes` | `allowed_hosts` (empty list = none, `null` = all). `allowed_paths` (empty or `null` = no FS). https://extism.org/docs/concepts/manifest. The page does not say what an omitted key defaults to. Set an empty list explicitly. | A `Plugin` per user | Mature. A JS PDK exists (QuickJS-based, UNVERIFIED). | Low. Its API is simple, but `Plugin` is sync and not `Send`-friendly (UNVERIFIED). Use `spawn_blocking`. |
| Rhai | rhai 1.27.0 | `Engine::set_max_operations` and an `on_progress` callback to terminate. UNVERIFIED in detail. The safety page lists CPU, memory, stack and time as threats and offers the `unchecked` feature to disable checks. https://rhai.rs/book/safety/ | `set_max_string_size/array_size/map_size/call_levels/expr_depths`. UNVERIFIED in detail. | No fs/net unless you register functions. `import` is the module resolver, so disable it. | In-process, so a bug in the host or engine is not a hard boundary | Mature, popular | Very low. It is a pure Rust crate. |
| Boa | boa_engine 0.22.0 | No fuel. Has some runtime-limits API (UNVERIFIED). | No hard cap | No net/fs unless you add it | In-process | Pre-1.0 | Low. |
| rquickjs (native QuickJS) | rquickjs 0.14.0 (MSRV 1.87) | `Runtime::set_interrupt_handler` for timeouts (UNVERIFIED) | `set_memory_limit`, `set_max_stack_size` (UNVERIFIED) | None by default | In-process C code. A QuickJS memory bug could escape the Rust boundary. Weaker than wasm. | Moderate | Low, but builds C. |
| Javy (QuickJS compiled to wasm) | javy 8.1.0 (CLI), javy-plugin-api 7.1.0 | Inherits Wasmtime fuel and epochs | Inherits Wasmtime limits | Gets only the WASI you give it. The README example uses stdin/stdout. https://github.com/bytecodealliance/javy | A Wasmtime store per run | Bytecode Alliance. Static modules are at least 869 KB, and dynamic linking gives 1 to 16 KB modules. | Low. Needs the `javy` CLI at build time for the guest. |

### 1.2 User-facing language

| Language | Verdict |
|---|---|
| JavaScript run in a QuickJS wasm guest | Recommended. Users already know it. You can pre-build one runner guest once, so there is no per-user compile step. |
| Rust or AssemblyScript compiled to wasm | Rejected for the hackathon. It needs a server-side compiler toolchain, which is itself an attack surface, and in-browser compilation is heavy. |
| Rhai | Good fallback. Simple, but an obscure language for users, and in-process. |

### 1.3 Recommendation: Wasmtime 50.0.1 core module plus a pre-built QuickJS runner guest

Crates for the host: `wasmtime = "50.0.1"`, `wasmtime-wasi = "50.0.1"` (only if the guest needs WASI stdio). Cargo features: UNVERIFIED, so check the docs.

Design: one prebuilt `runner.wasm`, built with the Javy CLI. Do not compile each user's JS. The runner reads one JSON document from stdin, evaluates the user's source string, and writes JSON to stdout.

Alternative without WASI: link the runner's imports with no WASI at all, and pass data through linear memory with `alloc(len)` and `run(ptr,len) -> packed(ptr,len)` exports. This is the smallest attack surface. It requires writing a small Rust guest with `rquickjs` compiled to `wasm32-unknown-unknown`. That is more work, so the Javy stdio route is quicker.

Interface sketch (JSON in, JSON out):
```
input  (stdin):  { "schema_version":1, "code":"<user JS>", "ctx":{ "agent_id":"...", "now":"...", "market":{...}, "balances":{...} } }
user JS defines:  function decide(ctx){ return [ {type:"propose_challenge", params:{...}} ]; }
output (stdout): { "ok":true, "proposals":[ {"type":"propose_challenge","params":{...}} ], "logs":["..."] }
                 or { "ok":false, "error":"..." }
```
Host steps:
1. Parse stdout as JSON with a size cap.
2. Deserialize into `Vec<Proposal>` with `#[serde(deny_unknown_fields)]`.
3. Cap the count at 10 and cap string lengths.
4. Pass each proposal to the Rust permission engine, which authorizes or rejects. Never trust the guest for amounts, addresses or limits.

Concrete limits (starting values, UNVERIFIED until tuned against the runner's real idle cost):

| Limit | Setting |
|---|---|
| Fuel | `consume_fuel(true)`; `set_fuel(50_000_000)` per run. QuickJS startup in wasm costs some of this, so measure first. |
| Wall clock | `epoch_interruption(true)`. A ticker thread calls `engine.increment_epoch()` every 10 ms, and `set_epoch_deadline(50)` gives 500 ms. Use both fuel and epoch, because host calls are not metered by fuel. |
| Memory | `StoreLimitsBuilder::new().memory_size(32<<20).instances(1).tables(2).memories(1).table_elements(10_000).trap_on_grow_failure(true).build()`, then `store.limiter(\|d\| &mut d.limits)` |
| Stack | `max_wasm_stack(512 KiB)` (the default) |
| Network | None. Link no socket-related imports, and do not enable WASI sockets. |
| Filesystem | None. No `preopened_dir`. Stdin and stdout are in-memory pipes only. |
| Env, clock, random | Do not inherit env. Pass `now` via ctx, and seed any `Math.random` deterministically. |
| I/O size | stdin at most 256 KiB, stdout at most 64 KiB, code at most 64 KiB |
| Concurrency | A `tokio::sync::Semaphore` (e.g. 4) around `spawn_blocking` calls |
| Per-user isolation | New `Store` and `Instance` per execution. Never reuse state across users. |
| Defence in depth | Run the executor as a separate low-privilege process or container with no secrets in its environment. The main backend should not hold the keys the sandbox could reach. |

Residual risk: the Wasmtime isolation is strong, but a Wasmtime CVE is possible (UNVERIFIED whether any are open). Keep the crate current.

### 1.4 Fallback if time runs out

1. Ship Rhai 1.27.0 in a separate worker process (not in the main backend) with `max_operations`, size limits and `import` disabled. Or:
2. Defer custom code. Offer only the visual builder plus a restricted expression language inside the JSON graph (for example `jsonlogic` or a tiny hand-written expression evaluator). This cannot loop, so it needs no CPU sandbox.

## 2. Visual builder

### 2.1 State of the options

| Item | Finding | Source |
|---|---|---|
| React Flow | `@xyflow/react` 12.12.1 (published 2026-11-02), MIT. Peer deps: react >= 17. A 13.0.0-next.3 tag exists but is not stable. | npm registry |
| Next.js | next 16.4.6 supports React 18.2 and 19. v12 added SSR support through node `width`, `height` and `handles`. UNVERIFIED for App Router specifically. The conservative approach is a `"use client"` component, loaded with `next/dynamic` and `ssr:false` if hydration issues appear. Also import `@xyflow/react/dist/style.css`. | https://reactflow.dev/learn/troubleshooting/migrate-to-v12 |
| Custom nodes | A React component registered in a `nodeTypes` object defined outside the component. `Handle` components give typed ports. | https://reactflow.dev/learn/customization/custom-nodes |
| Serialization | `reactFlowInstance.toObject()` returns `{nodes, edges, viewport}`. | https://reactflow.dev/api-reference/types/react-flow-instance |
| Validation hooks | `isValidConnection` (return false to block an edge), `onConnect`, `onBeforeDelete`. | https://reactflow.dev/api-reference/react-flow |
| Layout | `@dagrejs/dagre` 3.1.3 (MIT), optional | npm registry |
| Rete.js | `rete` 2.0.6 (MIT). More opinionated, with its own plugin system. Smaller React ecosystem. | npm registry |
| Blockly | `blockly` 13.4.0 (Apache-2.0). Block-and-statement programming UI. Oriented to generating code, not typed node graphs. | npm registry |

Recommendation: React Flow. It has the most examples, MIT licence, and typed `Handle` ports map directly to the schema. Rete is heavier to learn. Blockly fits sequential code better than a trigger/condition/action graph.

Do not persist the editor's `toObject()` output directly. Convert it to the domain schema below and store the layout (x, y) in a separate field. The backend must never trust frontend-only validation.

### 2.2 Graph JSON schema pattern

```json
{
  "schema_version": 1,
  "nodes": [
    {"id":"n1","type":"trigger.schedule","params":{"every_minutes":60},"ui":{"x":0,"y":0}},
    {"id":"n2","type":"reason.llm","params":{"prompt_template":"...","output_schema":"challenge_idea_v1"}},
    {"id":"n3","type":"condition.compare","params":{"left":"$n2.confidence","op":">=","right":0.7}},
    {"id":"n4","type":"action.propose_challenge","params":{"max_stake_usdc_minor":5000000}}
  ],
  "edges": [
    {"id":"e1","from":{"node":"n1","port":"out"},"to":{"node":"n2","port":"in"}},
    {"id":"e2","from":{"node":"n3","port":"true"},"to":{"node":"n4","port":"in"}}
  ]
}
```

Design rules:
- **Node type registry**: a `type` string (namespaced `trigger.*`, `reason.*`, `condition.*`, `action.*`) maps to a Rust enum with `#[serde(tag="type", deny_unknown_fields)]`. Unknown types are rejected.
- **Typed ports**: each node type declares input and output ports with a data type (`Event`, `Number`, `Bool`, `Json:<schema_id>`, `Proposal`). An edge is valid only if the source type is assignable to the target type. Mirror the same table in the frontend's `isValidConnection`.
- **Versioning**: `schema_version` is an integer. The backend keeps migrators from version N to N+1, and stores the raw graph with its version in a `jsonb` column alongside the agent config version.
- **Validation (backend, authoritative)**:
  1. Exactly one trigger node (or an explicit allowed set).
  2. No cycles: Kahn's algorithm or a DFS with colours. Reject if the graph is not a DAG.
  3. Unique node ids, and every edge endpoint exists with a declared port.
  4. Type-compatible edges.
  5. Every required input port is connected.
  6. All paths end in a terminal `action.*` node, and actions have no outgoing edges.
  7. No orphan nodes unreachable from the trigger.
  8. Caps: at most 50 nodes, 100 edges, bounded `params` size.
  9. Action params validated against the permission engine limits at run time, not only at save time.
- **Execution**: topological order, with a per-run step budget and a timeout. LLM output is parsed against a typed schema before it feeds a condition.
- **Comparable tools** (no doc fetched, UNVERIFIED in detail): n8n stores workflows as JSON (confirmed only at https://docs.n8n.io/workflows/export-import/), and from memory has a `nodes` array and a `connections` object keyed by source node name. Node-RED stores a flat array of node objects, each with `id`, `type`, `z` (tab) and `wires` (arrays of target ids per output). The Node-RED doc page returned 404 when fetched. Both put edges on the source node, which makes cycle and port checks harder than a separate `edges` list. A separate edge list is the better pattern here.

## 3. Rust crates

| Crate | Current stable | MSRV | Notes |
|---|---|---|---|
| axum | 0.8.9 | 1.80 | 0.8 uses `/{param}` path syntax instead of `/:param`. |
| tokio | 1.54.1 | 1.85 | |
| sqlx | 0.9.0 (released 2026-05-21) | 1.94.0 | Check 0.9 changes before copying older tutorials. UNVERIFIED which feature flags changed. Offline mode: `cargo sqlx prepare` writes `.sqlx/`, and builds set `SQLX_OFFLINE=true`. Migrations: `sqlx::migrate!()`. |
| sqlx-cli | 0.9.0 | n/a | |
| alloy | 2.5.0 | 1.94.1 | A major version bump from 1.x. Older guides and examples may not compile. Provider, `sol!`, signer and subscription APIs need verification against 2.x docs. UNVERIFIED. |
| wasmtime | 50.0.1 | 1.97.0 | Sets the project-wide minimum toolchain (Rust 1.97). |
| serde | 1.0.229 | | |
| schemars | 1.2.3 | 1.74 | 1.x is a major change from 0.8. |
| validator | 0.21.0 | | garde 0.23.0 is the alternative. Pick one. validator is more common. |
| reqwest | 0.13.5 | 1.85.0 | Use `rustls`, as the TLS default may differ in 0.13. UNVERIFIED. |
| tower-http | 0.7.1 | | Use for CORS, trace, timeout and limit layers. Check it matches the axum 0.8 `tower` version. UNVERIFIED. |
| tracing | 0.1.44 | | Pair with `tracing-subscriber`. |
| rust_decimal | 1.43.0 | | |
| utoipa | 6.0.0 | 1.88 | OpenAPI generation. |
| ts-rs | 12.0.1 | | Generates TypeScript types from Rust structs. Simpler than OpenAPI if only types are needed. |

Overall minimum Rust: 1.97 (wasmtime), and rustup stable should satisfy all of the above. Pin it with `rust-toolchain.toml`.

USDC money handling: store integers in minor units (6 decimals, so 1 USDC = 1_000_000) as `i64`/`BIGINT`, or `NUMERIC(38,0)` if it may hold on-chain `uint256`. Alloy's `U256` suits on-chain values. Convert at the edges only. Use `rust_decimal` just for display and parsing user input. Never use `f64` for money. On the JSON boundary, send amounts as strings (`"5000000"`) to avoid JS number precision loss.

Type sharing: `ts-rs` 12.0.1 is the least effort (derive on structs, export `.ts` files). `utoipa` 6.0.0 plus `openapi-typescript` gives a typed client and docs, at more cost. Recommend `ts-rs` for the hackathon. UNVERIFIED that the two work well together with `schemars` 1.x.

## 4. Local Postgres on macOS without Docker

| Option | Steps |
|---|---|
| Homebrew | `brew install postgresql@17` (check `brew search postgresql` for the current major, UNVERIFIED), `brew services start postgresql@17`, add the keg `bin` to PATH, then `createdb <name>`. Default superuser is your macOS user with no password. |
| Postgres.app | Download the app (https://postgresapp.com), start a server, and add `/Applications/Postgres.app/Contents/Versions/latest/bin` to PATH. |

sqlx-cli (0.9.0):
```
cargo install sqlx-cli --no-default-features --features rustls,postgres
export DATABASE_URL=postgres://localhost/<db>
sqlx database create
sqlx migrate add <name>      # creates migrations/<ts>_<name>.sql
sqlx migrate run
cargo sqlx prepare           # writes .sqlx/ for offline builds; commit it
```
Flags are as I recall them, UNVERIFIED against 0.9.

## 5. Unresolved questions

1. Does the Javy 8.x CLI still build a static module whose runner can eval a source string passed at run time? If not, build the runner on `rquickjs` for wasm instead.
2. Real fuel cost of QuickJS startup in the Javy runner. This sets the fuel budget.
3. Are `wasmtime` 50 and `wasmtime-wasi` 50 the right pair, and what is the minimal WASI API for in-memory stdio? Check the `wasmtime_wasi` docs.
4. alloy 2.x API for HTTP/WS providers, `sol!`, signers and log subscription. Read the 2.x migration notes before writing chain code. Also check that Arc works with the standard alloy provider (needs the research on Arc).
5. sqlx 0.9 changes versus 0.8 (feature flags, `query!` behaviour).
6. Should the sandbox run as a separate process? Recommended, but it adds deployment work.
7. Where does LLM output enter the graph (a typed `reason.llm` node), and how are prompts protected from injection from market data?
8. Do the Next.js App Router and React 19 work cleanly with `@xyflow/react` 12.12.1? Likely, but only verified indirectly.

# Populated portfolio and forum — 2026-11-20

Owner requested a 5–7k wallet, plenty of simulated transactions, three working agent strategies with different multipliers, and a Public Forum tab. Implemented 6000 simulated USDC, actual clone families2×3×5×, API-generated challenge/ledger history, and local persistent forum.

## Integrated work

- `371f1c8`: Public Forum UI and clearer Firms/Challenges descriptions. UI168tests,TypeScript,targetedlint,productionwebpackbuild pass.
- `2c4310a`: API-only fresh-database portfolio seed script and reproducible documentation. Lead ran it successfully, not just syntax checked.
- `1d020f1`: forum storage/API, migration0016, six idempotent fictional threads. Three isolated PostgreSQL tests passed, format/diff clean. Lead reviewed source and API/UI compatibility.

## Running now

- Web `http://127.0.0.1:3000`, session70186; proxy upstream `http://127.0.0.1:8090/api`.
- Backend session37430, binary `/private/tmp/tzw-forum-target/debug/aen-server`, database `aen_video_demo_20261120`.
- Fake market, ticks15s, cooldown600, funding window300; no Arc signer or xAI key in this process. Sandbox module explicitly points at root repository `backend/crates/sandbox/guest/runner.wasm`.
- Original `aen_dev` preserved. Older8080 backend stopped. No remote pushes.

## Verified capture

6000 total simulated USDC;1500 unallocated;218 committed;36 settled and3active challenges;22 wallet entries and274 agent-ledger entries. Atlas Momentum2instances;Vega Contrarian3;Lumen Cautious5. Actual family balances after seeded history1900/1064/1536. Aggregate wallet remains6000. Runtime may advance these counts.

Six fictional threads/replies seeded via `/dev/seed-forum`. HTTP200 for Wallet, My Agents, Public Forum, Challenges and Firms; frontend-proxy API assertions checked money conservation, three roots/ten total agents, six demo threads. UI forms covered by backend persistence tests; no claim of new live browser form testing.

## Reproduce / next

Follow `docs/demo-portfolio.md`; script refuses nonempty/partial databases and avoids blind clone retries. Use current source build, not an older copied root binary. Record the populated app. All history is synthetic simulated activity; forum is a local single-owner prototype with owner-authored agent attribution, not a hosted public service. Firms are teams with role/compensation agreements; Challenges are individual measurable staked deals.

Another agent is actively editing `demo-video/` and has unrelated uncommitted dashboard/navigation files in this shared checkout. Those changes were preserved and excluded from this task's commits.

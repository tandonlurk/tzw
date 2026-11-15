# Security review, M1 (branch `security-tests`)

Scope: `aen-core` (money, ledger, `authorize`), `aen-server` API + migrations, `ChallengeEscrow`. No production code was changed. Threat model: `docs/security/threat-model.md`.

Test results: `cargo test --workspace` 138 pass, 1 ignored (SEC-01 repro); `forge test` 80 pass (72 existing + 8 new). New tests: `backend/crates/core/tests/adversarial_{authorize,ledger}.rs`, `backend/crates/server/tests/adversarial_{api,integrity}.rs` (use db `aen_sec_test`, or `TEST_DATABASE_URL`), `contracts/test/ChallengeEscrow.Adversarial.t.sol`. Run the ignored repro with `cargo test -p aen-server --test adversarial_integrity -- --include-ignored`.

Things that were attacked and held: authorize boundaries, zero, overflow (200k random decisions, no Allow violating policy); ledger random sequences (always balanced, no negatives, rejected sets change nothing); 24 parallel allocations of 30 against 100 (exactly 3 succeed); 16 parallel replays of one idempotency key (posted once); key reuse across agents/endpoints (409); malformed amounts, headers, ids (4xx, never 5xx, no ledger rows); i64::MAX double deposit (refused, wallet intact); append-only triggers; concurrent edits (consecutive versions); extreme policies rejected; escrow donations, huge stakes, claim redirection, no-dust over random terminal paths, absorbing terminal states, cross-challenge isolation.

## Verified bugs

### SEC-01 (Medium now, High once real funds) CSRF: POST endpoints ignore Content-Type
`ApiJson` reads body bytes without checking `Content-Type`. A web page the owner visits can submit a "simple" cross-origin POST (`text/plain`, no preflight) to `http://localhost:8080/api/wallet/simulated-deposit`, `/agents`, `/agents/{id}/allocate`, `/status`. CORS only hides the response; the write happens.
Repro: ignored test `state_changing_posts_must_require_a_json_content_type`: `curl -X POST localhost:8080/api/wallet/simulated-deposit -H 'content-type: text/plain' -H 'origin: https://evil.example' -d '{"amount":"1000000"}'` returns 200.
Fix: in `ApiJson::from_request`, require `application/json` (415 otherwise); PUT already needs preflight. Also add Host/Origin allow-list (SEC-02).

### SEC-03 (Medium once M2 settles interactions) Settlement is accounted per agent, not per interaction
`settle()` moves stakes through `Escrow(id)` which nets to zero, so `check_balances` only checks each agent's total `committed`. Settling the same interaction twice under different idempotency keys succeeds if the agents hold committed funds from other interactions, consuming those stakes. Test `settle_twice_with_different_keys_is_not_prevented_by_the_ledger` pins this behavior.
Fix: derive the idempotency key from `(interaction_id, reason)` in the caller, enforce state `Resolving -> Settled` in the same transaction under a row lock on the interaction, and add a unique partial index on `ledger_entries (interaction_id) WHERE reason = 'settle'`. Optionally track per-interaction committed amounts in the ledger.

### SEC-05 (Low) Oversize body returns 400, not 413
`ApiJson` maps the body-limit rejection to 400. Cosmetic; map `LengthLimitError` to 413.

## Design gaps and theoretical concerns

- SEC-02 (Medium for any public deployment, known D-9): no authentication, no Host check, so DNS rebinding and any local process can use the API. Loopback bind is the only control. Needed before a tunnel (QuickNode webhook) is exposed: expose only the webhook route, add auth + Host check.
- SEC-04 (Low/Medium, verified behavior): `ChallengeEscrow.create` accepts `resolveDeadline = type(uint64).max`. The timeout refund then never opens and only the resolver can release stakes. Fix: cap `resolveDeadline - fundDeadline` (e.g. 30 days) in `create`, and have agents verify `getChallenge` terms before depositing. Test `test_unboundedResolveDeadline_removesTheTimeoutRefund`.
- SEC-07 (Low, inherent): resolver may be a party and resolve its own challenge. Fix: reject `partyA/B == RESOLVER` in `create` (cheap) or document. Test `test_resolverCanBeAPartyAndResolveInItsOwnFavour`.
- SEC-06 (Info): direct token transfers to the escrow are stranded; token balance is `>= totalHeld`, not `==` as `contracts/README.md` says. Safe, but correct the README.
- SEC-08 (Low): `forbid_mutation` triggers are row-level, so `TRUNCATE` (and a superuser) bypasses append-only; the DB does not enforce balanced sets (only the app does). Fix: `BEFORE TRUNCATE` trigger, and a deferred constraint trigger asserting per-key `SUM(delta) = 0`; use a least-privilege DB role for the app (no TRUNCATE/UPDATE/DELETE).
- SEC-09 (Low): idempotency keys are global, client-chosen, and the replay check ignores `interaction_id`; any caller can pre-claim a key (single user today). Scope keys per env and caller; compare `interaction_id` on replay.
- Escrow trust assumptions (README) stand: resolver can award any Active challenge, no disputes, USDC upgradeable/blocklistable, not run on Arc's fork (`arc-forge`). A returndata-bomb token could grief gas, but the token is trusted.
- Unimplemented and unreviewable yet: key custody (D-3, needs owner approval), QuickNode webhook auth, LLM output validation, custom-code sandbox. See threat model for required controls.

## Priorities
1. SEC-01 (small fix, do before any demo on a shared machine). 2. SEC-03 before M2 settlement lands. 3. SEC-04/SEC-07 before the first Arc deploy (cheap contract changes; redeploy is free on testnet). 4. SEC-02 and webhook auth before exposing a tunnel.

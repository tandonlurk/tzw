# QA / Security

## Grok Bot practice review (2026-11-20)

- Objective/scope: read-only integration/security review of backend capability boundary and practice UI. Assigned independent review specialist; lead owns integration. Status: static review and local automated validation complete.
- Reviewed: distinct Bot/owner credentials; namespace fail-closed behavior; Host/Origin enforcement; server-assigned round roles; immutable snapshot/history; row-lock round cap; absence of financial/config writes; escaped text and HTTP(S) evidence links; Basic/Advanced routing.
- Finding/fix: 10-second polling initially unmounted the owner reply editor and lost drafts. UI now preserves draft state through reloads/errors and resets per agent. Verified by static review.
- No unresolved high/medium static findings. Reviewer changed no files and did not use the backend test database. Frontend specialist reports 168 tests, TypeScript, targeted lint, and production build passing.
- Backend specialist reports four PostgreSQL integration tests passing on isolated `aen_sparring_test`: credentials/roles, Host/Origin, immutable snapshots and no financial writes, input/URL bounds and concurrent round admission. Initial sandbox PostgreSQL connection was refused; approved unrestricted test run passed.
- Backend library regression: 84 passed, 0 failed, 1 ignored after approved rerun for mock HTTP bindings. Backend/UI commits integrated on `integration-1119`; diff check clean. No unresolved high/medium review findings.
- Limitations/next: actual Grok Bot access and live round remain unverified. Do not claim trusted model authorship from a token-authenticated source label, evidence verification, or RL improvement.

- **Objective:** Failure-case tests and threat model.
- **Scope:** Tests across modules; docs/security/.
- **Dependencies:** M1 code exists.
- **Assigned agent:** QA / Security specialist (branch `qa`)
- **Status:** M1 pass done on branch `security-tests` (see log); sandbox/webhook tests wait for those features

## Files changed

## Work completed

## Tests run and results

## Decisions made

## Assumptions

## Blockers

## Unresolved questions

## Recommended next steps
- Start with authorize() and ledger invariants, then contract fuzzing, then webhook and sandbox.

## Log (security-tests branch)
- core/tests/adversarial_authorize.rs: 14 tests pass (boundaries, overflow, random allow-invariant, money parsing).
- server/tests/adversarial_api.rs (11) + adversarial_integrity.rs (4) pass; db aen_sec_test. No real bug found in allocate concurrency. Next: contracts adversarial, threat model, report.
- contracts/test/ChallengeEscrow.Adversarial.t.sol 8 pass. SEC-01 (CSRF text/plain) repro ignored test in adversarial_integrity.rs. Next: threat-model.md, reports/security-m1.md, cleanup.
- DONE: threat-model.md, reports/security-m1.md. Totals: cargo 138 pass + 1 ignored (SEC-01); forge 80 pass.
- Next: when M2 lands, re-test SEC-03 (settle per interaction); write webhook signature/replay tests and sandbox escape tests when those exist; run escrow tests under arc-forge.

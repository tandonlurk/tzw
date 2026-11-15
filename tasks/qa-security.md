# QA / Security

- **Objective:** Failure-case tests and threat model.
- **Scope:** Tests across modules; docs/security/.
- **Dependencies:** M1 code exists.
- **Assigned agent:** QA / Security specialist (branch `qa`)
- **Status:** not started

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

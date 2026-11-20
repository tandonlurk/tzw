# Grok Bot mock adversary — 2026-11-20

## Owner direction

Owner selected mock adversary after reviewing Grok Bot product docs. Existing Grok model API code does not demonstrate Grok Bot use. This workflow does not require an xAI API key. D-75 records the implementation boundary.

## Implementation

- Backend: persistent practice sessions with immutable agent-version snapshots and bounded append-only critique/reply history. Separate restricted `SPARRING_BOT_TOKEN` namespace for reading sessions and submitting adversary rounds; no financial/configuration effects.
- Frontend: Sparring tab in Basic and Advanced agent workspaces, session creation/list, copyable Bot prompt, critique/source rendering, owner reply and proposed revision. Drafts survive polling and errors.
- Lead: API contract, setup/run instructions, decisions, tracker, and honest submission distinction between model API and Bot product.
- Read-only review found a draft-loss issue during polling; UI specialist fixed it. No unresolved high/medium static-review findings.

## Validation

- Frontend: 168 tests pass, TypeScript passes, targeted lint clean, production webpack build passes. UI integrated as `84a5058`.
- Backend: cargo check passes. Four PostgreSQL integration tests pass on isolated `aen_sparring_test`, including concurrent 12-round cap, credential/role boundary, Host/Origin, frozen strategy snapshot, and zero financial writes. Library tests: 84 passed, 0 failed, 1 ignored (live Gemini). Initial sandbox attempts denied local database/mock-HTTP connections; approved unrestricted reruns passed. Owner demo database was not touched. Backend integrated as `8c38667`; format/diff checks clean.
- No real Grok Bot round or browser interaction with the new live backend has been verified yet.

## Next live steps

1. Owner installs/signs into Grok Bot and confirms access.
2. Generate a separate `SPARRING_BOT_TOKEN` (at least 32 characters, never the owner API token), save in backend environment, restart with migrations. Keep the general API protected behind the public tunnel and the web server using its token proxy.
3. Configure Bot with public HTTPS backend URL and restricted token via secure secret handoff; never share `.env`, owner token, wallet seeds, or keys.
4. Create a thesis in an agent's Sparring tab and paste its generated prompt into Grok Bot. Observe an actual Bot critique saved in TZW, then submit an owner revision and request another critique.
5. Record both Bot task output and saved TZW rounds. Until this is done, report the integration as locally tested and live Bot use as unverified.

Full instructions: `docs/setup/grok-bot-sparring.md`. No automatic rebuttal, strategy activation, backtest, critique quality score, or reinforcement-learning training is implemented or claimed.

## Coordination

Another agent is actively editing and committing `demo-video/` on shared `integration-1119`. Preserve its files and do not switch the shared checkout. Backend and UI specialists use isolated worktrees and commits. Nothing is pushed by this task. PR #10 merge was already confirmed by owner.

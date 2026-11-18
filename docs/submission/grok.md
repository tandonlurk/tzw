# Grok Bot (draft)

Criteria as published: "Grok Bot must do real work in the project." The exact meaning of "Grok Bot" is UNVERIFIED (ask at sponsor office hours, Nov 16-19).

## Where Grok does work

- **Agent reasoning** (`backend/crates/server/src/grok.rs`): given market context and the agent's configuration, Grok returns a structured view (side, conviction, proposed terms) via JSON-schema output. The backend validates it and turns it into an `Intent`; `core::authorize` decides whether anything moves. Grok cannot authorize funds.
- **Co-captain** (`backend/crates/server/src/grok_cocaptain.rs`): on request, Grok drafts suggestions for the owner. Suggestions can only touch a fixed set of config fields (D-37), are validated, and take effect only when the owner accepts them.
- Without `XAI_API_KEY` both fall back to rule-based logic, so the demo runs offline.

## Honest status

- Endpoint `https://api.x.ai/v1`, default model `grok-4.7`: **not yet called live**. Both paths are tested against a mock server only (request shape, timeouts, malformed output rejected). Waiting on the owner's key and D-8; no `XAI_API_KEY` as of 2026-11-18.
- Rule-based reasoning is what the recorded demo uses unless a key is provided before submission.

# Grok Bot (draft)

Criteria as published: "Grok Bot must do real work in the project." Sponsor qualification is not independently confirmed. The owner supplied the [Grok Bot product documentation](https://docs.x.ai/grok-bot/overview) on 2026-11-20; Grok Bot is a separate cloud-agent product, so the model API integration below is not evidence of using that product.

## Mock adversary — current submission direction

Owner selected Grok Bot as a mock adversary (D-75): the owner creates a thesis for an agent, Grok Bot reads the version-linked practice brief, researches objections, and submits a structured critique with source links and proposed revised terms. The owner can reply or record a revision for another round. The saved exchange shows what changed and why without rewriting the agent's historical decisions.

The dedicated Bot API uses a restricted credential with no ability to edit configurations, approve transactions, move funds, or settle challenges. A practice revision does not automatically affect the live runtime. Initial integration uses the Bot's cloud terminal and HTTP; it does not claim a custom MCP connector.

**Status 2026-11-20:** built and integrated on `integration-1119` (`8c38667`, `84a5058`). Backend library 84 passed / 1 ignored and four PostgreSQL integration tests passed; web 168 tests, TypeScript, targeted lint and production build passed. Live Grok Bot round still pending owner setup. Do not claim a successful live Bot round until it has actually been run and recorded. Backend/UI test fixtures do not prove that Grok Bot produced a critique. Setup and recording instructions: `docs/setup/grok-bot-sparring.md`.

This demonstrates adversarial feedback and persistent practice history. It does not demonstrate reinforcement-learning training, fine-tuning, backtested improvements, or automatic strategy deployment.

## Separate optional model-API integration

- **Agent reasoning** (`backend/crates/server/src/grok.rs`): given market context and the agent's configuration, Grok returns a structured view (side, conviction, proposed terms) via JSON-schema output. The backend validates it and turns it into an `Intent`; `core::authorize` decides whether anything moves. Grok cannot authorize funds.
- **Co-captain** (`backend/crates/server/src/grok_cocaptain.rs`): on request, Grok drafts suggestions for the owner. Suggestions can only touch a fixed set of config fields (D-37), are validated, and take effect only when the owner accepts them.
- Without `XAI_API_KEY` both fall back to rule-based logic, so the demo runs offline.

## Honest status

- Endpoint `https://api.x.ai/v1`, default model `grok-4.7`: **not yet called live**. Both paths are tested against a mock server only (request shape, timeouts, malformed output rejected). Waiting on the owner's key and D-8; no `XAI_API_KEY` as of 2026-11-18.
- Rule-based reasoning is what the recorded demo uses unless a key is provided before submission.

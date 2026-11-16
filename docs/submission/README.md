# Submission drafts (UBC 2026 Hackathon)

Drafts for the Devpost write-up, one file per award. Source of bounty criteria: `docs/research/sponsors.md` section 6 (event page read 2026-11-14; the Devpost page itself was not readable, so criteria are as published on the event page only).

Every file states what is real, what is simulated, what is unverified and what is not built. Keep it that way when editing: judges should never mistake simulated funds or mock-tested integrations for live ones.

| File | Award | Status today |
|---|---|---|
| `main-track.md` | Agentic Payments & AI (D-17, owner to confirm) | Simulated lifecycle works end to end |
| `arc.md` | Best use of Arc | Contract tested locally, not deployed (blocked on D-3 custody + faucet) |
| `gemini.md` | Gemini Prediction Markets | Built on the public PM read API; live-probed once, tests use replayed fixtures |
| `quicknode.md` | QuickNode Streams | Webhook ingestion built and tested locally; no stream configured, nothing exposed, payload shape unverified |
| `grok.md` | Grok Bot | Wired, mock-tested only; not called against the live API (no key yet) |
| `elevenlabs.md` | ElevenLabs | Not built |

Before submitting: re-check each "Status" line against `TODO.md`, record the demo video, and confirm the deadline (D-18: Nov 20, 12 PM CST per the event page).

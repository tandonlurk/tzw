# Submission drafts (UBC 2026 Hackathon)

Drafts for the Devpost write-up, one file per award. Source of bounty criteria: `docs/research/sponsors.md` section 6 (event page read 2026-11-14; the Devpost page itself was not readable, so criteria are as published on the event page only).

Every file states what is real, what is simulated, what is unverified and what is not built. Keep it that way when editing: judges should never mistake simulated funds or mock-tested integrations for live ones.

| File | Award | Status today |
|---|---|---|
| `main-track.md` | Agentic Payments & AI (D-17, owner to confirm) | Simulated lifecycle, co-captain, collaborations, custom code, approach block and firms work end to end (simulated); three full challenges settled on Arc testnet (one on live Gemini PM data) |
| `arc.md` | Best use of Arc | Escrow, vault factory and vaults deployed and verified on Arc testnet; three live lifecycle runs through the server, drift 0 each time (tx links in the file); owner browser-wallet path not yet exercised with a real signature |
| `gemini.md` | Gemini Prediction Markets | Built on the public PM read API; one full challenge discovered and resolved on live Gemini PM data (2026-11-17/18). The PM API answered HTTP 500 from about 05:44 to at least 06:32 UTC on 2026-11-18, so the newer bid/ask-mid pricing is not verified live |
| `quicknode.md` | QuickNode Streams | Webhook ingestion built and tested locally; no stream configured (no QuickNode key), nothing exposed, payload shape unverified |
| `grok.md` | Grok Bot | Sparring infrastructure built and locally tested; live Grok Bot connection deferred by owner (2026-11-20). Describe as planned integration, not completed Bot use. Existing model-API reasoner remains separately mock-tested only |
| `elevenlabs.md` | ElevenLabs | Not built (no key) |

Last refreshed 2026-11-18 01:35 EST (overnight item 7) against `TODO.md`, `docs/demo.md` and `reports/handoffs/2026-11-18-*`.

Before submitting: re-check each "Status" line against `TODO.md`, record the demo video, and confirm the deadline (D-18: Nov 20, 12 PM CST per the event page).

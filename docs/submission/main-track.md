# Main track: Agentic Payments & AI (draft)

Track choice is D-17, awaiting owner confirmation. The event page describes this track as "AI agents with wallets, budgets, guardrails".

## Pitch

An economic network for user-owned AI agents. Each agent represents its owner's strategy and beliefs, finds disagreements with other agents on prediction-market questions, negotiates a measurable challenge, commits funds inside limits its owner set, and builds a persistent, auditable track record.

## What works today (simulated funds and Arc testnet USDC, labelled as such everywhere)

- Agents with versioned configuration, policy and a visual behavior graph; edits create a new version and never rewrite history.
- Opportunity discovery from Gemini exchange prediction-market contracts (live data by default; an offline fake catalog of 7 events / 14 contracts across crypto, commodities and energy, gold, a Fed decision and a sports final keeps the scripted demo deterministic and is labelled "fake market data" on screen).
- **Live on Arc testnet (2026-11-17/18):** the autonomous scheduler ran a full challenge on a live Gemini contract (`GEMI-BTC15M2611180000-UP`) with testnet USDC in two agent vaults: propose, counter, accept, owner approval, escrow create, both vault stakes, resolve onchain from Gemini's published outcome, ledger settle, vault release; ledger = onchain afterwards (drift 0). Two more settled testnet runs on 2026-11-18 (one on Gemini BTC spot after the PM API failed, one rehearsing the video script on the fake feed), both drift 0 with every escrow and vault-release tx recorded and shown on the challenge page with explorer links. Tx links: `arc.md`; commands: `docs/demo.md` "Testnet (Arc) demo" and "Testnet video script".
- Bounded negotiation (propose, counter, accept) with explicit terms: market, threshold, end time, stakes, resolution formula.
- One authorization gate (`core::authorize`) outside the model: per-interaction and aggregate limits, balance, allowed type, approval threshold. Model or rule output is only ever an `Intent`.
- Owner approvals for stakes above the automatic limit; double-entry ledger; settle, void and refund paths; realized P&L, win rate, dashboard charts.
- Collaborations: two agents pool contributions toward an objective and split the payout by an agreed ratio, with the same `authorize` checks and exact integer accounting (list, detail and propose views in the web app).
- Custom-code sandbox: a user's JavaScript `decide(ctx)` runs in QuickJS inside Wasmtime with CPU, time and memory limits and no file, network or environment access; its output is only ever a proposed intent (code editor with a dry-run test in the web app).
- Approach block and firms (simulated funds only): a stance wheel (competitive ↔ collaborative, cautious ↔ ambitious, firm role) and a market wheel (category, structure, keywords) gate what an agent discovers and accepts and size its stakes; starter templates carry one. A firm lead keeps 80% of a won challenge, member scouts and analysts share 20% by role plus a task fee, losses stay with the lead. One-call demo: `POST /api/dev/seed-firm-demo` (lead +3.80, scout +0.70, analyst +0.50). Awaiting the owner's review (D-64, D-65).
- Co-captain: rule-based suggestions (e.g. halve the max stake after three losses) that the owner accepts or dismisses; accepted changes go through the same validated, versioned edit.

## Honest limits

- Funds are simulated or Arc testnet USDC (no real value); nothing touches mainnet. On testnet the backend holds a resolver and an operator key (operator can only stake vault funds into the escrow within the owner's onchain caps); the live runs used a testnet-only "test owner" key in place of the owner's browser wallet, whose path is built but not yet exercised with a real signature.
- Resolution is backend-administered from market data. It is not trustless and the market API is not an oracle.
- Grok reasoning is wired but only mock-tested (no key).
- Firms and collaborations are simulated only; nothing about them is onchain.
- On 2026-11-18 Gemini's prediction-market API returned HTTP 500 for hours, so one testnet run fell back to BTC spot, where the `last` trade was 17 min stale and the challenge resolved as a tie on it (correct per the formula, owner to decide on a mid-price or staleness rule).
- Local-first: the API checks Host and Origin and requires a bearer token when one is configured or when bound beyond loopback; not deployed publicly.

## Demo

`docs/demo.md` (simulated run with expected numbers, and the Arc testnet run on live Gemini data) and `backend/README.md` "Demo script" / "Arc testnet".

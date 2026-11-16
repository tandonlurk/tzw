# Main track: Agentic Payments & AI (draft)

Track choice is D-17, awaiting owner confirmation. The event page describes this track as "AI agents with wallets, budgets, guardrails".

## Pitch

An economic network for user-owned AI agents. Each agent represents its owner's strategy and beliefs, finds disagreements with other agents on prediction-market questions, negotiates a measurable challenge, commits funds inside limits its owner set, and builds a persistent, auditable track record.

## What works today (simulated funds, labelled as such everywhere)

- Agents with versioned configuration, policy and a visual behavior graph; edits create a new version and never rewrite history.
- Opportunity discovery from Gemini exchange prediction-market contracts (fake market in the demo for determinism).
- Bounded negotiation (propose, counter, accept) with explicit terms: market, threshold, end time, stakes, resolution formula.
- One authorization gate (`core::authorize`) outside the model: per-interaction and aggregate limits, balance, allowed type, approval threshold. Model or rule output is only ever an `Intent`.
- Owner approvals for stakes above the automatic limit; double-entry ledger; settle, void and refund paths; realized P&L, win rate, dashboard charts.
- Co-captain: rule-based suggestions (e.g. halve the max stake after three losses) that the owner accepts or dismisses; accepted changes go through the same validated, versioned edit.

## Honest limits

- Funds are simulated. Nothing signs or sends a transaction yet: wallet custody (D-3) awaits owner approval.
- Resolution is backend-administered from market data. It is not trustless and the market API is not an oracle.
- Grok reasoning is wired but only mock-tested.
- No authentication beyond loopback + Host/Origin checks; not deployed publicly.

## Demo

`docs/demo.md` (simulated run with expected numbers) and `backend/README.md` "Demo script".

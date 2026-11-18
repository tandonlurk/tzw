# Animated explainers: storyboard (2026-11-18, overnight item 8)

Storyboard only, no animation code. For the owner to pick from after the UI redesign (TODO "animated
explainers" note, 2026-11-17). Most of the product happens in the background; the user mostly sees
performance summaries and a transaction log. These five explainers show the hidden steps.

Every frame below maps to a real field or event the app already has, so an explainer can be driven by
a real interaction instead of invented numbers (AGENTS.md 4.5: no fake data without a label).

## Rules for every explainer

- **Two modes.** *Replay* plays a real interaction from `GET /interactions/:id` (terms, negotiation,
  approvals, `escrow` txs, timeline) and `GET /activity?interaction_id=`. *Illustrative* uses the demo
  numbers below and shows a permanent "Example" tag. Never mix the two in one animation.
- **Environment badge on screen the whole time**: `simulated` or `Arc testnet`. Simulated runs show no
  chain, no tx hashes, no explorer links; the money moves between ledger boxes instead.
- **Honesty captions are fixed text**, not optional: resolution is "backend-administered, not
  trustless"; Gemini prices are "market data, not an oracle"; firms are "simulated only".
- **Semantic colors** (AGENTS.md 4.4): amber = waiting on someone (approval, funding window), ice blue =
  neutral info / money in transit, mint = win / paid out, rose = loss. Take them from the theme tokens,
  not hard-coded.
- **Short.** 6–12 s each, skippable, replayable; respect `prefers-reduced-motion` (fall back to the
  same frames as a static step list).
- **Where they live**: one "How it works" entry per explainer, plus an inline "Show me" on the screen
  where the step happens (listed per scene).

Demo numbers used for *Illustrative* mode (all from `docs/demo.md`, verified runs):

| Scene | Numbers |
|---|---|
| Discovery / negotiation | fake BTC +3 %, contract "at or above 0.55"; bull offers 5.00, skeptic counters 2.50 per side, bull accepts |
| Approval | skeptic's 2.50 is above its automatic limit of 1.00 → owner approval |
| Escrow (Arc testnet) | 1.25 per side; escrow `0x4179…5698`; vaults bull `0x3b5b…57bE`, skeptic `0x8390…1d75` |
| Resolution | observed 0.70 vs 0.55 → outcome `a` (bull); bull +2.50 / skeptic −2.50 (simulated) or +1.25 / −1.25 (testnet) |
| Firm payout | lead wins +5.00; 80/20 → 1.00 pool split scout 0.60 / analyst 0.40 by weight, plus 2 × 0.10 task fees; lead net +3.80, scout 0.70, analyst 0.50 |

## 1. Discovery: "How did my agent find this?"

Goal: show that an agent watches markets through its own approach and limits, and that a proposal comes
from data plus rules, not from the model deciding to spend.

| # | Frame | Source |
|---|---|---|
| 1 | Agent card with its approach wheel (stance point: collaboration × ambition) and market filters as chips ("Crypto", "> 3 % move") | graph `approach.profile` node |
| 2 | A stream of market tiles scrolls past; tiles outside the filters fade out | `/markets/preview` (fake catalog or Gemini) |
| 3 | One tile stays: "BTC up +3 %, contract at 0.55" — priced at the bid/ask mid where available | discovery snapshot in the proposal's activity row |
| 4 | The agent's logic blocks light up in order: market move → evaluate → conviction ≥ threshold → propose | agent graph (builder) |
| 5 | A proposal card appears with explicit terms: proposition, side, stake, end time | interaction `terms` |
| Caption | "Market data is used to find the idea. Limits are checked separately (next)." | — |

Show me: Activity row `proposal_created`, agent detail.

## 2. Negotiation: "Why 2.50 and not 5.00?"

Goal: two independent agents bargain within their own limits until terms are explicit.

| # | Frame | Source |
|---|---|---|
| 1 | Split screen: Bull (left) and Skeptic (right), each with its own wallet and "max per challenge" gauge | agent config / policy |
| 2 | Bull's offer slides across: 5.00 on "at or above 0.55" | negotiation round 1 |
| 3 | Skeptic's gauge shows 5.00 is past its comfort; a counter slides back: 2.50 per side | round 2 (counter) |
| 4 | Bull accepts; the two cards merge into one term sheet with a "rounds 2 of 3" pip | round 3 / `accepted` |
| 5 | Term sheet stamps: proposition, resolution source, end time, stake per side, formula | interaction `terms` |
| Caption | "Each agent stays inside its owner's limits. The model can suggest a stake; it cannot authorize one." | — |

Show me: interaction detail → Negotiation.

## 3. Approval and escrow lock: "Where is my money right now?"

The hardest one and the most important. Goal: make the four money states visible (available → reserved →
locked in escrow → released) and show who can move funds at each step.

| # | Frame | Source |
|---|---|---|
| 1 | Skeptic's 2.50 hits its 1.00 automatic limit: an amber gate closes, "Owner approval needed" | `approvals` row |
| 2 | Owner taps Approve; the gate opens. Simulated: both stakes move from *Available* to *Committed* boxes in the ledger, done | approval decision, ledger rows |
| 3 | Arc testnet: each agent's vault is a box with the owner's key icon and the backend's operator badge; caps shown on it | `/agents/:id/vault` |
| 4 | Escrow contract appears between them: "escrow created, funding open until …" | activity `escrow_created`, `escrow.create` tx |
| 5 | 1.25 leaves each vault into the escrow (two tx chips with explorer links); escrow shows "Active, 2.50 locked" | `stake_deposited` ×2, `stake_committed` ×2 |
| 6 | Callout on the escrow: "Can only ever pay one of these two parties. If the resolver disappears, anyone can refund both after the deadline + 1 hour." | `ChallengeEscrow` guarantees |
| 7 | Callout on each vault: "Owner can pause, withdraw, or revoke the operator at any time." | `AgentVault` owner functions |
| 8 | Reconcile strip: onchain 1.25 = ledger 1.25, drift 0 | vault view `drift` |
| Caption | "Locked funds sit in the escrow contract, not with the other agent and not with us." | — |

Show me: Approvals card, interaction detail → escrow tx card, agent vault panel.

## 4. Resolution: "Who decided I won?"

Goal: show the exact rule, the data point, and who applied it, without overstating trust.

| # | Frame | Source |
|---|---|---|
| 1 | Clock runs out on the term sheet's end time | `terms.end_ts` |
| 2 | The resolution source card: "Gemini prediction market, YES price at end" (or "Fake market (dev)", labelled) | `Resolved by` label |
| 3 | Formula fills in: observed 0.70 ≥ threshold 0.55 → side A (Bull) | interaction `resolution` |
| 4 | Backend resolver stamps the escrow "Resolved → Bull" with tx chip; caption "backend-administered, not trustless" | `escrow.resolve` tx |
| 5 | 2.50 flows from escrow into Bull's vault (mint); Skeptic's stake is shown leaving earlier, not a second deduction | `vault_released` rows |
| 6 | Both P&L tiles tick: Bull +1.25, Skeptic −1.25; portfolio realized 0 when one owner holds both sides | `/stats/portfolio?env=` |
| Caption | "One rule, one data point, written down before any money moved." | — |

Edge variants (small end cards, same layout): *Void* (resolver returns both stakes, ice blue),
*Timeout refund* (anyone refunds after deadline + grace, amber → ice blue), *Unfunded* (funding window
closes, reserved funds go back, no chain move).

Show me: interaction detail (settled), Dashboard P&L.

## 5. Firm payout: "Why did my scout earn 0.70?"

Goal: a lead agent profits most; members earn a share and fees without risking losses. Label the whole
explainer "simulated only" (D-65).

| # | Frame | Source |
|---|---|---|
| 1 | Firm card: lead in the center, scout and analyst orbiting with role rings; terms "80 / 20, task fee 0.10" | `/firms/:id` |
| 2 | Scout and analyst each drop a contribution token into the lead's proposal (weights 60 / 40); as each is used, a 0.10 task fee moves lead → member right away, before any outcome (if the lead's limits deny it, show "fee denied") | `/firms/:id/contributions`, `firm_task_fee` / `firm_fee_denied` |
| 3 | Lead's challenge plays out (reuse scenes 2–4 at 2× speed) and wins +5.00 | settled interaction |
| 4 | +5.00 splits into an 80 % bar (lead) and a 20 % pool (1.00) | firm payout rows |
| 5 | Pool pours into scout 0.60 and analyst 0.40 by weight (on top of the fees already paid) | ledger transfers |
| 6 | Totals: lead +3.80, scout 0.70, analyst 0.50 | agent balances |
| 7 | Loss variant end card: lead loses, members keep their task fees and pay nothing toward the loss | D-65, design doc "Task fee (guaranteed)" |
| Caption | "Members are paid from the lead's profit. They never cover its losses." | — |

Show me: firm detail page, agent detail ledger.

## Open questions for the owner

1. Replay mode needs a small frontend adapter that turns an `InteractionDetail` into frames; worth it, or
   illustrative-only for the hackathon video?
2. Tooling: the owner was going to look for Claude visual tools on GitHub. Any of CSS transitions, a
   Lottie file, or a short recorded video fits this storyboard; the frames don't assume one.
3. Scene 3 is long (8 frames). Split into "Approval" and "Escrow lock" if it runs past 12 s.
4. Firm split and fee reading (80/20, fee per used contribution) is still D-65 "awaiting review"; the
   numbers in scene 5 change if that reading changes.

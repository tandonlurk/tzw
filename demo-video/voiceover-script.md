# Voiceover script v2: theory, then live demo (2026-11-20)

A draft to edit. Two acts:

1. **Theory (about 1:35):** what TZW is and why, told over the minimal animations
   (`animations/renders/*-min.mp4`, no on-screen text).
2. **Live demo (about 1:15):** the real app, screen-recorded, following one challenge from start to
   finish, plus the features around it.

Then a 6 s close. Target 2:56, ceiling 3:00. Pace: about 2.5 words per second. Lines in [brackets]
are the first to cut. Lines marked ⚑ are honesty points the minimal animations no longer show; the
voice has to say them.

The animation pool stays as it is. Act 1 uses 01, 02, 03, 05, 07, 08 and 12. 06 and 10 are spare
**inserts** you can drop into Act 2 (noted below) or leave out.

---

## Act 1: Theory (animations)

### 01-cold-open-min · 4 s
> This is TZW. An economy for the agents you own.

### 02-problem-min · 21 s
*Two agents, two stakes, nothing holding them*
> AI agents can think and act on their own. But when two of them disagree and want to put money on
> it, who holds the money? Who decides who won?

*A fleet against one*
> Firms run whole fleets of agents. One person with one agent can't keep up.

*Agents wired only to the market*
> And your agent can trade with a market, but it can't deal with other agents.

### 03-agent-to-agent-min · 10 s
> TZW is where agents meet. Yours find disagreements with other people's agents, turn them into
> measurable challenges, and put money behind them, inside limits you set.

### 05-lifecycle-map-min · 8 s
> Every challenge takes the same six steps: discover, negotiate, check limits, lock, resolve,
> record.

### 07-negotiate-guard-min · 14 s
> The agents bargain on their own until the terms are exact. Then the stake goes through four
> checks. The AI can only suggest it. Our backend checks your limits, the agent's vault enforces
> them onchain, and anything above your limit waits for you.

### 08-escrow-min · 20 s
> Both stakes then go into an escrow contract on Arc that can only pay one of the two agents, or
> refund both. When time runs out, the result comes from a source named up front, here a Gemini
> prediction market, and the winner is paid.
>
> ⚑ Today our backend reads that result and settles it. It's administered, not trustless, and the
> app says so.

### 12-firms-min · 18 s
> And agents can team up. A lead agent opens a firm, and smaller agents join as scouts and analysts.
> They earn a fee for each contribution the lead uses, plus a share of the profit. If the lead
> loses, the loss is the lead's alone.

**Act 1 total ≈ 95 s.**

---

## Act 2: Live demo (screen recordings)

Bridge line (over the first frames of D1):
> Now the real thing. Everything you'll see is running: simulated funds, and real transactions on Arc
> testnet with no real value.

Record each clip as `screen-recordings/2N-…` (numbers 21–29 keep Act 2 after Act 1 when sorted).
Setup per clip: the session letter refers to "Screen-recording shot list" in `animation-plan.md`
(A = Arc testnet lifecycle, S = simulated with history, F = firm demo). Do the setup off camera.

| Clip | Dur | Session | On screen (what to click) | Voiceover |
|---|---|---|---|---|
| `21-overview` | 9 s | S | Overview: portfolio, funds by state, P&L by agent; then My Agents | "This is my account. Each agent has its own wallet, what it has at stake, and its record." |
| `22-agent-setup` | 10 s | S | Demo Bull → **Logic** tab: click the conviction block, change it, save. Then **Configuration**: max per challenge, approval threshold | "I build each agent's strategy from blocks, and I set hard limits it can't go past." |
| `23-multiply` | 7 s | S | Agent page → **Multiply**: 10 copies, some variance, budget → confirm; My Agents shows the family | "To keep up, I can multiply an agent, each copy a little different, from the same budget." [Optional insert: `10-scale-min`] |
| `24-negotiate` | 10 s | A | **Dev · Run tick** twice → Challenges → the new challenge → Negotiation (5.00… countered, accepted) | "On Arc testnet, my agent found a disagreement and the two agents negotiated the stake down." |
| `25-approve` | 6 s | A | Approvals → approve the skeptic's card | "That stake is above this agent's limit, so it waits for me. I approve." |
| `26-settled` | 10 s | A | After the end time: Run tick → open the settled challenge: observed vs threshold, outcome, the "backend-administered" note, ledger rows | "When it ends, the result is checked against the terms written at the start, and the winner is paid." |
| `27-proof` | 12 s | A | Challenge page: tx links + QuickNode **Stream** badges → click a tx → Arc explorer page → back to the agent's vault panel (drift 0) | "Every step is a public transaction you can open. QuickNode Streams watches the same contracts on its own and confirms each one, and the vault matches our ledger exactly." |
| `28-cocaptain` | 6 s | S | Overview "Needs your attention" → Co-captain → **Accept** | "A co-captain suggests changes. It can't move money; I decide." |
| `29-firms` | 6 s | F | Firms → Firm demo desk: members, roles, earned | "And here's a firm in action, on simulated funds." |

**Act 2 total ≈ 76 s** (plus the bridge line over 21).

Notes for recording:
- `27-proof`: the Stream badges only appear if the QuickNode stream was delivering during Session A
  (`GET /api/streams`, `docs/setup/quicknode-streams-filter.md`). If they're missing, drop that
  sentence and keep the explorer + vault part.
- `24`/`26` use the dev fake feed (the challenge title shows "[fake]"). Say "a controllable test
  feed" if it's readable on screen; the escrow and money are still real Arc testnet. The Gemini
  outcome was shown in Act 1 (08 replays a real Gemini-settled run).
- Don't name Grok as doing the reasoning unless a live-key run happens before recording.

---

## Close

### 14-close-min · 6 s
> Agents you own. Limits you set. Every stake on the record. This is TZW.

---

## Always say (at least once)

- Money is **Arc testnet USDC (no real value)** or **simulated**: bridge line, 29.
- Resolution is **backend-administered, not trustless**: 08.
- Don't say "audited", "guaranteed", or "100 % safe" (`animation-plan.md` section 6).

## If it runs long

Cut in this order: 23 (−7 s), the second sentence of 03 (−4 s), 05 (−8 s, the strip still appears
in 07 and 08), 28 (−6 s).

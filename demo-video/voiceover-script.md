# Voiceover script (example draft, 2026-11-20)

A starting draft to edit. It follows the scene list in `story-outline.md` and is written for the
**minimal** animations (`animations/` `-min` compositions), which show no titles or captions. That means
the voice has to say the things the screen no longer shows, including the honesty lines marked ⚑.

Pace: about 2.5 words per second, relaxed. Each block lists its scene time and a word budget. One file
per scene: `voiceover/NN-slug.wav` (see `README.md`). If a take runs long, cut the lines in [brackets]
first.

---

### 01-cold-open · 4 s · ~6 words
> This is TZW. An economy for the agents you own.

### 02-problem · 21 s · ~50 words
*Beat 1, two agents and two stakes with nothing around them (0–7 s)*
> AI agents can already think and act. But when two of them disagree and want to put money on it,
> who holds the money? Who decides who won?

*Beat 2, a fleet of agents against one (7–14 s)*
> Firms run whole fleets of agents, around the clock. One person with one agent can't keep up.

*Beat 3, agents wired only to the market (14–21 s)*
> And today, your agent can trade with a market. It can't deal with other agents.

### 03-agent-to-agent · 10 s · ~25 words
> TZW closes that gap. Your agents find disagreements with other people's agents, turn them into
> measurable challenges, and put money behind them, inside limits you set.

### 04-meet-tzw · screen recording · 10 s · ~25 words
> Every agent is yours: its own wallet, its own limits, its own track record. [You can see what each
> one has at stake and how it's doing.]

### 05-lifecycle-map · 8 s · ~18 words
> Every challenge goes through the same six steps: discover, negotiate, check limits, lock, resolve,
> record.

### 06-discover · 10 s · ~24 words
> It starts with data. Your agent watches Gemini's prediction markets through its own filters. When a
> move matches its rules, it proposes a challenge, with exact terms.
>
> ⚑ (if time) Gemini's prices are market data, not an oracle.

### 07-negotiate-guard · 14 s · ~34 words
> The other agent can push back. Here one offers five dollars, the other counters two-fifty, and they
> agree. Then four checks: the AI only suggests, our backend checks your limits, the vault enforces
> them onchain, and anything above your limit waits for you.

### 08-escrow · 20 s · ~48 words
> This is a real run on Arc testnet. Both stakes go into an escrow contract that can only pay one of
> the two agents, or refund both. When the clock ran out, Gemini settled the market "no", so the
> skeptic got paid. And our ledger matches the chain exactly.
>
> ⚑ Today our backend reads Gemini's result and settles it. That's administered, not trustless, and
> the app says so.

### 09-trust-live · screen recording + overlay · 22 s · ~52 words
> You can check all of it. The negotiation, the approval, the settled challenge. Every escrow step is
> a public transaction on Arc testnet, tied to the agent's vault. And QuickNode Streams watches the
> same contracts independently, so two separate views of the chain have to agree, step by step.

### 10-scale · 15 s · ~36 words
> To keep up, split one wallet into many agents, one strategy each, built from blocks. Or multiply an
> agent ten or a hundred times, each a little different. Same money, smaller stakes. And clones never
> bet against each other.

### 11-control · screen recording + overlay · 12 s · ~28 words
> You stay in charge. Change the strategy, change the limits. A co-captain watches your agents and
> suggests changes, but it can't move money. You decide.

### 12-firms · 18 s · ~42 words
> What's new: firms. A lead agent opens a firm, and smaller agents join as scouts and analysts. They
> earn a fee for each contribution the lead uses, plus a share of the profit. If the lead loses, the
> loss is the lead's alone.
>
> ⚑ Firms run on simulated funds today.

### 13-firms-live · screen recording · 6 s · ~12 words
> [Here's the firm demo running in the app, on simulated funds.]

### 14-close · 6 s · ~14 words
> Agents you own. Limits you set. Every stake on the record. This is TZW.

---

## Always say (at least once)

The minimal animations keep only a small "testnet" / "simulated" label in the corner, so the voice
must make the rest clear:

- Money on screen is **Arc testnet USDC (no real value)** or **simulated**. Covered in 08, 12, 13.
- Resolution is **backend-administered, not trustless**. Covered in 08.
- Gemini prices are **market data, not an oracle**. Covered in 06 if time allows; otherwise fold it
  into 08 ("Gemini settled the market…" already names the source).
- Don't say "audited", "guaranteed", "100 % safe", or that Grok does the reasoning (no live key run
  yet). See `animation-plan.md` section 6 for the full list.

## Timing check

Word budget total: about 400 words over about 2:56. If you read more slowly, cut 13 first, then the
⚑ line in 06 (keep the one in 08), then the bracketed line in 04.

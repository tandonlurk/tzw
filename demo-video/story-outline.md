# Demo video: story outline (v2, 2026-11-19)

> **2026-11-20:** the video is now two acts: theory over the minimal animations, then a live demo
> of the app. The current running order and shot list are in `voiceover-script.md` (v2). The scene
> table below remains the animation pool.

Supersedes the run sheet in `animation-plan.md` section 2. The animation specs, shot list and honesty
flags in that file still apply; this file adds the story spine and the diagram scenes that explain
what TZW does in the background. Diagram sources: `docs/design/explainers-storyboard.md` (scenes 1–5).

## The story in one breath

Agents can already think and act, but they can't deal with each other: nobody holds the money, nobody
fairly decides who won, and only firms can afford fleets. TZW is the place where agents you own meet
other agents, turn a disagreement into a measurable challenge, negotiate the stake inside your limits,
lock it in escrow on Arc, settle it from a named source, and keep a public record. Then it scales:
split your wallet into strategies, multiply an agent, or form a **firm** where small agents work for a
lead and share its profit.

## The recurring device: the lifecycle map

One horizontal diagram, introduced once (scene 05) and then shrunk to a strip at the top of every
explainer, with the current station lit:

```
 Discover → Negotiate → Check limits → Lock → Resolve → Record
   (data)    (agents)    (outside AI)  (Arc)  (source)  (ledger + chain)
```

It tells the viewer where they are in the story and makes "lots of machinery in the background" feel
ordered instead of busy.

## Scenes (target 2:56, ceiling 3:00)

| # | Prefix | Kind | Dur | Story job | Diagram / b-roll |
|---|---|---|---|---|---|
| 01 | `01-cold-open` | anim | 4 s | Brand | Wordmark + "An economy for the agents you own." |
| 02 | `02-problem` | anim | 21 s | Why now: three gaps | Trust (unheld stake, "Is it fair?") · speed gap (firm fleet vs one person) · isolation (agents only wired to "Market") |
| 03 | `03-agent-to-agent` | anim | 10 s | The idea | Market shrinks to a data chip; agent ↔ agent lines with challenge pills |
| 04 | `04-meet-tzw` | screen + overlay | 10 s | It's real, it's yours | Overview → My Agents; lower-thirds |
| 05 | `05-lifecycle-map` | **diagram** | 8 s | How a challenge works, at a glance | The six-station map draws left to right; each station gets a one-word icon; collapses to the top strip |
| 06 | `06-discover` | **diagram** | 10 s | Where ideas come from | Market tiles stream past (Gemini prediction markets); the agent's filters drop most; logic blocks light in order (move → conviction ≥ threshold → propose); a proposal card with explicit terms |
| 07 | `07-negotiate-guard` | **diagram** | 14 s | Agents bargain; the AI can't spend | Offers slide across (5.00 → 2.50, "round 2 of 3"); then the guard stack: **model suggests** → **backend checks limits** (per-challenge, exposure, balance) → **vault caps onchain** → **owner approval** above 1.00. Caption: "The model can suggest a stake. It cannot authorize one." |
| 08 | `08-escrow` | anim (replay) | 20 s | Where the money is | Live Gemini run on Arc testnet: vaults, term sheet, approval gate, escrow lock, Gemini outcome, payout, drift 0 (spec: `animation-plan.md` 05-escrow) |
| 09 | `09-trust-live` | screen + overlay | 22 s | Proof | Challenge → Approvals → settled → Activity → Arc explorer → vault. Overlay adds the **two-witness** diagram: our receipts and QuickNode Streams both see the same escrow events → "confirmed" (D-74) |
| 10 | `10-scale` | **diagram** | 15 s | Keep up with firms | Wallet splits into strategy agents built from blocks; one agent multiplies 1× → 10× → 100× with variance; a **family** ring shows siblings never challenge each other (D-73). Trade-off: same money, smaller stakes |
| 11 | `11-control` | screen + overlay | 12 s | You stay in charge | Logic canvas save, limits, co-captain suggestion accepted |
| 12 | `12-firms` | **diagram** | 18 s | What's new: firms | Firm forms (lead + scout + analyst with roles) → contributions flow in, task fees flow out → lead wins +5.00 → 80/20 split → totals → loss variant: members keep fees, lead takes the loss |
| 13 | `13-firms-live` | screen | 6 s | It runs | Firms page → Firm demo desk |
| 14 | `14-close` | anim | 6 s | Promise | Wordmark, "Agents you own. Limits you set. Every stake on the record.", funds footer |

Animations (built in `animations/`): 01, 02, 03, 05, 06, 07, 08, 10, 12, 14 = 126 s.
Overlays (need the recordings first): 04, 09, 11. Screen only: 13.

Cut order if long: 13 (−6 s), 05 folds into 06 (−4 s), 02 trims each beat to 6 s (−3 s).

## Honesty rules for every diagram

- `Example` tag on illustrative scenes (05, 06, 07, 10); `Replay · <run>` on replays (08, 12). Never both.
- Env badge always on: `Arc testnet · no real value` or `Simulated funds`.
- Fixed captions where relevant: "Resolution is backend-administered, not trustless." /
  "Gemini prices are market data, not an oracle." / "Firms run on simulated funds."
- Multiplier shown only up to 100× (D-73: 1–100 per request, 200 per family).
- Grok is not named as doing the reasoning unless a live-key run happens before recording.

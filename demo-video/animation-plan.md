# Demo video: animation plan

Plan only (2026-11-19). Nothing here is built yet. Story spine: the owner's handwritten script
(Problem 1–3, Solution 1–3, What's new: Firms). Visual source of truth: `web/app/globals.css`,
`docs/design/ui.md` (D-72), `docs/design/explainers-storyboard.md` (scenes 3–5 are reused here).
Every number on screen comes from a verified run in `docs/demo.md`, `backend/README.md` or
`docs/submission/arc.md`, or carries an "Example" tag.

## 1. Stack

**Remotion (React + TypeScript), as its own package in `demo-video/animations/`, rendering 1920x1080
at 30 fps:** it imports `web/app/globals.css` directly, so the OKLCH tokens, Inter / JetBrains Mono
and agent colours are the app's own; timing is frame-exact and scriptable against the voiceover;
it renders full frames to H.264 MP4 and overlays to ProRes 4444 with alpha; and the same CLI can
batch-render every segment for the assembly step. (GSAP in a browser needs screen capture to get
video out and is not frame-exact; Manim is Python and cannot reuse the CSS tokens or the app's look.)
Nothing is added to `web/`. Licence check: Remotion is free for individuals and companies of up to
3 people; confirm the team size (open question 6).

Frame rate: 30 fps everywhere. Screen recordings at 60 fps are fine; the assembly conforms to 30.

## 2. Run sheet (2:45 total, ceiling 3:00)

Order change vs the script: **Solution 3 (agents can only talk to the market) moves up to open the
solution**, as the direct answer to Problem 3, so the viewer knows what TZW is before the trust and
scale details. To keep the script order instead, renumber `03-agent-to-agent` to sit after
`08-control`.

| # | File prefix | Type | Dur | In → out | Voiceover (summary) | On screen |
|---|---|---|---|---|---|---|
| 01 | `01-cold-open` | animation | 0:05 | 0:00–0:05 | none, or "This is TZW." | `TZW` wordmark with the yellow tick on charcoal; subline "An economy for the agents you own." |
| 02 | `02-problem` | animation | 0:27 | 0:05–0:32 | Problem 1–3: agents can't deal with each other with trust ("is it fair?"); firms run fleets of agents, individuals can't keep up; there is no public place for personal agents to meet other agents | Three 9 s beats on one stage: two agents and an unheld stake; firm fleet vs one person; agents wired only to a "Market" box |
| 03 | `03-agent-to-agent` | animation | 0:10 | 0:32–0:42 | Solution 3: TZW closes that gap; your agents find disagreements with other agents, negotiate terms and put money behind them, inside limits you set | Continues 02's ring: market shrinks to a data chip, agent-to-agent lines draw with challenge pills |
| 04 | `04-meet-tzw` | screen recording + overlay | 0:12 | 0:42–0:54 | Each agent is yours: its own wallet, limits and track record | Overview, then My Agents cards. Overlay: two lower-thirds |
| 05 | `05-escrow` | animation | 0:22 | 0:54–1:16 | Solution 1a–b: once agents agree, both stakes go into an escrow contract that can only pay one of them or refund both; the source is fixed up front; owners approve anything above the limit | Replay of the live Gemini run on Arc testnet: vaults, term sheet, approval gate, escrow lock, Gemini outcome, payout, drift 0 |
| 06 | `06-trust-live` | screen recording + overlay | 0:30 | 1:16–1:46 | Solution 1c–d: this is real; every escrow step is a public Arc testnet transaction tied to the agent's vault, reconciled with our ledger | Challenges → Approvals → settled challenge → Activity → Arc explorer → vault panel. Overlay: lower-thirds + tx "receipt" chips stacking onto the vault |
| 07 | `07-split-and-multiply` | animation | 0:15 | 1:46–2:01 | Solution 2a: split one wallet into many agents, one strategy each, built from blocks; multiply one agent ×N with variations; the trade-off: same money, smaller stakes | Wallet splits into template agents; block glyphs snap together; one agent fans out 1× → 10× → 100× (→ 1000× only if supported) |
| 08 | `08-control` | screen recording + overlay | 0:15 | 2:01–2:16 | Solution 2b: you set the logic and limits; the co-captain shows up on your Overview and suggests changes; it can't move money, you decide | Logic canvas edit + save; Configuration limits; Overview "Needs your attention" → Co-captain accept |
| 09 | `09-firms` | animation | 0:15 | 2:16–2:31 | What's new: Firms. A lead agent opens a firm; smaller agents join as scouts and analysts, earn a fee and a share of profit; losses stay with the lead | Firm demo replay: fees, contributions, win, 80/20 split, totals |
| 10 | `10-firms-live` | screen recording | 0:08 | 2:31–2:39 | (VO from 09 runs on, or one line: "running today on simulated funds") | Firms page → "Firm demo desk (simulated)" detail: members, roles, earned |
| 11 | `11-close` | animation | 0:06 | 2:39–2:45 | One-line close | Wordmark, three-line promise, "Simulated and Arc testnet funds only. Nothing here is real money." |

Totals: animations 100 s (01, 02, 03, 05, 07, 09, 11), screen recordings 65 s (04, 06, 08, 10).
Four animations carry the explanation (02, 05, 07, 09); 01, 03 and 11 are short and share their
components. If the voiceover runs long, cut in this order: 10 (−8 s), 01 folds into 02 (−5 s),
06 loses its vault clip (−6 s).

## 3. Shared visual vocabulary (every animation)

- **Canvas:** `--bg` charcoal; cards on `--surface` / `--surface-2` with `--line` hairline borders and
  `--r-outer` 12 px radius, like the app's cards. Inter for text (tabular numbers), JetBrains Mono for
  addresses, tickers, amounts in chips, and the wordmark.
- **Colour semantics (D-72):** `--blue` = interactive, selected, money in transit; `--yellow` = money at
  stake, pending, needs approval; `--mint` = gain / paid out; `--rose` = loss only (never for "error"
  or "blocked", use `--text-3`). Agent avatars: monogram in a circle in `--agent-N`, the same hue
  rule as `lib/agentColors.ts`.
- **Motion:** the app's eases (`--ease-out`, `--ease-spring` for snaps and stamps); 8–12 frame moves;
  nothing loops for decoration. One idea per beat, with a held frame (≥ 1 s) before each cut so the
  voiceover can land.
- **Env badge (always on, top right):** `Arc testnet · no real value` or `Simulated funds`, in the
  sidebar-footer style. Illustrative scenes also show an `Example` tag top left; replays show
  `Replay · <run, date>`. Never both.
- **Fixed honesty captions** (verbatim, small, bottom): "Resolution is backend-administered, not
  trustless." / "Gemini prices are market data, not an oracle." / "Firms run on simulated funds."
- **Logic-block glyphs** (for "legos"): the canvas shapes from `docs/design/ui.md`: trigger circle
  (blue), condition diamond (yellow), action hexagon (mint), approach parallelogram (steel).

Reusable components to build once: `AgentAvatar`, `MoneyChip`, `VaultBox`, `EscrowBox`, `TermSheet`,
`TxChip`, `EnvBadge`, `Caption`, `LowerThird`, `BlockGlyph`, `Wordmark`.

## 4. Animation specs

Times are seconds from the start of the clip (30 fps: s × 30 = frame).

### 01-cold-open (5 s, full frame)

| s | Beat |
|---|---|
| 0.0–0.6 | Fade up from black to `--bg` |
| 0.6–1.6 | `T`, `Z`, `W` set in mono at 0.22 em tracking, 60 ms apart; the 6 px yellow tick snaps in (`--ease-spring`) |
| 1.6–2.4 | Subline in Inter `--text-2`: "An economy for the agents you own." |
| 2.4–5.0 | Hold |

Uses: wordmark exactly as `app/shell.css` `.wordmark`. Needs: nothing. Full frame.

### 02-problem (27 s, full frame, three beats)

Beat 1, trust (0–9 s)

| s | Beat |
|---|---|
| 0–1.5 | Two agent avatars slide in left and right (`--agent-0`, `--agent-1`), each with a dim label "Owner A" / "Owner B" |
| 1.5–3.5 | A yellow `MoneyChip` leaves each and meets in the middle, floating with nothing around it |
| 3.5–6 | A dashed outline tries to form around the chips; two questions type in above it: "Who holds the money?" "Who decides who won?" |
| 6–9 | Chips drift apart; large caption "Is it fair?"; hold 1 s |

Beat 2, speed gap (9–18 s)

| s | Beat |
|---|---|
| 9–11 | Split frame. Left, label "Firms": a 6×4 grid of small agent dots. Right, label "You": one avatar |
| 11–15 | Left grid blinks in staggered fast pulses (`--blue`) and a tick strip at the bottom fills densely; right avatar pulses once every 2 s, its strip nearly empty |
| 15–18 | Caption: "Firms run fleets of agents. Individuals can't keep up." Hold. No statistics or numbers on screen (we have no sourced figure) |

Beat 3, isolated agents (18–27 s)

| s | Beat |
|---|---|
| 18–20 | A "Market" box (steel) in the centre; five personal agents settle in a ring, each wired to it with a solid line |
| 20–24 | Dashed lines try to run agent → agent; each fades out at mid-span with a small `×` in `--text-3` |
| 24–27 | Caption: "Agents can trade with the market. Not with each other." Hold the ring (03 starts from this exact frame) |

Uses: avatars, agent colours, tokens. Needs: VO for Problem 1–3, about 8 s each; beat boundaries
move to the VO. Full frame.

### 03-agent-to-agent (10 s, full frame, continues 02)

| s | Beat |
|---|---|
| 0–2 | Agent → market lines dim to 30 %; the Market box shrinks and floats to the top as a steel chip "Gemini prediction markets · data" |
| 2–6 | Blue lines draw between agent pairs (three pairs); on each, a challenge pill pops: "BTC contract ≥ 0.55 at end?" (with `Example` tag on the frame) |
| 6–8 | Small yellow chips flow from both ends of each line into a lock icon at the midpoint (preview of 05) |
| 8–10 | Title: "Agents you own, making measurable challenges with each other." Under it, `--text-3`: "inside limits you set" |

Uses: demo proposition from `docs/demo.md` ("at or above 0.55"). Needs: VO Solution 3. Full frame.

### 05-escrow (22 s, full frame): replay of the live Gemini run

Replays the 2026-11-17/18 Arc testnet run that resolved on Gemini's published outcome
(`docs/submission/arc.md`, `docs/submission/gemini.md`). Tag: `Replay · Arc testnet, 18 Nov 2026`.
Badge: `Arc testnet · no real value`. All values below are from that run.

| s | Beat |
|---|---|
| 0–2 | Two `VaultBox`es: "Demo Bull" vault `0xC287…8F11` and "Demo Skeptic" vault `0xD6C9…9Ee4`, each 2.50 USDC (yellow), a key icon "Owner's wallet" and a small badge "Operator: stake only" |
| 2–5 | `TermSheet` drops in between: `GEMI-BTC15M2611180000-UP` · "YES price at or above 0.14 at 00:01:54 UTC" · 1.25 per side · "Resolution: Gemini published outcome". A pip: "Negotiated 2.50 → 1.25" |
| 5–7 | A yellow gate closes on the skeptic's side: "Above its automatic limit (1.00): owner approval". A stamp: "Approved" and the gate opens |
| 7–11 | `EscrowBox` "ChallengeEscrow `0x4179…5698`" rises below; 1.25 leaves each vault (blue in transit, yellow once inside); a `TxChip` "stake ✓" pops per side; escrow reads "Locked 2.50"; vaults tick 2.50 → 1.25 |
| 11–14 | Two callouts on the escrow: "Pays one of these two, or refunds both." / "If no one resolves: anyone can refund after the deadline + 1 h." |
| 14–18 | The term sheet's clock runs out. A steel chip drops from the top: "Gemini: event settled NO". Formula row fills: "YES ≥ 0.14? → No → Skeptic". Stamp on the escrow: "Resolved by TZW backend"; fixed caption "Resolution is backend-administered, not trustless." |
| 18–21 | 2.50 flows escrow → skeptic vault (mint): skeptic 1.25 → 3.75, tag "+1.25"; bull stays 1.25, tag "−1.25" (rose). No second deduction |
| 21–22 | Strip under each vault: "onchain = ledger · drift 0" |

Needs: VO Solution 1a–b (about 20 s). Full frame. Option: if the owner records a fresh live-Gemini
run (shot list path B), swap in its values; never mix runs in one clip.

### 06-trust-live overlay (30 s, transparent, over the screen recording)

| Clip | Lower-third (left, 3 s in / hold / out) | Extra |
|---|---|---|
| a Challenges → negotiation | "Negotiated by the agents: 2.50 offered, 1.25 countered" | — |
| b Approvals | "Above the agent's limit: waits for you" | — |
| c Settled challenge | "Resolution is backend-administered, not trustless" (the page says it too) | Soft blue ring highlights the page's own note |
| d Activity → explorer | "Public transaction on Arc testnet" | `TxChip`s (escrow create, stake, stake, resolve, release) pop off the explorer frame one by one |
| e Vault panel | "Ledger = chain · drift 0" | The chips from d fly to the vault panel and stack as a receipt list: "proof, attached to the agent's vault" |

Env badge `Arc testnet · no real value` on top throughout. Needs: the recording first, plus in/out
times per clip (the agent can read them off the file). Transparent ProRes 4444.

### 07-split-and-multiply (15 s, full frame) — depends on the clone feature

`Example` tag; badge `Simulated funds`.

| s | Beat |
|---|---|
| 0–2.5 | One wallet card: "Wallet" with the app's "Funds by state" bar, "100.00 (simulated)" |
| 2.5–6 | The bar splits into three agent cards from the real starter templates: "Momentum bull", "Skeptic", "Cautious", 30 / 30 / 40 allocated; each gets its own "max per challenge" line |
| 6–8 | On each card, 3–4 `BlockGlyph`s snap together (circle → diamond → hexagon), like lego: the agent's logic |
| 8–12.5 | **Multiplier (needs the clone feature):** "Momentum bull" is selected; a counter steps 1× → 10× → 100×; clones fan out into a grid; each clone's stance dot (approach wheel) sits slightly off the original (parameter variance). Capital readout under the grid: "30.00 split across 100 clones = 0.30 each". 1000× only if the feature is merged and was run at that size |
| 12.5–15 | Two trade-off lines: "Same money, smaller stakes." / "Your agents can meet each other: one owner on both sides nets to 0." (verified in `docs/demo.md`: portfolio realized 0) |

If the clone feature is not merged by recording time: drop 8–12.5, stretch the split, and the VO
says nothing about multiplying. Needs: VO Solution 2a; the clone feature's real name, N limits and
what "variance" changes (which fields, how much). Full frame.

### 08-control overlay (15 s, transparent)

| Clip | Lower-third |
|---|---|
| a Logic canvas edit + save | "Your strategy, as blocks. Every save is a new version" |
| b Configuration limits | "Limits are checked outside the model" |
| c Overview → Co-captain accept | "The co-captain suggests. It can't move money. You decide" |

Needs: recording and clip times. Transparent ProRes 4444.

### 09-firms (15 s, full frame): replay of the firm demo

Tag `Replay · simulated firm demo`; badge `Simulated funds`; caption "Firms run on simulated funds."
Values from `backend/README.md` "Firm demo".

| s | Beat |
|---|---|
| 0–3 | "Firm Lead" (larger avatar) centre; firm card "Firm demo desk · lead keeps 80 % · members share 20 % · task fee 0.10" |
| 3–6 | "Firm Scout" and "Firm Analyst" orbit in with role rings; each drops a contribution token into the lead (scout: a market candidate, analyst: a signal); a 0.10 fee chip moves lead → each member right away (yellow) |
| 6–9 | The lead opens a challenge against "Firm Rival" (same line + lock as 03); it wins: "+5.00" (mint) |
| 9–12 | The +5.00 bar splits 80 / 20; the 1.00 pool pours to scout 0.60 and analyst 0.40 |
| 12–15 | Totals: lead +3.80, scout +0.70, analyst +0.50, rival −5.00 (rose). Caption: "Members are paid from the lead's profit. They never cover its losses." |

Needs: VO What's new (about 14 s). Full frame.

### 11-close (6 s, full frame)

| s | Beat |
|---|---|
| 0–1 | Wordmark settles in (reuse 01) |
| 1–4 | Three lines appear one by one: "Agents you own." "Limits you set." "Every stake on the record." |
| 4–6 | Small line: "Escrow on Arc testnet · market data from Gemini prediction markets". Footer, verbatim from the sidebar: "Simulated and Arc testnet funds only. Nothing here is real money." Repo URL if the owner wants it |

Needs: nothing (optional one VO line).

## 5. Screen-recording shot list

Record in Chrome at 1920x1080, app in **Basic** mode, zoom 100 %, no bookmarks bar, notifications
off. Keep the sidebar (with its funds footer) in frame. The dashed **Dev · Run tick** control is on
by default (Settings can hide it); keep it visible in Session A, since it is how ticks happen on
camera, and hide it for Sessions S and F. Three backend sessions, each on its own
**fresh database** (the numbers depend on it). Do all curl setup off camera.

### Session A: Arc testnet lifecycle (segments 06, and 04 if you prefer testnet agents)

Setup: `docs/demo.md` "Testnet video script" → "Pick the market path at T-15 min" and "Setup (off
camera, path A)" (db `aen_video`, `MARKET_DATA=fake`, Arc env vars, `seed-demo`,
`setup-demo-vaults` 2.50 each, bull `horizon_secs=60`, fake market +3 % / 0.55). Path B (live Gemini
data, ~15 min, outcome not controllable) only if the Gemini check returns 200 and you want Gemini
on screen; then 05 should replay that run instead.

| File | Steps (shot numbers from the "Testnet video script" shot list) | Must be visible |
|---|---|---|
| `06-trust-live-a` | Shots 4–5: Overview → **Dev · Run tick** twice; open Challenges → the challenge → Negotiation | "proposed 1, countered 1", then "accepted 1, approvals requested 1"; offers 2.50 → 1.25 |
| `06-trust-live-b` | Shot 6: Approvals → approve the skeptic's card | "The stake is above this agent's automatic limit of 1.00 USDC." → state funded |
| (off camera) | Shot 7 tick (escrow + stakes, `activated: 1`); shot 10: fake market to 0.70; wait for `end_ts` (~4 min) | — |
| `06-trust-live-c` | Shot 11: **Dev · Run tick** on Challenges after `end_ts`; open the settled challenge | state settled, observed 0.70 vs 0.55, the page's "backend-administered, not trustless" note, ledger rows with tx links |
| `06-trust-live-d` | Shots 8 and 12: Activity → expand `stake_deposited` / `challenge_settled`, open its explorer link | Arc explorer tx page (`explorer.testnet.arc.io`) for the escrow `0x4179…5698` |
| `06-trust-live-e` | Shot 14: My Agents → Demo Bull → vault panel | "Arc testnet vault": onchain 3.75 = ledger 3.75, exposure 0, drift 0 |

Cleanup: "Cleanup (off camera)" in the same section (withdraw both vaults).

Note: path A's market feed is the dev fake feed. The challenge title shows "[fake] …" and the
resolution label says so. Leave it visible; the VO says "a controllable test feed; the money and
escrow are real Arc testnet" (as `docs/demo.md` already instructs).

### Session S: simulated, rich history (segments 04 and 08)

Setup: `backend/README.md` "Demo script (offline, simulated funds)" then "Co-captain step" (three
rounds, bull +7.50 / skeptic −7.50, two pending suggestions), web app on port 3000 pointed at it.
Do not accept the suggestion off camera.

| File | Steps | Must be visible |
|---|---|---|
| `04-meet-tzw-a` | Overview `/`: slow scroll from portfolio value → Funds by state → P&L by agent | Agents' names with "(simulated)", sidebar footer |
| `04-meet-tzw-b` | My Agents `/agents` | Agent cards: available / committed / P&L, sparkline |
| `08-control-a` | My Agents → Demo Bull → **Logic** tab: click the conviction condition (diamond), change its threshold, ⌘S | "Save as version N" |
| `08-control-b` | Demo Skeptic → **Configuration**: scroll to Risk / Permissions | Max per challenge 10.00, approval threshold 1.00 (simulated) |
| `08-control-c` | Overview → "Needs your attention" shows the co-captain card → open **Co-captain** → Accept "lower max stake" | Suggestion text ("lost its last 3 challenges… Halving its maximum stake…"), status accepted |
| `07-…` (only if the clone feature ships) | My Agents → clone an agent N times | Whatever the feature shows; can replace or follow the 07 animation |

Optional, if time: Terminal `/advanced` (⌘\\) with the "Compare 2×2" preset, 3 s, as a cutaway in 04.

### Session F: firm demo (segment 10)

Setup: `backend/README.md` "Firm demo (offline, simulated funds)" on db `aen_firm_demo`, all steps
through the settle tick.

| File | Steps | Must be visible |
|---|---|---|
| `10-firms-live` | Firms `/firms` → "Firm demo desk (simulated)" | Lead / members / roles, split 80 / 20, task fee, earned: scout 0.70, analyst 0.50; lead realized 3.80, paid to members 1.20 |

## 6. Honesty flags (script vs what the project does)

| Script says | What is true | Say instead |
|---|---|---|
| "100% trust / 100% safety" (Solution 1a, Problem 1 answer) | The escrow can only pay one of the two parties or refund both, and anyone can refund after the deadline + 1 h. But resolution is **backend-administered** (D-6, D-19): our resolver key reads Gemini and can award any active challenge; no disputes (D-5). Only on Arc **testnet**; firms, collaborations and most demos are simulated | "Stakes are locked in an escrow contract that can only pay one of the two agents, or refund both. Today our backend reads the result from Gemini and settles it, and every challenge says so." |
| "Risk-backed guarantees" | No guarantee fund or insurance exists. What exists: hard limits (per-challenge, total exposure, approval threshold) in `authorize`, plus onchain caps in each vault; the firm **task fee** (D-65) is the only "guarantee", a fixed fee a lead pays members, which its own limits can deny | "Hard limits you set, checked outside the AI and again onchain in each agent's vault." Keep "guaranteed fee" for Firms only |
| "Floor ops hold" | Nothing in the project has this name. Closest: the approval queue (stakes above the limit wait for the owner) and the escrow hold | "Anything above your limit waits for your approval; agreed stakes are held in escrow until the end." (see open question 1) |
| "Judges (Gemini?)" | Gemini is the **exchange**: its prediction-market contract price or published outcome is the resolution source, read by our backend. A market data API is not an oracle; there are no judges | "Each challenge names its source and formula up front, from a Gemini prediction-market contract." |
| "Agents control 100% of the interaction… no foul play between users" | Agents do negotiate on their own within owner limits (3-offer cap, D-27), and agents never trade on Gemini. But owners approve above-limit stakes, owners can edit config, TZW resolves; and the app has **one demo user** (D-9): multi-owner play with auth is not built, so "between users" is not shown | "Agents negotiate with each other directly, inside their owners' limits. Owners set the rules; they don't haggle." Avoid "no foul play" |
| "Contracts are audited" | **Not externally audited.** Internal security review (`reports/security-m1.md`), threat model, 100 Foundry tests incl. fuzz, invariants and adversarial; source verified on Blockscout (verification ≠ audit) | "Open source, verified on Arc's explorer, and tested with 100 contract tests including fuzzing. Not yet externally audited." |
| "We mint transactions" | Nothing is minted (no tokens, NFTs or receipts as assets). On Arc testnet, escrow create / stake / resolve / release are ordinary transactions; everything is also in a double-entry ledger. Simulated runs have no transactions | "Every stake and payout on Arc testnet is an onchain transaction you can open in the explorer." |
| "Transactions are public to participating users and attached to your wallet as proof" | Arc testnet txs are public **to everyone**, not just participants. They are tied to the agent's vault, which the owner's wallet owns; the app links them (challenge page, Activity) and reconciles vault vs ledger (drift 0). Caveat: the live runs used a testnet "test owner" key; the MetaMask path is built but not yet run with a real signature. Negotiation itself is offchain (in our database) | "On Arc testnet each escrow step is a public transaction tied to your agent's vault, and the app checks it against its own ledger." |
| 1× / 100× / 1000× agent multiplier | **Being built on another branch** ("clone an agent N times with parameter variance"); not merged, no commits on any local branch as of this plan. 1000 agents is untested for tick time and UI | Show only what the merged feature does at a size it was actually run at. Trade-off line is true today: same capital split thinner; one owner on both sides nets to 0 |
| "Legos" | The Logic canvas: blocks connected into a versioned graph (D-72). Real | Use it: "build each agent's strategy from blocks" |
| "In-house helper is the default view; it can apply variance to agents" | The co-captain has its own page; Overview's "Needs your attention" shows its suggestions, so it does appear on the default view. It is **advisory**: rule-based (6 rules); Grok version mock-tested only. It can change only 6 limit fields (D-37) and only when the owner accepts. It does not clone or apply variance | "A co-captain watches your agents and suggests limit changes on your Overview. It can't move money; you accept or dismiss." |
| Problem 2 "firms have it, the public doesn't" | Opinion, fine as framing. No sourced figures in the repo | Keep it qualitative; no numbers on screen |
| Firms: "one large agent opens contracts for smaller ones" | A lead agent runs a firm; scout and analyst agents join, contribute candidates and signals; the lead proposes the challenge and pays a task fee per used contribution plus 20 % of profit; losses stay with the lead. **Simulated only**, D-65 awaiting review | "A lead agent opens a firm; smaller agents join as scouts and analysts, and earn a fee plus a share of the profit. Losses stay with the lead." |
| Sponsors in VO | Arc: live on testnet. Gemini: live data, one run resolved on it. Grok: wired, mock-tested only (no key). QuickNode: webhook built locally, no stream. ElevenLabs: not built | Name only Arc and Gemini as working; don't say Grok "reasons" unless a live key run happens before recording |

Always: say "testnet USDC (no real value)" or "simulated" whenever money is on screen.

## 7. Open questions

1. What did "floor ops hold" mean: the approval queue, the escrow hold, or something else?
2. OK to move Solution 3 ("only the market") up to open the solution (section 2)?
3. Lifecycle recording: path A (fake feed, controllable, ~4 min) or path B (live Gemini, ~15 min,
   outcome not controllable)? 05 replays the live Gemini run either way.
4. Clone feature: name, max N actually run, and which parameters vary? Decides how far 07 goes.
5. Music bed and voiceover: your own voice, and any music? (ElevenLabs TTS for the VO would not make
   voice central to the product.)
6. Team size for the Remotion licence (free up to 3 people), and show the repo URL in 11?

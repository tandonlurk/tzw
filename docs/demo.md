# Demo plan

Goal: one coherent lifecycle, about 4 minutes, on Arc testnet with real testnet USDC. Anything simulated is labelled on screen.

| # | Step | What the audience sees | Proves |
|---|---|---|---|
| 1 | Create agent "Bull" from a template | Config form: goals, risk, limits (max 5 USDC per challenge, approval above 3) | Config-based creation |
| 2 | Open the logic builder | Blocks: market move → evaluate → conviction ≥ 0.6 → propose challenge. Edit a threshold, validate, save | Visual builder, versioned graph |
| 3 | Show second agent "Skeptic" (pre-made, different owner settings) | Its own wallet address, balance, strategy | Persistent identity + wallet context |
| 4 | Trigger a tick | Both agents read the Gemini exchange BTC price and form opposite views | Independent discovery |
| 5 | Negotiation | Activity feed: proposal, counter-offer on stake/threshold, acceptance, explicit terms | Negotiated measurable challenge |
| 6 | Permission check | Bull's stake exceeds its auto limit → approval card; owner approves. Also show a denied over-limit attempt | Enforced limits outside the model |
| 7 | Funding | Two deposit transactions, explorer links, events arriving via QuickNode Streams | Onchain escrow, event monitoring |
| 8 | Resolution | Short horizon (about 2–3 min) elapses, backend reads price, calls resolve; labelled "backend-administered" | Monitoring + settlement |
| 9 | Results | Balances, realized P&L, win rate, history and dashboard chart update | Accounting |
| 10 | (If built) Custom-code agent | Paste a small strategy, test-run in sandbox, show it proposing and being limited by policy | Custom code cannot bypass permissions |
| 11 | (If built) Co-captain | Grok suggests a limit change; owner accepts | Advisory layer |

Fallbacks: a recorded run of steps 7–9 in case testnet or faucet is down; a `simulated` environment that runs the same lifecycle against the ledger only, clearly labelled.

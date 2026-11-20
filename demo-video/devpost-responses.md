# Devpost responses

Companion to `voiceover-script.md`. Submission copy, separate from the narration.

## Elevator pitch

TZW is an economy for the agents you own. Build agents around your strategies, set their spending limits, and let them discover opportunities, negotiate challenges, and commit testnet USDC through escrow on Arc. Every outcome builds a measurable track record—giving agents room to act while you stay in control.

## About the project

### Inspiration

AI agents can already research markets, write code, and execute tasks. We wanted to explore what happens when they can make commitments to each other.

An agent economy needs more than capable models. Agents need identities, budgets, enforceable permissions, and a way to settle agreements and measure results. We built TZW to bring those pieces together—and make them accessible to individuals who want agents representing their own strategies and beliefs.

### What it does

TZW is an economic network for user-owned agents. Owners configure agents through visual logic blocks or sandboxed JavaScript, choose their strategies, and set financial limits.

Agents discover opportunities using Gemini exchange prediction-market data, find disagreements, and negotiate measurable challenges. Before any commitment, the backend checks spending limits, available balances, and approval requirements. Approved challenges lock testnet USDC in escrow on Arc, settle against predefined criteria, and update each agent’s performance history.

Owners can follow negotiations, approve larger commitments, inspect transactions, and compare results through the dashboard. TZW also supports simulated collaborations and “firms,” where agents take roles such as scout, analyst, and lead and share proceeds under an explicit agreement.

### How we built it

We built the backend in Rust using Axum and PostgreSQL, with a double-entry ledger and an authorization engine that operates independently of agent reasoning. Configurations are versioned, so changing an agent’s strategy preserves the history of its previous decisions.

The frontend uses Next.js, React, and TypeScript, with a visual behavior editor, a code workspace, wallet views, and performance charts. Custom JavaScript runs in QuickJS inside Wasmtime, with resource limits and no access to files, networking, or secrets.

Our Solidity contracts—ChallengeEscrow, AgentVault, and VaultFactory—are deployed and verified on Arc testnet. Gemini supplies market information, while QuickNode Streams delivers onchain events that we cross-check against transaction receipts and display in the activity timeline.

### Challenges we ran into

The hardest part was keeping financial state consistent across agent decisions, database records, and blockchain transactions. Retries, delayed confirmations, and interrupted funding cannot cause duplicate payments or misleading balances. We used explicit lifecycle states, idempotent operations, verified receipts, and ledger reconciliation to handle those cases.

External data introduced another challenge. Gemini’s prediction-market API experienced outages during testing, and a spot-price fallback exposed stale trade data. We added a freshness guard so stale spot data causes settlement to wait and eventually refund rather than resolve using an outdated observation.

We also had to make a process that mostly happens in the background understandable. The interface connects proposals, counteroffers, approvals, escrow transactions, and outcomes into a readable history.

### Accomplishments that we’re proud of

We completed an autonomous challenge lifecycle on Arc testnet using live Gemini prediction-market data: discovery, negotiation, owner approval, escrow funding, resolution, payout, and reconciliation. Both agents’ final ledger balances matched their onchain vault balances.

We also verified real QuickNode Streams deliveries and connected them to visible confirmations in the product. Beyond the core lifecycle, we built visual agent programming, sandboxed custom code, versioned strategies, and simulated agent firms.

Most importantly, spending authority remains separate from agent reasoning. Every proposed action must pass independent permission checks, with additional limits enforced by the vault contracts.

### What we learned

Giving an agent a wallet is only one part of making it an economic participant. The surrounding system needs precise agreements, constrained authority, reliable accounting, and a record that an owner can understand.

We also learned how much the resolution rules matter. A market-data API provides observations, but deciding which observation counts, when it is valid, and what happens when it is unavailable requires explicit design.

Building the full lifecycle forced us to connect autonomy with accountability: what did the agent propose, what was it permitted to do, and what actually happened?

### What’s next for TZW

We want to support more independently owned agents, richer collaboration agreements, and user-operated runtimes. We also want stronger resolution mechanisms and dispute handling; the current prototype uses backend-administered resolution.

We built a strategy-sparring workflow for a future Grok Bot integration, where owners could challenge an agent’s thesis before activating it. Live Bot integration remains future work. Voice interaction through ElevenLabs is another planned extension.

The current project uses only simulated funds and Arc testnet assets. Supporting real assets would require further security review and validation of the custody and permission model.

## Project Description

TZW is an economy for the agents you own. It lets people build agents around their strategies, give them bounded financial permissions, and track how those strategies perform through measurable interactions.

Agents use Gemini exchange prediction-market data to discover opportunities and negotiate challenges with explicit stakes, deadlines, and resolution criteria. An independent authorization engine checks each commitment, requests owner approval when required, and coordinates testnet USDC escrow on Arc. Results become part of a persistent performance history, with QuickNode Streams providing visible cross-checks of onchain events.

TZW combines configurable agent behavior, enforceable budgets, agent-to-agent negotiation, and auditable settlement in one workflow. Owners can build strategies visually or through sandboxed JavaScript. Simulated firms extend the model to collaboration, letting scouts and analysts support a lead agent and share proceeds.

The project uses Rust, Axum, PostgreSQL, Next.js, React, TypeScript, Solidity, Foundry, Alloy, Wasmtime, and QuickJS. Its verified live sponsor integrations are Arc testnet, Gemini exchange prediction-market data, and QuickNode Streams. Next steps include broader multi-owner participation, stronger resolution mechanisms, and connecting the existing sparring prototype to Grok Bot.

## Technical Summary

TZW separates agent behavior, financial authorization, and transaction execution. A Rust runtime executes versioned behavior graphs and produces structured action intents. Each intent passes through an independent authorization engine that checks balances, per-interaction limits, aggregate exposure, allowed interaction types, and approval requirements.

PostgreSQL stores agent configurations, negotiations, approvals, lifecycle states, and a double-entry ledger using integer USDC units. Solidity vaults restrict the operator to staking into the designated escrow within owner-set caps. The backend verifies transaction receipts before recording financial effects and reconciles ledger balances against vault balances.

Gemini exchange APIs supply discovery and resolution data. Resolution is administered by the backend and is not trustless. QuickNode Streams provides authenticated event deliveries that are matched against recorded transactions; stream events do not independently authorize payments or modify the ledger.

The Next.js frontend exposes agent configuration, visual logic editing, sandboxed code testing, approvals, wallet balances, activity, and performance. Completed Arc testnet runs include a full challenge using live Gemini prediction-market data. Firms and collaborations currently use simulated funds; no mainnet assets are supported.

# AGENTS.md — Autonomous Agent Economic Network

> Permanent project context and operating instructions for all coding agents and subagents.
>
> This file defines what we are building, how the project should be organized, how agents should collaborate, and which decisions require human review. Read this before making changes.

---

# 1. Project Context

## 1.1 The problem

AI agents are increasingly capable of making decisions, using tools, and executing tasks independently. However, they generally operate in isolated environments.

There is not yet a sufficiently accessible, user-oriented economic environment in which independently owned agents can consistently:

- Represent their owners' strategies, beliefs, and objectives.
- Discover opportunities and disagreements with other agents.
- Negotiate measurable economic challenges.
- Commit and manage funds within predefined permissions.
- Collaborate with other agents and divide responsibilities or compensation.
- Build a persistent record of their economic decisions and performance.

The project aims to create infrastructure for this type of agent economy.

## 1.2 Product thesis

Build a decentralized economic network for autonomous, user-owned AI agents.

Each agent represents its owner's preferences, strategy, and financial boundaries. Agents can independently discover opportunities, interact with other agents, negotiate challenges, collaborate, and manage permitted economic activity.

The platform is not simply a leaderboard of AI models or a conventional agent marketplace. It is an environment where agents can become persistent economic participants.

## 1.3 Core product principles

1. **Agents represent owners.** Each agent has its own identity, configuration, wallet, permissions, and performance history.
2. **Autonomy is bounded.** Owners define what an agent can and cannot do.
3. **Agents interact economically.** Challenges and collaborations should result in meaningful commitments, payments, or measurable outcomes.
4. **Interactions are auditable.** Users can understand what happened, why an action was taken at a high level, and what funds were involved.
5. **Performance is measurable.** Track actual outcomes rather than relying on an LLM's self-reported confidence.
6. **The user remains in control.** Financial permissions, funding, and risk limits must be explicit.

## 1.4 Event constraints

This is a hackathon project built over a **one-week event**.

- **Submission deadline:** Thursday, November 19, 2026 (working target; exact time to be confirmed). Plan to have the demo flow working by end of day Wednesday, November 18.
- **Sponsor bounties targeted:** all of them — Arc, Grok / xAI, Gemini (the exchange), QuickNode Streams, and ElevenLabs.
- **ElevenLabs is the lowest priority.** Implement it last, after everything else works.
- **The demo must show one coherent lifecycle:** agents identify an opportunity, negotiate a challenge, commit testnet or simulated funds within their permissions, the challenge resolves, and the outcome is recorded and visible in the frontend.

Scale process to the time available. When the process rules in this file conflict with shipping a working, correct demo by the deadline, keep the security and financial-correctness rules (section 8) and lighten the paperwork.

---

# 2. Product Scope

## 2.1 Main interaction types

### A. Challenges

Two or more agents identify a disagreement or opportunity and formulate a measurable challenge.

Example:

- Agent A believes an asset will increase by more than 5% within a specified period.
- Agent B disagrees.
- They negotiate a common measurable proposition.
- They agree on the conditions, duration, and financial commitment.
- Funds are committed according to the applicable permissions.
- The challenge is monitored and resolved.
- The outcome and financial results are recorded.

Challenges must have clearly defined resolution criteria. Do not create ambiguous financial contracts.

### B. Collaborations

Agents pool capabilities or resources toward a shared objective.

Examples:

- Agents agree to divide research responsibilities.
- Agents combine information to evaluate an opportunity.
- Agents agree to compensation or a distribution of proceeds.

For the initial product, collaboration can be demonstrated with a simple agreement and simulated or testnet-backed financial flow. Avoid building an overly complex autonomous contracting system before the core challenge workflow works.

### C. Services

Agents may eventually hire other agents for specialized services.

This is a future extension, not a required initial feature.

## 2.2 Agent configuration

An agent is a persistent entity representing its owner's objectives and preferences.

Its configuration may include:

- Name and description.
- Owner-defined goals and beliefs.
- Strategy/personality instructions.
- Preferred opportunity types.
- Risk tolerance.
- Maximum commitment per interaction.
- Maximum aggregate exposure.
- Allowed transaction types.
- Approval requirements.
- Challenge horizon.
- Minimum conviction or confidence threshold.
- Stop conditions.
- Execution schedule.
- Model and tool configuration, where applicable.

Do not treat natural-language personality instructions as enforceable financial controls.

Financial limits must be validated independently of the language model.

For the MVP, agent personalization means configuration, instructions, and accumulated history. It does not require model fine-tuning.

## 2.3 Co-captain

Grok acts as the owner's **co-captain**: an advisory assistant that watches the owner's agents and offers suggestions.

Examples:

- Suggesting configuration or risk-limit changes based on an agent's history.
- Pointing out opportunities, pending approvals, or underperforming agents.
- Explaining what an agent did and why, in plain language.

The co-captain is advisory only. It cannot authorize transactions, change permissions, or move funds. The owner accepts or rejects each suggestion, and accepted changes go through the same validation as any other configuration edit.

Start with text suggestions. Voice interaction with the co-captain through ElevenLabs is a later addition (see 3.3 and section 10).

---

# 3. Initial Technical Direction

These are intended architectural directions, not assumptions that must be implemented without verification.

## 3.1 Blockchain

Preferred chain: **Arc**.

The research agent must verify:

- Current network availability and chain ID.
- RPC and explorer availability.
- USDC contract address and supported token behavior.
- Testnet faucet availability and limitations.
- Solidity tooling compatibility.
- Contract deployment and verification process.
- QuickNode Streams support for the selected network.

Do not invent contract addresses, chain IDs, RPC URLs, or faucet instructions.

If Arc presents a blocking issue, document the issue and recommend alternatives rather than silently switching chains.

## 3.2 Funds and wallets

The MVP uses a combination of:

- Testnet USDC for actual onchain demonstrations.
- Simulated balances and positions where real integrations are unavailable or unnecessary.

Every balance and transaction must identify its source and environment.

Never imply that simulated funds are real funds.

The wallet architecture must distinguish:

1. User-owned funds.
2. Funds assigned or made available to an agent.
3. Funds committed to active challenges.
4. Funds associated with collaborations.
5. Available and settled balances.

The initial wallet model should support hybrid permissions.

Examples:

- Low-risk actions may execute automatically within a strict budget.
- Higher-value transactions may require approval.
- Contract settlement must follow explicit, independently enforced conditions.

The exact permission model and wallet custody architecture require human review before real financial functionality is introduced.

## 3.3 Integrations to investigate

All integrations must be checked against official documentation and actual hackathon requirements.

### Grok / xAI

Role:
- Agent reasoning.
- Opportunity discovery.
- Negotiation.
- Challenge formulation.
- Collaboration planning.
- The owner-facing co-captain (section 2.3).

Verify API availability, authentication, model access, pricing, rate limits, and the actual sponsor bounty requirements.

### Gemini (the crypto exchange)

"Gemini" in this project always means the **Gemini exchange**, not Google's Gemini AI models. Do not use Google's Gemini API to satisfy this integration.

Potential role:
- Market and external information retrieval.
- Research and data gathering.
- Market-related opportunity discovery.

Verify the exact APIs, supported data, authentication requirements, and whether any relevant prediction-market capabilities are actually accessible.

Do not assume Gemini can create or resolve arbitrary financial markets.

### QuickNode Streams

Potential role:
- Onchain event monitoring.
- Wallet activity.
- Contract event ingestion.
- Transaction status updates.
- Updating the frontend's activity records.

Verify network support and the correct event subscription mechanism.

### ElevenLabs

Potential role:
- Voice interaction with the co-captain.
- Spoken agent updates.
- Optional voice-based onboarding or agent interaction.

Voice is secondary to the core economic functionality and is the last integration to implement.

Verify actual API availability and sponsor requirements before allocating significant implementation time.

## 3.4 General architecture

The lead agent should evaluate and propose an architecture with clear boundaries between:

- Frontend application.
- Backend API.
- Agent orchestration/runtime.
- External model and data integrations.
- Database and persistent history.
- Wallet and permission management.
- Smart contracts.
- Blockchain event monitoring.
- Testing and deployment.

Suggested starting stack:

- TypeScript.
- Next.js for the frontend.
- TypeScript backend/orchestration.
- Solidity for EVM contracts.
- Foundry or another verified EVM development toolchain.
- Supabase/PostgreSQL or another justified persistent data store.
- Viem or ethers for EVM interaction.

These are preferences, not mandatory choices. Inspect the repository and verify compatibility before selecting or installing dependencies.

Avoid unnecessary infrastructure and excessive abstractions.

---

# 4. Frontend Requirements

## 4.1 Current design philosophy

The frontend should be functional, clear, and attractive without becoming a large design project.

The visual design is intentionally provisional.

The owner will research visual references and refine the final interface later. Do not spend excessive time inventing a sophisticated design system, elaborate animations, or a complex network visualization.

Build reusable components and keep the UI separate from business logic so the visual design can be replaced without rewriting the underlying system.

## 4.2 Primary user requirements

The initial frontend must provide:

### A. My Agents

A clear list or menu of all agents belonging to the user.

Each agent should expose:

- Name and identity.
- Current status.
- Individual wallet balance.
- P&L.
- Relevant performance statistics.
- Active deals/challenges.
- Capital committed.
- Historical outcomes.
- A way to open the agent's detail page.
- A way to edit the agent's configuration.

### B. Agent details and editing

Each agent should have a straightforward detail view containing:

- Wallet balance and transaction history.
- Realized and unrealized P&L, where applicable.
- Active challenges and collaborations.
- Historical deals and outcomes.
- Performance statistics.
- Current configuration and permissions.

Users must be able to edit an agent's instructions, strategy, risk limits, and permissions.

Changing an agent's configuration must not rewrite its historical performance or transaction records.

### C. User wallet

A user-level wallet overview should display:

- Total available balance.
- Total capital allocated to agents.
- Total committed capital.
- Available versus committed funds.
- Relevant transaction history.

Clearly distinguish balances that are real testnet assets from simulated balances.

### D. Dashboard and graphs

Include useful, understandable visualizations such as:

- Aggregate portfolio P&L over time.
- Individual agent performance comparisons.
- Capital allocation across agents.
- Deal/challenge outcomes.
- Active versus settled interactions.

Charts must reflect actual available data. Do not invent performance metrics or present fabricated sample data as real activity.

### E. Basic activity

A simple activity view should let users see:

- Agent proposals.
- Negotiations.
- Accepted and rejected challenges.
- Collaborations.
- Transactions.
- Settlement events.

Use readable descriptions rather than exposing raw technical logs as the default interface.

Detailed technical traces may be available in expandable views.

### F. Co-captain suggestions

A simple place where the owner sees the co-captain's suggestions (section 2.3) and can accept or dismiss them.

This comes after the core views above work.

## 4.3 Navigation

Start with a simple navigation structure. The lead agent may refine it after inspecting the application.

Suggested structure:

- Dashboard.
- My Agents.
- Wallet.
- Activity.

Agent detail pages should be accessible from My Agents.

Avoid adding unnecessary navigation items just because the underlying architecture supports future functionality.

## 4.4 Provisional color palette

Use the following palette as a starting point:

| Purpose | Color | Hex |
|---|---|---|
| Main background | Midnight charcoal | #101318 |
| Secondary surface | Dark slate | #202630 |
| Main text | Soft light grey | #D5E0DC |
| Positive performance | Mint green | #A3E6C1 |
| Negative performance | Dusty rose | #D98B9C |
| Pending / attention | Warm amber | #E7B66A |
| Neutral information | Ice blue | #8CC9E8 |

Use color semantically and consistently.

Rose indicates losses, negative movement, and adverse outcomes. Mint indicates positive performance and successful outcomes. Amber indicates pending decisions or attention.

The final visual identity may change. Do not hardcode these colors throughout unrelated business logic.

## 4.5 Frontend engineering constraints

- Keep components modular.
- Separate presentation from data fetching and business logic.
- Use reusable chart components.
- Build around stable API/data contracts.
- Make loading, empty, error, and disconnected states clear.
- Make responsive layouts.
- Do not create fake interactivity.
- Do not display mock data without labeling it.
- Avoid complex visualizations until the core functionality is complete.

The frontend must communicate what the product does without requiring the user to understand blockchain infrastructure.

---

# 5. Agent System and Runtime

## 5.1 Agent lifecycle

The intended lifecycle is:

1. Owner creates an agent.
2. Owner configures goals, preferences, and permissions.
3. Agent receives a persistent identity and associated wallet context.
4. Agent observes available information and relevant events.
5. Agent identifies an opportunity, disagreement, or collaboration.
6. Agent proposes an interaction.
7. Agents negotiate and agree on terms.
8. Permissions and financial limits are checked.
9. The interaction is executed or committed.
10. The interaction is monitored.
11. It is resolved or settled.
12. Results update the agent's history and performance statistics.

The initial runtime should support event-driven execution and scheduled monitoring.

Avoid requiring continuously running autonomous processes if event-driven execution is sufficient.

## 5.2 Agent reasoning

Agents may use model APIs to reason about opportunities, formulate propositions, and negotiate.

However:

- Model-generated confidence is not objective probability.
- LLM output must never be treated as financial authorization.
- Structured outputs should be validated before execution.
- Every proposed interaction must have explicit terms.
- The runtime must preserve the inputs, decisions, and resulting actions needed for auditability.

Store concise decision summaries, not private internal chain-of-thought.

## 5.3 Interaction state

Use explicit, persistent interaction states.

Example:

- Proposed.
- Negotiating.
- Accepted.
- Awaiting approval.
- Funded.
- Active.
- Resolving.
- Settled.
- Cancelled.
- Disputed.
- Failed.

The backend should be the source of truth for interaction state.

Do not rely on frontend-only state to represent a financial commitment.

---

# 6. Specialist Agent Roles

The lead orchestrator is responsible for coordinating all work.

## 6.1 Lead / Orchestrator

Responsibilities:

- Understand the product and architecture.
- Inspect the repository.
- Maintain the implementation plan.
- Delegate scoped tasks.
- Coordinate dependencies.
- Resolve integration issues.
- Maintain the central project tracker.
- Review subagent deliverables.
- Merge specialist branches.
- Identify blockers.
- Prepare human-readable daily summaries.

The lead owns shared architecture and integration.

It must not blindly accept subagent work without checking its compatibility with the project.

## 6.2 Research / Documentation Agent

Responsibilities:

- Verify official documentation.
- Research Arc and EVM infrastructure.
- Verify sponsor integrations and bounty requirements.
- Investigate available SDKs, APIs, and authentication.
- Identify technical risks and constraints.
- Recommend implementation approaches.

Deliverables:

- Source-linked research notes.
- Verified API specifications.
- Integration feasibility reports.
- Explicit unresolved questions.

Research must distinguish verified facts from assumptions.

## 6.3 Smart Contracts Agent

Responsibilities:

- Design escrow and settlement logic.
- Implement wallet permission enforcement where appropriate.
- Implement relevant contract events.
- Write contract tests.
- Document deployment and verification procedures.

Boundaries:

- Never introduce unrestricted agent spending.
- Never expose private keys.
- Never treat an LLM response as transaction authorization.
- Never implement real-fund functionality without human review.
- Document contract risks and assumptions.

## 6.4 Backend / Agent Runtime Agent

Responsibilities:

- Agent identity and configuration.
- Persistent agent state.
- Wallet associations.
- Agent execution and orchestration.
- Challenge discovery and formulation.
- Negotiation workflows.
- Collaboration workflows.
- Co-captain suggestions.
- External API integrations.
- Database schemas and API endpoints.
- Performance and interaction history.

The backend is responsible for enforcing business rules and validating structured model output.

## 6.5 Frontend / UI Agent

Responsibilities:

- Dashboard.
- My Agents.
- Agent details.
- Agent configuration editing.
- User wallet overview.
- Performance graphs.
- Activity history.
- Co-captain suggestions view.
- Loading and error states.

Boundaries:

- Do not independently redefine the product architecture.
- Do not duplicate financial logic in the frontend.
- Do not invent financial values.
- Keep the initial UI simple and modular.
- Coordinate API requirements with the backend agent.

## 6.6 QA / Security Agent

Responsibilities:

- Unit and integration tests.
- Contract testing.
- API validation.
- Wallet and permission threat modeling.
- Financial workflow testing.
- Regression testing.
- Demo verification.
- Security and reliability reports.

The QA agent should test failure scenarios, not merely successful demonstrations.

---

# 7. Multi-Agent Coordination Rules

## 7.1 Parallel execution

Subagents may work in parallel on separate modules.

The lead agent owns shared architecture and cross-module integration.

Before parallel work begins:

1. Define module boundaries.
2. Define interfaces and expected deliverables.
3. Identify dependencies.
4. Assign file ownership.
5. Prevent simultaneous modification of shared files.

Do not let multiple agents independently rewrite the same architecture or central configuration.

## 7.2 Delegation

Subagents may recommend further delegation.

Creating additional subagents requires lead approval.

Use a limited delegation depth. Avoid uncontrolled recursive agent spawning.

The lead must account for the cost of coordination and review.

## 7.3 Task tracking

Maintain both:

- A central project tracker.
- Individual task files for each specialist.

Suggested structure:

    /AGENTS.md
    /docs/
        architecture.md
        decisions.md
        research/
        security/
    /tasks/
        project.md
        research.md
        contracts.md
        backend.md
        frontend.md
        qa-security.md
    /reports/
        daily/
        handoffs/

Adapt this structure to the existing repository rather than creating duplicate documentation systems.

## 7.4 Required task file contents

Each task file should include:

- Objective.
- Scope.
- Dependencies.
- Assigned agent.
- Status.
- Files changed.
- Work completed.
- Tests run and results.
- Decisions made.
- Assumptions.
- Blockers.
- Unresolved questions.
- Recommended next steps.

Update task files when meaningful progress occurs, not only at the end.

Keep entries short. A task file is a working log, not a report.

## 7.5 Handoff protocol

Every subagent handoff must be understandable without reading its entire conversation.

It must include:

1. What was accomplished.
2. What files were created or changed.
3. How to run or test the work.
4. Which assumptions were made.
5. What remains unfinished.
6. Any integration requirements.
7. Any decisions requiring human review.

The lead should consolidate these reports into the central tracker.

## 7.6 Human review

The owner expects agents to make meaningful progress autonomously during the day, with human review primarily occurring in the evening.

Agents should therefore:

- Avoid unnecessary interruptions.
- Document questions rather than repeatedly asking them.
- Make reversible, low-risk decisions independently.
- Escalate critical decisions in a consolidated report.
- Keep work in independently reviewable increments.

### Proceed with documented defaults (testnet and simulated funds)

On testnet and simulated funds, agents do **not** wait for approval on the following. Choose a reasonable default, implement it, record the decision and its rationale in `docs/decisions.md`, and flag it in the daily summary for evening review:

- Financial contract semantics.
- Spending permissions and limits.
- Settlement, timeout, refund, and dispute rules.
- Wallet and permission data models.

The owner may overturn any of these in review, so keep them easy to change: isolate the logic, avoid spreading the assumption across modules, and do not build further work that would be expensive to redo on top of an unreviewed default.

### Stop and wait for human review

The following require explicit owner approval before implementation or activation:

- Any functionality that touches real (mainnet) funds or real user assets.
- Wallet custody and key-management architecture: how keys are generated, stored, and used to sign.
- Deploying contracts to, or transacting on, any mainnet.
- Major architectural changes that are difficult to reverse.
- Switching away from Arc or dropping a sponsor integration.

## 7.7 Git workflow

- Each specialist works on its own branch, in its own worktree when running in parallel.
- Specialists commit their own work in small, reviewable increments using conventional commit messages.
- The lead merges specialist branches into `main` after reviewing integration compatibility.
- Never push to the remote, force-push, or rewrite shared history without the owner asking.
- Never commit secrets, `.env` files, dependencies, or build artifacts. Set up `.gitignore` before the first `git add`.
- Do not add AI co-author trailers or attribution to commits, PRs, or code comments.

---

# 8. Security and Financial Correctness

This section overrides convenience or implementation speed.

## 8.1 Secrets

- Never place private keys, API keys, or credentials in source code.
- Never expose secrets to the frontend.
- Never send private keys to an LLM.
- Use environment variables or an appropriate secret-management mechanism.
- Provide safe `.env.example` files with placeholders only.
- Never commit real `.env` files.

## 8.2 Transaction authorization

An agent's ability to reason does not imply permission to transact.

Enforce financial limits outside the model.

Validate:

- Maximum per-transaction commitment.
- Maximum aggregate exposure.
- Available balance.
- Allowed interaction type.
- Approval requirements.
- Valid recipient and contract.
- Current interaction state.

Where possible, enforce critical restrictions at the smart-contract or wallet-policy layer.

The co-captain is subject to the same rule: its suggestions are never authorization.

## 8.3 Escrow and settlement

- Define the conditions for locking funds.
- Define the conditions for releasing funds.
- Define refund behavior.
- Define timeout and cancellation behavior.
- Define resolution sources.
- Define dispute handling or explicitly exclude it from the MVP.

Never describe a backend-administered resolution as fully trustless.

## 8.4 Market and proposition resolution

Challenges must be objectively measurable.

For financial propositions, distinguish:

- Market data used to form a challenge.
- The authoritative source used to resolve it.
- The time window.
- The exact resolution formula.

Do not assume that a market data API is an oracle.

If a reliable resolution mechanism is unavailable, clearly scope the demo as simulated or manually adjudicated.

## 8.5 Performance metrics

Distinguish:

- Realized P&L.
- Unrealized P&L.
- Gross exposure.
- Available balance.
- Committed capital.
- Returns over a defined period.
- Win rate and number of resolved interactions.

Do not double-count committed capital or treat transfers as profits.

Every financial metric must have a defined formula and data source.

---

# 9. Development Standards

## 9.1 General principles

- Prefer simple, understandable implementations.
- Avoid unnecessary abstractions.
- Do not introduce dependencies without justification.
- Follow existing repository conventions.
- Keep interfaces between modules explicit.
- Validate all external inputs.
- Use typed schemas for agent outputs.
- Handle failures and timeouts.
- Log meaningful operational events.

## 9.2 Testing

Every feature should include appropriate tests.

Priority test areas:

1. Wallet permission enforcement.
2. Challenge lifecycle.
3. Escrow funding and settlement.
4. Balance accounting.
5. Agent configuration changes.
6. External API failure behavior.
7. Event ingestion and duplicate handling.
8. Frontend representation of backend states.

A successful frontend demonstration does not replace backend or contract tests.

With one week available, spend testing effort in the order above. Areas 1–4 are not optional; presentation-only code may ship with lighter coverage.

## 9.3 Documentation

Document:

- Setup instructions.
- Environment variables.
- Database configuration.
- Contract deployment.
- Test execution.
- API endpoints.
- Agent lifecycle.
- Demo instructions.
- Known limitations.

Do not leave critical setup steps only in agent conversation history.

## 9.4 Running the project

Prerequisites: Rust (`~/.cargo/bin`), Foundry (`~/.foundry/bin`), PostgreSQL 17 (Homebrew service, `/opt/homebrew/opt/postgresql@17/bin`), Node 22. If not on PATH: `export PATH="$HOME/.cargo/bin:$HOME/.foundry/bin:/opt/homebrew/opt/postgresql@17/bin:$PATH"`.

- **Repo location:** keep it OUTSIDE iCloud-synced folders (`~/dev/hackathongs/tzw2`). See `reports/handoffs/2026-11-15-recovery.md`.
- **Backend:** `cd backend && DATABASE_URL="postgres://$USER@localhost/aen_dev" cargo run -p aen-server` (port 8080, migrations run on startup). Tests: `cargo test --workspace` (databases `aen_test` and `aen_sec_test`, create both with `createdb`; set `TEST_DATABASE_URL` if needed). Regenerate schemas: `cargo run -p aen-core --example export_schemas`.
- **Frontend:** `cd web && npm ci && npm run dev` (port 3000, backend at `http://localhost:8080/api`). Tests: `npm test`, `npm run build`.
- **Contracts:** `git submodule update --init && cd contracts && forge test`. Not deployed anywhere yet.
- **End-to-end demo flow (simulated funds):** fresh db, run the backend with `MARKET_DATA=fake TICK_INTERVAL_SECS=0`, then follow "Demo script" in `backend/README.md` (`/api/dev/seed-demo`, `/api/dev/tick`, `/api/dev/fake-market`, approve the pending approval); watch it in the web app under Activity, Approvals and Dashboard. Expected numbers: `docs/demo.md`.

---

# 10. MVP Priorities

Prioritize the following sequence. Every sponsor integration is in scope; each is placed where the product first needs it rather than grouped at the end.

### Priority 0 — Research

- Verify Arc, Grok / xAI, the Gemini exchange, QuickNode Streams, and ElevenLabs against official documentation.
- Confirm each sponsor's bounty requirements.

Research runs in parallel with Priority 1 and must not block it.

### Priority 1 — Functional foundation

- Repository and environment setup.
- Database and core data models.
- Persistent agent identities.
- Agent configuration.
- Wallet association and permissions.
- Basic dashboard and agent list.

### Priority 2 — Economic interaction

- Grok-based agent reasoning.
- Gemini exchange market data for opportunity discovery.
- Challenge discovery and formulation.
- Negotiation.
- Interaction state management.
- Financial permission checks.
- Testnet escrow and settlement on Arc where feasible.

### Priority 3 — Observability and collaboration

- QuickNode Streams event monitoring.
- Activity history.
- Performance graphs.
- Collaboration proposals and agreement structure.
- Simulated or testnet financial flow for collaborations.

### Priority 4 — Co-captain and polish

- Grok co-captain text suggestions.
- Demo improvements.

### Priority 5 — Voice

- ElevenLabs voice interaction with the co-captain.
- Spoken agent updates.

The lead may adjust the order after research, but must explain major changes.

---

# 11. Definition of Done

A feature is not complete simply because code has been written.

A feature is complete when:

- Its intended behavior is implemented.
- Its interface with other modules is documented.
- Relevant tests pass.
- Error states are handled.
- Security implications are considered.
- The task file is updated.
- The lead has reviewed integration compatibility.
- Any limitations are documented.

The overall MVP should demonstrate a coherent lifecycle in which agents can identify an opportunity, interact with other agents, negotiate, execute permitted financial actions, and record outcomes.

---

# 12. Final Operating Principle

Build the economic infrastructure first and keep the interface replaceable.

Do not sacrifice financial correctness, auditability, or product coherence to add superficial features or sponsor integrations.

The goal is not to demonstrate a set of disconnected APIs. The goal is to demonstrate a functioning environment in which autonomous agents can participate in an economy under explicit human-defined constraints.

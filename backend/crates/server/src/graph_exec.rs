//! Walks a validated behavior graph for one trigger and returns the intents
//! it wants, plus a concise trace.
//!
//! This is the only place blocks and reasoner output become `Intent`s, and an
//! intent is all it can produce: it has no database, ledger or network
//! handle. Whether an intent may run is decided later by `core::authorize`.
//! Conditions here are owner preferences, never security.

use std::collections::BTreeMap;
use std::sync::Arc;

use aen_core::authorize::Intent;
use aen_core::challenge::{ChallengeTerms, Party};
use aen_core::graph::{self, CUSTOM_CODE, Edge, Graph, Node};
use aen_core::money::Money;
use aen_sandbox::Sandbox;
use serde::Serialize;
use serde_json::{Value, json};

use crate::custom_code;
use crate::market::MarketView;
use crate::models::{display_bps, display_usdc};
use crate::reasoner::{AgentCtx, ReasonRequest, Reasoner, ReasonerOutput, Task};

pub const TRIGGER_SCHEDULE: &str = "trigger.schedule";
pub const TRIGGER_MARKET_MOVE: &str = "trigger.market_move";
pub const TRIGGER_CHALLENGE_RECEIVED: &str = "trigger.challenge_received";

/// A run visits each node at most once; this bounds a pathological graph.
const MAX_STEPS: usize = 64;

/// An intent plus the data needed to apply it. `intent` is what
/// `authorize()` judges; `terms` only describes the challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedIntent {
    pub intent: Intent,
    /// New terms, for a proposal or a counter-offer.
    pub terms: Option<ChallengeTerms>,
    /// The graph ended in `action.request_approval`: the owner wants to be
    /// asked even if the policy would allow the stake automatically.
    pub ask_owner: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TraceStep {
    pub node_id: String,
    pub node_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub intents: Vec<PlannedIntent>,
    pub trace: Vec<TraceStep>,
    /// Set when the run stopped early (reasoner failure, rejected output,
    /// invalid graph). No intents are returned in that case.
    pub error: Option<String>,
    /// Set when a `custom.code` block ran (or could not).
    pub code: Option<CodeRun>,
}

impl RunResult {
    /// What produced the decision, for `agent_runs.reasoner`.
    pub fn engine(&self, reasoner: &'static str) -> &'static str {
        if self.code.is_some() {
            custom_code::ENGINE
        } else {
            reasoner
        }
    }
}

/// What a `custom.code` block did, for the audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeRun {
    pub node_id: String,
    pub fuel_used: Option<u64>,
    /// The strategy's `console.*` lines (capped by the sandbox).
    pub logs: Vec<String>,
    pub error: Option<String>,
    pub module_sha256: Option<String>,
}

pub struct RunInput<'a> {
    pub graph: &'a Graph,
    pub trigger: &'a str,
    pub agent: &'a AgentCtx,
    pub market: &'a MarketView,
    /// Unix seconds.
    pub now: i64,
    /// Terms on the table (for `trigger.challenge_received`).
    pub incoming: Option<&'a ChallengeTerms>,
    pub party: Party,
    pub funding_window_secs: i64,
    /// Runs `custom.code` blocks; `None` = unavailable on this server.
    pub sandbox: Option<&'a Arc<Sandbox>>,
}

/// What flows along an edge.
#[derive(Debug, Clone)]
enum Flow {
    Event,
    Assessment(ReasonerOutput),
    Proposal {
        terms: ChallengeTerms,
        summary: String,
    },
}

/// The flow used to answer a challenge when an agent's graph has no
/// `trigger.challenge_received`: accept at or above the policy's minimum
/// conviction, otherwise reject.
pub fn default_response_graph(min_conviction_bps: u32) -> Graph {
    let node = |id: &str, node_type: &str, params: Value| Node {
        id: id.into(),
        node_type: node_type.into(),
        params,
        ui: None,
    };
    let edge = |from: &str, port: &str, to: &str| Edge {
        from: from.into(),
        from_port: port.into(),
        to: to.into(),
        to_port: "in".into(),
    };
    Graph {
        schema_version: graph::SCHEMA_VERSION,
        nodes: vec![
            node("default_trigger", TRIGGER_CHALLENGE_RECEIVED, json!({})),
            node("default_evaluate", "reason.evaluate_proposal", json!({})),
            node(
                "default_conviction",
                "cond.conviction_gte",
                json!({ "min_bps": min_conviction_bps }),
            ),
            node("default_accept", "action.accept", json!({})),
            node(
                "default_reject",
                "action.reject",
                json!({ "reason": "Below my minimum conviction." }),
            ),
        ],
        edges: vec![
            edge("default_trigger", "out", "default_evaluate"),
            edge("default_evaluate", "out", "default_conviction"),
            edge("default_conviction", "true", "default_accept"),
            edge("default_conviction", "false", "default_reject"),
        ],
    }
}

/// The first node of `trigger` type, if the graph has that flow.
pub fn find_trigger<'a>(graph: &'a Graph, trigger: &str) -> Option<&'a Node> {
    graph.nodes.iter().find(|n| n.node_type == trigger)
}

fn money_param(node: &Node, name: &str) -> Result<Money, String> {
    node.params
        .get(name)
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("node {} has no valid `{name}`", node.id))
}

struct Walker<'a> {
    input: &'a RunInput<'a>,
    reasoner: &'a dyn Reasoner,
    nodes: BTreeMap<&'a str, &'a Node>,
    intents: Vec<PlannedIntent>,
    trace: Vec<TraceStep>,
    code: Option<CodeRun>,
}

impl<'a> Walker<'a> {
    fn note(&mut self, node: &Node, summary: impl Into<String>) {
        self.trace.push(TraceStep {
            node_id: node.id.clone(),
            node_type: node.node_type.clone(),
            summary: summary.into(),
        });
    }

    async fn ask(&self, task: Task, node: &Node) -> Result<ReasonerOutput, String> {
        let req = ReasonRequest {
            task,
            agent: self.input.agent,
            market: self.input.market,
            now: self.input.now,
            incoming: self.input.incoming,
            party: self.input.party,
            funding_window_secs: self.input.funding_window_secs,
            instructions: node.params.get("instructions").and_then(Value::as_str),
        };
        let out = self
            .reasoner
            .reason(&req)
            .await
            .map_err(|e| e.to_string())?;
        // Every output is validated, whichever reasoner produced it.
        out.validate(&req).map_err(|e| e.to_string())?;
        Ok(out)
    }

    fn incoming(&self, node: &Node) -> Result<&'a ChallengeTerms, String> {
        self.input
            .incoming
            .ok_or_else(|| format!("node {} needs a received challenge", node.id))
    }

    /// Runs one node. Returns the output port and value to continue with, or
    /// `None` when the node ends the path.
    async fn step(
        &mut self,
        node: &Node,
        flow: Flow,
    ) -> Result<Option<(&'static str, Flow)>, String> {
        let mismatch = || format!("node {} received the wrong kind of input", node.id);
        match node.node_type.as_str() {
            TRIGGER_SCHEDULE | TRIGGER_MARKET_MOVE => {
                self.note(node, format!("Looked at {}.", self.input.market.title));
                Ok(Some(("out", Flow::Event)))
            }
            TRIGGER_CHALLENGE_RECEIVED => {
                let terms = self.incoming(node)?.clone();
                self.note(node, "Received a challenge.");
                Ok(Some((
                    "out",
                    Flow::Proposal {
                        terms,
                        summary: String::new(),
                    },
                )))
            }
            "reason.evaluate_opportunity" => {
                let out = self.ask(Task::EvaluateOpportunity, node).await?;
                self.note(node, out.summary.clone());
                Ok(Some(("out", Flow::Assessment(out))))
            }
            "reason.evaluate_proposal" => {
                self.incoming(node)?;
                let out = self.ask(Task::EvaluateProposal, node).await?;
                self.note(node, out.summary.clone());
                Ok(Some(("out", Flow::Assessment(out))))
            }
            "reason.generate_proposal" => {
                let out = self.ask(Task::GenerateProposal, node).await?;
                let terms = out
                    .terms
                    .ok_or_else(|| "reasoner returned no terms".to_owned())?;
                self.note(node, out.summary.clone());
                Ok(Some((
                    "out",
                    Flow::Proposal {
                        terms,
                        summary: out.summary,
                    },
                )))
            }
            "cond.conviction_gte" => {
                let Flow::Assessment(a) = &flow else {
                    return Err(mismatch());
                };
                let min = node
                    .params
                    .get("min_bps")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let pass = u64::from(a.conviction_bps) >= min;
                self.note(
                    node,
                    format!(
                        "Conviction {} vs minimum {}: {pass}.",
                        display_bps(i64::from(a.conviction_bps)),
                        display_bps(i64::try_from(min).unwrap_or(i64::MAX))
                    ),
                );
                Ok(Some((if pass { "true" } else { "false" }, flow)))
            }
            "cond.exposure_within" => {
                let max = money_param(node, "max")?;
                let pass = self.input.agent.exposure <= max;
                self.note(
                    node,
                    format!(
                        "Exposure {} vs limit {}: {pass}.",
                        display_usdc(self.input.agent.exposure),
                        display_usdc(max)
                    ),
                );
                Ok(Some((if pass { "true" } else { "false" }, flow)))
            }
            "cond.matches_strategy" => {
                let Flow::Assessment(a) = &flow else {
                    return Err(mismatch());
                };
                let pass = a.matches_strategy;
                self.note(node, format!("Fits the strategy: {pass}."));
                Ok(Some((if pass { "true" } else { "false" }, flow)))
            }
            "cond.stake_lte" => {
                let Flow::Proposal { terms, .. } = &flow else {
                    return Err(mismatch());
                };
                let max = money_param(node, "max")?;
                let pass = terms.stake <= max;
                self.note(
                    node,
                    format!(
                        "Stake {} vs limit {}: {pass}.",
                        display_usdc(terms.stake),
                        display_usdc(max)
                    ),
                );
                Ok(Some((if pass { "true" } else { "false" }, flow)))
            }
            "action.propose_challenge" => {
                let Flow::Proposal { terms, summary } = flow else {
                    return Err(mismatch());
                };
                self.propose(node, terms, summary, false);
                Ok(None)
            }
            "action.accept" => {
                let stake = self.incoming(node)?.stake;
                let summary = match &flow {
                    Flow::Assessment(a) => a.summary.clone(),
                    _ => "Accepted the terms.".to_owned(),
                };
                self.accept(node, stake, summary, false);
                Ok(None)
            }
            "action.counter" => {
                self.incoming(node)?;
                let out = self.ask(Task::GenerateCounter, node).await?;
                let terms = out
                    .terms
                    .ok_or_else(|| "reasoner returned no counter terms".to_owned())?;
                self.note(node, format!("Counter-offer: {}", out.summary));
                self.intents.push(PlannedIntent {
                    intent: Intent::CounterOffer { stake: terms.stake },
                    terms: Some(terms),
                    ask_owner: false,
                    summary: out.summary,
                });
                Ok(None)
            }
            "action.reject" => {
                let reason = node
                    .params
                    .get("reason")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("Rejected.")
                    .to_owned();
                self.note(node, format!("Reject: {reason}"));
                self.intents.push(PlannedIntent {
                    intent: Intent::Reject,
                    terms: None,
                    ask_owner: false,
                    summary: reason,
                });
                Ok(None)
            }
            "action.request_approval" => {
                match flow {
                    // A new proposal the owner wants to see first.
                    Flow::Proposal { terms, summary } if self.input.incoming.is_none() => {
                        self.propose(node, terms, summary, true);
                    }
                    // Answering a received challenge: accept, pending the owner.
                    Flow::Assessment(a) if self.input.incoming.is_some() => {
                        let stake = self.incoming(node)?.stake;
                        self.accept(node, stake, a.summary, true);
                    }
                    Flow::Proposal { .. } if self.input.incoming.is_some() => {
                        let stake = self.incoming(node)?.stake;
                        self.accept(node, stake, "Accepted the terms.".to_owned(), true);
                    }
                    _ => self.note(node, "Nothing to ask the owner about yet."),
                }
                Ok(None)
            }
            CUSTOM_CODE => {
                self.run_code(node).await?;
                Ok(None)
            }
            other => Err(format!("unknown block type {other}")),
        }
    }

    /// Runs the node's JavaScript in the sandbox and plans its intents. Any
    /// sandbox error, or an intent this flow cannot use, ends the run with no
    /// intents (fail closed).
    async fn run_code(&mut self, node: &Node) -> Result<(), String> {
        let mut code = CodeRun {
            node_id: node.id.clone(),
            fuel_used: None,
            logs: Vec::new(),
            error: None,
            module_sha256: None,
        };
        let result = self.run_code_inner(node, &mut code).await;
        if let Err(e) = &result {
            code.error = Some(e.clone());
        }
        self.code = Some(code);
        result
    }

    async fn run_code_inner(&mut self, node: &Node, code: &mut CodeRun) -> Result<(), String> {
        let source = node
            .params
            .get("source")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("node {} has no source", node.id))?;
        let Some(sandbox) = self.input.sandbox else {
            return Err(format!(
                "custom code could not run: {}",
                custom_code::UNAVAILABLE
            ));
        };
        code.module_sha256 = Some(sandbox.module_sha256().to_owned());
        let ctx = custom_code::build_ctx(self.input);
        let now_ms = self.input.now.saturating_mul(1000);
        let seed = custom_code::seed(self.input.agent.id, now_ms, self.input.trigger);
        let out = sandbox
            .run_blocking(source.to_owned(), ctx, now_ms, seed)
            .await
            .map_err(|e| format!("custom code failed: {e}"))?;
        code.fuel_used = Some(out.fuel_used);
        code.logs = out.logs.clone();
        let mut summary = format!(
            "Custom code returned {} intent(s) using {} fuel.",
            out.intents.len(),
            out.fuel_used
        );
        if !out.logs.is_empty() {
            let logs: String = out.logs.join(" | ").chars().take(300).collect();
            summary.push_str(&format!(" Logs: {logs}"));
        }
        self.note(node, summary);
        for intent in out.intents {
            let planned = self.plan_code_intent(intent)?;
            self.intents.push(planned);
        }
        Ok(())
    }

    /// Turns one intent from custom code into a planned intent with terms,
    /// using only what the flow allows (D-52):
    /// - looking for opportunities: `propose_challenge` (the claim is the
    ///   rule-based template shown as `ctx.proposal`, with the code's stake)
    ///   or `reject` (do nothing);
    /// - answering a challenge: `accept_challenge` at the stake on the table,
    ///   `counter_offer` with a different stake, or `reject`.
    fn plan_code_intent(&self, intent: Intent) -> Result<PlannedIntent, String> {
        let name = serde_json::to_value(intent)
            .ok()
            .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_default();
        let unusable = |why: &str| Err(format!("custom code returned {name}, {why}"));
        let check = |terms: &ChallengeTerms| {
            let errors = terms.validate();
            if errors.is_empty() {
                Ok(())
            } else {
                let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
                Err(format!(
                    "custom code {name}: invalid terms: {}",
                    list.join("; ")
                ))
            }
        };
        match (intent, self.input.incoming) {
            (Intent::Reject, _) => Ok(PlannedIntent {
                intent,
                terms: None,
                ask_owner: false,
                summary: "My custom code declined.".into(),
            }),
            (Intent::ProposeChallenge { stake }, None) => {
                let mut terms = custom_code::proposal_template(self.input).ok_or_else(|| {
                    "no challenge terms could be formed for this market".to_owned()
                })?;
                terms.stake = stake;
                check(&terms)?;
                Ok(PlannedIntent {
                    intent,
                    summary: format!(
                        "My custom code chose to stake {} on this claim.",
                        crate::lifecycle::stake_text(stake, self.input.agent.env)
                    ),
                    terms: Some(terms),
                    ask_owner: false,
                })
            }
            (Intent::AcceptChallenge { stake }, Some(incoming)) => {
                if stake != incoming.stake {
                    return unusable("but its stake must equal the stake on the table");
                }
                Ok(PlannedIntent {
                    intent,
                    terms: None,
                    ask_owner: false,
                    summary: "My custom code accepted the terms.".into(),
                })
            }
            (Intent::CounterOffer { stake }, Some(incoming)) => {
                if stake == incoming.stake {
                    return unusable("but a counter-offer must change the stake");
                }
                let terms = ChallengeTerms {
                    stake,
                    ..incoming.clone()
                };
                check(&terms)?;
                Ok(PlannedIntent {
                    intent,
                    summary: format!(
                        "My custom code offered {} per side instead.",
                        crate::lifecycle::stake_text(stake, self.input.agent.env)
                    ),
                    terms: Some(terms),
                    ask_owner: false,
                })
            }
            (Intent::ProposeChallenge { .. }, Some(_)) => {
                unusable("which cannot answer a received challenge")
            }
            (Intent::AcceptChallenge { .. } | Intent::CounterOffer { .. }, None) => {
                unusable("but there is no challenge to answer")
            }
            (Intent::CommitFunds { .. }, _) => {
                unusable("but stakes are committed by the runtime once both sides agree")
            }
            (Intent::ProposeCollaboration { .. }, _) => {
                unusable("which custom code cannot start yet")
            }
            (Intent::PayTaskFee { .. }, _) => {
                unusable("but firm task fees are paid by the runtime")
            }
        }
    }

    fn propose(&mut self, node: &Node, terms: ChallengeTerms, summary: String, ask_owner: bool) {
        let suffix = if ask_owner {
            " (owner approval requested)"
        } else {
            ""
        };
        self.note(node, format!("Propose a challenge{suffix}."));
        self.intents.push(PlannedIntent {
            intent: Intent::ProposeChallenge { stake: terms.stake },
            terms: Some(terms),
            ask_owner,
            summary,
        });
    }

    fn accept(&mut self, node: &Node, stake: Money, summary: String, ask_owner: bool) {
        let suffix = if ask_owner {
            " (owner approval requested)"
        } else {
            ""
        };
        self.note(node, format!("Accept the challenge{suffix}."));
        self.intents.push(PlannedIntent {
            intent: Intent::AcceptChallenge { stake },
            terms: None,
            ask_owner,
            summary,
        });
    }
}

/// Runs the flow that starts at `input.trigger`. Never panics on a bad graph:
/// problems end the run with `error` set and no intents.
pub async fn run(reasoner: &dyn Reasoner, input: &RunInput<'_>) -> RunResult {
    let mut walker = Walker {
        input,
        reasoner,
        nodes: input
            .graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n))
            .collect(),
        intents: Vec::new(),
        trace: Vec::new(),
        code: None,
    };
    let fail = |walker: Walker<'_>, error: String| RunResult {
        intents: Vec::new(),
        trace: walker.trace,
        error: Some(error),
        code: walker.code,
    };

    // Graphs are validated on save; validate again so a stored graph that no
    // longer matches the catalog cannot run.
    let errors = graph::validate(input.graph);
    if let Some(first) = errors.first() {
        return fail(walker, format!("graph is not valid: {}", first.message));
    }
    let Some(start) = find_trigger(input.graph, input.trigger) else {
        return fail(walker, format!("graph has no {} flow", input.trigger));
    };

    let mut stack: Vec<(&Node, Flow)> = vec![(start, Flow::Event)];
    let mut steps = 0;
    while let Some((node, flow)) = stack.pop() {
        steps += 1;
        if steps > MAX_STEPS {
            return fail(walker, "graph run exceeded the step limit".into());
        }
        match walker.step(node, flow).await {
            Err(error) => return fail(walker, error),
            Ok(None) => {}
            Ok(Some((port, out))) => {
                for edge in &input.graph.edges {
                    if edge.from == node.id && edge.from_port == port {
                        match walker.nodes.get(edge.to.as_str()) {
                            Some(next) => stack.push((next, out.clone())),
                            None => {
                                return fail(walker, format!("edge to unknown node {}", edge.to));
                            }
                        }
                    }
                }
            }
        }
    }
    RunResult {
        intents: walker.intents,
        trace: walker.trace,
        error: None,
        code: walker.code,
    }
}

#[cfg(test)]
mod tests {
    use aen_core::challenge::Direction;

    use super::*;
    use crate::market::BoxFut;
    use crate::reasoner::tests::{agent, m, market};
    use crate::reasoner::{ReasonerError, RuleBasedReasoner};

    fn graph(value: Value) -> Graph {
        serde_json::from_value(value).unwrap()
    }

    fn momentum_graph() -> Graph {
        graph(json!({
            "schema_version": 1,
            "nodes": [
                {"id": "trigger", "type": "trigger.market_move", "params": {"symbol": "BTCUSD", "threshold_bps": 200, "window_secs": 3600}},
                {"id": "evaluate", "type": "reason.evaluate_opportunity", "params": {}},
                {"id": "conviction", "type": "cond.conviction_gte", "params": {"min_bps": 6000}},
                {"id": "generate", "type": "reason.generate_proposal", "params": {}},
                {"id": "stake", "type": "cond.stake_lte", "params": {"max": "5000000"}},
                {"id": "propose", "type": "action.propose_challenge", "params": {}},
                {"id": "ask", "type": "action.request_approval", "params": {}}
            ],
            "edges": [
                {"from": "trigger", "from_port": "out", "to": "evaluate", "to_port": "in"},
                {"from": "evaluate", "from_port": "out", "to": "conviction", "to_port": "in"},
                {"from": "conviction", "from_port": "true", "to": "generate", "to_port": "in"},
                {"from": "generate", "from_port": "out", "to": "stake", "to_port": "in"},
                {"from": "stake", "from_port": "true", "to": "propose", "to_port": "in"},
                {"from": "stake", "from_port": "false", "to": "ask", "to_port": "in"}
            ]
        }))
    }

    fn skeptic_graph() -> Graph {
        graph(json!({
            "schema_version": 1,
            "nodes": [
                {"id": "trigger", "type": "trigger.challenge_received", "params": {}},
                {"id": "evaluate", "type": "reason.evaluate_proposal", "params": {}},
                {"id": "strategy", "type": "cond.matches_strategy", "params": {}},
                {"id": "conviction", "type": "cond.conviction_gte", "params": {"min_bps": 7000}},
                {"id": "accept", "type": "action.accept", "params": {}},
                {"id": "counter", "type": "action.counter", "params": {}},
                {"id": "reject", "type": "action.reject", "params": {"reason": "Does not fit my strategy."}}
            ],
            "edges": [
                {"from": "trigger", "from_port": "out", "to": "evaluate", "to_port": "in"},
                {"from": "evaluate", "from_port": "out", "to": "strategy", "to_port": "in"},
                {"from": "strategy", "from_port": "true", "to": "conviction", "to_port": "in"},
                {"from": "strategy", "from_port": "false", "to": "reject", "to_port": "in"},
                {"from": "conviction", "from_port": "true", "to": "accept", "to_port": "in"},
                {"from": "conviction", "from_port": "false", "to": "counter", "to_port": "in"}
            ]
        }))
    }

    async fn run_with(
        reasoner: &dyn Reasoner,
        graph: &Graph,
        trigger: &str,
        agent: &AgentCtx,
        market: &MarketView,
        incoming: Option<&ChallengeTerms>,
    ) -> RunResult {
        let input = RunInput {
            graph,
            trigger,
            agent,
            market,
            now: 1_000,
            incoming,
            party: if incoming.is_some() {
                Party::B
            } else {
                Party::A
            },
            funding_window_secs: 300,
            sandbox: None,
        };
        run(reasoner, &input).await
    }

    async fn bull_proposal(move_bps: i64) -> RunResult {
        run_with(
            &RuleBasedReasoner,
            &momentum_graph(),
            TRIGGER_MARKET_MOVE,
            &agent("Follow momentum."),
            &market(move_bps),
            None,
        )
        .await
    }

    #[tokio::test]
    async fn momentum_flow_emits_one_proposal_with_a_trace() {
        let result = bull_proposal(300).await;
        assert_eq!(result.error, None);
        assert_eq!(result.intents.len(), 1);
        let planned = &result.intents[0];
        assert_eq!(
            planned.intent,
            Intent::ProposeChallenge {
                stake: m(5_000_000)
            }
        );
        assert!(!planned.ask_owner);
        let terms = planned.terms.as_ref().unwrap();
        assert_eq!(terms.direction, Direction::Above);
        let visited: Vec<&str> = result.trace.iter().map(|s| s.node_id.as_str()).collect();
        assert_eq!(
            visited,
            [
                "trigger",
                "evaluate",
                "conviction",
                "generate",
                "stake",
                "propose"
            ]
        );
    }

    #[tokio::test]
    async fn low_conviction_stops_without_an_intent() {
        // +100 bps -> 5500 bps conviction, below the 6000 the graph wants.
        let result = bull_proposal(100).await;
        assert_eq!(result.error, None);
        assert!(result.intents.is_empty());
        assert_eq!(result.trace.last().unwrap().node_id, "conviction");
    }

    #[tokio::test]
    async fn large_stake_goes_to_the_owner() {
        // +700 bps -> 8500 conviction -> maximum stake, above the 5 USDC branch.
        let result = bull_proposal(700).await;
        let planned = &result.intents[0];
        assert_eq!(
            planned.intent,
            Intent::ProposeChallenge {
                stake: m(10_000_000)
            }
        );
        assert!(planned.ask_owner);
    }

    #[tokio::test]
    async fn skeptic_counters_accepts_or_rejects() {
        let terms = bull_proposal(300).await.intents[0].terms.clone().unwrap();
        let skeptic = agent("Most sharp moves revert.");
        let answer = |view: MarketView, who: AgentCtx| {
            let terms = terms.clone();
            async move {
                let g = skeptic_graph();
                let r = run_with(
                    &RuleBasedReasoner,
                    &g,
                    TRIGGER_CHALLENGE_RECEIVED,
                    &who,
                    &view,
                    Some(&terms),
                )
                .await;
                assert_eq!(r.error, None);
                r.intents
            }
        };

        // 6500 bps for its side: below 7000, so it counters with half.
        let intents = answer(market(300), skeptic.clone()).await;
        assert_eq!(intents.len(), 1);
        assert_eq!(
            intents[0].intent,
            Intent::CounterOffer {
                stake: m(2_500_000)
            }
        );
        assert_eq!(intents[0].terms.as_ref().unwrap().stake, m(2_500_000));

        // A sharper move convinces it.
        let intents = answer(market(400), skeptic.clone()).await;
        assert_eq!(
            intents[0].intent,
            Intent::AcceptChallenge {
                stake: m(5_000_000)
            }
        );

        // Off-strategy markets are rejected with the block's reason.
        let mut sports = skeptic;
        sports.config.categories = vec!["sports".into()];
        let intents = answer(market(300), sports).await;
        assert_eq!(intents[0].intent, Intent::Reject);
        assert_eq!(intents[0].summary, "Does not fit my strategy.");
    }

    #[tokio::test]
    async fn default_response_flow_accepts_or_rejects_on_policy_conviction() {
        let terms = bull_proposal(300).await.intents[0].terms.clone().unwrap();
        let skeptic = agent("Most sharp moves revert.");
        for (min, expected) in [
            (
                6500,
                Intent::AcceptChallenge {
                    stake: m(5_000_000),
                },
            ),
            (6501, Intent::Reject),
        ] {
            let g = default_response_graph(min);
            assert!(graph::validate(&g).is_empty());
            let r = run_with(
                &RuleBasedReasoner,
                &g,
                TRIGGER_CHALLENGE_RECEIVED,
                &skeptic,
                &market(300),
                Some(&terms),
            )
            .await;
            assert_eq!(r.intents[0].intent, expected);
        }
    }

    /// A reasoner that returns whatever it was built with.
    struct Scripted(Result<ReasonerOutput, ReasonerError>);

    impl Reasoner for Scripted {
        fn name(&self) -> &'static str {
            "scripted"
        }
        fn reason<'a>(
            &'a self,
            _req: &'a ReasonRequest<'a>,
        ) -> BoxFut<'a, Result<ReasonerOutput, ReasonerError>> {
            Box::pin(async move { self.0.clone() })
        }
    }

    #[tokio::test]
    async fn malformed_reasoner_output_produces_no_intent() {
        let bad = ReasonerOutput {
            conviction_bps: 99_999,
            stance: Direction::Above,
            matches_strategy: true,
            terms: None,
            summary: "all in".into(),
        };
        for scripted in [
            Scripted(Ok(bad)),
            Scripted(Err(ReasonerError::Unavailable("timeout".into()))),
        ] {
            let r = run_with(
                &scripted,
                &momentum_graph(),
                TRIGGER_MARKET_MOVE,
                &agent("Follow momentum."),
                &market(300),
                None,
            )
            .await;
            assert!(r.intents.is_empty());
            assert!(r.error.is_some());
        }
    }

    #[tokio::test]
    async fn missing_flow_and_invalid_graph_are_errors() {
        let a = agent("Follow momentum.");
        let view = market(300);
        let r = run_with(
            &RuleBasedReasoner,
            &momentum_graph(),
            TRIGGER_CHALLENGE_RECEIVED,
            &a,
            &view,
            None,
        )
        .await;
        assert!(
            r.error
                .unwrap()
                .contains("no trigger.challenge_received flow")
        );

        let mut broken = momentum_graph();
        broken.nodes[1].node_type = "reason.read_tea_leaves".into();
        let r = run_with(
            &RuleBasedReasoner,
            &broken,
            TRIGGER_MARKET_MOVE,
            &a,
            &view,
            None,
        )
        .await;
        assert!(r.intents.is_empty());
        assert!(r.error.unwrap().starts_with("graph is not valid"));
    }

    fn code_graph(trigger: &str, source: &str) -> Graph {
        let params = if trigger == TRIGGER_CHALLENGE_RECEIVED {
            json!({})
        } else {
            json!({"every_secs": 60})
        };
        graph(json!({
            "schema_version": 1,
            "nodes": [
                {"id": "t", "type": trigger, "params": params},
                {"id": "code", "type": "custom.code", "params": {"source": source}}
            ],
            "edges": [{"from": "t", "from_port": "out", "to": "code", "to_port": "in"}]
        }))
    }

    /// Runs `source` as the agent's whole flow; `None` if the sandbox guest
    /// is not built (the test is skipped).
    async fn run_code(
        source: &str,
        incoming: Option<&ChallengeTerms>,
        with_sandbox: bool,
    ) -> Option<RunResult> {
        let sandbox = crate::custom_code::shared_sandbox();
        if sandbox.is_none() && with_sandbox {
            eprintln!("SKIP: sandbox guest not built");
            return None;
        }
        let trigger = if incoming.is_some() {
            TRIGGER_CHALLENGE_RECEIVED
        } else {
            TRIGGER_SCHEDULE
        };
        let g = code_graph(trigger, source);
        assert!(graph::validate(&g).is_empty());
        let (a, view) = (agent("Follow momentum."), market(300));
        let input = RunInput {
            graph: &g,
            trigger,
            agent: &a,
            market: &view,
            now: 1_000,
            incoming,
            party: if incoming.is_some() {
                Party::B
            } else {
                Party::A
            },
            funding_window_secs: 300,
            sandbox: if with_sandbox { sandbox.as_ref() } else { None },
        };
        Some(run(&RuleBasedReasoner, &input).await)
    }

    #[tokio::test]
    async fn custom_code_proposes_with_the_template_claim_and_its_stake() {
        let src = r#"function decide(ctx) {
            console.log(ctx.proposal.direction, ctx.balances.available);
            return [{ type: "propose_challenge", stake: "2500000" }]; }"#;
        let Some(r) = run_code(src, None, true).await else {
            return;
        };
        assert_eq!(r.error, None);
        assert_eq!(r.engine("rule_based"), "custom_code");
        let planned = &r.intents[0];
        assert_eq!(
            planned.intent,
            Intent::ProposeChallenge {
                stake: m(2_500_000)
            }
        );
        let terms = planned.terms.as_ref().unwrap();
        assert_eq!(terms.stake, m(2_500_000));
        assert_eq!(terms.direction, Direction::Above);
        assert!(terms.start_ts > 1_000);
        let code = r.code.unwrap();
        assert_eq!(code.logs, vec!["above 100000000"]);
        assert!(code.fuel_used.unwrap() > 0);
        assert!(r.trace.last().unwrap().summary.contains("1 intent(s)"));
    }

    #[tokio::test]
    async fn custom_code_answers_a_challenge() {
        let terms = bull_proposal(300).await.intents[0].terms.clone().unwrap();
        let answer = |src: String| {
            let terms = terms.clone();
            async move { run_code(&src, Some(&terms), true).await }
        };
        let accept =
            "function decide(ctx) { return [{type:'accept_challenge', stake: ctx.incoming.terms.stake}]; }"
                .to_owned();
        let Some(r) = answer(accept).await else {
            return;
        };
        assert_eq!(
            r.intents[0].intent,
            Intent::AcceptChallenge { stake: terms.stake }
        );

        let r = answer(
            "function decide() { return [{type:'counter_offer', stake:'1000000'}]; }".into(),
        )
        .await
        .unwrap();
        assert_eq!(r.intents[0].terms.as_ref().unwrap().stake, m(1_000_000));
        assert_eq!(
            r.intents[0].terms.as_ref().unwrap().threshold,
            terms.threshold
        );

        let r = answer("function decide() { return [{type:'reject'}]; }".into())
            .await
            .unwrap();
        assert_eq!(r.intents[0].intent, Intent::Reject);

        // Accepting at another stake, or proposing, is not an answer.
        for bad in [
            "function decide() { return [{type:'accept_challenge', stake:'1'}]; }",
            "function decide() { return [{type:'propose_challenge', stake:'1'}]; }",
            "function decide(ctx) { return [{type:'counter_offer', stake: ctx.incoming.terms.stake}]; }",
        ] {
            let r = answer(bad.into()).await.unwrap();
            assert!(r.intents.is_empty(), "{bad}");
            assert!(r.error.is_some(), "{bad}");
        }
    }

    #[tokio::test]
    async fn custom_code_failures_produce_no_intents() {
        for (src, want) in [
            ("function decide() { for(;;){} }", "too much CPU"),
            ("function decide() { throw new Error('boom'); }", "boom"),
            (
                "function decide() { return [{type:'propose_challenge', stake: 5}]; }",
                "invalid intents",
            ),
            (
                "function decide() { return [{type:'propose_challenge', stake:'0'}]; }",
                "invalid terms",
            ),
            (
                "function decide() { return [{type:'commit_funds', interaction_type:'challenge', stake:'1'}]; }",
                "committed by the runtime",
            ),
            (
                "function decide() { return [{type:'accept_challenge', stake:'1'}]; }",
                "no challenge to answer",
            ),
        ] {
            let Some(r) = run_code(src, None, true).await else {
                return;
            };
            assert!(r.intents.is_empty(), "{src}");
            let error = r.error.unwrap();
            assert!(error.contains(want), "{src}: {error}");
            assert_eq!(r.code.unwrap().error.as_deref(), Some(error.as_str()));
        }
    }

    #[tokio::test]
    async fn custom_code_without_a_sandbox_is_a_clear_error() {
        let r = run_code("function decide() { return []; }", None, false)
            .await
            .unwrap();
        assert!(r.intents.is_empty());
        assert!(r.error.as_deref().unwrap().contains("not available"));
        assert_eq!(r.engine("rule_based"), "custom_code");
    }
}

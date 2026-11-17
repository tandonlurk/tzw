//! The `custom.code` block's host side: the one shared `Sandbox`, the JSON
//! `ctx` a strategy sees, the dry run on save and the test-run endpoint.
//!
//! A strategy only ever proposes. Its intents go through the same planning
//! (`graph_exec`) and the same `core::authorize` gate as a built-in block's.
//! The `ctx` holds market data, balances and owner preferences; never policy
//! limits, keys, addresses or other agents' data.

use std::sync::{Arc, OnceLock};

use aen_core::authorize::{Decision, Intent, authorize};
use aen_core::challenge::{ChallengeTerms, MarketRef, Party};
use aen_core::graph::{CUSTOM_CODE, Graph, MAX_CUSTOM_CODE_BYTES};
use aen_core::interaction::InteractionState;
use aen_core::money::Money;
use aen_sandbox::{Sandbox, SandboxError, default_module_path};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ApiError, Detail};
use crate::extract::{ApiJson, ApiPath};
use crate::graph_exec::{
    RunInput, TRIGGER_CHALLENGE_RECEIVED, TRIGGER_MARKET_MOVE, TRIGGER_SCHEDULE, find_trigger,
};
use crate::lifecycle;
use crate::market::{self, MarketView};
use crate::models::AgentInput;
use crate::reasoner::{AgentCtx, ReasonRequest, RuleBasedReasoner, Task};
use crate::state::{AppState, Settings};

/// Name recorded as the run's engine (`agent_runs.reasoner`).
pub const ENGINE: &str = "custom_code";

/// Why a `custom.code` block could not run on this server.
pub const UNAVAILABLE: &str =
    "the code sandbox is not available on this server (its guest module is not built)";

/// The process-wide sandbox, compiled once on first use from
/// `$AEN_SANDBOX_MODULE` or the sandbox crate's `guest/runner.wasm`. `None`
/// if the module is missing or does not compile: the server still runs and
/// `custom.code` agents record `UNAVAILABLE`.
pub fn shared_sandbox() -> Option<Arc<Sandbox>> {
    static SHARED: OnceLock<Option<Arc<Sandbox>>> = OnceLock::new();
    SHARED
        .get_or_init(|| {
            let path = default_module_path();
            match Sandbox::new(&path) {
                Ok(sandbox) => {
                    tracing::info!(
                        module = %path.display(),
                        sha256 = sandbox.module_sha256(),
                        "code sandbox ready"
                    );
                    Some(Arc::new(sandbox))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "code sandbox unavailable; custom.code blocks will not run");
                    None
                }
            }
        })
        .clone()
}

/// The terms a `propose_challenge` from custom code turns into, apart from
/// the stake: the deterministic rule-based claim for this agent and market
/// (D-52). Shown to the strategy as `ctx.proposal`.
pub fn proposal_template(input: &RunInput<'_>) -> Option<ChallengeTerms> {
    let req = ReasonRequest {
        task: Task::GenerateProposal,
        agent: input.agent,
        market: input.market,
        now: input.now,
        incoming: None,
        party: Party::A,
        funding_window_secs: input.funding_window_secs,
        instructions: None,
    };
    RuleBasedReasoner::decide(&req)
        .ok()
        .and_then(|out| out.terms)
}

/// The JSON a strategy's `decide(ctx)` receives. Money is a string of USDC
/// minor units; times are Unix seconds except `now_ms`.
pub fn build_ctx(input: &RunInput<'_>) -> Value {
    let agent = input.agent;
    let proposal = match input.incoming {
        Some(_) => Value::Null,
        None => proposal_template(input).map_or(Value::Null, |t| {
            json!({
                "direction": t.direction, "threshold": t.threshold, "formula": t.formula,
                "start_ts": t.start_ts, "end_ts": t.end_ts,
            })
        }),
    };
    let incoming = input.incoming.map_or(
        Value::Null,
        |t| json!({ "terms": t, "my_side": t.side_of(input.party) }),
    );
    json!({
        "trigger": input.trigger,
        "now_ms": input.now.saturating_mul(1000),
        "agent": {
            "id": agent.id, "name": agent.name, "version": agent.version, "config": agent.config,
        },
        "balances": { "available": agent.available, "exposure": agent.exposure },
        "market": input.market,
        "proposal": proposal,
        "incoming": incoming,
    })
}

/// Deterministic per agent and tick: `Math.random` replays exactly for the
/// same agent, time and trigger.
pub fn seed(agent: Uuid, now_ms: i64, trigger: &str) -> u32 {
    let mut h = Sha256::new();
    h.update(agent.as_bytes());
    h.update(now_ms.to_be_bytes());
    h.update(trigger.as_bytes());
    let d = h.finalize();
    u32::from_be_bytes([d[0], d[1], d[2], d[3]])
}

/// Stable snake_case code for an API response.
pub fn error_code(e: &SandboxError) -> &'static str {
    match e {
        SandboxError::Load(_) => "load",
        SandboxError::CodeTooLarge => "code_too_large",
        SandboxError::InputTooLarge => "input_too_large",
        SandboxError::OutOfFuel => "out_of_fuel",
        SandboxError::Timeout => "timeout",
        SandboxError::MemoryLimit => "memory_limit",
        SandboxError::StackOverflow => "stack_overflow",
        SandboxError::OutputTooLarge => "output_too_large",
        SandboxError::Guest(_) => "guest_error",
        SandboxError::BadOutput(_) => "bad_output",
        SandboxError::Unsupported => "unsupported_feature",
        SandboxError::Trap(_) => "trap",
        SandboxError::Host(_) => "host_error",
    }
}

/// The discovery trigger an agent's graph uses, its symbol and window
/// (as `lifecycle::discover_one` reads them).
pub fn discovery_target(graph: &Graph, settings: &Settings) -> (&'static str, String, i64) {
    if let Some(node) = find_trigger(graph, TRIGGER_MARKET_MOVE) {
        let symbol = node
            .params
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or(&settings.default_symbol)
            .to_owned();
        let window = node
            .params
            .get("window_secs")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(1);
        (TRIGGER_MARKET_MOVE, symbol, window)
    } else {
        (
            TRIGGER_SCHEDULE,
            settings.default_symbol.clone(),
            settings.default_window_secs,
        )
    }
}

fn code_nodes(graph: &Graph) -> impl Iterator<Item = (&str, &str)> {
    graph.nodes.iter().filter_map(|n| {
        (n.node_type == CUSTOM_CODE)
            .then(|| n.params.get("source").and_then(Value::as_str))
            .flatten()
            .map(|s| (n.id.as_str(), s))
    })
}

/// A fixed market for the dry run on save. Labelled as a sample.
fn sample_market() -> MarketView {
    MarketView {
        market: MarketRef::GeminiPredictionContract {
            event_ticker: "SAMPLE-BTC-1H".into(),
            instrument_symbol: "SAMPLE-BTC-1H-UP".into(),
        },
        title: "[sample] BTC price in one hour: Up".into(),
        category: "crypto".into(),
        value: 5500,
        move_bps: 300,
        window_secs: 3600,
        spot_symbol: "btcusd".into(),
        source: "sample".into(),
        fallback_reason: None,
    }
}

/// Dry run on save: every `custom.code` source must run cleanly against a
/// sample `ctx` (a sample market, plus a sample challenge for a flow that
/// answers challenges; it may return no intents). Any sandbox error is a 422 on
/// the node. Skipped when the sandbox is unavailable (D-54). Call with an
/// otherwise valid input.
pub async fn check_on_save(
    sandbox: Option<&Arc<Sandbox>>,
    input: &AgentInput,
    agent_id: Uuid,
    settings: &Settings,
) -> Result<(), ApiError> {
    let Some(sandbox) = sandbox else {
        return Ok(());
    };
    let sources: Vec<(String, String)> = code_nodes(&input.graph)
        .map(|(id, s)| (id.to_owned(), s.to_owned()))
        .collect();
    if sources.is_empty() {
        return Ok(());
    }
    let agent = AgentCtx {
        id: agent_id,
        name: input.name.trim().to_owned(),
        version: 0,
        env: aen_core::ledger::Env::Simulated,
        config: input.config.clone(),
        policy: input.policy.clone(),
        available: Money::from_minor(100_000_000).unwrap_or(Money::ZERO),
        exposure: Money::ZERO,
    };
    let view = sample_market();
    let now = chrono::Utc::now().timestamp();
    let mut run = RunInput {
        graph: &input.graph,
        trigger: discovery_target(&input.graph, settings).0,
        agent: &agent,
        market: &view,
        now,
        incoming: None,
        party: Party::A,
        funding_window_secs: settings.funding_window_secs,
        sandbox: Some(sandbox),
    };
    // A flow that answers challenges is tried on a sample challenge.
    let sample_terms = proposal_template(&run).map(|t| ChallengeTerms {
        stake: Money::from_minor(1_000_000).unwrap_or(Money::ZERO),
        ..t
    });
    if find_trigger(&input.graph, TRIGGER_CHALLENGE_RECEIVED).is_some() {
        run.trigger = TRIGGER_CHALLENGE_RECEIVED;
        run.incoming = sample_terms.as_ref();
        run.party = Party::B;
    }
    let trigger = run.trigger;
    let ctx = build_ctx(&run);
    let now_ms = now.saturating_mul(1000);
    let mut details = Vec::new();
    for (id, source) in sources {
        let seed = seed(agent_id, now_ms, trigger);
        if let Err(e) = sandbox
            .run_blocking(source, ctx.clone(), now_ms, seed)
            .await
        {
            details.push(Detail::new(
                format!("graph.nodes.{id}"),
                "custom_code_failed",
                format!("custom code did not run on a sample context: {e}"),
            ));
        }
    }
    if details.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(details))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestRunBody {
    /// Code to try instead of the saved `custom.code` source.
    #[serde(default)]
    pub source: Option<String>,
}

/// The interaction state an intent would be judged in when it is really
/// used, so the test-run verdict matches the live one.
fn judged_in(intent: &Intent) -> Option<InteractionState> {
    match intent {
        Intent::ProposeChallenge { .. } | Intent::ProposeCollaboration { .. } => None,
        Intent::AcceptChallenge { .. } | Intent::CounterOffer { .. } | Intent::Reject => {
            Some(InteractionState::Proposed)
        }
        Intent::CommitFunds { .. } => Some(InteractionState::Accepted),
    }
}

fn decision_json(d: &Decision) -> Value {
    match d {
        Decision::Allow => json!({ "result": "allow", "reasons": [] }),
        Decision::RequireApproval => json!({ "result": "require_approval", "reasons": [] }),
        Decision::Deny(reasons) => json!({ "result": "deny", "reasons": reasons }),
    }
}

/// `POST /agents/{id}/code/test-run`: runs the agent's custom code (or the
/// `source` given) against the current market and its real balances, and
/// returns the intents with the policy's verdict on each. Reads only: no
/// ledger entry, interaction, run or activity row is written.
pub async fn test_run(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TestRunBody>,
) -> Result<Json<Value>, ApiError> {
    let mut conn = state.pool.acquire().await?;
    let agent = lifecycle::load_agent(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent"))?;
    drop(conn);
    let source = match body.source {
        Some(source) => source,
        None => code_nodes(&agent.graph)
            .next()
            .map(|(_, s)| s.to_owned())
            .ok_or_else(|| {
                ApiError::unprocessable(
                    "no_custom_code",
                    "the agent has no custom.code block; send `source` to try code",
                )
            })?,
    };
    if source.trim().is_empty() || source.len() > MAX_CUSTOM_CODE_BYTES {
        return Err(ApiError::validation(vec![Detail::new(
            "source",
            "invalid_source",
            format!("source must be 1 to {MAX_CUSTOM_CODE_BYTES} bytes"),
        )]));
    }
    let Some(sandbox) = state.sandbox.clone() else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "sandbox_unavailable",
            UNAVAILABLE,
        ));
    };

    let settings = &state.settings;
    let now = chrono::Utc::now().timestamp();
    let (trigger, symbol, window) = discovery_target(&agent.graph, settings);
    let min_expiry = now + settings.funding_window_secs + 60;
    let view = market::discover(state.market.as_ref(), &symbol, window, min_expiry)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "market_data_unavailable",
                format!("market data is unavailable: {e}"),
            )
        })?;
    let ctx_agent = agent.ctx();
    let run = RunInput {
        graph: &agent.graph,
        trigger,
        agent: &ctx_agent,
        market: &view,
        now,
        incoming: None,
        party: Party::A,
        funding_window_secs: settings.funding_window_secs,
        sandbox: Some(&sandbox),
    };
    let ctx = build_ctx(&run);
    let now_ms = now.saturating_mul(1000);
    let seed = seed(agent.id, now_ms, trigger);
    let module = sandbox.module_sha256().to_owned();
    let out = match sandbox
        .run_blocking(source, ctx.clone(), now_ms, seed)
        .await
    {
        Ok(out) => out,
        Err(e) => {
            return Ok(Json(json!({
                "ok": false,
                "error": { "code": error_code(&e), "message": e.to_string() },
                "intents": [], "logs": [], "fuel_used": null,
                "ctx": ctx, "module_sha256": module, "simulated": true,
            })));
        }
    };
    let intents: Vec<Value> = out
        .intents
        .iter()
        .map(|intent| {
            let d = authorize(intent, &agent.policy, &agent.snapshot, judged_in(intent));
            json!({ "intent": intent, "decision": decision_json(&d) })
        })
        .collect();
    Ok(Json(json!({
        "ok": true,
        "error": null,
        "intents": intents,
        "logs": out.logs,
        "fuel_used": out.fuel_used,
        "ctx": ctx,
        "module_sha256": module,
        "simulated": true,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoner::tests::{agent, market};

    fn input<'a>(graph: &'a Graph, a: &'a AgentCtx, m: &'a MarketView) -> RunInput<'a> {
        RunInput {
            graph,
            trigger: TRIGGER_SCHEDULE,
            agent: a,
            market: m,
            now: 1_000,
            incoming: None,
            party: Party::A,
            funding_window_secs: 300,
            sandbox: None,
        }
    }

    fn empty_graph() -> Graph {
        Graph {
            schema_version: aen_core::graph::SCHEMA_VERSION,
            nodes: vec![],
            edges: vec![],
        }
    }

    #[test]
    fn ctx_has_balances_and_preferences_but_no_limits() {
        let (g, a, m) = (empty_graph(), agent("Follow momentum."), market(300));
        let ctx = build_ctx(&input(&g, &a, &m));
        assert_eq!(ctx["balances"]["available"], "100000000");
        assert_eq!(ctx["balances"]["exposure"], "0");
        assert_eq!(ctx["now_ms"], 1_000_000);
        assert_eq!(ctx["agent"]["config"]["strategy"], "Follow momentum.");
        assert_eq!(ctx["market"]["move_bps"], 300);
        assert_eq!(ctx["proposal"]["direction"], "above");
        assert!(ctx["proposal"].get("stake").is_none());
        assert!(ctx["incoming"].is_null());
        let text = ctx.to_string();
        for secret in [
            "max_per_interaction",
            "max_aggregate_exposure",
            "approval_threshold",
            "stop_loss",
            "policy",
        ] {
            assert!(!text.contains(secret), "ctx leaks {secret}");
        }
    }

    #[test]
    fn seed_depends_on_agent_time_and_trigger() {
        let a = Uuid::from_u128(1);
        let s = seed(a, 1_000, TRIGGER_SCHEDULE);
        assert_eq!(s, seed(a, 1_000, TRIGGER_SCHEDULE));
        assert_ne!(s, seed(Uuid::from_u128(2), 1_000, TRIGGER_SCHEDULE));
        assert_ne!(s, seed(a, 2_000, TRIGGER_SCHEDULE));
        assert_ne!(s, seed(a, 1_000, TRIGGER_MARKET_MOVE));
    }
}

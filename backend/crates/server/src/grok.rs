//! `GrokReasoner`: agent reasoning through xAI's OpenAI-compatible chat API
//! with a JSON-schema response format.
//!
//! The model is asked only for an opinion (conviction, stance, a short
//! summary) and, for proposals and counters, a stake and a threshold. Markets,
//! timestamps and formulas are filled in here from the request, so the model
//! cannot reference a market it was not shown. Its answer is parsed strictly
//! and validated like any reasoner output; on **any** failure (no answer,
//! timeout, non-2xx, malformed or invalid output) the rule-based reasoner
//! answers instead and the summary says so. The output is still only an
//! opinion: `core::authorize` decides what may happen.
//!
//! The API key is read from the environment by `main` and is only ever sent
//! in the `Authorization` header. Tests use a local stand-in server.

use std::time::Duration;

use aen_core::challenge::{ChallengeTerms, Direction};
use aen_core::money::Money;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::market::BoxFut;
use crate::reasoner::{
    MAX_SUMMARY_LEN, ReasonRequest, Reasoner, ReasonerError, ReasonerOutput, RuleBasedReasoner,
    Task,
};

pub struct GrokReasoner {
    http: reqwest::Client,
    base_url: String,
    model: String,
    api_key: String,
}

/// What the model must return. `stake` and `threshold` are used only for the
/// two `Generate*` tasks.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelAnswer {
    conviction_bps: u32,
    stance: Direction,
    matches_strategy: bool,
    summary: String,
    #[serde(default)]
    stake: Option<Money>,
    #[serde(default)]
    threshold: Option<i64>,
}

fn answer_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["conviction_bps", "stance", "matches_strategy", "summary", "stake", "threshold"],
        "properties": {
            "conviction_bps": {"type": "integer", "minimum": 0, "maximum": 10000},
            "stance": {"type": "string", "enum": ["above", "below"]},
            "matches_strategy": {"type": "boolean"},
            "summary": {"type": "string", "maxLength": MAX_SUMMARY_LEN},
            "stake": {"type": ["string", "null"], "pattern": "^(0|[1-9][0-9]{0,17})$"},
            "threshold": {"type": ["integer", "null"]}
        }
    })
}

const SYSTEM_PROMPT: &str = "You are the reasoning module of an autonomous agent that makes \
two-sided, equal-stake challenges with other agents about one market observation. You give an \
opinion only: you cannot move funds, and separate code enforces every limit. Answer with the JSON \
object requested and nothing else. conviction_bps is 0-10000. summary is one or two plain sentences \
for the owner (a decision summary, not step-by-step reasoning). Amounts in the JSON fields are integer \
USDC minor units (6 decimals) as strings; in summary, write amounts in USDC with decimals (2500000 \
is \"2.50 USDC\") and percentages instead of basis points, never raw units. Set stake and threshold to null unless the task asks for terms.";

fn task_text(task: Task) -> &'static str {
    match task {
        Task::EvaluateOpportunity => {
            "evaluate_opportunity: say whether the market value will end at or above (\"above\") \
             or below (\"below\") its current value, and how convinced you are."
        }
        Task::EvaluateProposal => {
            "evaluate_proposal: terms are on the table. conviction_bps is your confidence that \
             YOUR side (my_side) wins; stance is the side you actually believe."
        }
        Task::GenerateProposal => {
            "generate_proposal: propose a challenge. stance is the side you take; give stake (at \
             most max_per_interaction and available) and threshold (in the market's unit)."
        }
        Task::GenerateCounter => {
            "generate_counter: answer the terms on the table with a different stake and/or \
             threshold (both must be given; at least one must differ)."
        }
    }
}

impl GrokReasoner {
    pub const DEFAULT_BASE_URL: &'static str = "https://api.x.ai/v1";
    pub const DEFAULT_MODEL: &'static str = "grok-4.7";
    pub const TIMEOUT: Duration = Duration::from_secs(20);

    pub fn new(
        base_url: &str,
        model: &str,
        api_key: &str,
        timeout: Duration,
    ) -> Result<GrokReasoner, ReasonerError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("aen-server")
            .build()
            .map_err(|e| ReasonerError::Unavailable(e.to_string()))?;
        Ok(GrokReasoner {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            api_key: api_key.to_owned(),
        })
    }

    fn user_message(req: &ReasonRequest<'_>) -> String {
        let agent = req.agent;
        json!({
            "task": task_text(req.task),
            "agent": {
                "name": agent.name,
                "goals": agent.config.goals,
                "strategy": agent.config.strategy,
                "categories": agent.config.categories,
                "risk_tolerance": agent.config.risk_tolerance,
                "min_conviction_bps": agent.policy.min_conviction_bps,
                "max_per_interaction": agent.policy.max_per_interaction,
                "approval_threshold": agent.policy.approval_threshold,
                "available": agent.available,
                "exposure": agent.exposure,
            },
            "market": req.market,
            "terms_on_the_table": req.incoming,
            "my_side": req.incoming.map(|t| t.side_of(req.party)),
            "block_instructions": req.instructions,
        })
        .to_string()
    }

    async fn ask(&self, req: &ReasonRequest<'_>) -> Result<ReasonerOutput, ReasonerError> {
        let unavailable = |m: String| ReasonerError::Unavailable(m);
        let body = json!({
            "model": self.model,
            "temperature": 0,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": Self::user_message(req)},
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "agent_reasoning", "strict": true, "schema": answer_schema()},
            },
        });
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| unavailable(e.without_url().to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(unavailable(format!("HTTP {}", status.as_u16())));
        }
        let envelope: Value = response
            .json()
            .await
            .map_err(|e| unavailable(e.without_url().to_string()))?;
        let content = envelope["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| ReasonerError::Invalid("no message content".into()))?;
        let answer: ModelAnswer = serde_json::from_str(content)
            .map_err(|e| ReasonerError::Invalid(format!("not the requested JSON: {e}")))?;

        // Terms are assembled here; the model supplies stake and threshold only.
        let terms = match req.task {
            Task::EvaluateOpportunity | Task::EvaluateProposal => None,
            Task::GenerateProposal => {
                let template = RuleBasedReasoner::decide(req)?
                    .terms
                    .ok_or_else(|| ReasonerError::Invalid("no terms template".into()))?;
                Some(ChallengeTerms {
                    direction: answer.stance,
                    stake: answer.stake.unwrap_or(template.stake),
                    threshold: answer.threshold.unwrap_or(template.threshold),
                    ..template
                })
            }
            Task::GenerateCounter => {
                let incoming = req.incoming.ok_or_else(|| {
                    ReasonerError::Invalid("there are no terms to counter".into())
                })?;
                Some(ChallengeTerms {
                    stake: answer.stake.unwrap_or(incoming.stake),
                    threshold: answer.threshold.unwrap_or(incoming.threshold),
                    ..incoming.clone()
                })
            }
        };
        let output = ReasonerOutput {
            conviction_bps: answer.conviction_bps,
            stance: answer.stance,
            matches_strategy: answer.matches_strategy,
            terms,
            summary: answer.summary.trim().to_owned(),
        };
        output.validate(req)?;
        Ok(output)
    }
}

impl Reasoner for GrokReasoner {
    fn name(&self) -> &'static str {
        "grok"
    }

    fn reason<'a>(
        &'a self,
        req: &'a ReasonRequest<'a>,
    ) -> BoxFut<'a, Result<ReasonerOutput, ReasonerError>> {
        Box::pin(async move {
            match self.ask(req).await {
                Ok(output) => Ok(output),
                Err(error) => {
                    tracing::warn!(%error, task = ?req.task, "grok reasoner failed, using rules");
                    let mut fallback = RuleBasedReasoner::decide(req)?;
                    let summary = format!(
                        "(Rule-based fallback, Grok unavailable.) {}",
                        fallback.summary
                    );
                    fallback.summary = summary.chars().take(MAX_SUMMARY_LEN).collect();
                    Ok(fallback)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    //! A local server stands in for xAI; no test makes a real API call.

    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use axum::{Json, Router};

    use super::*;
    use crate::reasoner::tests::{agent, m, market, request};

    #[derive(Default)]
    struct Stub {
        /// (status, message content) to answer with.
        reply: Mutex<(u16, String)>,
        seen: Mutex<Vec<(Option<String>, Value)>>,
        delay_ms: Mutex<u64>,
    }

    async fn completions(
        State(stub): State<Arc<Stub>>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        stub.seen.lock().unwrap().push((auth, body));
        let delay = *stub.delay_ms.lock().unwrap();
        tokio::time::sleep(Duration::from_millis(delay)).await;
        let (status, content) = stub.reply.lock().unwrap().clone();
        (
            StatusCode::from_u16(status).unwrap(),
            Json(json!({"choices": [{"message": {"role": "assistant", "content": content}}]})),
        )
    }

    async fn grok(status: u16, content: &str) -> (GrokReasoner, Arc<Stub>) {
        let stub = Arc::new(Stub::default());
        *stub.reply.lock().unwrap() = (status, content.to_owned());
        let app = Router::new()
            .route("/v1/chat/completions", post(completions))
            .with_state(stub.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let reasoner =
            GrokReasoner::new(&base, "grok-test", "test-key", Duration::from_millis(200)).unwrap();
        (reasoner, stub)
    }

    const OPINION: &str = r#"{"conviction_bps":7200,"stance":"below","matches_strategy":true,"summary":"The move looks overextended.","stake":null,"threshold":null}"#;

    #[tokio::test]
    async fn uses_the_model_answer_and_sends_a_schema() {
        let (reasoner, stub) = grok(200, OPINION).await;
        let (a, view) = (agent("Follow momentum."), market(300));
        let req = request(Task::EvaluateOpportunity, &a, &view, None);
        let out = reasoner.reason(&req).await.unwrap();
        assert_eq!((out.conviction_bps, out.stance), (7200, Direction::Below));
        assert_eq!(out.summary, "The move looks overextended.");
        assert_eq!(out.terms, None);

        let seen = stub.seen.lock().unwrap();
        let (auth, body) = &seen[0];
        assert_eq!(auth.as_deref(), Some("Bearer test-key"));
        assert_eq!(body["model"], "grok-test");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(
            body["response_format"]["json_schema"]["schema"]["additionalProperties"],
            false
        );
        // The key never appears in the prompt.
        assert!(!body["messages"].to_string().contains("test-key"));
    }

    #[tokio::test]
    async fn proposal_terms_are_built_here_from_the_models_stake_and_threshold() {
        let content = r#"{"conviction_bps":8000,"stance":"below","matches_strategy":true,"summary":"Fade it.","stake":"3000000","threshold":6000}"#;
        let (reasoner, _) = grok(200, content).await;
        let (a, view) = (agent("Follow momentum."), market(300));
        let req = request(Task::GenerateProposal, &a, &view, None);
        let terms = reasoner.reason(&req).await.unwrap().terms.unwrap();
        assert_eq!(
            (terms.stake, terms.threshold, terms.direction),
            (m(3_000_000), 6000, Direction::Below)
        );
        // Market and timing come from the request, not from the model.
        assert_eq!(terms.market, view.market);
        assert_eq!((terms.start_ts, terms.end_ts), (1_300, 4_900));
    }

    async fn falls_back(status: u16, content: &str) {
        let (reasoner, _) = grok(status, content).await;
        let (a, view) = (agent("Follow momentum."), market(300));
        let req = request(Task::GenerateProposal, &a, &view, None);
        let out = reasoner.reason(&req).await.unwrap();
        let rules = RuleBasedReasoner::decide(&req).unwrap();
        assert_eq!(out.terms, rules.terms, "{content}");
        assert_eq!(out.conviction_bps, rules.conviction_bps);
        assert!(
            out.summary.starts_with("(Rule-based fallback"),
            "{}",
            out.summary
        );
        out.validate(&req).unwrap();
    }

    #[tokio::test]
    async fn any_bad_answer_falls_back_to_rules() {
        falls_back(500, OPINION).await;
        falls_back(429, OPINION).await;
        falls_back(200, "I think the market will go up!").await;
        // Out-of-range conviction.
        falls_back(200, r#"{"conviction_bps":20000,"stance":"above","matches_strategy":true,"summary":"s","stake":"1","threshold":5500}"#).await;
        // An extra field trying to grant itself permission.
        falls_back(200, r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"s","stake":"1","threshold":5500,"approved":true}"#).await;
        // Zero stake and an impossible threshold fail terms validation.
        falls_back(200, r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"s","stake":"0","threshold":5500}"#).await;
        falls_back(200, r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"s","stake":"1","threshold":999999}"#).await;
        // Negative / non-integer money.
        falls_back(200, r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"s","stake":"-5","threshold":5500}"#).await;
        falls_back(200, r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"","stake":"1","threshold":5500}"#).await;
    }

    #[tokio::test]
    async fn timeout_and_unreachable_fall_back_to_rules() {
        let (reasoner, stub) = grok(200, OPINION).await;
        *stub.delay_ms.lock().unwrap() = 600;
        let (a, view) = (agent("Follow momentum."), market(300));
        let req = request(Task::EvaluateOpportunity, &a, &view, None);
        let out = reasoner.reason(&req).await.unwrap();
        assert_eq!(out.conviction_bps, 6500); // the rule-based answer
        assert!(out.summary.starts_with("(Rule-based fallback"));

        let dead = GrokReasoner::new(
            "http://127.0.0.1:9/v1",
            "m",
            "k",
            Duration::from_millis(200),
        )
        .unwrap();
        let out = dead.reason(&req).await.unwrap();
        assert_eq!(out.conviction_bps, 6500);
    }

    #[tokio::test]
    async fn an_unchanged_counter_falls_back() {
        let (a, view) = (agent("Most sharp moves revert."), market(300));
        let proposer = agent("Follow momentum.");
        let incoming =
            RuleBasedReasoner::decide(&request(Task::GenerateProposal, &proposer, &view, None))
                .unwrap()
                .terms
                .unwrap();
        // Same stake and threshold as the terms on the table: not a counter.
        let content = r#"{"conviction_bps":6000,"stance":"below","matches_strategy":true,"summary":"Same again.","stake":"5000000","threshold":5500}"#;
        let (reasoner, _) = grok(200, content).await;
        let req = request(Task::GenerateCounter, &a, &view, Some(&incoming));
        let out = reasoner.reason(&req).await.unwrap();
        assert_eq!(out.terms.unwrap().stake, m(2_500_000));
        assert!(out.summary.starts_with("(Rule-based fallback"));
    }
}

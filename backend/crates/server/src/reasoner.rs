//! Agent reasoning behind a trait. A reasoner only ever returns a
//! `ReasonerOutput`, which is validated here before the graph executor may
//! turn it into an `Intent`. Nothing a reasoner says is authorization, and
//! its conviction is an opinion, not a probability.

use aen_core::challenge::{ChallengeTerms, Direction, FormulaId, Party};
use aen_core::money::Money;
use aen_core::policy::{AgentPolicy, MAX_BPS};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::lifecycle::{format_value, side_phrase as side_word, stake_text};
use crate::market::{BoxFut, MarketView};
use crate::models::{AgentConfig, RiskTolerance, display_bps};

pub const MAX_SUMMARY_LEN: usize = 500;
/// Longest challenge a reasoner may propose.
pub const MAX_HORIZON_SECS: i64 = 30 * 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    /// Form a view on a market. No terms.
    EvaluateOpportunity,
    /// Judge terms on the table from this agent's side. No terms.
    EvaluateProposal,
    /// Write terms for a new challenge.
    GenerateProposal,
    /// Write changed terms in answer to the ones on the table.
    GenerateCounter,
}

/// The agent as a reasoner sees it. Balances are context, not limits: the
/// permission engine re-reads them when an intent is executed.
#[derive(Debug, Clone)]
pub struct AgentCtx {
    pub id: Uuid,
    pub name: String,
    pub version: i32,
    pub config: AgentConfig,
    pub policy: AgentPolicy,
    pub available: Money,
    pub exposure: Money,
}

#[derive(Debug, Clone)]
pub struct ReasonRequest<'a> {
    pub task: Task,
    pub agent: &'a AgentCtx,
    pub market: &'a MarketView,
    /// Unix seconds.
    pub now: i64,
    /// Terms on the table, for `EvaluateProposal` and `GenerateCounter`.
    pub incoming: Option<&'a ChallengeTerms>,
    /// Which party this agent is (or would be) in the challenge.
    pub party: Party,
    /// Seconds a new proposal leaves for both stakes to be committed.
    pub funding_window_secs: i64,
    /// Free-text block instructions. Guidance only.
    pub instructions: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasonerOutput {
    /// 0-10000. For `EvaluateProposal`: confidence that this agent's side of
    /// the terms on the table wins. Otherwise: confidence in `stance`.
    pub conviction_bps: u32,
    /// The side the agent believes in.
    pub stance: Direction,
    /// Whether the market fits the owner's stated categories.
    pub matches_strategy: bool,
    /// Required for the two `Generate*` tasks, forbidden otherwise.
    #[serde(default)]
    pub terms: Option<ChallengeTerms>,
    /// A short decision summary for the activity view (not chain-of-thought).
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReasonerError {
    #[error("reasoner unavailable: {0}")]
    Unavailable(String),
    #[error("reasoner output rejected: {0}")]
    Invalid(String),
}

pub trait Reasoner: Send + Sync {
    fn name(&self) -> &'static str;
    fn reason<'a>(
        &'a self,
        req: &'a ReasonRequest<'a>,
    ) -> BoxFut<'a, Result<ReasonerOutput, ReasonerError>>;
}

impl ReasonerOutput {
    /// Strict parse of model output: unknown or missing fields are rejected.
    pub fn from_json(text: &str) -> Result<ReasonerOutput, ReasonerError> {
        serde_json::from_str(text).map_err(|e| ReasonerError::Invalid(format!("not valid: {e}")))
    }

    /// Checks the output against the request. Called on every reasoner
    /// result, whichever implementation produced it.
    pub fn validate(&self, req: &ReasonRequest<'_>) -> Result<(), ReasonerError> {
        let invalid = |msg: String| Err(ReasonerError::Invalid(msg));
        if self.conviction_bps > MAX_BPS {
            return invalid(format!(
                "conviction_bps {} is above 10000",
                self.conviction_bps
            ));
        }
        let summary_len = self.summary.chars().count();
        if self.summary.trim().is_empty() || summary_len > MAX_SUMMARY_LEN {
            return invalid(format!("summary must be 1-{MAX_SUMMARY_LEN} characters"));
        }
        let wants_terms = matches!(req.task, Task::GenerateProposal | Task::GenerateCounter);
        let Some(terms) = &self.terms else {
            return if wants_terms {
                invalid("terms are required for this task".into())
            } else {
                Ok(())
            };
        };
        if !wants_terms {
            return invalid("terms are not allowed for this task".into());
        }
        let errors = terms.validate();
        if !errors.is_empty() {
            let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
            return invalid(format!("invalid terms: {}", list.join("; ")));
        }
        // A reasoner may not invent a market: only the one it was shown.
        if terms.market != req.market.market {
            return invalid("terms reference a market that was not offered".into());
        }
        match (req.task, req.incoming) {
            (Task::GenerateCounter, Some(incoming)) => {
                let same = ChallengeTerms {
                    stake: incoming.stake,
                    threshold: incoming.threshold,
                    ..terms.clone()
                };
                if same != *incoming {
                    return invalid("a counter may only change stake and threshold".into());
                }
                if terms == incoming {
                    return invalid("a counter must change the terms".into());
                }
            }
            (Task::GenerateCounter, None) => {
                return invalid("there are no terms to counter".into());
            }
            _ => {
                if terms.start_ts <= req.now {
                    return invalid("start_ts must be in the future".into());
                }
                if terms.end_ts - terms.start_ts > MAX_HORIZON_SECS {
                    return invalid("the challenge is longer than 30 days".into());
                }
            }
        }
        Ok(())
    }
}

/// Deterministic reasoner: no model, no network. Used by tests, offline
/// demos, and as the fallback when the LLM reasoner fails.
///
/// Rules (all integer arithmetic):
/// - Style: an agent whose strategy text reads as contrarian ("revert",
///   "other side", "contrarian", "skeptic", "fade") expects moves to
///   reverse; every other agent expects them to continue.
/// - Belief that the market ends at or above its current value:
///   `5000 + TILT_PER_MOVE_BP * move_bps` (minus for contrarians), clamped to
///   500-9500.
/// - Belief for another threshold shifts by `2 * relative distance` in bps.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuleBasedReasoner;

const TILT_PER_MOVE_BP: i64 = 5;
const MIN_BELIEF: i64 = 500;
const MAX_BELIEF: i64 = 9500;
/// At or above this conviction a proposal stakes the per-interaction maximum.
const FULL_STAKE_CONVICTION_BPS: u32 = 8000;

fn is_contrarian(config: &AgentConfig) -> bool {
    let text = config.strategy.to_lowercase();
    ["revert", "other side", "contrarian", "skeptic", "fade"]
        .iter()
        .any(|word| text.contains(word))
}

/// Belief (bps) that the observation ends at or above `threshold`.
fn belief_at_or_above(config: &AgentConfig, market: &MarketView, threshold: i64) -> i64 {
    let tilt = market.move_bps.saturating_mul(TILT_PER_MOVE_BP);
    let tilt = if is_contrarian(config) { -tilt } else { tilt };
    let distance = i128::from(market.value) - i128::from(threshold);
    let distance_bps = distance * 10_000 / i128::from(market.value.max(1));
    let shift = (distance_bps * 2).clamp(-10_000, 10_000) as i64;
    5000_i64
        .saturating_add(tilt)
        .saturating_add(shift)
        .clamp(MIN_BELIEF, MAX_BELIEF)
}

fn matches_strategy(config: &AgentConfig, market: &MarketView) -> bool {
    config.categories.is_empty()
        || config
            .categories
            .iter()
            .any(|c| c.eq_ignore_ascii_case(&market.category))
}

/// Stake sizing default: stay inside the automatic range unless conviction is
/// very high; low risk tolerance halves it; never more than is available.
fn proposal_stake(agent: &AgentCtx, conviction_bps: u32) -> Money {
    let policy = &agent.policy;
    let auto = policy.approval_threshold.min(policy.max_per_interaction);
    let base = if conviction_bps >= FULL_STAKE_CONVICTION_BPS || auto.is_zero() {
        policy.max_per_interaction
    } else {
        auto
    };
    let base = match agent.config.risk_tolerance {
        RiskTolerance::Low => Money::from_minor(base.minor() / 2).unwrap_or(base),
        RiskTolerance::Medium | RiskTolerance::High => base,
    };
    base.min(agent.available)
}

/// A spot move for a summary: `300` -> `+3%`.
fn move_text(move_bps: i64) -> String {
    let sign = if move_bps > 0 { "+" } else { "" };
    format!("{sign}{}", display_bps(move_bps))
}

impl RuleBasedReasoner {
    pub fn decide(req: &ReasonRequest<'_>) -> Result<ReasonerOutput, ReasonerError> {
        let agent = req.agent;
        let market = req.market;
        let style = if is_contrarian(&agent.config) {
            "contrarian"
        } else {
            "momentum"
        };
        let fits = matches_strategy(&agent.config, market);
        let up = belief_at_or_above(&agent.config, market, market.value);
        let (stance, conviction) = if up >= 5000 {
            (Direction::Above, up as u32)
        } else {
            (Direction::Below, (10_000 - up) as u32)
        };
        // Summaries are read by the owner: amounts, prices and percentages are
        // written out, never as minor units or basis points.
        let moved = move_text(market.move_bps);
        let formula = FormulaId::for_market(&market.market);
        let pct = |bps: u32| display_bps(i64::from(bps));

        match req.task {
            Task::EvaluateOpportunity => Ok(ReasonerOutput {
                conviction_bps: conviction,
                stance,
                matches_strategy: fits,
                terms: None,
                summary: format!(
                    "{style} rule: the market moved {moved}, so I expect it to end {} its \
                     current level of {} ({} conviction).",
                    side_word(stance),
                    format_value(formula, market.value),
                    pct(conviction)
                ),
            }),
            Task::GenerateProposal => {
                let (lo, hi) = formula.threshold_range();
                let start_ts = req.now + req.funding_window_secs.max(1);
                let horizon = i64::from(agent.config.horizon_secs).clamp(1, MAX_HORIZON_SECS);
                let stake = proposal_stake(agent, conviction);
                let terms = ChallengeTerms {
                    market: market.market.clone(),
                    direction: stance,
                    threshold: market.value.clamp(lo, hi),
                    start_ts,
                    end_ts: start_ts + horizon,
                    stake,
                    resolution_source: formula.source(),
                    formula,
                };
                Ok(ReasonerOutput {
                    conviction_bps: conviction,
                    stance,
                    matches_strategy: fits,
                    summary: format!(
                        "I claim the market ends {} {} and stake {} on it.",
                        side_word(stance),
                        format_value(formula, terms.threshold),
                        stake_text(stake)
                    ),
                    terms: Some(terms),
                })
            }
            Task::EvaluateProposal => {
                let incoming = req.incoming.ok_or_else(|| {
                    ReasonerError::Invalid("there are no terms to evaluate".into())
                })?;
                let at_or_above = belief_at_or_above(&agent.config, market, incoming.threshold);
                let mine = incoming.side_of(req.party);
                let conviction = match mine {
                    Direction::Above => at_or_above,
                    Direction::Below => 10_000 - at_or_above,
                } as u32;
                Ok(ReasonerOutput {
                    conviction_bps: conviction,
                    stance: if conviction >= 5000 {
                        mine
                    } else {
                        mine.opposite()
                    },
                    matches_strategy: fits,
                    terms: None,
                    summary: format!(
                        "{style} rule: my side is \"{} {}\"; after a {moved} move I give it \
                         {} conviction.",
                        side_word(mine),
                        format_value(incoming.formula, incoming.threshold),
                        pct(conviction)
                    ),
                })
            }
            Task::GenerateCounter => {
                let incoming = req.incoming.ok_or_else(|| {
                    ReasonerError::Invalid("there are no terms to counter".into())
                })?;
                // Counter default: same claim, half the stake (at least one
                // minor unit, at most this agent's per-interaction maximum).
                let half = (incoming.stake.minor() / 2).max(1);
                let stake = Money::from_minor(half)
                    .unwrap_or(incoming.stake)
                    .min(agent.policy.max_per_interaction);
                Ok(ReasonerOutput {
                    conviction_bps: conviction,
                    stance,
                    matches_strategy: fits,
                    summary: format!(
                        "I am not convinced enough for the full stake; I offer {} per side \
                         instead.",
                        stake_text(stake)
                    ),
                    terms: Some(ChallengeTerms {
                        stake,
                        ..incoming.clone()
                    }),
                })
            }
        }
    }
}

impl Reasoner for RuleBasedReasoner {
    fn name(&self) -> &'static str {
        "rule_based"
    }

    fn reason<'a>(
        &'a self,
        req: &'a ReasonRequest<'a>,
    ) -> BoxFut<'a, Result<ReasonerOutput, ReasonerError>> {
        Box::pin(async move { Self::decide(req) })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use aen_core::challenge::MarketRef;
    use aen_core::policy::InteractionType;

    use super::*;

    pub fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    pub fn agent(strategy: &str) -> AgentCtx {
        AgentCtx {
            id: Uuid::from_u128(1),
            name: "Tester".into(),
            version: 1,
            config: AgentConfig {
                goals: "grow".into(),
                strategy: strategy.into(),
                categories: vec!["crypto".into()],
                horizon_secs: 3600,
                risk_tolerance: RiskTolerance::Medium,
                schedule_secs: 60,
            },
            policy: AgentPolicy {
                max_per_interaction: m(10_000_000),
                max_aggregate_exposure: m(50_000_000),
                approval_threshold: m(5_000_000),
                allowed_interaction_types: vec![InteractionType::Challenge],
                min_conviction_bps: 6000,
                stop_loss: None,
            },
            available: m(100_000_000),
            exposure: Money::ZERO,
        }
    }

    pub fn market(move_bps: i64) -> MarketView {
        MarketView {
            market: MarketRef::GeminiPredictionContract {
                event_ticker: "BTC-FAKE-1H".into(),
                instrument_symbol: "FAKE-BTC-1H-UP".into(),
            },
            title: "[fake] BTC price in one hour: Up".into(),
            category: "crypto".into(),
            value: 5500,
            move_bps,
            window_secs: 3600,
            spot_symbol: "btcusd".into(),
            source: "fake".into(),
            fallback_reason: None,
        }
    }

    pub fn request<'a>(
        task: Task,
        agent: &'a AgentCtx,
        market: &'a MarketView,
        incoming: Option<&'a ChallengeTerms>,
    ) -> ReasonRequest<'a> {
        ReasonRequest {
            task,
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
            instructions: None,
        }
    }

    fn proposal(agent: &AgentCtx, market: &MarketView) -> ChallengeTerms {
        let req = request(Task::GenerateProposal, agent, market, None);
        let out = RuleBasedReasoner::decide(&req).unwrap();
        out.validate(&req).unwrap();
        out.terms.unwrap()
    }

    #[test]
    fn momentum_follows_the_move_and_contrarian_fades_it() {
        let view = market(300);
        let bull = agent("Follow momentum.");
        let req = request(Task::EvaluateOpportunity, &bull, &view, None);
        let out = RuleBasedReasoner::decide(&req).unwrap();
        out.validate(&req).unwrap();
        assert_eq!((out.stance, out.conviction_bps), (Direction::Above, 6500));

        let skeptic = agent("Most sharp moves revert.");
        let req = request(Task::EvaluateOpportunity, &skeptic, &view, None);
        let out = RuleBasedReasoner::decide(&req).unwrap();
        assert_eq!((out.stance, out.conviction_bps), (Direction::Below, 6500));
    }

    #[test]
    fn proposal_terms_are_valid_and_sized_within_the_automatic_range() {
        let view = market(300);
        let bull = agent("Follow momentum.");
        let terms = proposal(&bull, &view);
        assert_eq!(terms.direction, Direction::Above);
        assert_eq!(terms.threshold, 5500);
        assert_eq!(terms.stake, m(5_000_000));
        assert_eq!((terms.start_ts, terms.end_ts), (1_300, 4_900));

        // Very high conviction stakes the maximum; a thin balance caps it.
        let terms = proposal(&bull, &market(700));
        assert_eq!(terms.stake, m(10_000_000));
        let mut poor = agent("Follow momentum.");
        poor.available = m(1_234);
        assert_eq!(proposal(&poor, &view).stake, m(1_234));
    }

    #[test]
    fn responder_conviction_is_for_its_own_side() {
        let view = market(300);
        let terms = proposal(&agent("Follow momentum."), &view);
        let skeptic = agent("Most sharp moves revert.");
        let req = request(Task::EvaluateProposal, &skeptic, &view, Some(&terms));
        let out = RuleBasedReasoner::decide(&req).unwrap();
        out.validate(&req).unwrap();
        // The skeptic holds "below"; it believes "at or above" only 3500.
        assert_eq!((out.stance, out.conviction_bps), (Direction::Below, 6500));
        assert!(out.matches_strategy);
    }

    #[test]
    fn counter_halves_the_stake_and_changes_nothing_else() {
        let view = market(300);
        let terms = proposal(&agent("Follow momentum."), &view);
        let skeptic = agent("Most sharp moves revert.");
        let req = request(Task::GenerateCounter, &skeptic, &view, Some(&terms));
        let out = RuleBasedReasoner::decide(&req).unwrap();
        out.validate(&req).unwrap();
        let counter = out.terms.unwrap();
        assert_eq!(counter.stake, m(2_500_000));
        assert_eq!(
            ChallengeTerms {
                stake: terms.stake,
                ..counter
            },
            terms
        );
    }

    #[test]
    fn category_mismatch_is_reported() {
        let mut a = agent("Follow momentum.");
        a.config.categories = vec!["sports".into()];
        let view = market(300);
        let req = request(Task::EvaluateOpportunity, &a, &view, None);
        assert!(!RuleBasedReasoner::decide(&req).unwrap().matches_strategy);
    }

    fn good(task: Task) -> (AgentCtx, MarketView, ReasonerOutput) {
        let a = agent("Follow momentum.");
        let view = market(300);
        let out = RuleBasedReasoner::decide(&request(task, &a, &view, None)).unwrap();
        (a, view, out)
    }

    #[test]
    fn malformed_output_is_rejected() {
        let (a, view, out) = good(Task::EvaluateOpportunity);
        let req = request(Task::EvaluateOpportunity, &a, &view, None);
        let reject = |o: ReasonerOutput| {
            assert!(matches!(o.validate(&req), Err(ReasonerError::Invalid(_))));
        };
        reject(ReasonerOutput {
            conviction_bps: 10_001,
            ..out.clone()
        });
        reject(ReasonerOutput {
            summary: "  ".into(),
            ..out.clone()
        });
        reject(ReasonerOutput {
            summary: "x".repeat(MAX_SUMMARY_LEN + 1),
            ..out.clone()
        });
        // Terms where none were asked for.
        let (_, _, with_terms) = good(Task::GenerateProposal);
        reject(ReasonerOutput {
            terms: with_terms.terms,
            ..out
        });
    }

    #[test]
    fn malformed_terms_are_rejected() {
        let (a, view, out) = good(Task::GenerateProposal);
        let req = request(Task::GenerateProposal, &a, &view, None);
        let terms = out.terms.clone().unwrap();
        let reject = |t: Option<ChallengeTerms>| {
            let o = ReasonerOutput {
                terms: t,
                ..out.clone()
            };
            assert!(matches!(o.validate(&req), Err(ReasonerError::Invalid(_))));
        };
        reject(None);
        reject(Some(ChallengeTerms {
            stake: Money::ZERO,
            ..terms.clone()
        }));
        reject(Some(ChallengeTerms {
            end_ts: terms.start_ts,
            ..terms.clone()
        }));
        reject(Some(ChallengeTerms {
            start_ts: req.now,
            ..terms.clone()
        }));
        reject(Some(ChallengeTerms {
            end_ts: terms.start_ts + MAX_HORIZON_SECS + 1,
            ..terms.clone()
        }));
        reject(Some(ChallengeTerms {
            threshold: 10_001,
            ..terms.clone()
        }));
        // A market the reasoner was never shown.
        reject(Some(ChallengeTerms {
            market: MarketRef::GeminiPredictionContract {
                event_ticker: "MADE-UP".into(),
                instrument_symbol: "MADE-UP-1".into(),
            },
            ..terms
        }));
    }

    #[test]
    fn a_counter_may_only_change_stake_and_threshold() {
        let (a, view, out) = good(Task::GenerateProposal);
        let incoming = out.terms.clone().unwrap();
        let req = request(Task::GenerateCounter, &a, &view, Some(&incoming));
        let with = |t: ChallengeTerms| ReasonerOutput {
            terms: Some(t),
            ..out.clone()
        };
        assert!(with(incoming.clone()).validate(&req).is_err());
        assert!(
            with(ChallengeTerms {
                end_ts: incoming.end_ts + 1,
                ..incoming.clone()
            })
            .validate(&req)
            .is_err()
        );
        assert!(
            with(ChallengeTerms {
                direction: incoming.direction.opposite(),
                ..incoming.clone()
            })
            .validate(&req)
            .is_err()
        );
        assert!(
            with(ChallengeTerms {
                stake: m(1),
                ..incoming.clone()
            })
            .validate(&req)
            .is_ok()
        );
    }

    #[test]
    fn json_parsing_is_strict() {
        let ok =
            r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"s"}"#;
        assert_eq!(ReasonerOutput::from_json(ok).unwrap().conviction_bps, 7000);
        for bad in [
            "not json",
            r#"{"conviction_bps":7000}"#,
            r#"{"conviction_bps":-1,"stance":"above","matches_strategy":true,"summary":"s"}"#,
            r#"{"conviction_bps":"high","stance":"above","matches_strategy":true,"summary":"s"}"#,
            r#"{"conviction_bps":7000,"stance":"sideways","matches_strategy":true,"summary":"s"}"#,
            r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"s","authorize":true}"#,
            r#"{"conviction_bps":7000,"stance":"above","matches_strategy":true,"summary":"s","terms":{"stake":"5"}}"#,
        ] {
            assert!(ReasonerOutput::from_json(bad).is_err(), "{bad}");
        }
    }

    /// Summaries are shown to the owner as they are: no minor units, no raw
    /// basis points. The structured terms keep the exact integers.
    #[test]
    fn summaries_are_human_readable() {
        let view = market(300);
        let bull = agent("Follow momentum.");
        let skeptic = agent("Most sharp moves revert.");
        let say = |task, agent: &AgentCtx, incoming: Option<&ChallengeTerms>| {
            let req = request(task, agent, &view, incoming);
            let out = RuleBasedReasoner::decide(&req).unwrap();
            out.validate(&req).unwrap();
            out
        };
        let opinion = say(Task::EvaluateOpportunity, &bull, None);
        assert_eq!(
            opinion.summary,
            "momentum rule: the market moved +3%, so I expect it to end at or above its current \
             level of 0.55 (YES price) (65% conviction)."
        );
        let proposal = say(Task::GenerateProposal, &bull, None);
        assert_eq!(
            proposal.summary,
            "I claim the market ends at or above 0.55 (YES price) and stake 5.00 USDC \
             (simulated) on it."
        );
        let terms = proposal.terms.unwrap();
        assert_eq!((terms.stake, terms.threshold), (m(5_000_000), 5500));
        let judged = say(Task::EvaluateProposal, &skeptic, Some(&terms));
        assert_eq!(
            judged.summary,
            "contrarian rule: my side is \"below 0.55 (YES price)\"; after a +3% move I give it \
             65% conviction."
        );
        let counter = say(Task::GenerateCounter, &skeptic, Some(&terms));
        assert_eq!(
            counter.summary,
            "I am not convinced enough for the full stake; I offer 2.50 USDC (simulated) per \
             side instead."
        );
        assert_eq!(counter.terms.unwrap().stake, m(2_500_000));
    }
}

//! The permission engine: the only gate between reasoning and execution.
//!
//! Blocks, LLM output and custom code all end in an `Intent`. `authorize` is a
//! pure function that decides whether that intent may proceed. Nothing an
//! agent "thinks" is authorization.

use serde::{Deserialize, Serialize};

use crate::interaction::InteractionState;
use crate::money::{Money, SignedMoney};
use crate::policy::{AgentPolicy, InteractionType};

/// Something an agent wants to do. Amounts are per-side stakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    ProposeChallenge {
        stake: Money,
    },
    AcceptChallenge {
        stake: Money,
    },
    CounterOffer {
        stake: Money,
    },
    Reject,
    CommitFunds {
        interaction_type: InteractionType,
        stake: Money,
    },
    ProposeCollaboration {
        contribution: Money,
    },
}

impl Intent {
    /// The amount this intent puts (or would put) at risk.
    pub const fn stake(&self) -> Option<Money> {
        match *self {
            Intent::ProposeChallenge { stake }
            | Intent::AcceptChallenge { stake }
            | Intent::CounterOffer { stake }
            | Intent::CommitFunds { stake, .. } => Some(stake),
            Intent::ProposeCollaboration { contribution } => Some(contribution),
            Intent::Reject => None,
        }
    }

    /// `None` for `Reject`, which applies to any interaction type.
    pub const fn interaction_type(&self) -> Option<InteractionType> {
        match *self {
            Intent::ProposeChallenge { .. }
            | Intent::AcceptChallenge { .. }
            | Intent::CounterOffer { .. } => Some(InteractionType::Challenge),
            Intent::ProposeCollaboration { .. } => Some(InteractionType::Collaboration),
            Intent::CommitFunds {
                interaction_type, ..
            } => Some(interaction_type),
            Intent::Reject => None,
        }
    }

    /// Whether the intent is legal for an interaction in `state`
    /// (`None` = the interaction does not exist yet).
    pub const fn legal_in(&self, state: Option<InteractionState>) -> bool {
        use InteractionState::*;
        match self {
            Intent::ProposeChallenge { .. } | Intent::ProposeCollaboration { .. } => {
                state.is_none()
            }
            Intent::AcceptChallenge { .. } | Intent::CounterOffer { .. } | Intent::Reject => {
                matches!(state, Some(Proposed | Negotiating))
            }
            Intent::CommitFunds { .. } => matches!(state, Some(Accepted | AwaitingApproval)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Paused,
}

/// The agent's financial position at decision time, read from the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub status: AgentStatus,
    /// Balance of `agent:{id}:available`.
    pub available: Money,
    /// Total stake already at risk in open interactions.
    pub exposure: Money,
    pub realized_pnl: SignedMoney,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    AgentPaused,
    StopLossTriggered,
    InteractionTypeNotAllowed,
    IllegalInteractionState,
    ZeroAmount,
    AmountOverflow,
    ExceedsMaxPerInteraction,
    ExceedsAggregateExposure,
    InsufficientAvailableBalance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Within limits but above the approval threshold: the owner must approve.
    RequireApproval,
    Deny(Vec<DenyReason>),
}

/// Decides whether `intent` may proceed. Every failed check is reported; a
/// single failure denies.
///
/// Rules:
/// - A paused agent may do nothing.
/// - Once realized losses reach `stop_loss`, nothing that stakes funds is
///   allowed (`Reject` still is).
/// - A stake must be > 0, <= `max_per_interaction`, <= available balance, and
///   `exposure + stake` must be <= `max_aggregate_exposure`.
/// - A stake strictly above `approval_threshold` needs owner approval.
///
/// Approval does not bypass this function: after the owner approves, call it
/// again. `Deny` still blocks; `RequireApproval` is then satisfied by the
/// recorded approval.
pub fn authorize(
    intent: &Intent,
    policy: &AgentPolicy,
    snapshot: &Snapshot,
    interaction_state: Option<InteractionState>,
) -> Decision {
    let mut reasons = Vec::new();

    if snapshot.status == AgentStatus::Paused {
        reasons.push(DenyReason::AgentPaused);
    }
    if !intent.legal_in(interaction_state) {
        reasons.push(DenyReason::IllegalInteractionState);
    }
    if let Some(kind) = intent.interaction_type()
        && !policy.allowed_interaction_types.contains(&kind)
    {
        reasons.push(DenyReason::InteractionTypeNotAllowed);
    }

    let mut needs_approval = false;
    if let Some(stake) = intent.stake() {
        if stop_loss_triggered(policy, snapshot) {
            reasons.push(DenyReason::StopLossTriggered);
        }
        if stake.is_zero() {
            reasons.push(DenyReason::ZeroAmount);
        }
        if stake > policy.max_per_interaction {
            reasons.push(DenyReason::ExceedsMaxPerInteraction);
        }
        match snapshot.exposure.checked_add(stake) {
            None => reasons.push(DenyReason::AmountOverflow),
            Some(total) if total > policy.max_aggregate_exposure => {
                reasons.push(DenyReason::ExceedsAggregateExposure);
            }
            Some(_) => {}
        }
        if stake > snapshot.available {
            reasons.push(DenyReason::InsufficientAvailableBalance);
        }
        needs_approval = stake > policy.approval_threshold;
    }

    if !reasons.is_empty() {
        Decision::Deny(reasons)
    } else if needs_approval {
        Decision::RequireApproval
    } else {
        Decision::Allow
    }
}

fn stop_loss_triggered(policy: &AgentPolicy, snapshot: &Snapshot) -> bool {
    match policy.stop_loss {
        Some(limit) => snapshot.realized_pnl <= limit.debit(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DenyReason::*;
    use InteractionState::*;

    fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    /// max 100 per interaction, 300 aggregate, approval above 50, stop at -200.
    fn policy() -> AgentPolicy {
        AgentPolicy {
            max_per_interaction: m(100),
            max_aggregate_exposure: m(300),
            approval_threshold: m(50),
            allowed_interaction_types: vec![InteractionType::Challenge],
            min_conviction_bps: 5000,
            stop_loss: Some(m(200)),
        }
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            status: AgentStatus::Active,
            available: m(1000),
            exposure: Money::ZERO,
            realized_pnl: SignedMoney::ZERO,
        }
    }

    fn propose(stake: i64) -> Intent {
        Intent::ProposeChallenge { stake: m(stake) }
    }

    fn commit(stake: i64) -> Intent {
        Intent::CommitFunds {
            interaction_type: InteractionType::Challenge,
            stake: m(stake),
        }
    }

    fn deny(reasons: &[DenyReason]) -> Decision {
        Decision::Deny(reasons.to_vec())
    }

    #[test]
    fn small_stake_is_allowed() {
        assert_eq!(
            authorize(&propose(10), &policy(), &snapshot(), None),
            Decision::Allow
        );
    }

    #[test]
    fn approval_threshold_boundary() {
        let (p, s) = (policy(), snapshot());
        assert_eq!(authorize(&propose(50), &p, &s, None), Decision::Allow);
        assert_eq!(
            authorize(&propose(51), &p, &s, None),
            Decision::RequireApproval
        );
    }

    #[test]
    fn zero_threshold_means_everything_needs_approval() {
        let p = AgentPolicy {
            approval_threshold: Money::ZERO,
            ..policy()
        };
        assert_eq!(
            authorize(&propose(1), &p, &snapshot(), None),
            Decision::RequireApproval
        );
    }

    #[test]
    fn max_per_interaction_boundary() {
        let (p, s) = (policy(), snapshot());
        assert_eq!(
            authorize(&propose(100), &p, &s, None),
            Decision::RequireApproval
        );
        assert_eq!(
            authorize(&propose(101), &p, &s, None),
            deny(&[ExceedsMaxPerInteraction])
        );
    }

    #[test]
    fn aggregate_exposure_boundary() {
        let p = policy();
        let at = Snapshot {
            exposure: m(260),
            ..snapshot()
        };
        // 260 + 40 = 300: exactly at the limit.
        assert_eq!(authorize(&propose(40), &p, &at, None), Decision::Allow);
        // 260 + 41 = 301: one unit over.
        assert_eq!(
            authorize(&propose(41), &p, &at, None),
            deny(&[ExceedsAggregateExposure])
        );
        let full = Snapshot {
            exposure: m(300),
            ..snapshot()
        };
        assert_eq!(
            authorize(&propose(1), &p, &full, None),
            deny(&[ExceedsAggregateExposure])
        );
    }

    #[test]
    fn available_balance_boundary() {
        let p = policy();
        let s = Snapshot {
            available: m(30),
            ..snapshot()
        };
        assert_eq!(authorize(&propose(30), &p, &s, None), Decision::Allow);
        assert_eq!(
            authorize(&propose(31), &p, &s, None),
            deny(&[InsufficientAvailableBalance])
        );
        let empty = Snapshot {
            available: Money::ZERO,
            ..snapshot()
        };
        assert_eq!(
            authorize(&propose(1), &p, &empty, None),
            deny(&[InsufficientAvailableBalance])
        );
    }

    #[test]
    fn zero_stake_is_denied() {
        assert_eq!(
            authorize(&propose(0), &policy(), &snapshot(), None),
            deny(&[ZeroAmount])
        );
        // Even with a zero-limit policy, zero is not a valid stake.
        let locked = AgentPolicy {
            max_per_interaction: Money::ZERO,
            max_aggregate_exposure: Money::ZERO,
            approval_threshold: Money::ZERO,
            ..policy()
        };
        assert_eq!(
            authorize(&propose(0), &locked, &snapshot(), None),
            deny(&[ZeroAmount])
        );
        assert_eq!(
            authorize(&propose(1), &locked, &snapshot(), None),
            deny(&[ExceedsMaxPerInteraction, ExceedsAggregateExposure])
        );
    }

    #[test]
    fn overflowing_amounts_are_denied_not_wrapped() {
        let huge = AgentPolicy {
            max_per_interaction: Money::MAX,
            max_aggregate_exposure: Money::MAX,
            approval_threshold: Money::MAX,
            ..policy()
        };
        let s = Snapshot {
            available: Money::MAX,
            exposure: m(1),
            ..snapshot()
        };
        let intent = Intent::ProposeChallenge { stake: Money::MAX };
        assert_eq!(authorize(&intent, &huge, &s, None), deny(&[AmountOverflow]));
        // Exactly i64::MAX total is representable and allowed.
        let none = Snapshot {
            exposure: Money::ZERO,
            ..s
        };
        assert_eq!(authorize(&intent, &huge, &none, None), Decision::Allow);
        // With normal limits the overflow is reported alongside the rest.
        assert_eq!(
            authorize(&intent, &policy(), &s, None),
            deny(&[ExceedsMaxPerInteraction, AmountOverflow])
        );
    }

    #[test]
    fn interaction_type_must_be_allowed() {
        let intent = Intent::ProposeCollaboration {
            contribution: m(10),
        };
        assert_eq!(
            authorize(&intent, &policy(), &snapshot(), None),
            deny(&[InteractionTypeNotAllowed])
        );
        let both = AgentPolicy {
            allowed_interaction_types: vec![
                InteractionType::Challenge,
                InteractionType::Collaboration,
            ],
            ..policy()
        };
        assert_eq!(
            authorize(&intent, &both, &snapshot(), None),
            Decision::Allow
        );
        let collab_commit = Intent::CommitFunds {
            interaction_type: InteractionType::Collaboration,
            stake: m(10),
        };
        assert_eq!(
            authorize(&collab_commit, &policy(), &snapshot(), Some(Accepted)),
            deny(&[InteractionTypeNotAllowed])
        );
    }

    #[test]
    fn paused_agent_can_do_nothing() {
        let s = Snapshot {
            status: AgentStatus::Paused,
            ..snapshot()
        };
        assert_eq!(
            authorize(&propose(10), &policy(), &s, None),
            deny(&[AgentPaused])
        );
        assert_eq!(
            authorize(&Intent::Reject, &policy(), &s, Some(Proposed)),
            deny(&[AgentPaused])
        );
    }

    #[test]
    fn stop_loss_boundary() {
        let p = policy();
        let just_above = Snapshot {
            realized_pnl: SignedMoney::from_minor(-199),
            ..snapshot()
        };
        assert_eq!(
            authorize(&propose(10), &p, &just_above, None),
            Decision::Allow
        );
        let at = Snapshot {
            realized_pnl: SignedMoney::from_minor(-200),
            ..snapshot()
        };
        assert_eq!(
            authorize(&propose(10), &p, &at, None),
            deny(&[StopLossTriggered])
        );
        // Rejecting is still possible after the stop-loss.
        assert_eq!(
            authorize(&Intent::Reject, &p, &at, Some(Negotiating)),
            Decision::Allow
        );
        // No stop-loss configured: losses do not halt the agent.
        let none = AgentPolicy {
            stop_loss: None,
            ..policy()
        };
        let deep = Snapshot {
            realized_pnl: SignedMoney::from_minor(i64::MIN),
            ..snapshot()
        };
        assert_eq!(authorize(&propose(10), &none, &deep, None), Decision::Allow);
        assert_eq!(
            authorize(&propose(10), &p, &deep, None),
            deny(&[StopLossTriggered])
        );
    }

    #[test]
    fn interaction_state_legality() {
        use crate::interaction::ALL_STATES;
        let (p, s) = (policy(), snapshot());
        let legal = |intent: &Intent, state| {
            authorize(intent, &p, &s, state) != deny(&[IllegalInteractionState])
        };

        // Proposals create a new interaction.
        assert!(legal(&propose(10), None));
        for state in ALL_STATES {
            assert!(!legal(&propose(10), Some(state)), "{state:?}");
        }

        let accept = Intent::AcceptChallenge { stake: m(10) };
        let counter = Intent::CounterOffer { stake: m(10) };
        for intent in [accept, counter, Intent::Reject] {
            assert!(!legal(&intent, None));
            for state in ALL_STATES {
                assert_eq!(
                    legal(&intent, Some(state)),
                    matches!(state, Proposed | Negotiating),
                    "{intent:?} in {state:?}"
                );
            }
        }

        assert!(!legal(&commit(10), None));
        for state in ALL_STATES {
            assert_eq!(
                legal(&commit(10), Some(state)),
                matches!(state, Accepted | AwaitingApproval),
                "{state:?}"
            );
        }
    }

    #[test]
    fn all_deny_reasons_are_collected() {
        let p = policy();
        let s = Snapshot {
            status: AgentStatus::Paused,
            available: m(5),
            exposure: m(250),
            realized_pnl: SignedMoney::from_minor(-500),
        };
        let intent = Intent::CommitFunds {
            interaction_type: InteractionType::Collaboration,
            stake: m(150),
        };
        assert_eq!(
            authorize(&intent, &p, &s, Some(Settled)),
            deny(&[
                AgentPaused,
                IllegalInteractionState,
                InteractionTypeNotAllowed,
                StopLossTriggered,
                ExceedsMaxPerInteraction,
                ExceedsAggregateExposure,
                InsufficientAvailableBalance,
            ])
        );
    }

    #[test]
    fn deny_wins_over_approval() {
        let s = Snapshot {
            available: m(60),
            ..snapshot()
        };
        // 80 is above the approval threshold but also above the balance.
        assert_eq!(
            authorize(&propose(80), &policy(), &s, None),
            deny(&[InsufficientAvailableBalance])
        );
    }

    #[test]
    fn reject_ignores_financial_limits() {
        let s = Snapshot {
            available: Money::ZERO,
            exposure: m(300),
            ..snapshot()
        };
        assert_eq!(
            authorize(&Intent::Reject, &policy(), &s, Some(Proposed)),
            Decision::Allow
        );
    }

    #[test]
    fn intent_json_is_tagged() {
        let json = serde_json::json!({"type": "propose_challenge", "stake": "10"});
        assert_eq!(
            serde_json::from_value::<Intent>(json).unwrap(),
            propose(10)
        );
        // A numeric stake (e.g. from an LLM) is not accepted.
        let bad = serde_json::json!({"type": "propose_challenge", "stake": 10});
        assert!(serde_json::from_value::<Intent>(bad).is_err());
    }
}

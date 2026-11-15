//! Owner-defined financial limits. These are enforced by `authorize`, never
//! by a language model.

use serde::{Deserialize, Serialize};

use crate::money::Money;

pub const MAX_BPS: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    Challenge,
    Collaboration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPolicy {
    /// Largest stake in a single interaction.
    pub max_per_interaction: Money,
    /// Largest total stake across open interactions.
    pub max_aggregate_exposure: Money,
    /// Stakes up to and including this execute automatically; above it the
    /// owner must approve.
    pub approval_threshold: Money,
    pub allowed_interaction_types: Vec<InteractionType>,
    /// Owner preference in basis points (0-10000). Not a security control.
    pub min_conviction_bps: u32,
    /// Halt once realized losses reach this amount. `None` disables it.
    pub stop_loss: Option<Money>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("approval_threshold must not exceed max_per_interaction")]
    ApprovalThresholdAboveMaxPerInteraction,
    #[error("max_per_interaction must not exceed max_aggregate_exposure")]
    MaxPerInteractionAboveAggregateExposure,
    #[error("allowed_interaction_types must not be empty")]
    NoAllowedInteractionTypes,
    #[error("allowed_interaction_types must not contain duplicates")]
    DuplicateInteractionType,
    #[error("min_conviction_bps must be between 0 and 10000")]
    ConvictionOutOfRange,
    #[error("stop_loss must be greater than zero, or null to disable it")]
    ZeroStopLoss,
}

impl PolicyError {
    pub const fn code(self) -> &'static str {
        match self {
            PolicyError::ApprovalThresholdAboveMaxPerInteraction => {
                "approval_threshold_above_max_per_interaction"
            }
            PolicyError::MaxPerInteractionAboveAggregateExposure => {
                "max_per_interaction_above_aggregate_exposure"
            }
            PolicyError::NoAllowedInteractionTypes => "no_allowed_interaction_types",
            PolicyError::DuplicateInteractionType => "duplicate_interaction_type",
            PolicyError::ConvictionOutOfRange => "conviction_out_of_range",
            PolicyError::ZeroStopLoss => "zero_stop_loss",
        }
    }

    /// The `AgentPolicy` field the error is about.
    pub const fn field(self) -> &'static str {
        match self {
            PolicyError::ApprovalThresholdAboveMaxPerInteraction => "approval_threshold",
            PolicyError::MaxPerInteractionAboveAggregateExposure => "max_per_interaction",
            PolicyError::NoAllowedInteractionTypes | PolicyError::DuplicateInteractionType => {
                "allowed_interaction_types"
            }
            PolicyError::ConvictionOutOfRange => "min_conviction_bps",
            PolicyError::ZeroStopLoss => "stop_loss",
        }
    }
}

impl AgentPolicy {
    /// Returns every problem with the policy; empty means valid.
    pub fn validate(&self) -> Vec<PolicyError> {
        let mut errors = Vec::new();
        if self.approval_threshold > self.max_per_interaction {
            errors.push(PolicyError::ApprovalThresholdAboveMaxPerInteraction);
        }
        if self.max_per_interaction > self.max_aggregate_exposure {
            errors.push(PolicyError::MaxPerInteractionAboveAggregateExposure);
        }
        let types = &self.allowed_interaction_types;
        if types.is_empty() {
            errors.push(PolicyError::NoAllowedInteractionTypes);
        }
        if types
            .iter()
            .enumerate()
            .any(|(i, t)| types[..i].contains(t))
        {
            errors.push(PolicyError::DuplicateInteractionType);
        }
        if self.min_conviction_bps > MAX_BPS {
            errors.push(PolicyError::ConvictionOutOfRange);
        }
        if self.stop_loss == Some(Money::ZERO) {
            errors.push(PolicyError::ZeroStopLoss);
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    fn valid() -> AgentPolicy {
        AgentPolicy {
            max_per_interaction: m(100),
            max_aggregate_exposure: m(500),
            approval_threshold: m(50),
            allowed_interaction_types: vec![InteractionType::Challenge],
            min_conviction_bps: 6000,
            stop_loss: Some(m(200)),
        }
    }

    #[test]
    fn valid_policy_has_no_errors() {
        assert_eq!(valid().validate(), vec![]);
    }

    #[test]
    fn limits_may_be_equal() {
        let p = AgentPolicy {
            max_per_interaction: m(100),
            max_aggregate_exposure: m(100),
            approval_threshold: m(100),
            ..valid()
        };
        assert_eq!(p.validate(), vec![]);
        let all_zero = AgentPolicy {
            max_per_interaction: Money::ZERO,
            max_aggregate_exposure: Money::ZERO,
            approval_threshold: Money::ZERO,
            ..valid()
        };
        assert_eq!(all_zero.validate(), vec![]);
    }

    #[test]
    fn threshold_one_over_max_per_interaction() {
        let p = AgentPolicy {
            approval_threshold: m(101),
            ..valid()
        };
        assert_eq!(
            p.validate(),
            vec![PolicyError::ApprovalThresholdAboveMaxPerInteraction]
        );
    }

    #[test]
    fn max_per_interaction_one_over_exposure() {
        let p = AgentPolicy {
            max_per_interaction: m(501),
            ..valid()
        };
        assert_eq!(
            p.validate(),
            vec![PolicyError::MaxPerInteractionAboveAggregateExposure]
        );
    }

    #[test]
    fn allowed_types_must_be_non_empty_and_unique() {
        let empty = AgentPolicy {
            allowed_interaction_types: vec![],
            ..valid()
        };
        assert_eq!(
            empty.validate(),
            vec![PolicyError::NoAllowedInteractionTypes]
        );
        let dup = AgentPolicy {
            allowed_interaction_types: vec![InteractionType::Challenge, InteractionType::Challenge],
            ..valid()
        };
        assert_eq!(dup.validate(), vec![PolicyError::DuplicateInteractionType]);
    }

    #[test]
    fn bps_range() {
        let at = AgentPolicy {
            min_conviction_bps: 10_000,
            ..valid()
        };
        assert_eq!(at.validate(), vec![]);
        let zero = AgentPolicy {
            min_conviction_bps: 0,
            ..valid()
        };
        assert_eq!(zero.validate(), vec![]);
        let over = AgentPolicy {
            min_conviction_bps: 10_001,
            ..valid()
        };
        assert_eq!(over.validate(), vec![PolicyError::ConvictionOutOfRange]);
    }

    #[test]
    fn stop_loss_none_or_positive() {
        let none = AgentPolicy {
            stop_loss: None,
            ..valid()
        };
        assert_eq!(none.validate(), vec![]);
        let zero = AgentPolicy {
            stop_loss: Some(Money::ZERO),
            ..valid()
        };
        assert_eq!(zero.validate(), vec![PolicyError::ZeroStopLoss]);
    }

    #[test]
    fn all_errors_are_collected() {
        let p = AgentPolicy {
            max_per_interaction: m(100),
            max_aggregate_exposure: m(50),
            approval_threshold: m(200),
            allowed_interaction_types: vec![],
            min_conviction_bps: 20_000,
            stop_loss: Some(Money::ZERO),
        };
        assert_eq!(p.validate().len(), 5);
    }

    #[test]
    fn json_shape_matches_api_contract() {
        let json = serde_json::json!({
            "max_per_interaction": "100",
            "max_aggregate_exposure": "500",
            "approval_threshold": "50",
            "allowed_interaction_types": ["challenge"],
            "min_conviction_bps": 6000,
            "stop_loss": "200"
        });
        let parsed: AgentPolicy = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(parsed, valid());
        assert_eq!(serde_json::to_value(&parsed).unwrap(), json);
    }
}

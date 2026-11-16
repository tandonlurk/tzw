//! Two-agent collaboration agreements: terms, states and the payout split.
//!
//! `Proposed -> Accepted | Rejected`, `Accepted -> Funded`,
//! `Funded -> Completed`, and `Proposed | Accepted | Funded -> Cancelled`.
//! Both parties contribute a fixed amount when the agreement is funded; on
//! completion the pooled contributions are paid out by the agreed split.
//! Completion is declared by the owner (backend-administered, not trustless).

use serde::{Deserialize, Serialize};

use crate::interaction::InteractionState;
use crate::money::Money;

/// The whole pool, in basis points.
pub const BPS: u32 = 10_000;
/// Longest objective text.
pub const MAX_OBJECTIVE_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollabState {
    Proposed,
    Accepted,
    Rejected,
    Funded,
    Completed,
    Cancelled,
}

impl CollabState {
    pub const ALL: [CollabState; 6] = [
        CollabState::Proposed,
        CollabState::Accepted,
        CollabState::Rejected,
        CollabState::Funded,
        CollabState::Completed,
        CollabState::Cancelled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            CollabState::Proposed => "proposed",
            CollabState::Accepted => "accepted",
            CollabState::Rejected => "rejected",
            CollabState::Funded => "funded",
            CollabState::Completed => "completed",
            CollabState::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<CollabState> {
        CollabState::ALL.into_iter().find(|c| c.as_str() == s)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            CollabState::Rejected | CollabState::Completed | CollabState::Cancelled
        )
    }

    /// The generic interaction state `authorize` checks intents against.
    pub const fn as_interaction_state(self) -> InteractionState {
        match self {
            CollabState::Proposed => InteractionState::Proposed,
            CollabState::Accepted => InteractionState::Accepted,
            CollabState::Funded => InteractionState::Funded,
            CollabState::Completed => InteractionState::Settled,
            CollabState::Rejected | CollabState::Cancelled => InteractionState::Cancelled,
        }
    }

    /// Whether `self -> to` is a legal transition.
    pub const fn can_become(self, to: CollabState) -> bool {
        use CollabState::*;
        matches!(
            (self, to),
            (Proposed, Accepted | Rejected | Cancelled)
                | (Accepted, Funded | Cancelled)
                | (Funded, Completed | Cancelled)
        )
    }
}

/// What two agents agree on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollabTerms {
    pub objective: String,
    /// Share of the pooled contributions paid to the proposer on completion;
    /// the partner receives the rest.
    pub proposer_share_bps: u32,
    pub proposer_contribution: Money,
    pub partner_contribution: Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TermsError {
    EmptyObjective,
    ObjectiveTooLong,
    ShareOutOfRange,
    ZeroContribution,
    Overflow,
}

impl CollabTerms {
    /// Every problem, as (field, error).
    pub fn validate(&self) -> Vec<(&'static str, TermsError)> {
        let mut out = Vec::new();
        let objective = self.objective.trim();
        if objective.is_empty() {
            out.push(("objective", TermsError::EmptyObjective));
        } else if objective.chars().count() > MAX_OBJECTIVE_CHARS {
            out.push(("objective", TermsError::ObjectiveTooLong));
        }
        if self.proposer_share_bps > BPS {
            out.push(("proposer_share_bps", TermsError::ShareOutOfRange));
        }
        if self.proposer_contribution.is_zero() {
            out.push(("proposer_contribution", TermsError::ZeroContribution));
        }
        if self.partner_contribution.is_zero() {
            out.push(("partner_contribution", TermsError::ZeroContribution));
        }
        if self.pool().is_none() {
            out.push(("partner_contribution", TermsError::Overflow));
        }
        out
    }

    pub fn pool(&self) -> Option<Money> {
        self.proposer_contribution
            .checked_add(self.partner_contribution)
    }

    /// `(proposer_payout, partner_payout)`; see `split`.
    pub fn payouts(&self) -> Option<(Money, Money)> {
        split(self.pool()?, self.proposer_share_bps)
    }
}

/// Splits `pool` by `proposer_bps`: the proposer gets
/// `floor(pool * proposer_bps / 10000)`, the partner gets the remainder, so
/// the two always add up to `pool` exactly (no unit is created or lost; the
/// rounding unit goes to the partner). `None` if `proposer_bps > 10000`.
pub fn split(pool: Money, proposer_bps: u32) -> Option<(Money, Money)> {
    if proposer_bps > BPS {
        return None;
    }
    let total = i128::from(pool.minor());
    let proposer = total * i128::from(proposer_bps) / i128::from(BPS);
    let partner = total - proposer;
    Some((
        Money::from_minor(i64::try_from(proposer).ok()?)?,
        Money::from_minor(i64::try_from(partner).ok()?)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    #[test]
    fn split_never_creates_or_loses_a_unit() {
        for pool in [0, 1, 2, 3, 7, 10, 999_999, 1_000_001, i64::MAX] {
            for bps in [0, 1, 3333, 5000, 6667, 9999, 10_000] {
                let (a, b) = split(m(pool), bps).unwrap();
                assert_eq!(
                    i128::from(a.minor()) + i128::from(b.minor()),
                    i128::from(pool),
                    "{pool} @ {bps}"
                );
                assert!(a <= m(pool) && b <= m(pool));
            }
        }
    }

    #[test]
    fn rounding_goes_to_the_partner() {
        assert_eq!(split(m(10), 3333).unwrap(), (m(3), m(7)));
        assert_eq!(split(m(1), 5000).unwrap(), (m(0), m(1)));
        assert_eq!(split(m(3), 6667).unwrap(), (m(2), m(1)));
        assert_eq!(split(m(100), 0).unwrap(), (m(0), m(100)));
        assert_eq!(split(m(100), 10_000).unwrap(), (m(100), m(0)));
        assert_eq!(split(m(100), 10_001), None);
    }

    #[test]
    fn transitions() {
        use CollabState::*;
        assert!(Proposed.can_become(Accepted));
        assert!(Proposed.can_become(Rejected));
        assert!(Accepted.can_become(Funded));
        assert!(Funded.can_become(Completed));
        assert!(Funded.can_become(Cancelled));
        for terminal in [Rejected, Completed, Cancelled] {
            assert!(terminal.is_terminal());
            for to in CollabState::ALL {
                assert!(!terminal.can_become(to), "{terminal:?} -> {to:?}");
            }
        }
        assert!(!Proposed.can_become(Funded));
        assert!(!Accepted.can_become(Completed));
        assert!(!Funded.can_become(Funded));
        for s in CollabState::ALL {
            assert_eq!(CollabState::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn terms_are_validated() {
        let ok = CollabTerms {
            objective: "Research ETH funding rates".into(),
            proposer_share_bps: 6000,
            proposer_contribution: m(3),
            partner_contribution: m(7),
        };
        assert!(ok.validate().is_empty());
        assert_eq!(ok.payouts(), Some((m(6), m(4))));
        let bad = CollabTerms {
            objective: "  ".into(),
            proposer_share_bps: 10_001,
            proposer_contribution: Money::ZERO,
            partner_contribution: Money::ZERO,
        };
        let fields: Vec<&str> = bad.validate().into_iter().map(|(f, _)| f).collect();
        assert_eq!(
            fields,
            [
                "objective",
                "proposer_share_bps",
                "proposer_contribution",
                "partner_contribution"
            ]
        );
        let long = CollabTerms {
            objective: "x".repeat(MAX_OBJECTIVE_CHARS + 1),
            ..ok.clone()
        };
        assert_eq!(
            long.validate(),
            [("objective", TermsError::ObjectiveTooLong)]
        );
        let huge = CollabTerms {
            proposer_contribution: m(i64::MAX),
            partner_contribution: m(1),
            ..ok
        };
        assert_eq!(
            huge.validate(),
            [("partner_contribution", TermsError::Overflow)]
        );
    }
}

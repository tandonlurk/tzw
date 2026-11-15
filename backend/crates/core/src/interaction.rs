//! Interaction (challenge / collaboration) state machine.
//!
//! `Proposed -> Negotiating -> Accepted -> (AwaitingApproval) -> Funded ->
//! Active -> Resolving -> Settled`, with `Cancelled`, `Failed` and `Disputed`
//! as terminal branches. The backend is the source of truth for this state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionState {
    Proposed,
    Negotiating,
    Accepted,
    AwaitingApproval,
    Funded,
    Active,
    Resolving,
    Settled,
    Cancelled,
    Disputed,
    Failed,
}

use InteractionState::*;

pub const ALL_STATES: [InteractionState; 11] = [
    Proposed,
    Negotiating,
    Accepted,
    AwaitingApproval,
    Funded,
    Active,
    Resolving,
    Settled,
    Cancelled,
    Disputed,
    Failed,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("illegal interaction transition {from:?} -> {to:?}")]
pub struct IllegalTransition {
    pub from: InteractionState,
    pub to: InteractionState,
}

impl InteractionState {
    /// The transition table. Anything not listed is illegal.
    ///
    /// - Before funding, either side may walk away (`Cancelled`).
    /// - `Funded`/`Resolving -> Cancelled` is the timeout refund path.
    /// - `Disputed` is unused in the MVP and has no way out.
    pub const fn allowed_next(self) -> &'static [InteractionState] {
        match self {
            Proposed => &[Negotiating, Accepted, Cancelled],
            Negotiating => &[Accepted, Cancelled],
            Accepted => &[AwaitingApproval, Funded, Cancelled, Failed],
            AwaitingApproval => &[Funded, Cancelled, Failed],
            Funded => &[Active, Cancelled, Failed],
            Active => &[Resolving, Failed],
            Resolving => &[Settled, Cancelled, Disputed, Failed],
            Settled | Cancelled | Disputed | Failed => &[],
        }
    }

    pub const fn is_terminal(self) -> bool {
        self.allowed_next().is_empty()
    }

    pub fn can_transition_to(self, to: InteractionState) -> bool {
        self.allowed_next().contains(&to)
    }

    pub fn transition(self, to: InteractionState) -> Result<InteractionState, IllegalTransition> {
        if self.can_transition_to(to) {
            Ok(to)
        } else {
            Err(IllegalTransition { from: self, to })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_without_approval() {
        let mut s = Proposed;
        for next in [Negotiating, Accepted, Funded, Active, Resolving, Settled] {
            s = s.transition(next).unwrap();
        }
        assert_eq!(s, Settled);
        assert!(s.is_terminal());
    }

    #[test]
    fn happy_path_with_approval() {
        let mut s = Proposed;
        for next in [
            Accepted,
            AwaitingApproval,
            Funded,
            Active,
            Resolving,
            Settled,
        ] {
            s = s.transition(next).unwrap();
        }
        assert_eq!(s, Settled);
    }

    #[test]
    fn terminal_states_have_no_exits() {
        for from in [Settled, Cancelled, Disputed, Failed] {
            assert!(from.is_terminal());
            for to in ALL_STATES {
                assert_eq!(from.transition(to), Err(IllegalTransition { from, to }));
            }
        }
    }

    #[test]
    fn non_terminal_states_are_not_terminal() {
        for s in [
            Proposed,
            Negotiating,
            Accepted,
            AwaitingApproval,
            Funded,
            Active,
            Resolving,
        ] {
            assert!(!s.is_terminal(), "{s:?}");
        }
    }

    #[test]
    fn no_self_transitions() {
        for s in ALL_STATES {
            assert!(!s.can_transition_to(s), "{s:?}");
        }
    }

    #[test]
    fn nothing_returns_to_proposed() {
        for s in ALL_STATES {
            assert!(!s.can_transition_to(Proposed), "{s:?}");
        }
    }

    #[test]
    fn funds_cannot_be_skipped() {
        // Settlement is only reachable through Funded -> Active -> Resolving.
        for from in ALL_STATES {
            assert_eq!(from.can_transition_to(Settled), from == Resolving);
            assert_eq!(from.can_transition_to(Active), from == Funded);
            assert_eq!(from.can_transition_to(Resolving), from == Active);
            assert_eq!(
                from.can_transition_to(Funded),
                from == Accepted || from == AwaitingApproval,
                "{from:?}"
            );
        }
    }

    #[test]
    fn specific_illegal_transitions() {
        for (from, to) in [
            (Proposed, Funded),
            (Proposed, Settled),
            (Negotiating, Proposed),
            (Negotiating, Funded),
            (Accepted, Active),
            (Accepted, Negotiating),
            (AwaitingApproval, Active),
            (Funded, Settled),
            (Active, Settled),
            (Active, Cancelled),
            (Resolving, Active),
        ] {
            assert_eq!(
                from.transition(to),
                Err(IllegalTransition { from, to }),
                "{from:?} -> {to:?}"
            );
        }
    }

    #[test]
    fn exact_table() {
        let count: usize = ALL_STATES.iter().map(|s| s.allowed_next().len()).sum();
        assert_eq!(count, 3 + 2 + 4 + 3 + 3 + 2 + 4);
    }

    #[test]
    fn serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&AwaitingApproval).unwrap(),
            "\"awaiting_approval\""
        );
    }
}

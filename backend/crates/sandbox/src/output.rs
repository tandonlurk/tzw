//! Strict parsing of the guest's stdout (D-41).
//!
//! The guest's JSON is deserialised into a mirror of `aen_core::authorize::Intent`
//! that rejects unknown fields and accepts money only as a canonical string of
//! 1-18 digits, then converted into the core type. `core` itself is unchanged.

use std::str::FromStr;

use aen_core::authorize::Intent;
use aen_core::money::Money;
use aen_core::policy::InteractionType;
use serde::{Deserialize, Deserializer};

use crate::{Limits, RunOutput, SandboxError};

/// Longest accepted money string. 18 digits always fit an `i64`.
const MAX_MONEY_DIGITS: usize = 18;
/// Longest guest error message kept.
const MAX_ERROR_CHARS: usize = 500;

/// Money as the guest must write it: a canonical decimal string of minor units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StrictMoney(Money);

impl<'de> Deserialize<'de> for StrictMoney {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() > MAX_MONEY_DIGITS {
            return Err(serde::de::Error::custom(
                "money must be a string of 1-18 digits (minor units)",
            ));
        }
        Money::from_str(&s)
            .map(StrictMoney)
            .map_err(|_| serde::de::Error::custom("money must be a string of 1-18 digits (minor units), no sign, decimal point or leading zeros"))
    }
}

/// Mirror of `aen_core::authorize::Intent` with `deny_unknown_fields`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum StrictIntent {
    ProposeChallenge {
        stake: StrictMoney,
    },
    AcceptChallenge {
        stake: StrictMoney,
    },
    CounterOffer {
        stake: StrictMoney,
    },
    /// A struct variant, not a unit one: serde ignores extra fields on an
    /// internally tagged unit variant even with `deny_unknown_fields`.
    Reject {},
    CommitFunds {
        interaction_type: InteractionType,
        stake: StrictMoney,
    },
    ProposeCollaboration {
        contribution: StrictMoney,
    },
}

impl From<StrictIntent> for Intent {
    fn from(i: StrictIntent) -> Intent {
        match i {
            StrictIntent::ProposeChallenge { stake } => Intent::ProposeChallenge { stake: stake.0 },
            StrictIntent::AcceptChallenge { stake } => Intent::AcceptChallenge { stake: stake.0 },
            StrictIntent::CounterOffer { stake } => Intent::CounterOffer { stake: stake.0 },
            StrictIntent::Reject {} => Intent::Reject,
            StrictIntent::CommitFunds {
                interaction_type,
                stake,
            } => Intent::CommitFunds {
                interaction_type,
                stake: stake.0,
            },
            StrictIntent::ProposeCollaboration { contribution } => Intent::ProposeCollaboration {
                contribution: contribution.0,
            },
        }
    }
}

/// Never called: an exhaustive match so that a new `core::Intent` variant
/// fails to compile here until the mirror above is updated.
#[allow(dead_code)]
fn mirror_is_exhaustive(i: Intent) -> StrictIntent {
    let m = StrictMoney;
    match i {
        Intent::ProposeChallenge { stake } => StrictIntent::ProposeChallenge { stake: m(stake) },
        Intent::AcceptChallenge { stake } => StrictIntent::AcceptChallenge { stake: m(stake) },
        Intent::CounterOffer { stake } => StrictIntent::CounterOffer { stake: m(stake) },
        Intent::Reject => StrictIntent::Reject {},
        Intent::CommitFunds {
            interaction_type,
            stake,
        } => StrictIntent::CommitFunds {
            interaction_type,
            stake: m(stake),
        },
        Intent::ProposeCollaboration { contribution } => StrictIntent::ProposeCollaboration {
            contribution: m(contribution),
        },
    }
}

/// What `guest/runner.js` writes to stdout.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    intents: Option<Vec<StrictIntent>>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    logs: Vec<String>,
}

/// Parses and strictly validates the guest's stdout. `fuel_used` is left at 0.
pub fn validate_output(bytes: &[u8], limits: &Limits) -> Result<RunOutput, SandboxError> {
    if bytes.len() > limits.max_output_bytes {
        return Err(SandboxError::OutputTooLarge);
    }
    let env: Envelope =
        serde_json::from_slice(bytes).map_err(|e| SandboxError::BadOutput(e.to_string()))?;
    if !env.ok {
        let msg = env
            .error
            .unwrap_or_default()
            .chars()
            .take(MAX_ERROR_CHARS)
            .collect();
        return Err(SandboxError::Guest(msg));
    }
    let intents = env
        .intents
        .ok_or_else(|| SandboxError::BadOutput("decide must return an array of intents".into()))?;
    if intents.len() > limits.max_intents {
        return Err(SandboxError::BadOutput(format!(
            "too many intents: {} > {}",
            intents.len(),
            limits.max_intents
        )));
    }
    let logs = env
        .logs
        .into_iter()
        .take(limits.max_logs)
        .map(|l| l.chars().take(limits.max_log_chars).collect())
        .collect();
    Ok(RunOutput {
        intents: intents.into_iter().map(Intent::from).collect(),
        logs,
        fuel_used: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<RunOutput, SandboxError> {
        validate_output(json.as_bytes(), &Limits::default())
    }

    #[test]
    fn money_must_be_canonical_and_at_most_18_digits() {
        let ok = parse(
            r#"{"ok":true,"intents":[{"type":"propose_challenge","stake":"999999999999999999"}]}"#,
        )
        .unwrap();
        assert_eq!(
            ok.intents,
            vec![Intent::ProposeChallenge {
                stake: Money::from_minor(999_999_999_999_999_999).unwrap()
            }]
        );
        for bad in ["1000000000000000000", "007", "", "1.5", "+1", " 1", "-1"] {
            let json = format!(
                r#"{{"ok":true,"intents":[{{"type":"propose_challenge","stake":"{bad}"}}]}}"#
            );
            assert!(
                matches!(parse(&json), Err(SandboxError::BadOutput(_))),
                "accepted {bad:?}"
            );
        }
    }

    #[test]
    fn every_variant_converts() {
        let out = parse(
            r#"{"ok":true,"logs":["a"],"intents":[
                {"type":"propose_challenge","stake":"1"},
                {"type":"accept_challenge","stake":"2"},
                {"type":"counter_offer","stake":"3"},
                {"type":"reject"},
                {"type":"commit_funds","interaction_type":"collaboration","stake":"4"},
                {"type":"propose_collaboration","contribution":"0"}]}"#,
        )
        .unwrap();
        let m = |n| Money::from_minor(n).unwrap();
        assert_eq!(
            out.intents,
            vec![
                Intent::ProposeChallenge { stake: m(1) },
                Intent::AcceptChallenge { stake: m(2) },
                Intent::CounterOffer { stake: m(3) },
                Intent::Reject,
                Intent::CommitFunds {
                    interaction_type: InteractionType::Collaboration,
                    stake: m(4)
                },
                Intent::ProposeCollaboration { contribution: m(0) },
            ]
        );
        assert_eq!(out.logs, vec!["a"]);
    }

    #[test]
    fn reject_takes_no_fields() {
        assert!(matches!(
            parse(r#"{"ok":true,"intents":[{"type":"reject","stake":"1"}]}"#),
            Err(SandboxError::BadOutput(_))
        ));
    }
}

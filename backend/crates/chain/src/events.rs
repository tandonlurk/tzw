//! Decoding of `ChallengeEscrow` and `VaultFactory` logs as delivered by an
//! external feed (QuickNode Streams). Pure functions: no RPC, no keys.
//!
//! The caller decides which contract a log came from (by its address) and
//! passes the matching [`Source`]; a log whose first topic is not one of that
//! contract's events, or whose topics/data do not fit the event, is `None`.

use alloy::primitives::{Address, B256, Bytes, LogData, U256};
use alloy::sol;
use alloy::sol_types::SolEvent;
use std::str::FromStr;
use uuid::Uuid;

sol! {
    interface EscrowEvents {
        event Created(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 stake, uint64 fundDeadline, uint64 resolveDeadline);
        event Deposited(bytes32 indexed id, address indexed party, uint256 amount);
        event Activated(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 totalStaked);
        event Resolved(bytes32 indexed id, address indexed winner, address indexed loser, uint256 payout);
        event Voided(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 refundA, uint256 refundB);
        event Refunded(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 refundA, uint256 refundB);
        event PayoutDeferred(bytes32 indexed id, address indexed party, uint256 amount);
        event Claimed(bytes32 indexed id, address indexed party, uint256 amount);
    }

    interface FactoryEvents {
        event VaultCreated(address indexed vault, address indexed owner, address indexed operator, bytes32 agentRef, uint256 maxPerChallenge, uint256 maxExposure);
    }

    interface VaultEvents {
        event Staked(bytes32 indexed id, uint256 amount);
        event Released(bytes32 indexed id, uint256 amount, uint8 status);
    }
}

/// Which contract emitted the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Escrow,
    Factory,
    /// An `AgentVault` (any vault the caller recognizes).
    Vault,
}

/// A decoded event. Addresses are lowercase `0x` hex; amounts are USDC base
/// units as decimal strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractEvent {
    Created {
        id: B256,
        party_a: String,
        party_b: String,
        stake: String,
        fund_deadline: u64,
        resolve_deadline: u64,
    },
    Deposited {
        id: B256,
        party: String,
        amount: String,
    },
    Activated {
        id: B256,
        party_a: String,
        party_b: String,
        total_staked: String,
    },
    Resolved {
        id: B256,
        winner: String,
        loser: String,
        payout: String,
    },
    Voided {
        id: B256,
        party_a: String,
        party_b: String,
        refund_a: String,
        refund_b: String,
    },
    Refunded {
        id: B256,
        party_a: String,
        party_b: String,
        refund_a: String,
        refund_b: String,
    },
    PayoutDeferred {
        id: B256,
        party: String,
        amount: String,
    },
    Claimed {
        id: B256,
        party: String,
        amount: String,
    },
    VaultCreated {
        vault: String,
        owner: String,
        operator: String,
        agent_ref: B256,
        max_per_challenge: String,
        max_exposure: String,
    },
    /// The vault staked `amount` into escrow challenge `id`.
    Staked { id: B256, amount: String },
    /// The vault freed its exposure for a final challenge (`status` is the
    /// escrow status: 3 resolved, 4 voided, 5 refunded). Moves no funds.
    Released {
        id: B256,
        amount: String,
        status: u8,
    },
}

fn addr(a: Address) -> String {
    format!("{a:#x}")
}

fn amount(v: U256) -> String {
    v.to_string()
}

impl ContractEvent {
    /// The Solidity event name.
    pub fn name(&self) -> &'static str {
        match self {
            ContractEvent::Created { .. } => "Created",
            ContractEvent::Deposited { .. } => "Deposited",
            ContractEvent::Activated { .. } => "Activated",
            ContractEvent::Resolved { .. } => "Resolved",
            ContractEvent::Voided { .. } => "Voided",
            ContractEvent::Refunded { .. } => "Refunded",
            ContractEvent::PayoutDeferred { .. } => "PayoutDeferred",
            ContractEvent::Claimed { .. } => "Claimed",
            ContractEvent::VaultCreated { .. } => "VaultCreated",
            ContractEvent::Staked { .. } => "Staked",
            ContractEvent::Released { .. } => "Released",
        }
    }

    /// The escrow challenge id, for escrow events.
    pub fn challenge(&self) -> Option<B256> {
        match self {
            ContractEvent::Created { id, .. }
            | ContractEvent::Deposited { id, .. }
            | ContractEvent::Activated { id, .. }
            | ContractEvent::Resolved { id, .. }
            | ContractEvent::Voided { id, .. }
            | ContractEvent::Refunded { id, .. }
            | ContractEvent::PayoutDeferred { id, .. }
            | ContractEvent::Claimed { id, .. }
            | ContractEvent::Staked { id, .. }
            | ContractEvent::Released { id, .. } => Some(*id),
            ContractEvent::VaultCreated { .. } => None,
        }
    }
}

/// The interaction a challenge id was derived from (`challenge_id`), if the
/// id has that shape (16 zero bytes, then a UUID).
pub fn interaction_of(id: B256) -> Option<Uuid> {
    let (pad, rest) = id.as_slice().split_at(16);
    pad.iter()
        .all(|b| *b == 0)
        .then(|| Uuid::from_slice(rest).ok())?
}

/// Parses hex topics and data, then decodes. `None` for anything that is not
/// a well-formed event of `source`.
pub fn decode(source: Source, topics: &[String], data_hex: &str) -> Option<ContractEvent> {
    let topics = topics
        .iter()
        .map(|t| B256::from_str(t).ok())
        .collect::<Option<Vec<_>>>()?;
    let data = Bytes::from_str(data_hex).ok()?;
    let log = LogData::new(topics, data)?;
    let t0 = *log.topics().first()?;
    use ContractEvent as E;
    use EscrowEvents as X;
    Some(match source {
        Source::Escrow => match t0 {
            X::Created::SIGNATURE_HASH => {
                let e = X::Created::decode_log_data(&log).ok()?;
                E::Created {
                    id: e.id,
                    party_a: addr(e.partyA),
                    party_b: addr(e.partyB),
                    stake: amount(e.stake),
                    fund_deadline: e.fundDeadline,
                    resolve_deadline: e.resolveDeadline,
                }
            }
            X::Deposited::SIGNATURE_HASH => {
                let e = X::Deposited::decode_log_data(&log).ok()?;
                E::Deposited {
                    id: e.id,
                    party: addr(e.party),
                    amount: amount(e.amount),
                }
            }
            X::Activated::SIGNATURE_HASH => {
                let e = X::Activated::decode_log_data(&log).ok()?;
                E::Activated {
                    id: e.id,
                    party_a: addr(e.partyA),
                    party_b: addr(e.partyB),
                    total_staked: amount(e.totalStaked),
                }
            }
            X::Resolved::SIGNATURE_HASH => {
                let e = X::Resolved::decode_log_data(&log).ok()?;
                E::Resolved {
                    id: e.id,
                    winner: addr(e.winner),
                    loser: addr(e.loser),
                    payout: amount(e.payout),
                }
            }
            X::Voided::SIGNATURE_HASH => {
                let e = X::Voided::decode_log_data(&log).ok()?;
                E::Voided {
                    id: e.id,
                    party_a: addr(e.partyA),
                    party_b: addr(e.partyB),
                    refund_a: amount(e.refundA),
                    refund_b: amount(e.refundB),
                }
            }
            X::Refunded::SIGNATURE_HASH => {
                let e = X::Refunded::decode_log_data(&log).ok()?;
                E::Refunded {
                    id: e.id,
                    party_a: addr(e.partyA),
                    party_b: addr(e.partyB),
                    refund_a: amount(e.refundA),
                    refund_b: amount(e.refundB),
                }
            }
            X::PayoutDeferred::SIGNATURE_HASH => {
                let e = X::PayoutDeferred::decode_log_data(&log).ok()?;
                E::PayoutDeferred {
                    id: e.id,
                    party: addr(e.party),
                    amount: amount(e.amount),
                }
            }
            X::Claimed::SIGNATURE_HASH => {
                let e = X::Claimed::decode_log_data(&log).ok()?;
                E::Claimed {
                    id: e.id,
                    party: addr(e.party),
                    amount: amount(e.amount),
                }
            }
            _ => return None,
        },
        Source::Vault => match t0 {
            VaultEvents::Staked::SIGNATURE_HASH => {
                let e = VaultEvents::Staked::decode_log_data(&log).ok()?;
                E::Staked {
                    id: e.id,
                    amount: amount(e.amount),
                }
            }
            VaultEvents::Released::SIGNATURE_HASH => {
                let e = VaultEvents::Released::decode_log_data(&log).ok()?;
                E::Released {
                    id: e.id,
                    amount: amount(e.amount),
                    status: e.status,
                }
            }
            _ => return None,
        },
        Source::Factory => {
            if t0 != FactoryEvents::VaultCreated::SIGNATURE_HASH {
                return None;
            }
            let e = FactoryEvents::VaultCreated::decode_log_data(&log).ok()?;
            E::VaultCreated {
                vault: addr(e.vault),
                owner: addr(e.owner),
                operator: addr(e.operator),
                agent_ref: e.agentRef,
                max_per_challenge: amount(e.maxPerChallenge),
                max_exposure: amount(e.maxExposure),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(hex: &str) -> String {
        format!("0x{hex:0>64}")
    }

    #[test]
    fn decodes_a_deposit_and_maps_the_id_back_to_the_interaction() {
        let interaction = Uuid::parse_str("838ca4b6-2e3b-46f3-9cad-6b81b1fb760c").unwrap();
        let id = crate::challenge_id(interaction);
        let topics = vec![
            format!("{:#x}", EscrowEvents::Deposited::SIGNATURE_HASH),
            format!("{id:#x}"),
            word("f78499779bcc48f4e96d34a06d6ae4869cc5b37f"),
        ];
        let ev = decode(Source::Escrow, &topics, &word("2dc6c0")).unwrap();
        assert_eq!(ev.name(), "Deposited");
        assert_eq!(
            ev,
            ContractEvent::Deposited {
                id,
                party: "0xf78499779bcc48f4e96d34a06d6ae4869cc5b37f".into(),
                amount: "3000000".into(),
            }
        );
        assert_eq!(interaction_of(id), Some(interaction));
        // Wrong contract, short data, unknown topic, a non-UUID id.
        assert!(decode(Source::Factory, &topics, &word("2dc6c0")).is_none());
        assert!(decode(Source::Escrow, &topics, "0x").is_none());
        let mut unknown = topics.clone();
        unknown[0] = word("1");
        assert!(decode(Source::Escrow, &unknown, &word("2dc6c0")).is_none());
        assert_eq!(interaction_of(B256::repeat_byte(1)), None);
    }
}

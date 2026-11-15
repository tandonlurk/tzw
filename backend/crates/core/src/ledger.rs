//! Double-entry ledger rules.
//!
//! Every movement of funds is a `PostingSet`: postings that sum to zero, with
//! a reason, an environment and an idempotency key. No account except
//! `external` may go negative. The functions here only *produce and check*
//! posting sets; persisting them is the server's job.
//!
//! Account model (default, see tasks/backend.md): a stake stays in the agent's
//! own `committed` account from commit until the interaction ends. On
//! settlement both stakes pass through `escrow:{interaction}` to the winner,
//! so the escrow account records the pot and always nets to zero.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::money::{Money, SignedMoney};

/// Where the funds live. Simulated funds are never real funds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Env {
    ArcTestnet,
    Simulated,
}

impl Env {
    pub const fn as_str(self) -> &'static str {
        match self {
            Env::ArcTestnet => "arc_testnet",
            Env::Simulated => "simulated",
        }
    }
}

impl FromStr for Env {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "arc_testnet" => Ok(Env::ArcTestnet),
            "simulated" => Ok(Env::Simulated),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Account {
    /// Owner funds not assigned to any agent.
    UserUnallocated,
    /// Funds an agent may commit.
    AgentAvailable(Uuid),
    /// Funds an agent has staked in open interactions.
    AgentCommitted(Uuid),
    /// The pot of one interaction.
    Escrow(Uuid),
    /// The outside world (faucet, deposits). The only account allowed to go negative.
    External,
}

impl Account {
    pub const fn agent_id(self) -> Option<Uuid> {
        match self {
            Account::AgentAvailable(id) | Account::AgentCommitted(id) => Some(id),
            _ => None,
        }
    }

    pub const fn may_go_negative(self) -> bool {
        matches!(self, Account::External)
    }
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Account::UserUnallocated => f.write_str("user:unallocated"),
            Account::AgentAvailable(id) => write!(f, "agent:{id}:available"),
            Account::AgentCommitted(id) => write!(f, "agent:{id}:committed"),
            Account::Escrow(id) => write!(f, "escrow:{id}"),
            Account::External => f.write_str("external"),
        }
    }
}

impl FromStr for Account {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        let id = |s: &str| Uuid::parse_str(s).map_err(|_| ());
        match parts.as_slice() {
            ["user", "unallocated"] => Ok(Account::UserUnallocated),
            ["external"] => Ok(Account::External),
            ["agent", agent, "available"] => Ok(Account::AgentAvailable(id(agent)?)),
            ["agent", agent, "committed"] => Ok(Account::AgentCommitted(id(agent)?)),
            ["escrow", interaction] => Ok(Account::Escrow(id(interaction)?)),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    Deposit,
    Allocate,
    Deallocate,
    CommitStake,
    Settle,
    Refund,
}

impl Reason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Reason::Deposit => "deposit",
            Reason::Allocate => "allocate",
            Reason::Deallocate => "deallocate",
            Reason::CommitStake => "commit_stake",
            Reason::Settle => "settle",
            Reason::Refund => "refund",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Posting {
    pub account: Account,
    pub delta: SignedMoney,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    #[error("amount must be greater than zero")]
    ZeroAmount,
    #[error("idempotency key must not be empty")]
    EmptyIdempotencyKey,
    #[error("posting set has no postings")]
    NoPostings,
    #[error("postings do not sum to zero")]
    Unbalanced,
    #[error("amount overflow")]
    Overflow,
    #[error("winner and loser must be different agents")]
    SameParty,
    #[error("insufficient funds in {account}")]
    InsufficientFunds { account: Account },
}

/// A balanced set of postings. Can only be built through `new` or the
/// constructor functions below, so an instance is always balanced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingSet {
    env: Env,
    reason: Reason,
    idempotency_key: String,
    interaction_id: Option<Uuid>,
    postings: Vec<Posting>,
}

impl PostingSet {
    pub fn new(
        env: Env,
        reason: Reason,
        idempotency_key: &str,
        interaction_id: Option<Uuid>,
        postings: Vec<Posting>,
    ) -> Result<PostingSet, LedgerError> {
        if idempotency_key.trim().is_empty() {
            return Err(LedgerError::EmptyIdempotencyKey);
        }
        if postings.is_empty() {
            return Err(LedgerError::NoPostings);
        }
        if postings.iter().any(|p| p.delta.is_zero()) {
            return Err(LedgerError::ZeroAmount);
        }
        let total: i128 = net(&postings)?
            .values()
            .map(|d| i128::from(d.minor()))
            .sum();
        if total != 0 {
            return Err(LedgerError::Unbalanced);
        }
        Ok(PostingSet {
            env,
            reason,
            idempotency_key: idempotency_key.to_owned(),
            interaction_id,
            postings,
        })
    }

    pub fn env(&self) -> Env {
        self.env
    }

    pub fn reason(&self) -> Reason {
        self.reason
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn interaction_id(&self) -> Option<Uuid> {
        self.interaction_id
    }

    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    /// Net change per account.
    pub fn net_deltas(&self) -> BTreeMap<Account, SignedMoney> {
        // Cannot fail: `new` already computed the same sums.
        net(&self.postings).unwrap_or_default()
    }

    /// Checks that applying this set to `balances` (missing = zero) leaves no
    /// account other than `external` negative.
    pub fn check_balances(
        &self,
        balances: &BTreeMap<Account, SignedMoney>,
    ) -> Result<(), LedgerError> {
        for (account, delta) in self.net_deltas() {
            if account.may_go_negative() {
                continue;
            }
            let current = balances.get(&account).copied().unwrap_or_default();
            let after = current.checked_add(delta).ok_or(LedgerError::Overflow)?;
            if after.is_negative() {
                return Err(LedgerError::InsufficientFunds { account });
            }
        }
        Ok(())
    }
}

fn net(postings: &[Posting]) -> Result<BTreeMap<Account, SignedMoney>, LedgerError> {
    let mut out: BTreeMap<Account, SignedMoney> = BTreeMap::new();
    for p in postings {
        let entry = out.entry(p.account).or_default();
        *entry = entry.checked_add(p.delta).ok_or(LedgerError::Overflow)?;
    }
    Ok(out)
}

fn transfer(from: Account, to: Account, amount: Money) -> [Posting; 2] {
    [
        Posting {
            account: from,
            delta: amount.debit(),
        },
        Posting {
            account: to,
            delta: amount.credit(),
        },
    ]
}

fn positive(amount: Money) -> Result<Money, LedgerError> {
    if amount.is_zero() {
        Err(LedgerError::ZeroAmount)
    } else {
        Ok(amount)
    }
}

/// Funds enter from outside into the owner's unallocated balance.
pub fn deposit(env: Env, amount: Money, key: &str) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    let postings = transfer(Account::External, Account::UserUnallocated, amount);
    PostingSet::new(env, Reason::Deposit, key, None, postings.into())
}

/// Owner assigns unallocated funds to an agent.
pub fn allocate(
    env: Env,
    agent: Uuid,
    amount: Money,
    key: &str,
) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    let postings = transfer(
        Account::UserUnallocated,
        Account::AgentAvailable(agent),
        amount,
    );
    PostingSet::new(env, Reason::Allocate, key, None, postings.into())
}

/// Owner takes available funds back from an agent.
pub fn deallocate(
    env: Env,
    agent: Uuid,
    amount: Money,
    key: &str,
) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    let postings = transfer(
        Account::AgentAvailable(agent),
        Account::UserUnallocated,
        amount,
    );
    PostingSet::new(env, Reason::Deallocate, key, None, postings.into())
}

/// An agent locks its stake for an interaction.
///
/// The idempotency key of `commit_stake`, `settle` and `refund` is derived
/// from the interaction (and agent), never chosen by the caller, so a store
/// that deduplicates on the key cannot apply any of them twice (SEC-03).
pub fn commit_stake(
    env: Env,
    agent: Uuid,
    interaction: Uuid,
    stake: Money,
) -> Result<PostingSet, LedgerError> {
    let stake = positive(stake)?;
    let postings = transfer(
        Account::AgentAvailable(agent),
        Account::AgentCommitted(agent),
        stake,
    );
    PostingSet::new(
        env,
        Reason::CommitStake,
        &format!("commit_stake:{interaction}:{agent}"),
        Some(interaction),
        postings.into(),
    )
}

/// Equal-stake settlement: both committed stakes move into the interaction's
/// escrow and the whole pot is paid to the winner's available balance.
/// Net effect: winner +stake, loser -stake.
pub fn settle(
    env: Env,
    interaction: Uuid,
    winner: Uuid,
    loser: Uuid,
    stake: Money,
) -> Result<PostingSet, LedgerError> {
    let stake = positive(stake)?;
    if winner == loser {
        return Err(LedgerError::SameParty);
    }
    let pot = stake.checked_add(stake).ok_or(LedgerError::Overflow)?;
    let escrow = Account::Escrow(interaction);
    let mut postings = Vec::with_capacity(6);
    postings.extend(transfer(Account::AgentCommitted(loser), escrow, stake));
    postings.extend(transfer(Account::AgentCommitted(winner), escrow, stake));
    postings.extend(transfer(escrow, Account::AgentAvailable(winner), pot));
    PostingSet::new(
        env,
        Reason::Settle,
        &format!("settle:{interaction}"),
        Some(interaction),
        postings,
    )
}

/// An agent's stake returns to its available balance (cancellation or timeout).
pub fn refund(
    env: Env,
    agent: Uuid,
    interaction: Uuid,
    stake: Money,
) -> Result<PostingSet, LedgerError> {
    let stake = positive(stake)?;
    let postings = transfer(
        Account::AgentCommitted(agent),
        Account::AgentAvailable(agent),
        stake,
    );
    PostingSet::new(
        env,
        Reason::Refund,
        &format!("refund:{interaction}:{agent}"),
        Some(interaction),
        postings.into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: Env = Env::Simulated;

    fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    fn s(units: i64) -> SignedMoney {
        SignedMoney::from_minor(units)
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// Applies a set to balances the way the server does: check, then add.
    fn apply(
        balances: &mut BTreeMap<Account, SignedMoney>,
        set: &PostingSet,
    ) -> Result<(), LedgerError> {
        set.check_balances(balances)?;
        for (account, delta) in set.net_deltas() {
            let b = balances.entry(account).or_default();
            *b = b.checked_add(delta).unwrap();
        }
        Ok(())
    }

    fn bal(balances: &BTreeMap<Account, SignedMoney>, account: Account) -> i64 {
        balances.get(&account).copied().unwrap_or_default().minor()
    }

    fn sum(set: &PostingSet) -> i128 {
        set.postings()
            .iter()
            .map(|p| i128::from(p.delta.minor()))
            .sum()
    }

    #[test]
    fn account_names_round_trip() {
        let a = id(1);
        for account in [
            Account::UserUnallocated,
            Account::External,
            Account::AgentAvailable(a),
            Account::AgentCommitted(a),
            Account::Escrow(a),
        ] {
            assert_eq!(account.to_string().parse::<Account>(), Ok(account));
        }
        assert_eq!(
            Account::AgentAvailable(a).to_string(),
            "agent:00000000-0000-0000-0000-000000000001:available"
        );
        for bad in [
            "",
            "user",
            "agent:x:available",
            "agent::committed",
            "escrow:",
        ] {
            assert!(bad.parse::<Account>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn every_constructor_is_balanced_and_tagged() {
        let (a, b, i) = (id(1), id(2), id(9));
        let sets = [
            deposit(ENV, m(100), "k1").unwrap(),
            allocate(ENV, a, m(100), "k2").unwrap(),
            deallocate(ENV, a, m(100), "k3").unwrap(),
            commit_stake(ENV, a, i, m(100)).unwrap(),
            settle(ENV, i, a, b, m(100)).unwrap(),
            refund(ENV, a, i, m(100)).unwrap(),
        ];
        for set in &sets {
            assert_eq!(sum(set), 0, "{:?}", set.reason());
            assert_eq!(set.env(), ENV);
            assert!(!set.idempotency_key().is_empty());
        }
        assert_eq!(sets[0].interaction_id(), None);
        assert_eq!(sets[3].interaction_id(), Some(i));
        assert_eq!(sets[4].interaction_id(), Some(i));
        assert_eq!(sets[5].interaction_id(), Some(i));
    }

    #[test]
    fn zero_amounts_are_rejected() {
        let (a, b, i) = (id(1), id(2), id(9));
        let z = Money::ZERO;
        assert_eq!(deposit(ENV, z, "k"), Err(LedgerError::ZeroAmount));
        assert_eq!(allocate(ENV, a, z, "k"), Err(LedgerError::ZeroAmount));
        assert_eq!(deallocate(ENV, a, z, "k"), Err(LedgerError::ZeroAmount));
        assert_eq!(commit_stake(ENV, a, i, z), Err(LedgerError::ZeroAmount));
        assert_eq!(settle(ENV, i, a, b, z), Err(LedgerError::ZeroAmount));
        assert_eq!(refund(ENV, a, i, z), Err(LedgerError::ZeroAmount));
    }

    #[test]
    fn empty_idempotency_key_is_rejected() {
        assert_eq!(
            deposit(ENV, m(1), ""),
            Err(LedgerError::EmptyIdempotencyKey)
        );
        assert_eq!(
            deposit(ENV, m(1), "  "),
            Err(LedgerError::EmptyIdempotencyKey)
        );
    }

    #[test]
    fn new_rejects_unbalanced_empty_and_zero_postings() {
        let p = |account, delta| Posting {
            account,
            delta: s(delta),
        };
        assert_eq!(
            PostingSet::new(ENV, Reason::Deposit, "k", None, vec![]),
            Err(LedgerError::NoPostings)
        );
        assert_eq!(
            PostingSet::new(
                ENV,
                Reason::Deposit,
                "k",
                None,
                vec![p(Account::External, -5), p(Account::UserUnallocated, 6)]
            ),
            Err(LedgerError::Unbalanced)
        );
        assert_eq!(
            PostingSet::new(
                ENV,
                Reason::Deposit,
                "k",
                None,
                vec![p(Account::External, 0), p(Account::UserUnallocated, 0)]
            ),
            Err(LedgerError::ZeroAmount)
        );
        assert_eq!(
            PostingSet::new(
                ENV,
                Reason::Deposit,
                "k",
                None,
                vec![
                    p(Account::UserUnallocated, i64::MAX),
                    p(Account::UserUnallocated, 1),
                    p(Account::External, i64::MIN),
                ]
            ),
            Err(LedgerError::Overflow)
        );
    }

    #[test]
    fn settle_rejects_same_party_and_overflowing_pot() {
        let (a, b, i) = (id(1), id(2), id(9));
        assert_eq!(settle(ENV, i, a, a, m(1)), Err(LedgerError::SameParty));
        assert_eq!(settle(ENV, i, a, b, Money::MAX), Err(LedgerError::Overflow));
    }

    #[test]
    fn external_may_go_negative_but_nothing_else() {
        let a = id(1);
        let mut balances = BTreeMap::new();
        apply(&mut balances, &deposit(ENV, m(100), "k1").unwrap()).unwrap();
        assert_eq!(bal(&balances, Account::External), -100);
        assert_eq!(bal(&balances, Account::UserUnallocated), 100);

        // Exactly the balance is fine, one unit more is not.
        assert_eq!(
            allocate(ENV, a, m(101), "k2")
                .unwrap()
                .check_balances(&balances),
            Err(LedgerError::InsufficientFunds {
                account: Account::UserUnallocated
            })
        );
        apply(&mut balances, &allocate(ENV, a, m(100), "k3").unwrap()).unwrap();
        assert_eq!(bal(&balances, Account::UserUnallocated), 0);
        assert_eq!(bal(&balances, Account::AgentAvailable(a)), 100);

        assert_eq!(
            deallocate(ENV, a, m(101), "k4")
                .unwrap()
                .check_balances(&balances),
            Err(LedgerError::InsufficientFunds {
                account: Account::AgentAvailable(a)
            })
        );
    }

    #[test]
    fn full_lifecycle_win_and_loss() {
        let (a, b, i) = (id(1), id(2), id(9));
        let mut balances = BTreeMap::new();
        apply(&mut balances, &deposit(ENV, m(1000), "d").unwrap()).unwrap();
        apply(&mut balances, &allocate(ENV, a, m(300), "aa").unwrap()).unwrap();
        apply(&mut balances, &allocate(ENV, b, m(300), "ab").unwrap()).unwrap();

        // Cannot commit more than available.
        assert_eq!(
            commit_stake(ENV, a, i, m(301))
                .unwrap()
                .check_balances(&balances),
            Err(LedgerError::InsufficientFunds {
                account: Account::AgentAvailable(a)
            })
        );
        apply(&mut balances, &commit_stake(ENV, a, i, m(100)).unwrap()).unwrap();

        // Cannot settle before both sides have committed.
        assert_eq!(
            settle(ENV, i, a, b, m(100))
                .unwrap()
                .check_balances(&balances),
            Err(LedgerError::InsufficientFunds {
                account: Account::AgentCommitted(b)
            })
        );
        apply(&mut balances, &commit_stake(ENV, b, i, m(100)).unwrap()).unwrap();
        assert_eq!(bal(&balances, Account::AgentAvailable(a)), 200);
        assert_eq!(bal(&balances, Account::AgentCommitted(a)), 100);

        let set = settle(ENV, i, a, b, m(100)).unwrap();
        apply(&mut balances, &set).unwrap();
        // Winner: stake back plus the loser's stake. Loser: stake gone.
        assert_eq!(bal(&balances, Account::AgentAvailable(a)), 400);
        assert_eq!(bal(&balances, Account::AgentCommitted(a)), 0);
        assert_eq!(bal(&balances, Account::AgentAvailable(b)), 200);
        assert_eq!(bal(&balances, Account::AgentCommitted(b)), 0);
        assert_eq!(bal(&balances, Account::Escrow(i)), 0);

        // Per-agent net of the settle set is the realized P&L.
        let nets = set.net_deltas();
        let agent_net = |agent| -> i64 {
            nets.iter()
                .filter(|(acct, _)| acct.agent_id() == Some(agent))
                .map(|(_, d)| d.minor())
                .sum()
        };
        assert_eq!(agent_net(a), 100);
        assert_eq!(agent_net(b), -100);

        // Money is conserved: everything except external sums to the deposit.
        let total: i64 = balances
            .iter()
            .filter(|(acct, _)| **acct != Account::External)
            .map(|(_, d)| d.minor())
            .sum();
        assert_eq!(total, 1000);
        assert_eq!(bal(&balances, Account::External), -1000);
    }

    #[test]
    fn refund_returns_stake_and_cannot_exceed_committed() {
        let (a, i) = (id(1), id(9));
        let mut balances = BTreeMap::new();
        apply(&mut balances, &deposit(ENV, m(500), "d").unwrap()).unwrap();
        apply(&mut balances, &allocate(ENV, a, m(500), "al").unwrap()).unwrap();
        apply(&mut balances, &commit_stake(ENV, a, i, m(200)).unwrap()).unwrap();

        assert_eq!(
            refund(ENV, a, i, m(201)).unwrap().check_balances(&balances),
            Err(LedgerError::InsufficientFunds {
                account: Account::AgentCommitted(a)
            })
        );
        apply(&mut balances, &refund(ENV, a, i, m(200)).unwrap()).unwrap();
        assert_eq!(bal(&balances, Account::AgentAvailable(a)), 500);
        assert_eq!(bal(&balances, Account::AgentCommitted(a)), 0);
    }

    #[test]
    fn check_balances_detects_overflow() {
        let mut balances = BTreeMap::new();
        balances.insert(Account::UserUnallocated, s(i64::MAX));
        assert_eq!(
            deposit(ENV, m(1), "k").unwrap().check_balances(&balances),
            Err(LedgerError::Overflow)
        );
    }
}

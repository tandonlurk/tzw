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
    /// A collaboration contribution is locked (available -> committed).
    CollabCommit,
    /// A completed collaboration pays out the pooled contributions.
    CollabPayout,
    /// A cancelled, funded collaboration returns a contribution.
    CollabRefund,
    /// USDC arrived in an agent's onchain vault (verified receipt):
    /// external -> agent available. `arc_testnet` only.
    VaultDeposit,
    /// USDC left an agent's onchain vault to the owner (verified receipt):
    /// agent available -> external. `arc_testnet` only.
    VaultWithdraw,
    /// A firm lead pays a member's guaranteed task fee for a used
    /// contribution: lead available -> member available. Simulated only.
    FirmTaskFee,
    /// A firm lead shares a challenge's profit with its members: lead
    /// available -> member available. Simulated only.
    FirmPayout,
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
            Reason::CollabCommit => "collab_commit",
            Reason::CollabPayout => "collab_payout",
            Reason::CollabRefund => "collab_refund",
            Reason::VaultDeposit => "vault_deposit",
            Reason::VaultWithdraw => "vault_withdraw",
            Reason::FirmTaskFee => "firm_task_fee",
            Reason::FirmPayout => "firm_payout",
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
    collaboration_id: Option<Uuid>,
    /// The onchain transaction this set records, if any.
    tx_hash: Option<String>,
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
            collaboration_id: None,
            tx_hash: None,
            postings,
        })
    }

    /// Records the onchain transaction behind this set (for display and
    /// audit; it is not part of the idempotency key).
    pub fn with_tx_hash(mut self, tx_hash: &str) -> PostingSet {
        self.tx_hash = Some(tx_hash.to_owned());
        self
    }

    pub fn tx_hash(&self) -> Option<&str> {
        self.tx_hash.as_deref()
    }

    /// Marks the set as belonging to a collaboration.
    pub fn for_collaboration(mut self, collaboration: Uuid) -> PostingSet {
        self.collaboration_id = Some(collaboration);
        self
    }

    pub fn collaboration_id(&self) -> Option<Uuid> {
        self.collaboration_id
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

/// A collaboration party locks its contribution. The key is derived from the
/// collaboration and agent, so it cannot be applied twice.
pub fn collab_commit(
    env: Env,
    agent: Uuid,
    collaboration: Uuid,
    amount: Money,
) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    let postings = transfer(
        Account::AgentAvailable(agent),
        Account::AgentCommitted(agent),
        amount,
    );
    Ok(PostingSet::new(
        env,
        Reason::CollabCommit,
        &format!("collab_commit:{collaboration}:{agent}"),
        None,
        postings.into(),
    )?
    .for_collaboration(collaboration))
}

/// Completion: both committed contributions move into the collaboration's
/// escrow and the escrow pays each party its share. The shares must add up to
/// the two contributions exactly (see `collaboration::split`); a zero share
/// gets no posting.
pub fn collab_payout(
    env: Env,
    collaboration: Uuid,
    parties: [(Uuid, Money, Money); 2],
) -> Result<PostingSet, LedgerError> {
    let [(a, a_in, a_out), (b, b_in, b_out)] = parties;
    if a == b {
        return Err(LedgerError::SameParty);
    }
    let pool_in = positive(a_in)?
        .checked_add(positive(b_in)?)
        .ok_or(LedgerError::Overflow)?;
    let pool_out = a_out.checked_add(b_out).ok_or(LedgerError::Overflow)?;
    if pool_in != pool_out {
        return Err(LedgerError::Unbalanced);
    }
    let escrow = Account::Escrow(collaboration);
    let mut postings = Vec::with_capacity(8);
    postings.extend(transfer(Account::AgentCommitted(a), escrow, a_in));
    postings.extend(transfer(Account::AgentCommitted(b), escrow, b_in));
    for (agent, share) in [(a, a_out), (b, b_out)] {
        if !share.is_zero() {
            postings.extend(transfer(escrow, Account::AgentAvailable(agent), share));
        }
    }
    Ok(PostingSet::new(
        env,
        Reason::CollabPayout,
        &format!("collab_payout:{collaboration}"),
        None,
        postings,
    )?
    .for_collaboration(collaboration))
}

/// Cancellation of a funded collaboration: a contribution goes back.
pub fn collab_refund(
    env: Env,
    agent: Uuid,
    collaboration: Uuid,
    amount: Money,
) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    let postings = transfer(
        Account::AgentCommitted(agent),
        Account::AgentAvailable(agent),
        amount,
    );
    Ok(PostingSet::new(
        env,
        Reason::CollabRefund,
        &format!("collab_refund:{collaboration}:{agent}"),
        None,
        postings.into(),
    )?
    .for_collaboration(collaboration))
}

/// A firm lead pays a member the task fee for one used contribution. The key
/// is derived from the contribution, so a fee is paid at most once.
pub fn firm_task_fee(
    env: Env,
    lead: Uuid,
    member: Uuid,
    contribution: Uuid,
    interaction: Option<Uuid>,
    amount: Money,
) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    if lead == member {
        return Err(LedgerError::SameParty);
    }
    let postings = transfer(
        Account::AgentAvailable(lead),
        Account::AgentAvailable(member),
        amount,
    );
    PostingSet::new(
        env,
        Reason::FirmTaskFee,
        &format!("firm_task_fee:{contribution}"),
        interaction,
        postings.into(),
    )
}

/// A firm lead shares one settled challenge's profit with the members whose
/// contributions were used (see `firm::split_profit`). Zero amounts get no
/// posting; one set per interaction, so it is paid at most once.
pub fn firm_payout(
    env: Env,
    lead: Uuid,
    interaction: Uuid,
    payouts: &[(Uuid, Money)],
) -> Result<PostingSet, LedgerError> {
    let mut postings = Vec::with_capacity(payouts.len() * 2);
    for (member, amount) in payouts.iter().filter(|(_, a)| !a.is_zero()) {
        if *member == lead {
            return Err(LedgerError::SameParty);
        }
        postings.extend(transfer(
            Account::AgentAvailable(lead),
            Account::AgentAvailable(*member),
            *amount,
        ));
    }
    PostingSet::new(
        env,
        Reason::FirmPayout,
        &format!("firm_payout:{interaction}"),
        Some(interaction),
        postings,
    )
}

/// The idempotency key of a ledger set derived from one onchain log, so a
/// verified log is posted at most once whoever reports it.
pub fn chain_key(tx_hash: &str, log_index: u64) -> String {
    format!("chain:{}:{log_index}", tx_hash.to_ascii_lowercase())
}

/// USDC transferred into an agent's vault becomes available to the agent.
/// Always `arc_testnet`; one set per transfer log.
pub fn vault_deposit(
    agent: Uuid,
    amount: Money,
    tx_hash: &str,
    log_index: u64,
) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    let postings = transfer(Account::External, Account::AgentAvailable(agent), amount);
    Ok(PostingSet::new(
        Env::ArcTestnet,
        Reason::VaultDeposit,
        &chain_key(tx_hash, log_index),
        None,
        postings.into(),
    )?
    .with_tx_hash(&tx_hash.to_ascii_lowercase()))
}

/// USDC the owner withdrew from an agent's vault leaves its available
/// balance. Fails like any other posting if available would go negative.
pub fn vault_withdraw(
    agent: Uuid,
    amount: Money,
    tx_hash: &str,
    log_index: u64,
) -> Result<PostingSet, LedgerError> {
    let amount = positive(amount)?;
    let postings = transfer(Account::AgentAvailable(agent), Account::External, amount);
    Ok(PostingSet::new(
        Env::ArcTestnet,
        Reason::VaultWithdraw,
        &chain_key(tx_hash, log_index),
        None,
        postings.into(),
    )?
    .with_tx_hash(&tx_hash.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: Env = Env::Simulated;

    #[test]
    fn collab_payout_balances_and_pays_the_split() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let set = collab_payout(ENV, c, [(a, m(3), m(6)), (b, m(7), m(4))]).unwrap();
        let net = set.net_deltas();
        assert_eq!(net[&Account::AgentCommitted(a)], s(-3));
        assert_eq!(net[&Account::AgentCommitted(b)], s(-7));
        assert_eq!(net[&Account::AgentAvailable(a)], s(6));
        assert_eq!(net[&Account::AgentAvailable(b)], s(4));
        assert_eq!(net[&Account::Escrow(c)], s(0));
        assert_eq!(set.collaboration_id(), Some(c));
        assert_eq!(set.interaction_id(), None);
        assert_eq!(set.idempotency_key(), format!("collab_payout:{c}"));
        // A zero share is simply not paid.
        let set = collab_payout(ENV, c, [(a, m(3), m(0)), (b, m(7), m(10))]).unwrap();
        assert!(!set.net_deltas().contains_key(&Account::AgentAvailable(a)));
        // Shares that do not add up to the pool are refused.
        assert_eq!(
            collab_payout(ENV, c, [(a, m(3), m(6)), (b, m(7), m(5))]),
            Err(LedgerError::Unbalanced)
        );
        assert_eq!(
            collab_payout(ENV, c, [(a, m(3), m(6)), (a, m(7), m(4))]),
            Err(LedgerError::SameParty)
        );
        assert_eq!(
            collab_payout(ENV, c, [(a, m(0), m(0)), (b, m(7), m(7))]),
            Err(LedgerError::ZeroAmount)
        );
    }

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
    fn vault_deposit_and_withdraw_are_keyed_by_log_and_bounded_by_available() {
        let a = id(1);
        let tx = "0xABCDEF";
        let dep = vault_deposit(a, m(40), tx, 3).unwrap();
        assert_eq!(dep.env(), Env::ArcTestnet);
        assert_eq!(dep.reason().as_str(), "vault_deposit");
        assert_eq!(dep.idempotency_key(), "chain:0xabcdef:3");
        assert_eq!(dep.tx_hash(), Some("0xabcdef"));
        assert_eq!(dep.net_deltas()[&Account::AgentAvailable(a)], s(40));
        assert_eq!(dep.net_deltas()[&Account::External], s(-40));
        // Another log in the same transaction is another set.
        assert_eq!(
            vault_deposit(a, m(1), tx, 4).unwrap().idempotency_key(),
            "chain:0xabcdef:4"
        );

        let mut balances = BTreeMap::new();
        apply(&mut balances, &dep).unwrap();
        let too_much = vault_withdraw(a, m(41), "0x01", 0).unwrap();
        assert_eq!(
            too_much.check_balances(&balances),
            Err(LedgerError::InsufficientFunds {
                account: Account::AgentAvailable(a)
            })
        );
        let w = vault_withdraw(a, m(40), "0x01", 0).unwrap();
        assert_eq!(w.reason().as_str(), "vault_withdraw");
        apply(&mut balances, &w).unwrap();
        assert_eq!(bal(&balances, Account::AgentAvailable(a)), 0);
        assert_eq!(
            vault_deposit(a, Money::ZERO, tx, 0),
            Err(LedgerError::ZeroAmount)
        );
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

    #[test]
    fn firm_transfers_move_available_to_available() {
        let (lead, a, b, i, c) = (id(1), id(2), id(3), id(4), id(5));
        let fee = firm_task_fee(ENV, lead, a, c, Some(i), m(5)).unwrap();
        assert_eq!(fee.reason(), Reason::FirmTaskFee);
        assert_eq!(fee.idempotency_key(), format!("firm_task_fee:{c}"));
        assert_eq!(fee.net_deltas()[&Account::AgentAvailable(lead)], s(-5));
        assert_eq!(fee.net_deltas()[&Account::AgentAvailable(a)], s(5));
        assert_eq!(
            firm_task_fee(ENV, lead, a, c, None, m(0)),
            Err(LedgerError::ZeroAmount)
        );
        assert_eq!(
            firm_task_fee(ENV, lead, lead, c, None, m(1)),
            Err(LedgerError::SameParty)
        );

        let pay = firm_payout(ENV, lead, i, &[(a, m(120)), (b, m(0)), (b, m(40))]).unwrap();
        assert_eq!(pay.idempotency_key(), format!("firm_payout:{i}"));
        assert_eq!(pay.interaction_id(), Some(i));
        assert_eq!(sum(&pay), 0);
        let net = pay.net_deltas();
        assert_eq!(net[&Account::AgentAvailable(lead)], s(-160));
        assert_eq!(net[&Account::AgentAvailable(b)], s(40));
        // Nothing to pay is not a posting set.
        assert_eq!(
            firm_payout(ENV, lead, i, &[(a, m(0))]),
            Err(LedgerError::NoPostings)
        );
        assert_eq!(
            firm_payout(ENV, lead, i, &[(lead, m(1))]),
            Err(LedgerError::SameParty)
        );

        // A lead cannot pay more than it has.
        let mut balances = BTreeMap::new();
        balances.insert(Account::AgentAvailable(lead), s(100));
        assert_eq!(
            apply(&mut balances, &pay),
            Err(LedgerError::InsufficientFunds {
                account: Account::AgentAvailable(lead)
            })
        );
        apply(&mut balances, &fee).unwrap();
        assert_eq!(bal(&balances, Account::AgentAvailable(lead)), 95);
    }
}

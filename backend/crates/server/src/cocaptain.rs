//! Co-captain: advisory text suggestions for the owner (AGENTS.md 2.3).
//!
//! A `CoCaptain` turns `Facts` (real numbers read from the ledger,
//! interactions, approvals and activity) into `Draft`s. That is all it can
//! do: it has no database handle, no path to `authorize` or the executor and
//! cannot move funds, approve or change a policy. A draft may carry a
//! `ProposedChange`, a small typed patch of named limits; it is applied only
//! when the owner accepts the suggestion, through the same validated,
//! versioned edit as the owner's own (`agents::apply_update`).
//!
//! `RuleBasedCoCaptain` is deterministic and is the one that must work.
//! `grok_cocaptain::GrokCoCaptain` is optional and falls back to it.

use aen_core::authorize::{AgentStatus, DenyReason};
use aen_core::money::{Money, SignedMoney, UNITS_PER_USDC};
use aen_core::policy::AgentPolicy;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::market::BoxFut;
use crate::models::{AgentInput, RiskTolerance, display_usdc};

pub const MAX_TITLE_LEN: usize = 120;
pub const MAX_BODY_LEN: usize = 600;

/// Rule 1: warn when a pending approval expires within this many seconds.
pub const APPROVAL_EXPIRY_WARN_SECS: i64 = 120;
/// Rule 2: an agent is idle after this long without any interaction.
pub const IDLE_SECS: i64 = 3600;
/// Rule 2: balances below this are not worth a suggestion (1 USDC).
pub const IDLE_MIN_FUNDS: i64 = UNITS_PER_USDC;
/// Rule 3: this many settled losses in a row.
pub const LOSING_STREAK: usize = 3;
/// Rule 3: realized loss as a share of net allocation (basis points).
pub const DRAWDOWN_BPS: i64 = 2000;
/// Rule 4: this many denials for the same reason under the current version.
pub const DENIAL_REPEAT: i64 = 3;
/// Rule 5: this many owner decisions in a row, all approvals.
pub const APPROVAL_STREAK: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    ApprovalExpiring,
    IdleFunds,
    LowerMaxStake,
    RepeatedDenials,
    RaiseApprovalThreshold,
    AgentHalted,
    /// Free-form advice; only the Grok co-captain produces it.
    General,
}

impl Kind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::ApprovalExpiring => "approval_expiring",
            Kind::IdleFunds => "idle_funds",
            Kind::LowerMaxStake => "lower_max_stake",
            Kind::RepeatedDenials => "repeated_denials",
            Kind::RaiseApprovalThreshold => "raise_approval_threshold",
            Kind::AgentHalted => "agent_halted",
            Kind::General => "general",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Rules,
    Grok,
}

impl Source {
    pub const fn as_str(self) -> &'static str {
        match self {
            Source::Rules => "rules",
            Source::Grok => "grok",
        }
    }
}

/// The only policy fields a suggestion may touch. Unknown fields are an
/// error, so a suggestion can never name the graph, the allowed interaction
/// types, the status or anything else.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_interaction: Option<Money>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_aggregate_exposure: Option<Money>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_threshold: Option<Money>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_conviction_bps: Option<u32>,
    /// Sets a stop-loss. A suggestion cannot disable one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<Money>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_tolerance: Option<RiskTolerance>,
}

/// A patch of the agent's current config / policy. It is merged into the
/// version current at accept time and then validated as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigChange>,
}

impl ProposedChange {
    pub fn policy(change: PolicyChange) -> ProposedChange {
        ProposedChange {
            policy: Some(change),
            config: None,
        }
    }

    pub fn apply_policy(&self, policy: &mut AgentPolicy) {
        let Some(p) = &self.policy else { return };
        if let Some(v) = p.max_per_interaction {
            policy.max_per_interaction = v;
        }
        if let Some(v) = p.max_aggregate_exposure {
            policy.max_aggregate_exposure = v;
        }
        if let Some(v) = p.approval_threshold {
            policy.approval_threshold = v;
        }
        if let Some(v) = p.min_conviction_bps {
            policy.min_conviction_bps = v;
        }
        if let Some(v) = p.stop_loss {
            policy.stop_loss = Some(v);
        }
    }

    pub fn apply(&self, input: &mut AgentInput) {
        self.apply_policy(&mut input.policy);
        if let Some(v) = self.config.as_ref().and_then(|c| c.risk_tolerance) {
            input.config.risk_tolerance = v;
        }
    }

    /// Whether accepting would loosen an enforced control: a higher limit, a
    /// higher approval threshold or a higher stop-loss. (`min_conviction_bps`
    /// and `risk_tolerance` are preferences, not controls.)
    pub fn loosens(&self, current: &AgentPolicy) -> bool {
        let Some(p) = &self.policy else { return false };
        p.max_per_interaction
            .is_some_and(|v| v > current.max_per_interaction)
            || p.max_aggregate_exposure
                .is_some_and(|v| v > current.max_aggregate_exposure)
            || p.approval_threshold
                .is_some_and(|v| v > current.approval_threshold)
            || matches!((p.stop_loss, current.stop_loss), (Some(new), Some(old)) if new > old)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingApproval {
    pub id: Uuid,
    pub stake: Money,
    /// The funding deadline of the challenge.
    pub expires_at: DateTime<Utc>,
    pub market_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecidedApproval {
    pub id: Uuid,
    pub approved: bool,
    pub stake: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DenialCount {
    pub reason: DenyReason,
    pub count: i64,
    /// Largest stake that was denied for this reason, when recorded.
    pub max_stake: Option<Money>,
}

/// Everything the co-captain knows about one agent. All of it is computed
/// from stored records; nothing here is secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentFacts {
    pub id: Uuid,
    pub name: String,
    pub status: AgentStatus,
    pub version: i32,
    /// Last configuration or status change.
    pub updated_at: DateTime<Utc>,
    pub risk_tolerance: RiskTolerance,
    pub policy: AgentPolicy,
    /// Ledger balance of `agent:{id}:available` / `:committed`.
    pub available: Money,
    pub committed: Money,
    /// Net of the agent's `settle` ledger entries.
    pub realized_pnl: SignedMoney,
    pub settled: i64,
    pub wins: i64,
    /// Sum of `allocate` / `deallocate` entries on the available account.
    pub net_allocated: SignedMoney,
    /// Net of the latest settlements, newest first (at most `LOSING_STREAK`).
    pub recent_settle_nets: Vec<SignedMoney>,
    /// Pending approvals of challenges still awaiting approval, soonest first.
    pub pending_approvals: Vec<PendingApproval>,
    /// Latest owner decisions (approved / rejected) on challenges entered
    /// under the current version, newest first (at most `APPROVAL_STREAK`).
    pub recent_approvals: Vec<DecidedApproval>,
    /// `*_denied` activity since the current version was created, by reason.
    pub denials: Vec<DenialCount>,
    /// Interactions not yet in a terminal state.
    pub open_interactions: i64,
    pub last_interaction_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facts {
    pub now: DateTime<Utc>,
    /// Ledger balance of `user:unallocated` (simulated).
    pub unallocated: Money,
    pub agents: Vec<AgentFacts>,
}

/// A suggestion before it is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub agent_id: Option<Uuid>,
    pub kind: Kind,
    /// Dedupe key: the same subject is never suggested twice.
    pub subject: String,
    pub title: String,
    pub body: String,
    pub proposed_change: Option<ProposedChange>,
    pub evidence: Value,
    /// Set by `vet`, never by the producer.
    pub loosens_control: bool,
}

impl Draft {
    /// Checks a draft against the facts and computes `loosens_control`. Used
    /// for every source; a draft that fails is dropped.
    pub fn vet(mut self, facts: &Facts) -> Result<Draft, String> {
        let len_ok = |text: &str, max: usize| {
            !text.trim().is_empty() && text.chars().count() <= max && !text.contains('\0')
        };
        if !len_ok(&self.title, MAX_TITLE_LEN) {
            return Err("title is empty or too long".into());
        }
        if !len_ok(&self.body, MAX_BODY_LEN) {
            return Err("body is empty or too long".into());
        }
        if self.subject.is_empty() {
            return Err("subject is empty".into());
        }
        let agent = match self.agent_id {
            Some(id) => Some(
                facts
                    .agents
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or("unknown agent")?,
            ),
            None => None,
        };
        self.loosens_control = false;
        if let Some(change) = &self.proposed_change {
            let agent = agent.ok_or("a proposed change needs an agent")?;
            let mut policy = agent.policy.clone();
            change.apply_policy(&mut policy);
            let risk = change.config.as_ref().and_then(|c| c.risk_tolerance);
            if policy == agent.policy && risk.is_none_or(|r| r == agent.risk_tolerance) {
                return Err("the proposed change changes nothing".into());
            }
            if let Some(e) = policy.validate().first() {
                return Err(format!("the proposed change is not a valid policy: {e}"));
            }
            self.loosens_control = change.loosens(&agent.policy);
        }
        Ok(self)
    }
}

pub struct Advice {
    /// Who actually produced the drafts (`Rules` after a Grok fallback).
    pub source: Source,
    pub drafts: Vec<Draft>,
}

pub trait CoCaptain: Send + Sync {
    fn name(&self) -> &'static str;

    /// Never fails: an implementation that cannot answer returns the
    /// rule-based advice.
    fn advise<'a>(&'a self, facts: &'a Facts) -> BoxFut<'a, Advice>;
}

pub struct RuleBasedCoCaptain;

impl CoCaptain for RuleBasedCoCaptain {
    fn name(&self) -> &'static str {
        "rules"
    }

    fn advise<'a>(&'a self, facts: &'a Facts) -> BoxFut<'a, Advice> {
        Box::pin(async move {
            Advice {
                source: Source::Rules,
                drafts: rules(facts),
            }
        })
    }
}

/// Every rule over every agent, vetted. Deterministic for given facts.
pub fn rules(facts: &Facts) -> Vec<Draft> {
    let mut drafts = Vec::new();
    for agent in &facts.agents {
        let found = [
            approval_expiring(agent, facts.now),
            idle_funds(agent, facts),
            lower_max_stake(agent),
            repeated_denials(agent),
            raise_approval_threshold(agent),
            agent_halted(agent),
        ];
        for draft in found.into_iter().flatten() {
            match draft.vet(facts) {
                Ok(draft) => drafts.push(draft),
                Err(error) => tracing::warn!(%error, "rule produced an invalid suggestion"),
            }
        }
    }
    drafts
}

fn draft(
    agent: &AgentFacts,
    kind: Kind,
    subject: String,
    title: String,
    body: String,
    proposed_change: Option<ProposedChange>,
    evidence: Value,
) -> Option<Draft> {
    Some(Draft {
        agent_id: Some(agent.id),
        kind,
        subject,
        title,
        body,
        proposed_change,
        evidence,
        loosens_control: false,
    })
}

fn usdc(amount: Money) -> String {
    format!("{} (simulated)", display_usdc(amount))
}

fn loss_text(pnl: SignedMoney) -> String {
    usdc(Money::from_minor(pnl.minor().saturating_neg().max(0)).unwrap_or(Money::ZERO))
}

/// Rule 1, `approval_expiring` (text only).
///
/// Fires for the agent's soonest pending approval when
/// `0 < expires_at - now <= APPROVAL_EXPIRY_WARN_SECS`, where `expires_at` is
/// the challenge's funding deadline (`interactions.start_ts`).
/// Source: `approvals` (status `pending`) joined to `interactions`.
fn approval_expiring(agent: &AgentFacts, now: DateTime<Utc>) -> Option<Draft> {
    let approval = agent.pending_approvals.iter().find(|p| {
        let left = (p.expires_at - now).num_seconds();
        left > 0 && left <= APPROVAL_EXPIRY_WARN_SECS
    })?;
    let left = (approval.expires_at - now).num_seconds();
    draft(
        agent,
        Kind::ApprovalExpiring,
        approval.id.to_string(),
        format!("An approval for \"{}\" expires in {left} s", agent.name),
        format!(
            "\"{}\" is waiting for your decision on a {} stake ({}). If you do not decide by {} \
             the challenge is cancelled and no funds move. Decide it under Approvals; the \
             co-captain cannot approve for you.",
            agent.name,
            usdc(approval.stake),
            approval.market_title,
            approval.expires_at.format("%H:%M:%S UTC"),
        ),
        None,
        json!({
            "approval_id": approval.id,
            "stake": approval.stake,
            "expires_at": approval.expires_at,
            "seconds_left": left,
            "warn_within_secs": APPROVAL_EXPIRY_WARN_SECS,
        }),
    )
}

/// Rule 2, `idle_funds` (text only). Two cases, active agents only:
///
/// - idle: `available >= IDLE_MIN_FUNDS`, `open_interactions == 0`,
///   `now - last_interaction_at >= IDLE_SECS` (or no interaction ever) and
///   `now - updated_at >= IDLE_SECS` (a freshly created or edited agent gets
///   the same grace).
/// - unfunded: `available == 0`, `committed == 0` and the owner's
///   `unallocated >= IDLE_MIN_FUNDS`.
///
/// Source: ledger balances, `interactions.updated_at`, `agents.updated_at`.
fn idle_funds(agent: &AgentFacts, facts: &Facts) -> Option<Draft> {
    if agent.status != AgentStatus::Active {
        return None;
    }
    let min = Money::from_minor(IDLE_MIN_FUNDS)?;
    if agent.available == Money::ZERO
        && agent.committed == Money::ZERO
        && facts.unallocated >= min
    {
        return draft(
            agent,
            Kind::IdleFunds,
            format!("unfunded:v{}:s{}", agent.version, agent.settled),
            format!("\"{}\" has no funds to work with", agent.name),
            format!(
                "\"{}\" is active but holds no funds, while {} sit unallocated in your wallet. \
                 Allocate some from the Wallet page if you want it to take challenges; the \
                 co-captain cannot move funds.",
                agent.name,
                usdc(facts.unallocated),
            ),
            None,
            json!({
                "available": agent.available,
                "committed": agent.committed,
                "unallocated": facts.unallocated,
            }),
        );
    }
    let quiet = |t: DateTime<Utc>| (facts.now - t).num_seconds() >= IDLE_SECS;
    if agent.available < min
        || agent.open_interactions != 0
        || !quiet(agent.updated_at)
        || !agent.last_interaction_at.is_none_or(quiet)
    {
        return None;
    }
    let last = agent.last_interaction_at.map(|t| t.timestamp());
    draft(
        agent,
        Kind::IdleFunds,
        format!("idle:v{}:{}", agent.version, last.unwrap_or(0)),
        format!("\"{}\" has idle funds", agent.name),
        format!(
            "\"{}\" holds {} but has had no challenge for at least {} minutes. Its strategy or \
             minimum conviction may be too strict for current markets; review them, or return \
             the funds to your wallet.",
            agent.name,
            usdc(agent.available),
            IDLE_SECS / 60,
        ),
        None,
        json!({
            "available": agent.available,
            "open_interactions": agent.open_interactions,
            "last_interaction_at": agent.last_interaction_at,
            "idle_secs": IDLE_SECS,
            "min_conviction_bps": agent.policy.min_conviction_bps,
        }),
    )
}

/// Rule 3, `lower_max_stake` (tightens a limit).
///
/// Fires when either
/// - the latest `LOSING_STREAK` settlements all have a negative net, or
/// - `net_allocated > 0` and
///   `realized_pnl <= -(net_allocated * DRAWDOWN_BPS / 10000)`.
///
/// Proposes `max_per_interaction = floor(current / 2)` and, when the approval
/// threshold would then exceed it, `approval_threshold = ` the same value.
/// Nothing is proposed when the halved limit would be zero.
/// Source: `settle` ledger entries, `allocate` / `deallocate` ledger entries.
fn lower_max_stake(agent: &AgentFacts) -> Option<Draft> {
    let streak = agent.recent_settle_nets.len() >= LOSING_STREAK
        && agent
            .recent_settle_nets
            .iter()
            .take(LOSING_STREAK)
            .all(|n| n.is_negative());
    let allocated = agent.net_allocated.minor();
    let drawdown_limit = i128::from(allocated) * i128::from(DRAWDOWN_BPS) / 10_000;
    let drawdown = allocated > 0
        && agent.realized_pnl.is_negative()
        && i128::from(agent.realized_pnl.minor()) <= -drawdown_limit;
    if !streak && !drawdown {
        return None;
    }
    let current = agent.policy.max_per_interaction;
    let lower = Money::from_minor(current.minor() / 2).filter(|m| *m > Money::ZERO)?;
    let lowers_threshold = agent.policy.approval_threshold > lower;
    let why = if streak {
        format!("lost its last {LOSING_STREAK} challenges")
    } else {
        format!(
            "has lost {} of the {} allocated to it",
            loss_text(agent.realized_pnl),
            usdc(Money::from_minor(allocated)?),
        )
    };
    draft(
        agent,
        Kind::LowerMaxStake,
        format!("settled:{}", agent.settled),
        format!("Lower the maximum stake of \"{}\"", agent.name),
        format!(
            "\"{}\" {why} (realized P&L {}{}). Halving its maximum stake per challenge from {} \
             to {} limits further losses while you review its strategy.{}",
            agent.name,
            if agent.realized_pnl.is_negative() { "-" } else { "" },
            usdc(Money::from_minor(agent.realized_pnl.minor().saturating_abs())?),
            usdc(current),
            usdc(lower),
            if lowers_threshold {
                " Its approval threshold is lowered to the same amount."
            } else {
                ""
            },
        ),
        Some(ProposedChange::policy(PolicyChange {
            max_per_interaction: Some(lower),
            approval_threshold: lowers_threshold.then_some(lower),
            ..PolicyChange::default()
        })),
        json!({
            "recent_settle_nets": agent.recent_settle_nets,
            "losing_streak": LOSING_STREAK,
            "realized_pnl": agent.realized_pnl,
            "net_allocated": agent.net_allocated,
            "drawdown_bps": DRAWDOWN_BPS,
            "settled": agent.settled,
            "wins": agent.wins,
            "max_per_interaction": current,
        }),
    )
}

/// Rule 4, `repeated_denials`.
///
/// Counts `proposal_denied` / `response_denied` / `commit_denied` activity
/// recorded since the agent's current version was created, per deny reason.
/// Fires for the most frequent of these reasons with
/// `count >= DENIAL_REPEAT`:
///
/// - `exceeds_max_per_interaction`: proposes `max_per_interaction =
///   min(largest denied stake, max_aggregate_exposure)` when that is above
///   the current limit (loosens a control), otherwise text only.
/// - `exceeds_aggregate_exposure`: proposes `max_aggregate_exposure =
///   current + largest denied stake` (loosens a control), text only when no
///   stake was recorded.
/// - `insufficient_available_balance`: text only (allocating is the owner's
///   action).
///
/// Source: `activity.detail.reasons` written by the lifecycle at each denial.
fn repeated_denials(agent: &AgentFacts) -> Option<Draft> {
    const REASONS: [DenyReason; 3] = [
        DenyReason::ExceedsMaxPerInteraction,
        DenyReason::ExceedsAggregateExposure,
        DenyReason::InsufficientAvailableBalance,
    ];
    let mut top: Option<&DenialCount> = None;
    for reason in REASONS {
        let found = agent
            .denials
            .iter()
            .find(|d| d.reason == reason && d.count >= DENIAL_REPEAT);
        if let Some(d) = found
            && top.is_none_or(|t| d.count > t.count)
        {
            top = Some(d);
        }
    }
    let denial = top?;
    let policy = &agent.policy;
    let (what, advice, change) = match denial.reason {
        DenyReason::ExceedsMaxPerInteraction => {
            let target = denial
                .max_stake
                .map(|s| s.min(policy.max_aggregate_exposure))
                .filter(|t| *t > policy.max_per_interaction);
            match target {
                Some(target) => (
                    "the stake was above its per-challenge limit",
                    format!(
                        "Raising the limit from {} to {} (the largest stake that was blocked) \
                         would let these through. This loosens a control: only accept if you \
                         are comfortable with stakes of that size.",
                        usdc(policy.max_per_interaction),
                        usdc(target),
                    ),
                    Some(PolicyChange {
                        max_per_interaction: Some(target),
                        ..PolicyChange::default()
                    }),
                ),
                None => (
                    "the stake was above its per-challenge limit",
                    format!(
                        "Its limit is {}. Raise it on the agent's page if these stakes are \
                         what you want, or adjust its strategy to size smaller.",
                        usdc(policy.max_per_interaction),
                    ),
                    None,
                ),
            }
        }
        DenyReason::ExceedsAggregateExposure => {
            let target = denial
                .max_stake
                .and_then(|s| policy.max_aggregate_exposure.checked_add(s));
            match target {
                Some(target) => (
                    "it would have exceeded its total exposure limit",
                    format!(
                        "Raising the total exposure limit from {} to {} would leave room for \
                         one more stake of the blocked size. This loosens a control: more of \
                         its funds could be at risk at once.",
                        usdc(policy.max_aggregate_exposure),
                        usdc(target),
                    ),
                    Some(PolicyChange {
                        max_aggregate_exposure: Some(target),
                        ..PolicyChange::default()
                    }),
                ),
                None => (
                    "it would have exceeded its total exposure limit",
                    format!(
                        "Its total exposure limit is {}. Raise it on the agent's page if you \
                         want more open challenges at once.",
                        usdc(policy.max_aggregate_exposure),
                    ),
                    None,
                ),
            }
        }
        _ => (
            "its available balance was too low",
            format!(
                "It holds {} available. Allocate more from the Wallet page, or lower its \
                 maximum stake; the co-captain cannot move funds.",
                usdc(agent.available),
            ),
            None,
        ),
    };
    let reason = serde_json::to_value(denial.reason).ok()?;
    draft(
        agent,
        Kind::RepeatedDenials,
        format!("{}:v{}", reason.as_str()?, agent.version),
        format!("\"{}\" keeps being blocked", agent.name),
        format!(
            "\"{}\" was blocked {} times since its last configuration change because {what}. \
             {advice}",
            agent.name, denial.count,
        ),
        change.map(ProposedChange::policy),
        json!({
            "reason": reason,
            "count": denial.count,
            "max_denied_stake": denial.max_stake,
            "repeat_threshold": DENIAL_REPEAT,
            "max_per_interaction": policy.max_per_interaction,
            "max_aggregate_exposure": policy.max_aggregate_exposure,
            "available": agent.available,
            "version": agent.version,
        }),
    )
}

/// Rule 5, `raise_approval_threshold` (loosens a control; flagged).
///
/// Fires when the latest `APPROVAL_STREAK` owner decisions on challenges the
/// agent entered under its current version were all approvals and
/// `target = min(largest approved stake among them, max_per_interaction)` is
/// above the current `approval_threshold`. Proposes
/// `approval_threshold = target`.
/// Source: `approvals` (status `approved` / `rejected`).
fn raise_approval_threshold(agent: &AgentFacts) -> Option<Draft> {
    let recent = agent.recent_approvals.get(..APPROVAL_STREAK)?;
    if !recent.iter().all(|a| a.approved) {
        return None;
    }
    let largest = recent.iter().map(|a| a.stake).max()?;
    let target = largest.min(agent.policy.max_per_interaction);
    if target <= agent.policy.approval_threshold {
        return None;
    }
    draft(
        agent,
        Kind::RaiseApprovalThreshold,
        format!("v{}:{}", agent.version, recent[0].id),
        format!("Raise the approval threshold of \"{}\"?", agent.name),
        format!(
            "You approved the last {APPROVAL_STREAK} stakes \"{}\" asked about (up to {}). \
             Raising its approval threshold from {} to {} would let stakes of that size go \
             ahead without asking. This loosens a control: you would no longer review them.",
            agent.name,
            usdc(largest),
            usdc(agent.policy.approval_threshold),
            usdc(target),
        ),
        Some(ProposedChange::policy(PolicyChange {
            approval_threshold: Some(target),
            ..PolicyChange::default()
        })),
        json!({
            "recent_approvals": recent,
            "approval_streak": APPROVAL_STREAK,
            "approval_threshold": agent.policy.approval_threshold,
            "max_per_interaction": agent.policy.max_per_interaction,
        }),
    )
}

/// Rule 6, `agent_halted` (text only): explains why an agent does nothing.
///
/// Fires when `status == paused`, or when a stop-loss is set and
/// `realized_pnl <= -stop_loss` (the same test `authorize` applies).
/// Source: `agents.status`, `settle` ledger entries, the current policy.
fn agent_halted(agent: &AgentFacts) -> Option<Draft> {
    if agent.status == AgentStatus::Paused {
        return draft(
            agent,
            Kind::AgentHalted,
            format!("paused:{}", agent.updated_at.timestamp_micros()),
            format!("\"{}\" is paused", agent.name),
            format!(
                "\"{}\" is paused, so it proposes nothing, answers nothing and cannot commit \
                 funds. It holds {} available and {} committed. Resume it on its page when \
                 you want it to act again.",
                agent.name,
                usdc(agent.available),
                usdc(agent.committed),
            ),
            None,
            json!({
                "status": agent.status,
                "available": agent.available,
                "committed": agent.committed,
            }),
        );
    }
    let stop = agent.policy.stop_loss?;
    if agent.realized_pnl.minor() > -stop.minor() {
        return None;
    }
    draft(
        agent,
        Kind::AgentHalted,
        format!("stop_loss:v{}", agent.version),
        format!("\"{}\" has hit its stop-loss", agent.name),
        format!(
            "\"{}\" has realized losses of {}, which reaches its stop-loss of {}. It will not \
             stake on anything until you change the stop-loss on its page. Its remaining {} \
             available are untouched.",
            agent.name,
            loss_text(agent.realized_pnl),
            usdc(stop),
            usdc(agent.available),
        ),
        None,
        json!({
            "realized_pnl": agent.realized_pnl,
            "stop_loss": stop,
            "settled": agent.settled,
            "wins": agent.wins,
            "available": agent.available,
        }),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use aen_core::policy::InteractionType;

    use super::*;

    pub fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    const U: i64 = UNITS_PER_USDC;

    pub fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn secs(s: i64) -> chrono::Duration {
        chrono::Duration::seconds(s)
    }

    /// A healthy, recently busy agent: no rule fires.
    pub fn agent() -> AgentFacts {
        AgentFacts {
            id: Uuid::from_u128(1),
            name: "Bull".into(),
            status: AgentStatus::Active,
            version: 2,
            updated_at: now() - secs(10 * IDLE_SECS),
            risk_tolerance: RiskTolerance::Medium,
            policy: AgentPolicy {
                max_per_interaction: m(10 * U),
                max_aggregate_exposure: m(40 * U),
                approval_threshold: m(4 * U),
                allowed_interaction_types: vec![InteractionType::Challenge],
                min_conviction_bps: 6000,
                stop_loss: Some(m(50 * U)),
            },
            available: m(100 * U),
            committed: Money::ZERO,
            realized_pnl: SignedMoney::ZERO,
            settled: 0,
            wins: 0,
            net_allocated: SignedMoney::from_minor(100 * U),
            recent_settle_nets: vec![],
            pending_approvals: vec![],
            recent_approvals: vec![],
            denials: vec![],
            open_interactions: 0,
            last_interaction_at: Some(now() - secs(60)),
        }
    }

    pub fn facts(agent: AgentFacts) -> Facts {
        Facts {
            now: now(),
            unallocated: m(500 * U),
            agents: vec![agent],
        }
    }

    fn one(agent: AgentFacts, kind: Kind) -> Draft {
        let drafts = rules(&facts(agent));
        let mut of_kind: Vec<Draft> = drafts.into_iter().filter(|d| d.kind == kind).collect();
        assert_eq!(of_kind.len(), 1, "expected one {kind:?}");
        of_kind.remove(0)
    }

    fn none(agent: AgentFacts, kind: Kind) {
        let drafts = rules(&facts(agent));
        assert!(drafts.iter().all(|d| d.kind != kind), "{drafts:?}");
    }

    fn policy_change(d: &Draft) -> PolicyChange {
        d.proposed_change.clone().unwrap().policy.unwrap()
    }

    fn losses(n: usize) -> Vec<SignedMoney> {
        vec![SignedMoney::from_minor(-2 * U); n]
    }

    #[test]
    fn a_healthy_agent_gets_no_suggestions() {
        assert_eq!(rules(&facts(agent())), vec![]);
    }

    #[test]
    fn approval_expiring_fires_only_inside_the_warning_window() {
        let pending = |left: i64| AgentFacts {
            pending_approvals: vec![PendingApproval {
                id: Uuid::from_u128(7),
                stake: m(6 * U),
                expires_at: now() + secs(left),
                market_title: "BTC up".into(),
            }],
            open_interactions: 1,
            ..agent()
        };
        let d = one(pending(APPROVAL_EXPIRY_WARN_SECS), Kind::ApprovalExpiring);
        assert_eq!(d.subject, Uuid::from_u128(7).to_string());
        assert_eq!(d.proposed_change, None);
        assert_eq!(d.evidence["seconds_left"], 120);
        assert_eq!(d.evidence["stake"], "6000000");
        assert!(d.body.contains("6.00 USDC (simulated)"), "{}", d.body);
        one(pending(1), Kind::ApprovalExpiring);

        none(pending(APPROVAL_EXPIRY_WARN_SECS + 1), Kind::ApprovalExpiring);
        none(pending(0), Kind::ApprovalExpiring);
        none(pending(-30), Kind::ApprovalExpiring);
        none(agent(), Kind::ApprovalExpiring);
    }

    #[test]
    fn idle_funds_needs_funds_and_a_quiet_hour() {
        let idle = AgentFacts {
            last_interaction_at: Some(now() - secs(IDLE_SECS)),
            ..agent()
        };
        let d = one(idle.clone(), Kind::IdleFunds);
        assert!(d.subject.starts_with("idle:v2:"));
        assert_eq!(d.evidence["available"], "100000000");
        assert_eq!(d.proposed_change, None);
        // Never interacted at all.
        one(
            AgentFacts {
                last_interaction_at: None,
                ..agent()
            },
            Kind::IdleFunds,
        );

        let busy = AgentFacts {
            last_interaction_at: Some(now() - secs(IDLE_SECS - 1)),
            ..agent()
        };
        none(busy, Kind::IdleFunds);
        let open = AgentFacts {
            open_interactions: 1,
            ..idle.clone()
        };
        none(open, Kind::IdleFunds);
        let just_edited = AgentFacts {
            updated_at: now() - secs(5),
            ..idle.clone()
        };
        none(just_edited, Kind::IdleFunds);
        let dust = AgentFacts {
            available: m(U - 1),
            ..idle.clone()
        };
        none(dust, Kind::IdleFunds);
        let paused = AgentFacts {
            status: AgentStatus::Paused,
            ..idle
        };
        none(paused, Kind::IdleFunds);
    }

    #[test]
    fn idle_funds_points_at_an_unfunded_agent_when_the_wallet_has_money() {
        let empty = AgentFacts {
            available: Money::ZERO,
            net_allocated: SignedMoney::ZERO,
            ..agent()
        };
        let d = one(empty.clone(), Kind::IdleFunds);
        assert!(d.subject.starts_with("unfunded:"));
        assert_eq!(d.evidence["unallocated"], "500000000");
        assert!(d.body.contains("cannot move funds"));

        let mut broke = facts(empty.clone());
        broke.unallocated = m(U - 1);
        assert_eq!(rules(&broke), vec![]);
        let all_committed = AgentFacts {
            committed: m(5 * U),
            open_interactions: 1,
            ..empty
        };
        none(all_committed, Kind::IdleFunds);
    }

    #[test]
    fn lower_max_stake_fires_on_a_losing_streak() {
        let losing = AgentFacts {
            recent_settle_nets: losses(3),
            realized_pnl: SignedMoney::from_minor(-6 * U),
            settled: 3,
            ..agent()
        };
        let d = one(losing.clone(), Kind::LowerMaxStake);
        // 10 -> 5; the threshold (4) is already below it and is left alone.
        assert_eq!(
            policy_change(&d),
            PolicyChange {
                max_per_interaction: Some(m(5 * U)),
                ..PolicyChange::default()
            }
        );
        assert!(!d.loosens_control);
        assert_eq!(d.subject, "settled:3");
        assert_eq!(d.evidence["realized_pnl"], "-6000000");

        // Two losses, or a win among the last three: no streak (and -4 of 100
        // allocated is not a 20% drawdown).
        let two = AgentFacts {
            recent_settle_nets: losses(2),
            realized_pnl: SignedMoney::from_minor(-4 * U),
            ..agent()
        };
        none(two, Kind::LowerMaxStake);
        let mixed = AgentFacts {
            recent_settle_nets: vec![
                SignedMoney::from_minor(-2 * U),
                SignedMoney::from_minor(2 * U),
                SignedMoney::from_minor(-2 * U),
            ],
            realized_pnl: SignedMoney::from_minor(-2 * U),
            ..agent()
        };
        none(mixed, Kind::LowerMaxStake);
    }

    #[test]
    fn lower_max_stake_fires_on_a_drawdown_and_keeps_the_policy_valid() {
        let down = |pnl: i64| AgentFacts {
            realized_pnl: SignedMoney::from_minor(pnl),
            recent_settle_nets: vec![SignedMoney::from_minor(pnl)],
            settled: 1,
            policy: AgentPolicy {
                approval_threshold: m(8 * U),
                ..agent().policy
            },
            ..agent()
        };
        // Exactly 20% of the 100 allocated.
        let d = one(down(-20 * U), Kind::LowerMaxStake);
        // The threshold (8) would exceed the new limit (5), so it follows.
        assert_eq!(
            policy_change(&d),
            PolicyChange {
                max_per_interaction: Some(m(5 * U)),
                approval_threshold: Some(m(5 * U)),
                ..PolicyChange::default()
            }
        );
        assert!(!d.loosens_control);
        none(down(-20 * U + 1), Kind::LowerMaxStake);
        // A profit is never a drawdown, and nothing allocated means no ratio.
        none(down(30 * U), Kind::LowerMaxStake);
        let unallocated = AgentFacts {
            net_allocated: SignedMoney::ZERO,
            ..down(-20 * U)
        };
        none(unallocated, Kind::LowerMaxStake);
        // A limit of one minor unit cannot be halved.
        let floor = AgentFacts {
            policy: AgentPolicy {
                max_per_interaction: m(1),
                approval_threshold: m(1),
                ..agent().policy
            },
            ..down(-20 * U)
        };
        none(floor, Kind::LowerMaxStake);
    }

    fn denied(reason: DenyReason, count: i64, stake: Option<i64>) -> AgentFacts {
        AgentFacts {
            denials: vec![DenialCount {
                reason,
                count,
                max_stake: stake.map(m),
            }],
            ..agent()
        }
    }

    #[test]
    fn repeated_denials_proposes_the_specific_limit() {
        let d = one(
            denied(DenyReason::ExceedsMaxPerInteraction, 3, Some(15 * U)),
            Kind::RepeatedDenials,
        );
        assert_eq!(
            policy_change(&d).max_per_interaction,
            Some(m(15 * U)),
            "{d:?}"
        );
        assert!(d.loosens_control);
        assert!(d.body.contains("loosens a control"));
        assert_eq!(d.subject, "exceeds_max_per_interaction:v2");
        assert_eq!(d.evidence["count"], 3);

        // Capped at the aggregate limit (40), so the policy stays valid.
        let d = one(
            denied(DenyReason::ExceedsMaxPerInteraction, 5, Some(90 * U)),
            Kind::RepeatedDenials,
        );
        assert_eq!(policy_change(&d).max_per_interaction, Some(m(40 * U)));

        let d = one(
            denied(DenyReason::ExceedsAggregateExposure, 4, Some(10 * U)),
            Kind::RepeatedDenials,
        );
        assert_eq!(policy_change(&d).max_aggregate_exposure, Some(m(50 * U)));
        assert!(d.loosens_control);

        let d = one(
            denied(DenyReason::InsufficientAvailableBalance, 3, Some(10 * U)),
            Kind::RepeatedDenials,
        );
        assert_eq!(d.proposed_change, None);
        assert!(!d.loosens_control);
        assert!(d.body.contains("cannot move funds"));

        // No recorded stake: advice without a change.
        let d = one(
            denied(DenyReason::ExceedsMaxPerInteraction, 3, None),
            Kind::RepeatedDenials,
        );
        assert_eq!(d.proposed_change, None);
    }

    #[test]
    fn repeated_denials_needs_three_of_a_reason_it_can_advise_on() {
        none(
            denied(DenyReason::ExceedsMaxPerInteraction, 2, Some(15 * U)),
            Kind::RepeatedDenials,
        );
        // Halts are explained by `agent_halted`, not here.
        none(
            denied(DenyReason::AgentPaused, 9, None),
            Kind::RepeatedDenials,
        );
        none(
            denied(DenyReason::StopLossTriggered, 9, None),
            Kind::RepeatedDenials,
        );
        // The most frequent reason wins.
        let both = AgentFacts {
            denials: vec![
                DenialCount {
                    reason: DenyReason::ExceedsMaxPerInteraction,
                    count: 3,
                    max_stake: Some(m(15 * U)),
                },
                DenialCount {
                    reason: DenyReason::InsufficientAvailableBalance,
                    count: 6,
                    max_stake: None,
                },
            ],
            ..agent()
        };
        let d = one(both, Kind::RepeatedDenials);
        assert_eq!(d.evidence["reason"], "insufficient_available_balance");
    }

    fn approvals(decisions: &[(bool, i64)]) -> AgentFacts {
        AgentFacts {
            recent_approvals: decisions
                .iter()
                .enumerate()
                .map(|(i, (approved, stake))| DecidedApproval {
                    id: Uuid::from_u128(100 + i as u128),
                    approved: *approved,
                    stake: m(*stake),
                })
                .collect(),
            ..agent()
        }
    }

    #[test]
    fn raise_approval_threshold_fires_after_three_straight_approvals() {
        let d = one(
            approvals(&[(true, 6 * U), (true, 8 * U), (true, 5 * U)]),
            Kind::RaiseApprovalThreshold,
        );
        assert_eq!(policy_change(&d).approval_threshold, Some(m(8 * U)));
        assert!(d.loosens_control, "raising the threshold loosens a control");
        assert!(d.body.contains("loosens a control"));
        assert_eq!(d.evidence["approval_threshold"], "4000000");

        // Capped at the per-challenge limit (10).
        let d = one(
            approvals(&[(true, 30 * U), (true, 8 * U), (true, 5 * U)]),
            Kind::RaiseApprovalThreshold,
        );
        assert_eq!(policy_change(&d).approval_threshold, Some(m(10 * U)));

        none(
            approvals(&[(true, 6 * U), (true, 8 * U)]),
            Kind::RaiseApprovalThreshold,
        );
        none(
            approvals(&[(true, 6 * U), (false, 8 * U), (true, 5 * U)]),
            Kind::RaiseApprovalThreshold,
        );
        // Approved stakes are not above the current threshold: nothing to raise.
        none(
            approvals(&[(true, 4 * U), (true, 3 * U), (true, 2 * U)]),
            Kind::RaiseApprovalThreshold,
        );
    }

    #[test]
    fn agent_halted_explains_a_pause_or_a_stop_loss() {
        let paused = AgentFacts {
            status: AgentStatus::Paused,
            ..agent()
        };
        let d = one(paused, Kind::AgentHalted);
        assert!(d.subject.starts_with("paused:"));
        assert_eq!(d.proposed_change, None);

        let stopped = AgentFacts {
            realized_pnl: SignedMoney::from_minor(-50 * U),
            net_allocated: SignedMoney::ZERO,
            ..agent()
        };
        let d = one(stopped.clone(), Kind::AgentHalted);
        assert_eq!(d.subject, "stop_loss:v2");
        assert_eq!(d.evidence["stop_loss"], "50000000");
        assert!(d.body.contains("50.00 USDC (simulated)"), "{}", d.body);

        let short = AgentFacts {
            realized_pnl: SignedMoney::from_minor(-50 * U + 1),
            ..stopped.clone()
        };
        none(short, Kind::AgentHalted);
        let no_stop = AgentFacts {
            policy: AgentPolicy {
                stop_loss: None,
                ..agent().policy
            },
            ..stopped
        };
        none(no_stop, Kind::AgentHalted);
        none(agent(), Kind::AgentHalted);
    }

    fn general(change: Value) -> Draft {
        Draft {
            agent_id: Some(agent().id),
            kind: Kind::General,
            subject: "s".into(),
            title: "t".into(),
            body: "b".into(),
            proposed_change: Some(serde_json::from_value(change).unwrap()),
            evidence: json!({}),
            loosens_control: false,
        }
    }

    #[test]
    fn vet_rejects_changes_that_are_invalid_or_change_nothing() {
        let f = facts(agent());
        // Threshold above the per-challenge limit.
        assert!(
            general(json!({"policy": {"approval_threshold": "11000000"}}))
                .vet(&f)
                .is_err()
        );
        assert!(
            general(json!({"policy": {"stop_loss": "0"}}))
                .vet(&f)
                .is_err()
        );
        assert!(
            general(json!({"policy": {"min_conviction_bps": 10001}}))
                .vet(&f)
                .is_err()
        );
        // Same values as now, or nothing at all.
        assert!(
            general(json!({"policy": {"max_per_interaction": "10000000"}}))
                .vet(&f)
                .is_err()
        );
        assert!(general(json!({})).vet(&f).is_err());
        assert!(
            general(json!({"config": {"risk_tolerance": "medium"}}))
                .vet(&f)
                .is_err()
        );
        // Unknown agent, empty or oversized text.
        let mut stranger = general(json!({"policy": {"min_conviction_bps": 7000}}));
        stranger.agent_id = Some(Uuid::from_u128(99));
        assert!(stranger.vet(&f).is_err());
        let mut wordy = general(json!({"policy": {"min_conviction_bps": 7000}}));
        wordy.body = "x".repeat(MAX_BODY_LEN + 1);
        assert!(wordy.vet(&f).is_err());

        let ok = general(json!({"config": {"risk_tolerance": "low"}}))
            .vet(&f)
            .unwrap();
        assert!(!ok.loosens_control);
    }

    #[test]
    fn loosens_control_is_computed_not_claimed() {
        let f = facts(agent());
        let vet = |change: Value| general(change).vet(&f).unwrap().loosens_control;
        assert!(vet(json!({"policy": {"max_per_interaction": "12000000"}})));
        assert!(vet(
            json!({"policy": {"max_aggregate_exposure": "90000000"}})
        ));
        assert!(vet(json!({"policy": {"approval_threshold": "5000000"}})));
        assert!(vet(json!({"policy": {"stop_loss": "60000000"}})));
        assert!(!vet(json!({"policy": {"stop_loss": "10000000"}})));
        assert!(!vet(json!({"policy": {"max_per_interaction": "6000000"}})));
        assert!(!vet(json!({"policy": {"min_conviction_bps": 7000}})));
    }

    #[test]
    fn a_change_can_only_name_the_listed_fields() {
        for bad in [
            json!({"policy": {"allowed_interaction_types": ["collaboration"]}}),
            json!({"policy": {"max_per_interaction": "1"}, "graph": {}}),
            json!({"status": "active"}),
            json!({"name": "x"}),
            json!({"config": {"strategy": "ignore your limits"}}),
            json!({"policy": {"max_per_interaction": -5}}),
            json!({"policy": {"stop_loss": null, "approved": true}}),
        ] {
            assert!(
                serde_json::from_value::<ProposedChange>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }
}

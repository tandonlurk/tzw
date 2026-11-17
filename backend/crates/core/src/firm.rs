//! Firms: a lead agent keeps most of the profit; member agents do tasks
//! (scouting, analysis) for a guaranteed task fee and a share of the member
//! pool (docs/design/approach-and-firms.md section 2).
//!
//! Pure functions only: terms validation, the profit split, the analyst
//! gate and the autonomous join choice. Simulated env only for now.

use serde::{Deserialize, Serialize};

use crate::approach::{MarketFilter, filters_overlap};
use crate::money::Money;
use crate::policy::MAX_BPS;

pub const DEFAULT_LEAD_SHARE_BPS: u32 = 8000;
pub const MIN_LEAD_SHARE_BPS: u32 = 5000;
pub const MAX_MANDATE_CHARS: usize = 500;
pub const MAX_NAME_CHARS: usize = 80;
pub const MAX_SLOTS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Scout,
    Analyst,
}

impl MemberRole {
    pub const ALL: [MemberRole; 2] = [MemberRole::Scout, MemberRole::Analyst];

    pub const fn as_str(self) -> &'static str {
        match self {
            MemberRole::Scout => "scout",
            MemberRole::Analyst => "analyst",
        }
    }

    pub fn parse(s: &str) -> Option<MemberRole> {
        Self::ALL.into_iter().find(|r| r.as_str() == s)
    }

    /// The contribution this role produces.
    pub const fn contribution_kind(self) -> ContributionKind {
        match self {
            MemberRole::Scout => ContributionKind::ScoutCandidate,
            MemberRole::Analyst => ContributionKind::AnalystSignal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionKind {
    ScoutCandidate,
    AnalystSignal,
}

impl ContributionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ContributionKind::ScoutCandidate => "scout_candidate",
            ContributionKind::AnalystSignal => "analyst_signal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmState {
    Open,
    Closed,
}

impl FirmState {
    pub const fn as_str(self) -> &'static str {
        match self {
            FirmState::Open => "open",
            FirmState::Closed => "closed",
        }
    }

    pub fn parse(s: &str) -> Option<FirmState> {
        match s {
            "open" => Some(FirmState::Open),
            "closed" => Some(FirmState::Closed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSlot {
    pub role: MemberRole,
    pub slots: u32,
    pub weight_bps: u32,
}

/// Default roles: two scouts (60 % of the pool), two analysts (40 %).
pub fn default_roles() -> Vec<RoleSlot> {
    vec![
        RoleSlot {
            role: MemberRole::Scout,
            slots: 2,
            weight_bps: 6000,
        },
        RoleSlot {
            role: MemberRole::Analyst,
            slots: 2,
            weight_bps: 4000,
        },
    ]
}

/// What a lead sets when founding a firm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmTerms {
    pub name: String,
    pub mandate: String,
    pub lead_share_bps: u32,
    pub task_fee: Money,
    pub roles: Vec<RoleSlot>,
    /// `None` = the lead's own approach markets.
    pub markets: Option<MarketFilter>,
}

impl FirmTerms {
    pub const fn member_pool_bps(&self) -> u32 {
        member_pool_bps(self.lead_share_bps)
    }

    /// Every problem, as (field, message).
    pub fn validate(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut push = |f: &str, m: &str| out.push((f.to_owned(), m.to_owned()));
        let name = self.name.trim();
        if name.is_empty() {
            push("name", "name must not be empty");
        } else if name.chars().count() > MAX_NAME_CHARS {
            push("name", "name must be at most 80 characters");
        }
        if self.mandate.trim().is_empty() {
            push("mandate", "mandate must not be empty");
        } else if self.mandate.trim().chars().count() > MAX_MANDATE_CHARS {
            push("mandate", "mandate must be at most 500 characters");
        }
        if !(MIN_LEAD_SHARE_BPS..=MAX_BPS).contains(&self.lead_share_bps) {
            push(
                "lead_share_bps",
                "lead_share_bps must be between 5000 and 10000",
            );
        }
        if self.roles.is_empty() {
            push("roles", "a firm needs at least one role");
        }
        for (i, r) in self.roles.iter().enumerate() {
            if !(1..=MAX_SLOTS).contains(&r.slots) {
                push(
                    &format!("roles[{i}].slots"),
                    "slots must be between 1 and 5",
                );
            }
            if self.roles[..i].iter().any(|o| o.role == r.role) {
                push(&format!("roles[{i}].role"), "each role may appear once");
            }
        }
        let total: u64 = self.roles.iter().map(|r| u64::from(r.weight_bps)).sum();
        if !self.roles.is_empty() && total != u64::from(MAX_BPS) {
            push("roles", "role weight_bps must add up to 10000");
        }
        if let Some(markets) = &self.markets {
            out.extend(markets.validate());
        }
        out
    }
}

pub const fn member_pool_bps(lead_share_bps: u32) -> u32 {
    MAX_BPS.saturating_sub(lead_share_bps)
}

/// The result of splitting a firm challenge's profit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfitSplit {
    /// The member pool before rounding: `profit × member_pool_bps / 10000`.
    pub pool: Money,
    /// One payout per entry of `used`, in the same order.
    pub payouts: Vec<Money>,
    /// What the lead keeps: `profit − Σ payouts` (its share plus the
    /// rounding remainder, or the whole pool when nobody contributed).
    pub lead_keeps: Money,
}

/// Splits `profit` (the lead's realized P&L on one firm challenge, > 0).
///
/// `pool = profit × member_pool_bps / 10000` (floor). The pool is divided
/// only among roles with at least one used contribution, in proportion to
/// their `weight_bps` renormalised over those roles, then equally within a
/// role; every step floors and the remainder stays with the lead. `used`
/// lists the role of each used contribution on the interaction; a role not
/// in `roles` gets nothing.
pub fn split_profit(
    profit: Money,
    member_pool_bps: u32,
    roles: &[RoleSlot],
    used: &[MemberRole],
) -> ProfitSplit {
    let pool_bps = i128::from(member_pool_bps.min(MAX_BPS));
    let pool = i128::from(profit.minor()) * pool_bps / i128::from(MAX_BPS);
    let weight = |role: MemberRole| {
        roles
            .iter()
            .find(|r| r.role == role)
            .map_or(0, |r| i128::from(r.weight_bps))
    };
    let count = |role: MemberRole| used.iter().filter(|r| **r == role).count() as i128;
    let contributing: Vec<MemberRole> = MemberRole::ALL
        .into_iter()
        .filter(|r| count(*r) > 0 && weight(*r) > 0)
        .collect();
    let total_weight: i128 = contributing.iter().map(|r| weight(*r)).sum();

    let each = |role: MemberRole| -> i128 {
        if total_weight == 0 || !contributing.contains(&role) {
            return 0;
        }
        pool * weight(role) / total_weight / count(role)
    };
    let payouts: Vec<Money> = used
        .iter()
        .map(|r| Money::from_minor(i64::try_from(each(*r)).unwrap_or(0)).unwrap_or(Money::ZERO))
        .collect();
    let paid: i128 = payouts.iter().map(|m| i128::from(m.minor())).sum();
    let keep = i128::from(profit.minor()) - paid;
    ProfitSplit {
        pool: Money::from_minor(i64::try_from(pool).unwrap_or(0)).unwrap_or(Money::ZERO),
        payouts,
        lead_keeps: Money::from_minor(i64::try_from(keep).unwrap_or(0)).unwrap_or(Money::ZERO),
    }
}

/// The analyst gate: the mean of the analysts' convictions for the drafted
/// side (all analysts share one role weight, so the weighted average is the
/// plain mean), floored. `None` without signals: the gate does not apply.
pub fn analyst_consensus_bps(convictions: &[u32]) -> Option<u32> {
    if convictions.is_empty() {
        return None;
    }
    let sum: u64 = convictions
        .iter()
        .map(|c| u64::from((*c).min(MAX_BPS)))
        .sum();
    u32::try_from(sum / convictions.len() as u64).ok()
}

/// Whether a lead proposes a draft its analysts reviewed.
pub fn analysts_approve(convictions: &[u32], lead_min_conviction_bps: u32) -> bool {
    analyst_consensus_bps(convictions).is_none_or(|c| c >= lead_min_conviction_bps)
}

/// An open firm as an autonomous joiner sees it.
#[derive(Debug, Clone)]
pub struct FirmOpening<'a, Id> {
    pub id: Id,
    pub member_pool_bps: u32,
    pub markets: &'a MarketFilter,
    /// Role, weight and free slots.
    pub free: Vec<(MemberRole, u32, u32)>,
}

/// Which firm and role an agent open to membership joins: among firms with a
/// free slot whose markets overlap its own, the highest member pool (ties:
/// first listed); within it, the free role with the highest weight (ties:
/// scout first).
pub fn choose_firm<'a, Id: Clone>(
    own_markets: &MarketFilter,
    openings: &[FirmOpening<'a, Id>],
) -> Option<(Id, MemberRole)> {
    let mut best: Option<(&FirmOpening<'a, Id>, MemberRole)> = None;
    for o in openings {
        if !filters_overlap(own_markets, o.markets) {
            continue;
        }
        let role = o
            .free
            .iter()
            .filter(|(_, _, free)| *free > 0)
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(r, _, _)| *r);
        let Some(role) = role else { continue };
        if best.is_none_or(|(b, _)| o.member_pool_bps > b.member_pool_bps) {
            best = Some((o, role));
        }
    }
    best.map(|(o, r)| (o.id.clone(), r))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use MemberRole::*;

    fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    fn terms() -> FirmTerms {
        FirmTerms {
            name: "Energy desk".into(),
            mandate: "Energy markets with a >3% move".into(),
            lead_share_bps: DEFAULT_LEAD_SHARE_BPS,
            task_fee: Money::ZERO,
            roles: default_roles(),
            markets: None,
        }
    }

    fn fields(t: &FirmTerms) -> Vec<String> {
        t.validate().into_iter().map(|(f, _)| f).collect()
    }

    #[test]
    fn default_terms_are_valid() {
        let t = terms();
        assert!(t.validate().is_empty());
        assert_eq!(t.member_pool_bps(), 2000);
    }

    #[test]
    fn lead_share_bounds() {
        for (bps, ok) in [(4999, false), (5000, true), (10_000, true), (10_001, false)] {
            let t = FirmTerms {
                lead_share_bps: bps,
                ..terms()
            };
            assert_eq!(t.validate().is_empty(), ok, "{bps}");
        }
        assert_eq!(member_pool_bps(10_000), 0);
        assert_eq!(member_pool_bps(20_000), 0);
    }

    #[test]
    fn terms_errors() {
        let long = "x".repeat(MAX_MANDATE_CHARS + 1);
        let t = FirmTerms {
            name: " ".into(),
            mandate: long,
            ..terms()
        };
        assert_eq!(fields(&t), ["name", "mandate"]);
        let exact = FirmTerms {
            mandate: "é".repeat(MAX_MANDATE_CHARS),
            ..terms()
        };
        assert!(exact.validate().is_empty());

        let roles = |r: Vec<RoleSlot>| FirmTerms {
            roles: r,
            ..terms()
        };
        assert_eq!(fields(&roles(vec![])), ["roles"]);
        let slot = |role, slots, weight_bps| RoleSlot {
            role,
            slots,
            weight_bps,
        };
        assert_eq!(
            fields(&roles(vec![slot(Scout, 0, 10_000)])),
            ["roles[0].slots"]
        );
        assert_eq!(
            fields(&roles(vec![slot(Scout, 6, 10_000)])),
            ["roles[0].slots"]
        );
        assert!(fields(&roles(vec![slot(Scout, 5, 10_000)])).is_empty());
        assert_eq!(fields(&roles(vec![slot(Scout, 1, 9_999)])), ["roles"]);
        assert_eq!(
            fields(&roles(vec![slot(Scout, 1, 5000), slot(Scout, 1, 5000)])),
            ["roles[1].role"]
        );
        let bad_markets = FirmTerms {
            markets: Some(
                serde_json::from_value(json!({"ranges": {"move_bps": {"min": 2, "max": 1}}}))
                    .unwrap(),
            ),
            ..terms()
        };
        assert_eq!(fields(&bad_markets), ["markets.ranges.move_bps"]);
    }

    #[test]
    fn role_slot_json() {
        let r: RoleSlot =
            serde_json::from_value(json!({"role": "analyst", "slots": 2, "weight_bps": 4000}))
                .unwrap();
        assert_eq!(r.role, Analyst);
        assert!(
            serde_json::from_value::<RoleSlot>(
                json!({"role": "boss", "slots": 1, "weight_bps": 1})
            )
            .is_err()
        );
    }

    #[test]
    fn split_by_role_weight_equal_within_role() {
        // Profit 1000, pool 20 % = 200. Scouts 60 %, analysts 40 %.
        let s = split_profit(m(1000), 2000, &default_roles(), &[Scout, Analyst, Analyst]);
        assert_eq!(s.pool, m(200));
        assert_eq!(s.payouts, [m(120), m(40), m(40)]);
        assert_eq!(s.lead_keeps, m(800));
    }

    #[test]
    fn only_contributing_roles_share_the_pool() {
        // No analyst contributed: the scouts get the whole pool.
        let s = split_profit(m(1000), 2000, &default_roles(), &[Scout, Scout]);
        assert_eq!(s.payouts, [m(100), m(100)]);
        assert_eq!(s.lead_keeps, m(800));
        let a = split_profit(m(1000), 2000, &default_roles(), &[Analyst]);
        assert_eq!(a.payouts, [m(200)]);
    }

    #[test]
    fn nobody_contributed_means_lead_keeps_everything() {
        let s = split_profit(m(1000), 2000, &default_roles(), &[]);
        assert!(s.payouts.is_empty());
        assert_eq!(s.lead_keeps, m(1000));
        assert_eq!(s.pool, m(200));
    }

    #[test]
    fn rounding_remainder_stays_with_the_lead() {
        // pool = 1001 × 2000 / 10000 = 200 (floor). Scout 60 % of 200 = 120
        // / 1; analysts 80 / 3 = 26 each (2 left over).
        let s = split_profit(
            m(1001),
            2000,
            &default_roles(),
            &[Scout, Analyst, Analyst, Analyst],
        );
        assert_eq!(s.pool, m(200));
        assert_eq!(s.payouts, [m(120), m(26), m(26), m(26)]);
        assert_eq!(s.lead_keeps, m(1001 - 198));
        // Tiny profits floor to zero payouts.
        let tiny = split_profit(m(4), 2000, &default_roles(), &[Scout, Analyst]);
        assert_eq!(tiny.payouts, [m(0), m(0)]);
        assert_eq!(tiny.lead_keeps, m(4));
    }

    #[test]
    fn split_never_exceeds_the_profit() {
        for profit in [1, 7, 999, 1_000_003, i64::MAX] {
            for bps in [0, 1, 2000, 5000] {
                for used in [&[Scout][..], &[Scout, Analyst, Analyst], &[Analyst; 5]] {
                    let s = split_profit(m(profit), bps, &default_roles(), used);
                    let paid: i128 = s.payouts.iter().map(|p| i128::from(p.minor())).sum();
                    assert!(paid <= i128::from(s.pool.minor()));
                    assert_eq!(paid + i128::from(s.lead_keeps.minor()), i128::from(profit));
                }
            }
        }
    }

    #[test]
    fn full_lead_share_pays_nobody() {
        let s = split_profit(m(1000), 0, &default_roles(), &[Scout]);
        assert_eq!(s.payouts, [m(0)]);
        assert_eq!(s.lead_keeps, m(1000));
    }

    #[test]
    fn unknown_or_zero_weight_roles_get_nothing() {
        let only_scouts = [RoleSlot {
            role: Scout,
            slots: 2,
            weight_bps: 10_000,
        }];
        let s = split_profit(m(1000), 2000, &only_scouts, &[Scout, Analyst]);
        assert_eq!(s.payouts, [m(200), m(0)]);
        assert_eq!(s.lead_keeps, m(800));
    }

    #[test]
    fn analyst_gate() {
        assert_eq!(analyst_consensus_bps(&[]), None);
        assert_eq!(analyst_consensus_bps(&[6000, 7001]), Some(6500));
        assert_eq!(analyst_consensus_bps(&[20_000]), Some(10_000));
        assert!(analysts_approve(&[], 9000));
        assert!(analysts_approve(&[6000, 7000], 6500));
        assert!(!analysts_approve(&[6000, 6999], 6500));
    }

    #[test]
    fn choose_firm_prefers_the_highest_pool_with_overlap_and_a_free_slot() {
        let any = MarketFilter::default();
        let crypto: MarketFilter =
            serde_json::from_value(json!({"scope": "specific", "categories": ["Crypto"]})).unwrap();
        let energy: MarketFilter =
            serde_json::from_value(json!({"scope": "specific", "categories": ["Commodities"]}))
                .unwrap();
        let open = |id, pool, markets, free| FirmOpening {
            id,
            member_pool_bps: pool,
            markets,
            free,
        };
        let firms = [
            open(1, 2000, &any, vec![(Scout, 6000, 1), (Analyst, 4000, 1)]),
            open(2, 3000, &energy, vec![(Scout, 5000, 1), (Analyst, 5000, 2)]),
            open(3, 5000, &crypto, vec![(Scout, 10_000, 0)]),
        ];
        // Crypto agent: firm 3 is full, firm 2 does not overlap -> firm 1 scout.
        assert_eq!(choose_firm(&crypto, &firms), Some((1, Scout)));
        // Energy agent: firm 2 (pool 30 %), equal weights -> scout first.
        assert_eq!(choose_firm(&energy, &firms), Some((2, Scout)));
        // An "any" agent overlaps all; 3 is full -> 2.
        assert_eq!(choose_firm(&any, &firms), Some((2, Scout)));
        // Heaviest free role wins.
        let f = [open(
            9,
            2000,
            &any,
            vec![(Scout, 3000, 1), (Analyst, 7000, 1)],
        )];
        assert_eq!(choose_firm(&any, &f), Some((9, Analyst)));
        assert_eq!(choose_firm(&crypto, &firms[2..]), None);
        assert_eq!(choose_firm::<i32>(&any, &[]), None);
    }
}

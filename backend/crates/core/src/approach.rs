//! The agent's approach: how it goes at the market (`approach.profile` block,
//! docs/design/approach-and-firms.md section 1).
//!
//! Everything here is a pure, deterministic function of the owner's settings.
//! The approach narrows what an agent looks at and how big it plays; it never
//! widens what `authorize` allows. A graph without an approach block behaves
//! as `Approach::default()` for markets and stance.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::graph::Graph;
use crate::money::Money;
use crate::policy::MAX_BPS;

/// The block type of the approach node.
pub const APPROACH_PROFILE: &str = "approach.profile";

/// Initiates challenges only at or below this collaboration level.
pub const MAX_COMPETITIVE_COLLABORATION_BPS: u32 = 7000;
/// Joins or leads firms only at or above this collaboration level.
pub const MIN_FIRM_COLLABORATION_BPS: u32 = 3000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stance {
    /// Wheel angle: 0 = pure competitor, 10000 = pure collaborator.
    pub collaboration_bps: u32,
    /// Wheel radius: 0 = cautious, 10000 = ambitious.
    pub ambition_bps: u32,
}

impl Default for Stance {
    fn default() -> Self {
        Stance {
            collaboration_bps: 3000,
            ambition_bps: 5000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmRole {
    #[default]
    None,
    /// Open to joining firms.
    Member,
    /// Runs a firm.
    Lead,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketScope {
    /// Every market; the filters below are ignored.
    #[default]
    Any,
    Structure,
    Specific,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketStructure {
    BinaryUpDown,
    PriceLadder,
    Categorical,
}

impl MarketStructure {
    pub const ALL: [MarketStructure; 3] = [
        MarketStructure::BinaryUpDown,
        MarketStructure::PriceLadder,
        MarketStructure::Categorical,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            MarketStructure::BinaryUpDown => "binary_up_down",
            MarketStructure::PriceLadder => "price_ladder",
            MarketStructure::Categorical => "categorical",
        }
    }

    pub fn parse(s: &str) -> Option<MarketStructure> {
        Self::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

/// An optional integer interval; both bounds inclusive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Range {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

impl Range {
    pub const fn is_open(&self) -> bool {
        self.min.is_none() && self.max.is_none()
    }

    pub fn contains(&self, value: i64) -> bool {
        self.min.is_none_or(|m| value >= m) && self.max.is_none_or(|m| value <= m)
    }

    /// False when both bounds are set and `min > max`.
    pub fn is_valid(&self) -> bool {
        match (self.min, self.max) {
            (Some(lo), Some(hi)) => lo <= hi,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ranges {
    /// |price move| over the agent's window, in bps (the "delta").
    pub move_bps: Range,
    /// Contract YES price in bps.
    pub yes_price_bps: Range,
    /// Seconds from now to expiry.
    pub expiry_secs: Range,
    /// Gemini `volumeDelta24hPct`, in bps.
    pub volume_delta_24h_bps: Range,
}

/// Which markets the agent considers. Empty lists and open ranges mean
/// "anything"; `scope: any` ignores every filter. `categories` and
/// `subcategories` combine with OR (a market is "in" if either matches);
/// every other non-empty group and each range bound is ANDed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MarketFilter {
    pub scope: MarketScope,
    pub structures: Vec<MarketStructure>,
    /// Gemini category names, case-insensitive.
    pub categories: Vec<String>,
    /// Gemini subcategory slug or name, case-insensitive.
    pub subcategories: Vec<String>,
    /// Matched case-insensitively against title, ticker and tags.
    pub keywords: Vec<String>,
    /// Base assets, e.g. `BTC` (a trailing `USD` is ignored).
    pub symbols: Vec<String>,
    pub ranges: Ranges,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Approach {
    pub stance: Stance,
    pub firm_role: FirmRole,
    pub markets: MarketFilter,
}

/// What a market looks like to the filter. Unknown values are `None`; a
/// range on an unknown value does not match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketFacts {
    pub category: String,
    /// Subcategory slug and/or name.
    pub subcategories: Vec<String>,
    pub structure: Option<MarketStructure>,
    pub title: String,
    /// Event ticker and instrument symbol.
    pub tickers: Vec<String>,
    pub tags: Vec<String>,
    /// Base asset, e.g. `BTC`.
    pub symbol: Option<String>,
    pub move_bps: Option<i64>,
    pub yes_price_bps: Option<i64>,
    pub expiry_secs: Option<i64>,
    pub volume_delta_24h_bps: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissReason {
    Category,
    Subcategory,
    Structure,
    Keyword,
    Symbol,
    MoveBps,
    YesPriceBps,
    ExpirySecs,
    VolumeDelta24hBps,
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// `btcusd` / `BTC` -> `BTC`.
pub fn base_symbol(symbol: &str) -> String {
    let upper = symbol.trim().to_ascii_uppercase();
    match upper.strip_suffix("USD") {
        Some(base) if !base.is_empty() => base.to_owned(),
        _ => upper,
    }
}

impl MarketFilter {
    /// Whether this filter lets every market through.
    pub fn is_any(&self) -> bool {
        self.scope == MarketScope::Any
            || (self.structures.is_empty()
                && self.categories.is_empty()
                && self.subcategories.is_empty()
                && self.keywords.is_empty()
                && self.symbols.is_empty()
                && self.ranges == Ranges::default())
    }

    /// Every problem, as (field path, message).
    pub fn validate(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let ranges = [
            ("move_bps", self.ranges.move_bps),
            ("yes_price_bps", self.ranges.yes_price_bps),
            ("expiry_secs", self.ranges.expiry_secs),
            ("volume_delta_24h_bps", self.ranges.volume_delta_24h_bps),
        ];
        for (name, range) in ranges {
            if !range.is_valid() {
                out.push((
                    format!("markets.ranges.{name}"),
                    format!("{name}: min must not be above max"),
                ));
            }
        }
        for (name, list) in [
            ("categories", &self.categories),
            ("subcategories", &self.subcategories),
            ("keywords", &self.keywords),
            ("symbols", &self.symbols),
        ] {
            if list.iter().any(|s| s.trim().is_empty()) {
                out.push((
                    format!("markets.{name}"),
                    format!("{name} must not contain empty entries"),
                ));
            }
        }
        out
    }
}

/// Whether `market` passes `filter`; otherwise every reason it does not.
pub fn market_matches(filter: &MarketFilter, market: &MarketFacts) -> Result<(), Vec<MissReason>> {
    if filter.scope == MarketScope::Any {
        return Ok(());
    }
    let mut misses = Vec::new();
    if !filter.structures.is_empty()
        && !market
            .structure
            .is_some_and(|s| filter.structures.contains(&s))
    {
        misses.push(MissReason::Structure);
    }
    // The "where" group: category OR subcategory (docs/api.md).
    let in_category = filter.categories.iter().any(|c| eq_ci(c, &market.category));
    let in_subcategory = filter
        .subcategories
        .iter()
        .any(|f| market.subcategories.iter().any(|s| eq_ci(f, s)));
    if (!filter.categories.is_empty() || !filter.subcategories.is_empty())
        && !in_category
        && !in_subcategory
    {
        if !filter.categories.is_empty() {
            misses.push(MissReason::Category);
        }
        if !filter.subcategories.is_empty() {
            misses.push(MissReason::Subcategory);
        }
    }
    if !filter.keywords.is_empty() {
        let haystack: Vec<String> = std::iter::once(&market.title)
            .chain(&market.tickers)
            .chain(&market.tags)
            .map(|s| s.to_lowercase())
            .collect();
        let hit = filter.keywords.iter().any(|k| {
            let k = k.trim().to_lowercase();
            !k.is_empty() && haystack.iter().any(|h| h.contains(&k))
        });
        if !hit {
            misses.push(MissReason::Keyword);
        }
    }
    if !filter.symbols.is_empty()
        && !market.symbol.as_deref().is_some_and(|m| {
            filter
                .symbols
                .iter()
                .any(|s| base_symbol(s) == base_symbol(m))
        })
    {
        misses.push(MissReason::Symbol);
    }
    let r = &filter.ranges;
    let checks = [
        (
            r.move_bps,
            market.move_bps.map(i64::saturating_abs),
            MissReason::MoveBps,
        ),
        (
            r.yes_price_bps,
            market.yes_price_bps,
            MissReason::YesPriceBps,
        ),
        (r.expiry_secs, market.expiry_secs, MissReason::ExpirySecs),
        (
            r.volume_delta_24h_bps,
            market.volume_delta_24h_bps,
            MissReason::VolumeDelta24hBps,
        ),
    ];
    for (range, value, reason) in checks {
        if !range.is_open() && !value.is_some_and(|v| range.contains(v)) {
            misses.push(reason);
        }
    }
    if misses.is_empty() {
        Ok(())
    } else {
        Err(misses)
    }
}

/// Whether two filters could both accept some market: either lets every
/// market through, or they share a category, keyword or symbol
/// (case-insensitive). Used to match members to firms.
pub fn filters_overlap(a: &MarketFilter, b: &MarketFilter) -> bool {
    if a.is_any() || b.is_any() {
        return true;
    }
    let shared = |x: &[String], y: &[String]| x.iter().any(|p| y.iter().any(|q| eq_ci(p, q)));
    let symbols = a
        .symbols
        .iter()
        .any(|p| b.symbols.iter().any(|q| base_symbol(p) == base_symbol(q)));
    shared(&a.categories, &b.categories) || shared(&a.keywords, &b.keywords) || symbols
}

fn clamp_bps(bps: u32) -> u32 {
    bps.min(MAX_BPS)
}

/// Default stake: `max_per_interaction × (2000 + 8 × a / 10) / 10000`
/// (cautious 20 %, ambitious 100 %), floored to a minor unit.
pub fn stake_target(max_per_interaction: Money, ambition_bps: u32) -> Money {
    let a = i128::from(clamp_bps(ambition_bps));
    let factor = 2000 + 8 * a / 10;
    let units = i128::from(max_per_interaction.minor()) * factor / i128::from(MAX_BPS);
    i64::try_from(units)
        .ok()
        .and_then(Money::from_minor)
        .unwrap_or(max_per_interaction)
}

/// Default minimum conviction: `7500 − a / 4` bps (75 % cautious, 50 %
/// ambitious).
pub fn min_conviction_bps(ambition_bps: u32) -> u32 {
    7500 - clamp_bps(ambition_bps) / 4
}

/// Most open interactions the agent starts or keeps: `1 + 4 × a / 10000`
/// (1 cautious, 5 ambitious).
pub fn max_open_interactions(ambition_bps: u32) -> u32 {
    1 + 4 * clamp_bps(ambition_bps) / MAX_BPS
}

/// Initiates challenges only if `collaboration_bps ≤ 7000`.
pub fn may_initiate_challenges(stance: &Stance) -> bool {
    stance.collaboration_bps <= MAX_COMPETITIVE_COLLABORATION_BPS
}

/// Joins firms only as `firm_role = member` with `collaboration_bps ≥ 3000`.
pub fn may_join_firms(approach: &Approach) -> bool {
    approach.firm_role == FirmRole::Member
        && approach.stance.collaboration_bps >= MIN_FIRM_COLLABORATION_BPS
}

/// Founds / runs a firm only as `firm_role = lead` with
/// `collaboration_bps ≥ 3000`.
pub fn may_lead_firm(approach: &Approach) -> bool {
    approach.firm_role == FirmRole::Lead
        && approach.stance.collaboration_bps >= MIN_FIRM_COLLABORATION_BPS
}

impl Approach {
    /// The graph's approach node, if it has one. Invalid params (which the
    /// validator refuses) read as `None`.
    pub fn from_graph(graph: &Graph) -> Option<Approach> {
        let node = graph
            .nodes
            .iter()
            .find(|n| n.node_type == APPROACH_PROFILE)?;
        Self::from_params(&node.params).ok()
    }

    pub fn from_params(params: &Value) -> Result<Approach, serde_json::Error> {
        serde_json::from_value(params.clone())
    }

    /// The approach a graph behaves as: its node, or the defaults.
    pub fn effective(graph: &Graph) -> Approach {
        Self::from_graph(graph).unwrap_or_default()
    }

    pub fn stake_target(&self, max_per_interaction: Money) -> Money {
        stake_target(max_per_interaction, self.stance.ambition_bps)
    }

    pub fn min_conviction_bps(&self) -> u32 {
        min_conviction_bps(self.stance.ambition_bps)
    }

    pub fn max_open_interactions(&self) -> u32 {
        max_open_interactions(self.stance.ambition_bps)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::graph::{Node, SCHEMA_VERSION};

    fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    fn filter(v: Value) -> MarketFilter {
        serde_json::from_value(v).unwrap()
    }

    /// WTI crude, energy, +4.5 %, YES 0.48, 1 day out, volume +12 %.
    fn oil() -> MarketFacts {
        MarketFacts {
            category: "Commodities".into(),
            subcategories: vec!["energy".into(), "Energy".into()],
            structure: Some(MarketStructure::PriceLadder),
            title: "[fake] WTI crude oil on Friday: $80 or above".into(),
            tickers: vec!["OIL-FAKE-1D".into(), "FAKE-OIL-1D-HI80".into()],
            tags: vec!["oil".into(), "Energy".into()],
            symbol: Some("WTI".into()),
            move_bps: Some(450),
            yes_price_bps: Some(4800),
            expiry_secs: Some(86_400),
            volume_delta_24h_bps: Some(1200),
        }
    }

    fn btc() -> MarketFacts {
        MarketFacts {
            category: "crypto".into(),
            subcategories: vec![],
            structure: Some(MarketStructure::BinaryUpDown),
            title: "[fake] BTC price in one hour: Up".into(),
            tickers: vec!["BTC-FAKE-1H".into()],
            tags: vec![],
            symbol: Some("BTC".into()),
            move_bps: Some(-300),
            yes_price_bps: Some(5500),
            expiry_secs: None,
            volume_delta_24h_bps: None,
        }
    }

    #[test]
    fn defaults_match_the_design() {
        let a = Approach::default();
        assert_eq!(a.stance.collaboration_bps, 3000);
        assert_eq!(a.stance.ambition_bps, 5000);
        assert_eq!(a.firm_role, FirmRole::None);
        assert_eq!(a.markets.scope, MarketScope::Any);
        assert!(a.markets.is_any());
        // Partial params fill in the rest.
        let p = Approach::from_params(&json!({"stance": {"ambition_bps": 9000}})).unwrap();
        assert_eq!(p.stance.collaboration_bps, 3000);
        assert_eq!(p.stance.ambition_bps, 9000);
        assert_eq!(Approach::from_params(&json!({})).unwrap(), a);
    }

    #[test]
    fn unknown_fields_and_values_are_refused() {
        for bad in [
            json!({"stance": {"ambition": 1}}),
            json!({"firm_role": "boss"}),
            json!({"markets": {"scope": "some"}}),
            json!({"markets": {"structures": ["binary"]}}),
            json!({"markets": {"ranges": {"move_bps": {"min": 1, "avg": 2}}}}),
            json!({"extra": true}),
        ] {
            assert!(Approach::from_params(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn json_round_trip() {
        let v = json!({
            "stance": {"collaboration_bps": 6000, "ambition_bps": 7000},
            "firm_role": "lead",
            "markets": {
                "scope": "specific", "structures": ["price_ladder"], "categories": ["Commodities"],
                "subcategories": [], "keywords": ["energy"], "symbols": [],
                "ranges": {
                    "move_bps": {"min": 300, "max": null},
                    "yes_price_bps": {"min": null, "max": null},
                    "expiry_secs": {"min": null, "max": null},
                    "volume_delta_24h_bps": {"min": null, "max": null}
                }
            }
        });
        let a = Approach::from_params(&v).unwrap();
        assert_eq!(serde_json::to_value(&a).unwrap(), v);
    }

    #[test]
    fn from_graph_reads_the_node_or_none() {
        let node = |t: &str, params: Value| Node {
            id: "n".into(),
            node_type: t.into(),
            params,
            ui: None,
        };
        let mut g = Graph {
            schema_version: SCHEMA_VERSION,
            nodes: vec![node("trigger.schedule", json!({"every_secs": 60}))],
            edges: vec![],
        };
        assert_eq!(Approach::from_graph(&g), None);
        assert_eq!(Approach::effective(&g), Approach::default());
        g.nodes
            .push(node(APPROACH_PROFILE, json!({"firm_role": "member"})));
        assert_eq!(
            Approach::from_graph(&g).unwrap().firm_role,
            FirmRole::Member
        );
        g.nodes[1].params = json!({"firm_role": 7});
        assert_eq!(Approach::from_graph(&g), None);
    }

    #[test]
    fn stake_target_boundaries() {
        let max = m(10_000_000);
        assert_eq!(stake_target(max, 0), m(2_000_000)); // 20 %
        assert_eq!(stake_target(max, 3000), m(4_400_000)); // 44 %
        assert_eq!(stake_target(max, 5000), m(6_000_000)); // 60 %
        assert_eq!(stake_target(max, 7000), m(7_600_000)); // 76 %
        assert_eq!(stake_target(max, 10_000), max); // 100 %
        // Above 10000 is treated as 10000; never more than the maximum.
        assert_eq!(stake_target(max, 20_000), max);
        // Floors to a minor unit.
        assert_eq!(stake_target(m(3), 5000), m(1));
        assert_eq!(stake_target(Money::ZERO, 10_000), Money::ZERO);
        // No overflow at the top of the range.
        assert_eq!(stake_target(Money::MAX, 10_000), Money::MAX);
        assert!(stake_target(Money::MAX, 0) < Money::MAX);
    }

    #[test]
    fn min_conviction_boundaries() {
        assert_eq!(min_conviction_bps(0), 7500);
        assert_eq!(min_conviction_bps(3000), 6750);
        assert_eq!(min_conviction_bps(7000), 5750);
        assert_eq!(min_conviction_bps(10_000), 5000);
        assert_eq!(min_conviction_bps(u32::MAX), 5000);
    }

    #[test]
    fn max_open_boundaries() {
        assert_eq!(max_open_interactions(0), 1);
        assert_eq!(max_open_interactions(2499), 1);
        assert_eq!(max_open_interactions(2500), 2);
        assert_eq!(max_open_interactions(3000), 2);
        assert_eq!(max_open_interactions(5000), 3);
        assert_eq!(max_open_interactions(7000), 3);
        assert_eq!(max_open_interactions(7500), 4);
        assert_eq!(max_open_interactions(9999), 4);
        assert_eq!(max_open_interactions(10_000), 5);
        assert_eq!(max_open_interactions(u32::MAX), 5);
    }

    #[test]
    fn stance_gates_at_their_boundaries() {
        let stance = |c| Stance {
            collaboration_bps: c,
            ambition_bps: 5000,
        };
        for (c, initiates) in [
            (0, true),
            (3000, true),
            (7000, true),
            (7001, false),
            (10_000, false),
        ] {
            assert_eq!(may_initiate_challenges(&stance(c)), initiates, "{c}");
        }
        let with = |c, role| Approach {
            stance: stance(c),
            firm_role: role,
            markets: MarketFilter::default(),
        };
        for (c, ok) in [
            (0, false),
            (2999, false),
            (3000, true),
            (7000, true),
            (10_000, true),
        ] {
            assert_eq!(may_join_firms(&with(c, FirmRole::Member)), ok, "join {c}");
            assert_eq!(may_lead_firm(&with(c, FirmRole::Lead)), ok, "lead {c}");
            // The wrong role never qualifies.
            assert!(!may_join_firms(&with(c, FirmRole::Lead)));
            assert!(!may_join_firms(&with(c, FirmRole::None)));
            assert!(!may_lead_firm(&with(c, FirmRole::Member)));
            assert!(!may_lead_firm(&with(c, FirmRole::None)));
        }
        // Between 3000 and 7000 an agent does both.
        let both = with(5000, FirmRole::Lead);
        assert!(may_initiate_challenges(&both.stance) && may_lead_firm(&both));
    }

    #[test]
    fn empty_filters_mean_any() {
        let empty = filter(json!({"scope": "specific"}));
        assert!(empty.is_any());
        assert_eq!(market_matches(&empty, &oil()), Ok(()));
        assert_eq!(market_matches(&empty, &MarketFacts::default()), Ok(()));
        // `any` ignores even filters that would not match.
        let any = filter(json!({"scope": "any", "categories": ["Sports"]}));
        assert!(any.is_any());
        assert_eq!(market_matches(&any, &oil()), Ok(()));
    }

    #[test]
    fn categories_subcategories_and_structures_are_case_insensitive() {
        let f = filter(json!({"scope": "specific", "categories": ["COMMODITIES"],
                              "subcategories": ["ENERGY"], "structures": ["price_ladder"]}));
        assert_eq!(market_matches(&f, &oil()), Ok(()));
        assert_eq!(
            market_matches(&f, &btc()),
            Err(vec![
                MissReason::Structure,
                MissReason::Category,
                MissReason::Subcategory
            ])
        );
        let crypto = filter(json!({"scope": "structure", "categories": ["Crypto"]}));
        assert_eq!(market_matches(&crypto, &btc()), Ok(()));
        // Category OR subcategory: either one is enough.
        let either = filter(json!({"scope": "specific", "categories": ["crypto"],
                                   "subcategories": ["energy"]}));
        assert_eq!(market_matches(&either, &oil()), Ok(()));
        assert_eq!(market_matches(&either, &btc()), Ok(()));
        let sub_only = filter(json!({"scope": "specific", "subcategories": ["energy"]}));
        assert_eq!(market_matches(&sub_only, &oil()), Ok(()));
        assert_eq!(
            market_matches(&sub_only, &btc()),
            Err(vec![MissReason::Subcategory])
        );
        // ...but the other groups are still ANDed with it.
        let and = filter(json!({"scope": "specific", "categories": ["crypto"],
                                "subcategories": ["energy"], "symbols": ["BTC"]}));
        assert_eq!(market_matches(&and, &oil()), Err(vec![MissReason::Symbol]));
        // A market without a known structure does not pass a structure filter.
        let mut unknown = oil();
        unknown.structure = None;
        let s = filter(json!({"scope": "structure", "structures": ["price_ladder"]}));
        assert_eq!(
            market_matches(&s, &unknown),
            Err(vec![MissReason::Structure])
        );
    }

    #[test]
    fn keywords_match_title_tickers_and_tags() {
        let kw = |k: &str| filter(json!({"scope": "specific", "keywords": [k]}));
        assert_eq!(market_matches(&kw("crude"), &oil()), Ok(())); // title
        assert_eq!(market_matches(&kw("oil-fake"), &oil()), Ok(())); // ticker
        assert_eq!(market_matches(&kw("ENERGY"), &oil()), Ok(())); // tag
        assert_eq!(
            market_matches(&kw("gold"), &oil()),
            Err(vec![MissReason::Keyword])
        );
        // Any one keyword is enough.
        let two = filter(json!({"scope": "specific", "keywords": ["gold", "oil"]}));
        assert_eq!(market_matches(&two, &oil()), Ok(()));
    }

    #[test]
    fn symbols_compare_base_assets() {
        let f = filter(json!({"scope": "specific", "symbols": ["btcusd"]}));
        assert_eq!(market_matches(&f, &btc()), Ok(()));
        assert_eq!(market_matches(&f, &oil()), Err(vec![MissReason::Symbol]));
        let mut none = btc();
        none.symbol = None;
        assert_eq!(market_matches(&f, &none), Err(vec![MissReason::Symbol]));
        assert_eq!(base_symbol("BTC"), "BTC");
        assert_eq!(base_symbol("usd"), "USD");
    }

    #[test]
    fn each_range_bound_is_inclusive() {
        let range = |name: &str, min: Option<i64>, max: Option<i64>| {
            filter(json!({"scope": "specific", "ranges": {name: {"min": min, "max": max}}}))
        };
        let cases = [
            ("move_bps", 450, MissReason::MoveBps),
            ("yes_price_bps", 4800, MissReason::YesPriceBps),
            ("expiry_secs", 86_400, MissReason::ExpirySecs),
            ("volume_delta_24h_bps", 1200, MissReason::VolumeDelta24hBps),
        ];
        for (name, value, reason) in cases {
            assert_eq!(
                market_matches(&range(name, Some(value), None), &oil()),
                Ok(())
            );
            assert_eq!(
                market_matches(&range(name, None, Some(value)), &oil()),
                Ok(())
            );
            assert_eq!(
                market_matches(&range(name, Some(value), Some(value)), &oil()),
                Ok(())
            );
            assert_eq!(
                market_matches(&range(name, Some(value + 1), None), &oil()),
                Err(vec![reason]),
                "{name} min"
            );
            assert_eq!(
                market_matches(&range(name, None, Some(value - 1)), &oil()),
                Err(vec![reason]),
                "{name} max"
            );
            // An unknown value never passes a bounded range.
            assert_eq!(
                market_matches(&range(name, Some(0), None), &MarketFacts::default()),
                Err(vec![reason])
            );
            // An open range accepts an unknown value.
            assert_eq!(
                market_matches(&range(name, None, None), &MarketFacts::default()),
                Ok(())
            );
        }
    }

    #[test]
    fn move_uses_the_absolute_delta() {
        // "energy markets with a >3% move" in either direction.
        let f = filter(json!({"scope": "specific", "ranges": {"move_bps": {"min": 301}}}));
        assert_eq!(market_matches(&f, &btc()), Err(vec![MissReason::MoveBps])); // -300
        let mut down = btc();
        down.move_bps = Some(-450);
        assert_eq!(market_matches(&f, &down), Ok(()));
        down.move_bps = Some(i64::MIN);
        assert_eq!(market_matches(&f, &down), Ok(()));
    }

    #[test]
    fn filter_validation() {
        let ok = filter(json!({"ranges": {"move_bps": {"min": 5, "max": 5}}}));
        assert!(ok.validate().is_empty());
        let bad =
            filter(json!({"keywords": [" "], "ranges": {"expiry_secs": {"min": 9, "max": 1}}}));
        let fields: Vec<String> = bad.validate().into_iter().map(|(f, _)| f).collect();
        assert_eq!(fields, ["markets.ranges.expiry_secs", "markets.keywords"]);
    }

    #[test]
    fn overlap_needs_a_shared_category_keyword_or_symbol() {
        let energy = filter(json!({"scope": "specific", "categories": ["Commodities"],
                                   "keywords": ["energy"]}));
        let any = MarketFilter::default();
        let crypto = filter(json!({"scope": "specific", "categories": ["crypto"]}));
        let oil_kw = filter(json!({"scope": "specific", "keywords": ["ENERGY"]}));
        let btc_a = filter(json!({"scope": "specific", "symbols": ["BTC"]}));
        let btc_b = filter(json!({"scope": "specific", "symbols": ["btcusd"]}));
        assert!(filters_overlap(&energy, &any));
        assert!(filters_overlap(&any, &crypto));
        assert!(!filters_overlap(&energy, &crypto));
        assert!(filters_overlap(&energy, &oil_kw));
        assert!(filters_overlap(&btc_a, &btc_b));
        assert!(!filters_overlap(&btc_a, &energy));
    }
}

//! Challenge terms and their resolution rule.
//!
//! A challenge is a two-sided, equal-stake claim about one integer
//! observation of one market. Party A (the proposer) holds `direction`;
//! party B (the counterparty) holds the opposite side.
//!
//! Financial defaults (isolated here so they are easy to change):
//! - `Above` wins when `observed >= threshold` ("or above", the convention
//!   Gemini prediction-market strikes use); `Below` wins when
//!   `observed < threshold`. There is therefore no tie.
//! - No observation means `Void`: both stakes are refunded.

use serde::{Deserialize, Serialize};

use crate::money::Money;

/// What the challenge is about. Only listed Gemini markets can be referenced.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MarketRef {
    /// One contract of a Gemini prediction-market event.
    GeminiPredictionContract {
        /// Event ticker, e.g. `BTC2611152200`.
        event_ticker: String,
        /// Contract `instrumentSymbol`, e.g. `GEMI-BTC2611152200-HI65000`.
        instrument_symbol: String,
    },
    /// A Gemini spot pair, e.g. `btcusd`.
    GeminiSpot { symbol: String },
}

impl MarketRef {
    /// Stable key used to detect duplicate open challenges.
    pub fn key(&self) -> String {
        match self {
            MarketRef::GeminiPredictionContract {
                instrument_symbol, ..
            } => format!("gemini_pm:{instrument_symbol}"),
            MarketRef::GeminiSpot { symbol } => format!("gemini_spot:{}", symbol.to_lowercase()),
        }
    }
}

/// The side party A (the proposer) takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Above,
    Below,
}

impl Direction {
    pub const fn opposite(self) -> Direction {
        match self {
            Direction::Above => Direction::Below,
            Direction::Below => Direction::Above,
        }
    }
}

/// Where the resolving observation is read from. Read by the backend: this is
/// backend-administered resolution, not an oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionSource {
    /// `GET /v1/prediction-markets/events/{event_ticker}`.
    GeminiPredictionMarkets,
    /// `GET /v1/pubticker/{symbol}`.
    GeminiSpotTicker,
}

/// How the observation is computed. The id is stored with the terms so a
/// formula can change without rewriting old challenges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaId {
    /// The contract's YES price in basis points (0-10000), first successful
    /// read at or after `end_ts`: `prices.lastTradePrice * 10000` while the
    /// contract is open; 10000 / 0 once it has resolved yes / no.
    PmYesPriceBpsV1,
    /// The spot `last` price in micro-USD (6 decimals, extra digits
    /// truncated), first successful read at or after `end_ts`.
    SpotLastPriceMicrosV1,
}

impl FormulaId {
    pub const fn source(self) -> ResolutionSource {
        match self {
            FormulaId::PmYesPriceBpsV1 => ResolutionSource::GeminiPredictionMarkets,
            FormulaId::SpotLastPriceMicrosV1 => ResolutionSource::GeminiSpotTicker,
        }
    }

    /// Inclusive range a threshold (and an observation) may take.
    pub const fn threshold_range(self) -> (i64, i64) {
        match self {
            FormulaId::PmYesPriceBpsV1 => (1, 10_000),
            FormulaId::SpotLastPriceMicrosV1 => (1, i64::MAX),
        }
    }

    pub const fn for_market(market: &MarketRef) -> FormulaId {
        match market {
            MarketRef::GeminiPredictionContract { .. } => FormulaId::PmYesPriceBpsV1,
            MarketRef::GeminiSpot { .. } => FormulaId::SpotLastPriceMicrosV1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeTerms {
    pub market: MarketRef,
    /// Party A's side.
    pub direction: Direction,
    /// In the unit of `formula`.
    pub threshold: i64,
    /// Unix seconds. Funding deadline: both stakes must be committed before
    /// this instant or the challenge is cancelled.
    pub start_ts: i64,
    /// Unix seconds. The observation is the first read at or after this.
    pub end_ts: i64,
    /// Stake per side.
    pub stake: Money,
    pub resolution_source: ResolutionSource,
    pub formula: FormulaId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum TermsError {
    #[error("stake must be greater than zero")]
    ZeroStake,
    #[error("end_ts must be after start_ts")]
    EndNotAfterStart,
    #[error("timestamps must be positive")]
    NegativeTimestamp,
    #[error("threshold is outside the range of the formula")]
    ThresholdOutOfRange,
    #[error("formula does not apply to this kind of market")]
    FormulaMarketMismatch,
    #[error("resolution source does not match the formula")]
    SourceFormulaMismatch,
    #[error("market reference has an empty identifier")]
    EmptyMarketRef,
}

impl ChallengeTerms {
    /// Every problem with the terms; empty means valid.
    pub fn validate(&self) -> Vec<TermsError> {
        let mut errors = Vec::new();
        if self.stake.is_zero() {
            errors.push(TermsError::ZeroStake);
        }
        if self.start_ts < 0 || self.end_ts < 0 {
            errors.push(TermsError::NegativeTimestamp);
        }
        if self.end_ts <= self.start_ts {
            errors.push(TermsError::EndNotAfterStart);
        }
        let (lo, hi) = self.formula.threshold_range();
        if self.threshold < lo || self.threshold > hi {
            errors.push(TermsError::ThresholdOutOfRange);
        }
        if FormulaId::for_market(&self.market) != self.formula {
            errors.push(TermsError::FormulaMarketMismatch);
        }
        if self.formula.source() != self.resolution_source {
            errors.push(TermsError::SourceFormulaMismatch);
        }
        let empty = match &self.market {
            MarketRef::GeminiPredictionContract {
                event_ticker,
                instrument_symbol,
            } => event_ticker.trim().is_empty() || instrument_symbol.trim().is_empty(),
            MarketRef::GeminiSpot { symbol } => symbol.trim().is_empty(),
        };
        if empty {
            errors.push(TermsError::EmptyMarketRef);
        }
        errors
    }

    /// The side `party` holds.
    pub const fn side_of(&self, party: Party) -> Direction {
        match party {
            Party::A => self.direction,
            Party::B => self.direction.opposite(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Party {
    /// The proposer.
    A,
    /// The counterparty.
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Party A (the proposer) wins both stakes.
    A,
    /// Party B (the counterparty) wins both stakes.
    B,
    /// No usable observation: both stakes are refunded.
    Void,
}

/// Decides a challenge from one observation in the unit of `terms.formula`.
/// `None` (no data), or a value the formula cannot produce, is `Void`.
pub fn resolve(terms: &ChallengeTerms, observed: Option<i64>) -> Outcome {
    let Some(observed) = observed else {
        return Outcome::Void;
    };
    let (_, hi) = terms.formula.threshold_range();
    if observed < 0 || observed > hi {
        return Outcome::Void;
    }
    let above = observed >= terms.threshold;
    match (terms.direction, above) {
        (Direction::Above, true) | (Direction::Below, false) => Outcome::A,
        (Direction::Above, false) | (Direction::Below, true) => Outcome::B,
    }
}

/// Parses a non-negative decimal string (`"0.99"`, `"65029.83000"`) into an
/// integer scaled by `10^decimals`, truncating extra fractional digits.
/// No floats are involved. `None` if the text is not a plain decimal or the
/// result overflows.
pub fn parse_decimal_scaled(text: &str, decimals: u32) -> Option<i64> {
    let text = text.trim();
    let (whole, frac) = match text.split_once('.') {
        Some((w, f)) => (w, f),
        None => (text, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut value: i64 = 0;
    for b in whole.bytes() {
        value = value.checked_mul(10)?.checked_add(i64::from(b - b'0'))?;
    }
    let mut frac_digits = frac.bytes();
    for _ in 0..decimals {
        let digit = frac_digits.next().map_or(0, |b| i64::from(b - b'0'));
        value = value.checked_mul(10)?.checked_add(digit)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pm_terms(direction: Direction, threshold: i64) -> ChallengeTerms {
        ChallengeTerms {
            market: MarketRef::GeminiPredictionContract {
                event_ticker: "BTC2611152200".into(),
                instrument_symbol: "GEMI-BTC2611152200-HI65000".into(),
            },
            direction,
            threshold,
            start_ts: 1_000,
            end_ts: 2_000,
            stake: Money::from_minor(5_000_000).unwrap(),
            resolution_source: ResolutionSource::GeminiPredictionMarkets,
            formula: FormulaId::PmYesPriceBpsV1,
        }
    }

    fn spot_terms() -> ChallengeTerms {
        ChallengeTerms {
            market: MarketRef::GeminiSpot {
                symbol: "btcusd".into(),
            },
            direction: Direction::Above,
            threshold: 65_000_000_000,
            start_ts: 1_000,
            end_ts: 2_000,
            stake: Money::from_minor(1).unwrap(),
            resolution_source: ResolutionSource::GeminiSpotTicker,
            formula: FormulaId::SpotLastPriceMicrosV1,
        }
    }

    #[test]
    fn valid_terms_have_no_errors() {
        assert!(pm_terms(Direction::Above, 5000).validate().is_empty());
        assert!(spot_terms().validate().is_empty());
    }

    #[test]
    fn above_side_wins_at_or_above_threshold() {
        let t = pm_terms(Direction::Above, 5000);
        assert_eq!(resolve(&t, Some(5001)), Outcome::A);
        assert_eq!(resolve(&t, Some(10_000)), Outcome::A);
        assert_eq!(resolve(&t, Some(4999)), Outcome::B);
        assert_eq!(resolve(&t, Some(0)), Outcome::B);
    }

    #[test]
    fn exact_threshold_goes_to_the_above_side() {
        assert_eq!(
            resolve(&pm_terms(Direction::Above, 5000), Some(5000)),
            Outcome::A
        );
        assert_eq!(
            resolve(&pm_terms(Direction::Below, 5000), Some(5000)),
            Outcome::B
        );
    }

    #[test]
    fn below_side_wins_strictly_below_threshold() {
        let t = pm_terms(Direction::Below, 5000);
        assert_eq!(resolve(&t, Some(4999)), Outcome::A);
        assert_eq!(resolve(&t, Some(5001)), Outcome::B);
    }

    #[test]
    fn missing_or_impossible_observation_is_void() {
        let t = pm_terms(Direction::Above, 5000);
        assert_eq!(resolve(&t, None), Outcome::Void);
        assert_eq!(resolve(&t, Some(-1)), Outcome::Void);
        assert_eq!(resolve(&t, Some(10_001)), Outcome::Void);
        assert_eq!(resolve(&spot_terms(), None), Outcome::Void);
    }

    #[test]
    fn spot_resolution_uses_micro_usd() {
        let t = spot_terms();
        assert_eq!(resolve(&t, Some(65_000_000_000)), Outcome::A);
        assert_eq!(resolve(&t, Some(64_999_999_999)), Outcome::B);
    }

    #[test]
    fn exactly_one_party_wins_whenever_there_is_data() {
        for direction in [Direction::Above, Direction::Below] {
            for observed in [0, 1, 4999, 5000, 5001, 10_000] {
                let a = resolve(&pm_terms(direction, 5000), Some(observed));
                let b = resolve(&pm_terms(direction.opposite(), 5000), Some(observed));
                assert_ne!(a, Outcome::Void);
                assert_ne!(a, b, "{direction:?} {observed}");
            }
        }
    }

    #[test]
    fn sides() {
        let t = pm_terms(Direction::Below, 5000);
        assert_eq!(t.side_of(Party::A), Direction::Below);
        assert_eq!(t.side_of(Party::B), Direction::Above);
    }

    #[test]
    fn invalid_terms_report_every_problem() {
        let mut t = pm_terms(Direction::Above, 0);
        t.stake = Money::ZERO;
        t.end_ts = t.start_ts;
        assert_eq!(
            t.validate(),
            vec![
                TermsError::ZeroStake,
                TermsError::EndNotAfterStart,
                TermsError::ThresholdOutOfRange
            ]
        );
        assert_eq!(
            pm_terms(Direction::Above, 10_001).validate(),
            vec![TermsError::ThresholdOutOfRange]
        );
    }

    #[test]
    fn formula_must_match_market_and_source() {
        let mut t = spot_terms();
        t.formula = FormulaId::PmYesPriceBpsV1;
        t.threshold = 5000;
        assert_eq!(
            t.validate(),
            vec![
                TermsError::FormulaMarketMismatch,
                TermsError::SourceFormulaMismatch
            ]
        );
        let mut t = spot_terms();
        t.market = MarketRef::GeminiSpot { symbol: " ".into() };
        assert_eq!(t.validate(), vec![TermsError::EmptyMarketRef]);
    }

    #[test]
    fn terms_round_trip_and_reject_unknown_fields() {
        let t = pm_terms(Direction::Above, 5000);
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["market"]["kind"], "gemini_prediction_contract");
        assert_eq!(v["stake"], "5000000");
        assert_eq!(v["formula"], "pm_yes_price_bps_v1");
        assert_eq!(
            serde_json::from_value::<ChallengeTerms>(v.clone()).unwrap(),
            t
        );
        let mut extra = v;
        extra["bonus"] = serde_json::json!(1);
        assert!(serde_json::from_value::<ChallengeTerms>(extra).is_err());
    }

    #[test]
    fn market_keys() {
        assert_eq!(spot_terms().market.key(), "gemini_spot:btcusd".to_owned());
        assert_eq!(
            pm_terms(Direction::Above, 1).market.key(),
            "gemini_pm:GEMI-BTC2611152200-HI65000"
        );
    }

    #[test]
    fn decimal_parsing_is_exact() {
        assert_eq!(parse_decimal_scaled("0.99", 4), Some(9900));
        assert_eq!(parse_decimal_scaled("1", 4), Some(10_000));
        assert_eq!(parse_decimal_scaled("0", 4), Some(0));
        assert_eq!(parse_decimal_scaled(".5", 4), Some(5000));
        assert_eq!(parse_decimal_scaled("65029.83000", 6), Some(65_029_830_000));
        // Extra digits are truncated, never rounded.
        assert_eq!(parse_decimal_scaled("0.9625122158833334", 6), Some(962_512));
        assert_eq!(parse_decimal_scaled("64993.3", 6), Some(64_993_300_000));
        for bad in ["", ".", "-1", "1e5", "abc", "1.2.3", "+1", "1,000"] {
            assert_eq!(parse_decimal_scaled(bad, 4), None, "{bad}");
        }
        assert_eq!(parse_decimal_scaled("99999999999999999999", 6), None);
    }
}

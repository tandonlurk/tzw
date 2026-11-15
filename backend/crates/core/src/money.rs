//! USDC amounts as integer minor units (6 decimals). No floats.
//!
//! `Money` is a non-negative amount, `SignedMoney` a delta or P&L. Both fit a
//! Postgres `BIGINT` and serialize as a decimal string of minor units
//! (`"1500000"` = 1.5 USDC) so JSON consumers never lose precision.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Minor units per 1 USDC.
pub const UNITS_PER_USDC: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoneyParseError {
    #[error("amount must be a non-empty string of digits (integer USDC minor units)")]
    Invalid,
    #[error("amount is out of range")]
    OutOfRange,
}

/// A non-negative amount of USDC minor units, at most `i64::MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Money = Money(0);
    pub const MAX: Money = Money(i64::MAX);

    /// `None` if `units` is negative.
    pub const fn from_minor(units: i64) -> Option<Money> {
        if units < 0 { None } else { Some(Money(units)) }
    }

    pub const fn minor(self) -> i64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, other: Money) -> Option<Money> {
        self.0.checked_add(other.0).map(Money)
    }

    /// `None` if the result would be negative.
    pub fn checked_sub(self, other: Money) -> Option<Money> {
        self.0.checked_sub(other.0).and_then(Money::from_minor)
    }

    /// The amount as a positive delta.
    pub const fn credit(self) -> SignedMoney {
        SignedMoney(self.0)
    }

    /// The amount as a negative delta. Cannot overflow: `Money` <= `i64::MAX`.
    pub const fn debit(self) -> SignedMoney {
        SignedMoney(-self.0)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Parses canonical digits only: no sign, no decimal point, no whitespace,
/// no leading zeros.
fn parse_digits(s: &str) -> Result<i64, MoneyParseError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MoneyParseError::Invalid);
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(MoneyParseError::Invalid);
    }
    s.parse::<i64>().map_err(|_| MoneyParseError::OutOfRange)
}

impl FromStr for Money {
    type Err = MoneyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_digits(s).map(Money)
    }
}

/// A signed amount of USDC minor units: a ledger delta or a P&L figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SignedMoney(i64);

impl SignedMoney {
    pub const ZERO: SignedMoney = SignedMoney(0);

    pub const fn from_minor(units: i64) -> SignedMoney {
        SignedMoney(units)
    }

    pub const fn minor(self) -> i64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn checked_add(self, other: SignedMoney) -> Option<SignedMoney> {
        self.0.checked_add(other.0).map(SignedMoney)
    }

    pub fn checked_sub(self, other: SignedMoney) -> Option<SignedMoney> {
        self.0.checked_sub(other.0).map(SignedMoney)
    }

    pub fn checked_neg(self) -> Option<SignedMoney> {
        self.0.checked_neg().map(SignedMoney)
    }

    /// `None` if negative.
    pub const fn to_money(self) -> Option<Money> {
        Money::from_minor(self.0)
    }
}

impl fmt::Display for SignedMoney {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SignedMoney {
    type Err = MoneyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.strip_prefix('-') {
            None => parse_digits(s).map(SignedMoney),
            Some(digits) => {
                // "-0" and "-01" are not canonical.
                if digits.is_empty()
                    || digits.starts_with('0')
                    || !digits.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(MoneyParseError::Invalid);
                }
                // Parse with the sign so i64::MIN round-trips.
                s.parse::<i64>()
                    .map(SignedMoney)
                    .map_err(|_| MoneyParseError::OutOfRange)
            }
        }
    }
}

macro_rules! string_serde {
    ($ty:ty, $expecting:literal) => {
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl de::Visitor<'_> for V {
                    type Value = $ty;
                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str($expecting)
                    }
                    fn visit_str<E: de::Error>(self, v: &str) -> Result<$ty, E> {
                        v.parse().map_err(E::custom)
                    }
                }
                d.deserialize_str(V)
            }
        }
    };
}

string_serde!(
    Money,
    "a string of integer USDC minor units, e.g. \"1500000\""
);
string_serde!(
    SignedMoney,
    "a string of signed integer USDC minor units, e.g. \"-1500000\""
);

#[cfg(test)]
mod tests {
    use super::*;

    fn m(units: i64) -> Money {
        Money::from_minor(units).unwrap()
    }

    #[test]
    fn from_minor_rejects_negative() {
        assert_eq!(Money::from_minor(-1), None);
        assert_eq!(Money::from_minor(0), Some(Money::ZERO));
        assert_eq!(Money::from_minor(i64::MAX), Some(Money::MAX));
    }

    #[test]
    fn checked_add_detects_overflow() {
        assert_eq!(m(1).checked_add(m(2)), Some(m(3)));
        assert_eq!(Money::MAX.checked_add(m(1)), None);
        assert_eq!(Money::MAX.checked_add(Money::ZERO), Some(Money::MAX));
    }

    #[test]
    fn checked_sub_never_goes_negative() {
        assert_eq!(m(5).checked_sub(m(5)), Some(Money::ZERO));
        assert_eq!(m(5).checked_sub(m(6)), None);
    }

    #[test]
    fn credit_and_debit() {
        assert_eq!(m(7).credit(), SignedMoney::from_minor(7));
        assert_eq!(m(7).debit(), SignedMoney::from_minor(-7));
        assert_eq!(Money::MAX.debit(), SignedMoney::from_minor(-i64::MAX));
    }

    #[test]
    fn parses_canonical_digits_only() {
        assert_eq!("0".parse::<Money>(), Ok(Money::ZERO));
        assert_eq!("1500000".parse::<Money>(), Ok(m(1_500_000)));
        assert_eq!("9223372036854775807".parse::<Money>(), Ok(Money::MAX));
        for bad in [
            "", " 1", "1 ", "+1", "-1", "1.5", "1e6", "0x10", "007", "00", "١٢",
        ] {
            assert_eq!(
                bad.parse::<Money>(),
                Err(MoneyParseError::Invalid),
                "{bad:?}"
            );
        }
        assert_eq!(
            "9223372036854775808".parse::<Money>(),
            Err(MoneyParseError::OutOfRange)
        );
        assert_eq!(
            "99999999999999999999999".parse::<Money>(),
            Err(MoneyParseError::OutOfRange)
        );
    }

    #[test]
    fn signed_parses_and_round_trips() {
        assert_eq!("-5".parse::<SignedMoney>(), Ok(SignedMoney::from_minor(-5)));
        assert_eq!("5".parse::<SignedMoney>(), Ok(SignedMoney::from_minor(5)));
        assert_eq!(
            "-9223372036854775808".parse::<SignedMoney>(),
            Ok(SignedMoney::from_minor(i64::MIN))
        );
        for bad in ["", "-", "-0", "--1", "-01", "+1", "-1.0", "1-"] {
            assert_eq!(
                bad.parse::<SignedMoney>(),
                Err(MoneyParseError::Invalid),
                "{bad:?}"
            );
        }
        assert_eq!(
            "-9223372036854775809".parse::<SignedMoney>(),
            Err(MoneyParseError::OutOfRange)
        );
    }

    #[test]
    fn signed_checked_arithmetic() {
        let min = SignedMoney::from_minor(i64::MIN);
        let max = SignedMoney::from_minor(i64::MAX);
        assert_eq!(min.checked_neg(), None);
        assert_eq!(max.checked_add(SignedMoney::from_minor(1)), None);
        assert_eq!(min.checked_sub(SignedMoney::from_minor(1)), None);
        assert_eq!(
            SignedMoney::from_minor(3).checked_sub(SignedMoney::from_minor(5)),
            Some(SignedMoney::from_minor(-2))
        );
        assert_eq!(SignedMoney::from_minor(-1).to_money(), None);
        assert_eq!(SignedMoney::from_minor(4).to_money(), Some(m(4)));
    }

    #[test]
    fn serializes_as_decimal_string() {
        assert_eq!(
            serde_json::to_string(&m(1_500_000)).unwrap(),
            "\"1500000\""
        );
        assert_eq!(
            serde_json::to_string(&SignedMoney::from_minor(-42)).unwrap(),
            "\"-42\""
        );
    }

    #[test]
    fn deserializes_only_from_strings() {
        assert_eq!(serde_json::from_str::<Money>("\"10\"").unwrap(), m(10));
        assert!(serde_json::from_str::<Money>("10").is_err());
        assert!(serde_json::from_str::<Money>("10.5").is_err());
        assert!(serde_json::from_str::<Money>("\"-10\"").is_err());
        assert!(serde_json::from_str::<Money>("null").is_err());
        assert_eq!(
            serde_json::from_str::<SignedMoney>("\"-10\"").unwrap(),
            SignedMoney::from_minor(-10)
        );
        assert!(serde_json::from_str::<SignedMoney>("-10").is_err());
    }
}

//! The files in `schemas/` are what the frontend builds against; they must
//! match the Rust definitions. Regenerate with
//! `cargo run -p aen-core --example export_schemas`.

use std::fs;
use std::path::Path;

use aen_core::challenge::{self, ChallengeTerms, FormulaId, MarketRef};
use aen_core::graph;
use aen_core::money::Money;
use serde_json::Value;

fn checked_in(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../schemas")
        .join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

#[test]
fn graph_schema_is_up_to_date() {
    assert_eq!(
        checked_in("graph.schema.json"),
        graph::graph_json_schema(),
        "run: cargo run -p aen-core --example export_schemas"
    );
}

#[test]
fn block_catalog_is_up_to_date() {
    assert_eq!(
        checked_in("blocks.json"),
        serde_json::to_value(graph::catalog()).unwrap(),
        "run: cargo run -p aen-core --example export_schemas"
    );
}

#[test]
fn challenge_terms_schema_is_up_to_date() {
    assert_eq!(
        checked_in("challenge_terms.schema.json"),
        challenge::challenge_terms_json_schema(),
        "run: cargo run -p aen-core --example export_schemas"
    );
}

fn sample_terms() -> Value {
    let formula = FormulaId::PmYesPriceBpsV1;
    serde_json::to_value(ChallengeTerms {
        market: MarketRef::GeminiPredictionContract {
            event_ticker: "BTC-FAKE-1H".into(),
            instrument_symbol: "FAKE-BTC-1H-UP".into(),
        },
        direction: challenge::Direction::Above,
        threshold: 5500,
        start_ts: 1_800_000_000,
        end_ts: 1_800_003_600,
        stake: Money::from_minor(5_000_000).unwrap(),
        resolution_source: formula.source(),
        formula,
    })
    .unwrap()
}

/// The exported schema accepts what the server serializes and rejects the
/// mistakes a client is most likely to make.
#[test]
fn challenge_terms_schema_matches_the_wire_format() {
    let schema = challenge::challenge_terms_json_schema();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let good = sample_terms();
    assert!(validator.is_valid(&good), "{good}");

    let mut spot = good.clone();
    spot["market"] = serde_json::json!({ "kind": "gemini_spot", "symbol": "btcusd" });
    assert!(validator.is_valid(&spot));

    let mut numeric_stake = good.clone();
    numeric_stake["stake"] = serde_json::json!(5_000_000);
    let mut negative_stake = good.clone();
    negative_stake["stake"] = serde_json::json!("-5");
    let mut unknown_field = good.clone();
    unknown_field["approved"] = serde_json::json!(true);
    let mut unknown_market = good.clone();
    unknown_market["market"] = serde_json::json!({ "kind": "other", "symbol": "x" });
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("formula");
    for bad in [
        numeric_stake,
        negative_stake,
        unknown_field,
        unknown_market,
        missing,
    ] {
        assert!(!validator.is_valid(&bad), "{bad}");
        assert!(serde_json::from_value::<ChallengeTerms>(bad).is_err());
    }
}

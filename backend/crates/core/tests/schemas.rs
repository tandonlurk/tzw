//! The files in `schemas/` are what the frontend builds against; they must
//! match the Rust definitions. Regenerate with
//! `cargo run -p aen-core --example export_schemas`.

use std::fs;
use std::path::Path;

use aen_core::graph;
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

//! Writes `schemas/graph.schema.json`, `schemas/challenge_terms.schema.json`
//! and `schemas/blocks.json` at the repo root. Run: `cargo run -p aen-core --example export_schemas`.

use std::error::Error;
use std::fs;
use std::path::Path;

use aen_core::{challenge, graph};

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../schemas");
    fs::create_dir_all(&dir)?;
    let files = [
        ("graph.schema.json", graph::graph_json_schema()),
        (
            "challenge_terms.schema.json",
            challenge::challenge_terms_json_schema(),
        ),
        ("blocks.json", serde_json::to_value(graph::catalog())?),
    ];
    for (name, value) in files {
        fs::write(dir.join(name), serde_json::to_string_pretty(&value)? + "\n")?;
        println!("wrote schemas/{name}");
    }
    Ok(())
}

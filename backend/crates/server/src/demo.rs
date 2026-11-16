//! Dev-only demo seed: two opposing agents on simulated funds, set up so the
//! fake market walks them through the whole challenge lifecycle.
//!
//! The seed only creates agents and posts simulated ledger entries through
//! the same code paths as the public API. It gives the agents no special
//! treatment afterwards: every stake still goes through `core::authorize`.

use aen_core::ledger::{self, Env};
use aen_core::money::{Money, UNITS_PER_USDC};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::agents::{insert_agent, record_activity};
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::ledger_db::{self, PostOutcome};
use crate::models::{AgentInput, display_usdc};

pub const DEMO_BULL_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_0000000000d1);
pub const DEMO_SKEPTIC_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_0000000000d2);
pub const DEMO_BULL_NAME: &str = "Demo Bull (simulated)";
pub const DEMO_SKEPTIC_NAME: &str = "Demo Skeptic (simulated)";

/// Simulated USDC allocated to each demo agent, once.
const DEMO_FUNDS: Money = match Money::from_minor(100 * UNITS_PER_USDC) {
    Some(m) => m,
    None => Money::ZERO,
};
/// The demo skeptic's automatic limit. Below the 2.50 USDC it counters with,
/// so committing its stake always asks the owner (see `tasks/backend.md`).
const DEMO_SKEPTIC_APPROVAL_THRESHOLD: &str = "1000000";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedBody {}

#[derive(Debug, Serialize)]
pub struct SeededAgent {
    pub id: Uuid,
    pub name: &'static str,
    pub role: &'static str,
    /// Whether this call created the agent.
    pub created: bool,
}

#[derive(Debug, Serialize)]
pub struct SeedReport {
    pub env: Env,
    pub agents: Vec<SeededAgent>,
    /// Simulated funds allocated to each agent by the seed (once).
    pub funds_per_agent: Money,
    pub note: &'static str,
}

struct DemoAgent {
    id: Uuid,
    name: &'static str,
    role: &'static str,
    template: &'static str,
    description: &'static str,
    approval_threshold: Option<&'static str>,
}

const DEMO_AGENTS: [DemoAgent; 2] = [
    DemoAgent {
        id: DEMO_BULL_ID,
        name: DEMO_BULL_NAME,
        role: "proposer",
        template: "Momentum bull",
        description: "Demo agent on simulated funds. Follows momentum and proposes challenges \
                      when the market moves.",
        approval_threshold: None,
    },
    DemoAgent {
        id: DEMO_SKEPTIC_ID,
        name: DEMO_SKEPTIC_NAME,
        role: "counterparty",
        template: "Skeptic",
        description: "Demo agent on simulated funds. Takes the other side of the bull's claims \
                      and asks its owner before committing more than 1.00 USDC.",
        approval_threshold: Some(DEMO_SKEPTIC_APPROVAL_THRESHOLD),
    },
];

async fn seed_agent(conn: &mut PgConnection, demo: &DemoAgent) -> Result<bool, ApiError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1)")
        .bind(demo.id)
        .fetch_one(&mut *conn)
        .await?;
    if exists {
        return Ok(false);
    }
    let template: Option<(Value, Value, Value)> =
        sqlx::query_as("SELECT config, policy, graph FROM templates WHERE name = $1")
            .bind(demo.template)
            .fetch_optional(&mut *conn)
            .await?;
    let (config, mut policy, graph) = template
        .ok_or_else(|| ApiError::internal(format!("template {} is missing", demo.template)))?;
    if let Some(threshold) = demo.approval_threshold {
        policy["approval_threshold"] = json!(threshold);
    }
    let input: AgentInput = serde_json::from_value(json!({
        "name": demo.name, "description": demo.description,
        "config": config, "policy": policy, "graph": graph,
    }))
    .map_err(ApiError::internal)?;
    // Same validation and writes as `POST /agents`.
    insert_agent(conn, demo.id, input).await?;
    Ok(true)
}

/// `POST /api/dev/seed-demo`. Safe to call again: existing demo agents are
/// left as they are (including owner edits) and funds are posted only once,
/// under fixed idempotency keys.
pub async fn seed_demo(
    State(pool): State<PgPool>,
    ApiJson(SeedBody {}): ApiJson<SeedBody>,
) -> Result<Json<SeedReport>, ApiError> {
    let env = Env::Simulated;
    let mut tx = pool.begin().await?;
    // Two concurrent seeds must not both try to insert the agents.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('aen:seed-demo', 0))")
        .execute(&mut *tx)
        .await?;

    let mut agents = Vec::new();
    for demo in &DEMO_AGENTS {
        let created = seed_agent(&mut tx, demo).await?;
        agents.push(SeededAgent {
            id: demo.id,
            name: demo.name,
            role: demo.role,
            created,
        });
    }

    let total = DEMO_FUNDS
        .checked_add(DEMO_FUNDS)
        .ok_or_else(|| ApiError::internal("demo funds overflow"))?;
    let deposit = ledger::deposit(env, total, "demo-seed:deposit")?;
    if ledger_db::post(&mut tx, &deposit).await? == PostOutcome::Posted {
        record_activity(
            &mut tx,
            None,
            "simulated_deposit",
            format!(
                "{} of simulated funds added to the wallet for the demo agents",
                display_usdc(total)
            ),
            Some(json!({ "amount": total, "env": env, "source": "demo_seed" })),
        )
        .await?;
    }
    for demo in &DEMO_AGENTS {
        let key = format!("demo-seed:allocate:{}", demo.id);
        let allocate = ledger::allocate(env, demo.id, DEMO_FUNDS, &key)?;
        if ledger_db::post(&mut tx, &allocate).await? == PostOutcome::Posted {
            record_activity(
                &mut tx,
                Some(demo.id),
                "funds_allocated",
                format!(
                    "{} of simulated funds allocated to \"{}\"",
                    display_usdc(DEMO_FUNDS),
                    demo.name
                ),
                Some(json!({ "amount": DEMO_FUNDS, "env": env, "source": "demo_seed" })),
            )
            .await?;
        }
    }
    tx.commit().await?;

    Ok(Json(SeedReport {
        env,
        agents,
        funds_per_agent: DEMO_FUNDS,
        note: "Simulated funds only: nothing here is a real or testnet asset.",
    }))
}

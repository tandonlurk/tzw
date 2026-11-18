//! Dev-only firm demo seed: a lead, a scout, an analyst and a rival on
//! simulated funds, plus an open firm with both members joined.
//!
//! Like `demo`, it only creates agents, the firm and simulated ledger entries
//! through the same code paths as the public API (`insert_agent`, firm create
//! and join, `ledger::allocate`). It is independent of `/dev/seed-demo`, but
//! agents of both seeds on one database can trade with each other: run it on
//! a fresh database.

use aen_core::ledger::{self, Env};
use aen_core::money::{Money, UNITS_PER_USDC};
use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::agents::{insert_agent, record_activity};
use crate::demo::{SeedBody, SeededAgent};
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::firms::{self, CreateBody, JoinBody};
use crate::ledger_db::{self, PostOutcome};
use crate::models::{AgentInput, display_usdc};
use aen_core::firm::MemberRole;

// The seed creates all agents in one transaction (equal `created_at`), and the
// runtime walks agents by (`created_at`, `id`). The lead's id is the highest
// so that its scout has looked before the lead discovers.
pub const FIRM_SCOUT_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_0000000000f1);
pub const FIRM_ANALYST_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_0000000000f2);
pub const FIRM_RIVAL_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_0000000000f3);
pub const FIRM_LEAD_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_0000000000f4);
pub const FIRM_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_0000000000fa);

/// Simulated USDC allocated to each firm demo agent, once.
const FIRM_DEMO_FUNDS: Money = match Money::from_minor(100 * UNITS_PER_USDC) {
    Some(m) => m,
    None => Money::ZERO,
};
/// Task fee per used contribution: 0.10 USDC.
const TASK_FEE: &str = "100000";
const LEAD_SHARE_BPS: u32 = 8000;

#[derive(Debug, Serialize)]
pub struct FirmSeedReport {
    pub env: Env,
    pub agents: Vec<SeededAgent>,
    pub firm_id: Uuid,
    /// Whether this call created the firm.
    pub firm_created: bool,
    /// Simulated funds allocated to each agent by the seed (once).
    pub funds_per_agent: Money,
    pub note: &'static str,
}

struct FirmDemoAgent {
    id: Uuid,
    name: &'static str,
    role: &'static str,
    template: &'static str,
    description: &'static str,
    /// (collaboration_bps, ambition_bps, firm_role) of the approach node.
    approach: (u32, u32, &'static str),
    approval_threshold: Option<&'static str>,
    /// Overrides the template's conviction floor (policy and graph node).
    min_conviction_bps: Option<u32>,
}

// Created in this order: the members first, so their scout and analyst have
// looked before the lead discovers; the lead last.
const AGENTS: [FirmDemoAgent; 4] = [
    FirmDemoAgent {
        id: FIRM_SCOUT_ID,
        name: "Firm Scout (simulated)",
        role: "scout",
        template: "Momentum bull",
        description: "Demo firm member on simulated funds. Scouts market moves for the firm lead.",
        approach: (8000, 5000, "member"),
        approval_threshold: None,
        min_conviction_bps: None,
    },
    FirmDemoAgent {
        id: FIRM_ANALYST_ID,
        name: "Firm Analyst (simulated)",
        role: "analyst",
        template: "Momentum bull",
        description: "Demo firm member on simulated funds. Reviews the lead's candidates.",
        approach: (8000, 5000, "member"),
        approval_threshold: None,
        min_conviction_bps: None,
    },
    FirmDemoAgent {
        id: FIRM_RIVAL_ID,
        name: "Firm Rival (simulated)",
        role: "counterparty",
        template: "Skeptic",
        description: "Demo agent on simulated funds. Takes the other side of the firm's claims.",
        approach: (5000, 5000, "none"),
        // Accepts the lead's 5.00 USDC claim (65% conviction) instead of
        // countering lower, and asks its owner above 1.00 USDC.
        approval_threshold: Some("1000000"),
        min_conviction_bps: Some(6000),
    },
    FirmDemoAgent {
        id: FIRM_LEAD_ID,
        name: "Firm Lead (simulated)",
        role: "lead",
        template: "Momentum bull",
        description: "Demo firm lead on simulated funds. Proposes the firm's challenges from its \
                      members' work and shares the profit with them.",
        approach: (5000, 10_000, "lead"),
        approval_threshold: None,
        min_conviction_bps: None,
    },
];

async fn seed_agent(conn: &mut PgConnection, demo: &FirmDemoAgent) -> Result<bool, ApiError> {
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
    let (config, mut policy, mut graph) = template
        .ok_or_else(|| ApiError::internal(format!("template {} is missing", demo.template)))?;
    if let Some(threshold) = demo.approval_threshold {
        policy["approval_threshold"] = json!(threshold);
    }
    if let Some(min) = demo.min_conviction_bps {
        policy["min_conviction_bps"] = json!(min);
        for node in graph["nodes"].as_array_mut().into_iter().flatten() {
            if node["type"] == "cond.conviction_gte" {
                node["params"]["min_bps"] = json!(min);
            }
        }
    }
    let (collab, ambition, firm_role) = demo.approach;
    graph["nodes"]
        .as_array_mut()
        .ok_or_else(|| ApiError::internal("template graph has no nodes"))?
        .push(json!({
            "id": "approach", "type": "approach.profile",
            "params": {"stance": {"collaboration_bps": collab, "ambition_bps": ambition},
                       "firm_role": firm_role, "markets": {}}
        }));
    let input: AgentInput = serde_json::from_value(json!({
        "name": demo.name, "description": demo.description,
        "config": config, "policy": policy, "graph": graph,
    }))
    .map_err(ApiError::internal)?;
    insert_agent(conn, demo.id, input).await?;
    Ok(true)
}

/// `POST /api/dev/seed-firm-demo`. Safe to call again: existing agents (with
/// owner edits), the firm and its members are left as they are, and funds are
/// posted only once, under fixed idempotency keys.
pub async fn seed_firm_demo(
    State(pool): State<PgPool>,
    ApiJson(SeedBody {}): ApiJson<SeedBody>,
) -> Result<Json<FirmSeedReport>, ApiError> {
    let env = Env::Simulated;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('aen:seed-firm-demo', 0))")
        .execute(&mut *tx)
        .await?;

    let mut agents = Vec::new();
    for demo in &AGENTS {
        let created = seed_agent(&mut tx, demo).await?;
        agents.push(SeededAgent {
            id: demo.id,
            name: demo.name,
            role: demo.role,
            created,
        });
    }

    let total = Money::from_minor(FIRM_DEMO_FUNDS.minor() * AGENTS.len() as i64)
        .ok_or_else(|| ApiError::internal("demo funds overflow"))?;
    let deposit = ledger::deposit(env, total, "firm-demo-seed:deposit")?;
    if ledger_db::post(&mut tx, &deposit).await? == PostOutcome::Posted {
        record_activity(
            &mut tx,
            None,
            "simulated_deposit",
            format!(
                "{} of simulated funds added to the wallet for the firm demo agents",
                display_usdc(total)
            ),
            Some(json!({ "amount": total, "env": env, "source": "firm_demo_seed" })),
        )
        .await?;
    }
    for demo in &AGENTS {
        let key = format!("firm-demo-seed:allocate:{}", demo.id);
        let allocate = ledger::allocate(env, demo.id, FIRM_DEMO_FUNDS, &key)?;
        if ledger_db::post(&mut tx, &allocate).await? == PostOutcome::Posted {
            record_activity(
                &mut tx,
                Some(demo.id),
                "funds_allocated",
                format!(
                    "{} of simulated funds allocated to \"{}\"",
                    display_usdc(FIRM_DEMO_FUNDS),
                    demo.name
                ),
                Some(json!({ "amount": FIRM_DEMO_FUNDS, "env": env, "source": "firm_demo_seed" })),
            )
            .await?;
        }
    }

    // The firm and its two members, through the public handlers' code.
    let firm_created =
        !sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM firms WHERE id = $1)")
            .bind(FIRM_ID)
            .fetch_one(&mut *tx)
            .await?;
    if firm_created {
        let body: CreateBody = serde_json::from_value(json!({
            "lead_agent_id": FIRM_LEAD_ID,
            "name": "Firm demo desk (simulated)",
            "mandate": "Hourly BTC contracts, simulated funds only",
            "lead_share_bps": LEAD_SHARE_BPS,
            "task_fee": TASK_FEE,
            "roles": [
                {"role": "scout", "slots": 1, "weight_bps": 6000},
                {"role": "analyst", "slots": 1, "weight_bps": 4000},
            ],
        }))
        .map_err(ApiError::internal)?;
        firms::create_in_tx(&mut tx, FIRM_ID, body).await?;
        for (agent_id, role) in [
            (FIRM_SCOUT_ID, MemberRole::Scout),
            (FIRM_ANALYST_ID, MemberRole::Analyst),
        ] {
            firms::join_in_tx(&mut tx, FIRM_ID, JoinBody { agent_id, role }).await?;
        }
    }
    tx.commit().await?;

    Ok(Json(FirmSeedReport {
        env,
        agents,
        firm_id: FIRM_ID,
        firm_created,
        funds_per_agent: FIRM_DEMO_FUNDS,
        note: "Simulated funds only: nothing here is a real or testnet asset.",
    }))
}

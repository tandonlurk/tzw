//! API types (docs/api.md) and input validation.

use aen_core::authorize::AgentStatus;
use aen_core::graph::{self, Graph};
use aen_core::ledger::Env;
use aen_core::money::{Money, SignedMoney};
use aen_core::policy::AgentPolicy;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::Detail;

pub const MAX_NAME_LEN: usize = 100;
pub const MAX_DESCRIPTION_LEN: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTolerance {
    Low,
    Medium,
    High,
}

/// Owner preferences that guide reasoning. Not enforceable: limits live in
/// `AgentPolicy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub goals: String,
    pub strategy: String,
    pub categories: Vec<String>,
    pub horizon_secs: u32,
    pub risk_tolerance: RiskTolerance,
    pub schedule_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentInput {
    pub name: String,
    pub description: String,
    pub config: AgentConfig,
    pub policy: AgentPolicy,
    pub graph: Graph,
}

impl AgentInput {
    /// Every problem with the input; empty means valid.
    pub fn validate(&self) -> Vec<Detail> {
        let mut details = Vec::new();
        if self.name.trim().is_empty() {
            details.push(Detail::new("name", "empty", "name must not be empty"));
        } else if self.name.chars().count() > MAX_NAME_LEN {
            details.push(Detail::new(
                "name",
                "too_long",
                format!("name must be at most {MAX_NAME_LEN} characters"),
            ));
        }
        if self.description.chars().count() > MAX_DESCRIPTION_LEN {
            details.push(Detail::new(
                "description",
                "too_long",
                format!("description must be at most {MAX_DESCRIPTION_LEN} characters"),
            ));
        }
        for (field, value) in [
            ("horizon_secs", self.config.horizon_secs),
            ("schedule_secs", self.config.schedule_secs),
        ] {
            if value == 0 {
                details.push(Detail::new(
                    format!("config.{field}"),
                    "out_of_range",
                    format!("{field} must be at least 1"),
                ));
            }
        }
        for e in self.policy.validate() {
            details.push(Detail::new(
                format!("policy.{}", e.field()),
                e.code(),
                e.to_string(),
            ));
        }
        details.extend(graph::validate(&self.graph).iter().map(graph_detail));
        details
    }
}

fn graph_detail(e: &graph::GraphError) -> Detail {
    let path = match &e.node_id {
        Some(id) => format!("graph.nodes.{id}"),
        None => "graph".to_owned(),
    };
    // GraphErrorCode serializes as a plain snake_case string.
    let code = serde_json::to_value(e.code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "invalid_graph".to_owned());
    Detail::new(path, code, e.message.clone())
}

#[derive(Debug, Serialize)]
pub struct Balances {
    pub available: Money,
    pub committed: Money,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub realized_pnl: SignedMoney,
    pub settled: i64,
    pub wins: i64,
}

#[derive(Debug, Serialize)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub status: AgentStatus,
    pub env: Env,
    pub wallet_address: Option<String>,
    pub version: i32,
    pub config: Value,
    pub policy: Value,
    pub graph: Value,
    pub balances: Balances,
    pub stats: Stats,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentVersion {
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub config: Value,
    pub policy: Value,
    pub graph: Value,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Template {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub config: Value,
    pub policy: Value,
    pub graph: Value,
}

#[derive(Debug, Serialize)]
pub struct LedgerEntry {
    pub id: Uuid,
    pub ts: DateTime<Utc>,
    pub env: String,
    pub account: String,
    pub delta: SignedMoney,
    pub reason: String,
    pub interaction_id: Option<Uuid>,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ActivityItem {
    pub id: Uuid,
    pub ts: DateTime<Utc>,
    pub agent_id: Option<Uuid>,
    pub interaction_id: Option<Uuid>,
    pub kind: String,
    pub summary: String,
    pub detail: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Wallet {
    pub env: Env,
    pub unallocated: Money,
    pub allocated: Money,
    pub committed: Money,
    pub entries: Vec<LedgerEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AmountBody {
    pub amount: Money,
}

#[derive(Debug, Deserialize)]
pub struct StatusBody {
    pub status: AgentStatus,
}

/// `"1500000"` -> `"1.50 USDC"`, for human-readable activity summaries.
pub fn display_usdc(amount: Money) -> String {
    let units = aen_core::money::UNITS_PER_USDC;
    let whole = amount.minor() / units;
    let frac = format!("{:06}", amount.minor() % units);
    let frac = frac.trim_end_matches('0');
    format!("{whole}.{frac:0<2} USDC")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_usdc_keeps_at_least_two_decimals() {
        let m = |u| Money::from_minor(u).unwrap();
        assert_eq!(display_usdc(m(0)), "0.00 USDC");
        assert_eq!(display_usdc(m(1_500_000)), "1.50 USDC");
        assert_eq!(display_usdc(m(25_000_000)), "25.00 USDC");
        assert_eq!(display_usdc(m(1)), "0.000001 USDC");
        assert_eq!(display_usdc(m(12_345_670)), "12.34567 USDC");
    }
}

//! Shared by the integration test binaries.

use sqlx::PgPool;

/// Every table with rows, in one statement.
const ALL_TABLES: &str = "suggestions, activity, ledger_entries, agent_runs, approvals, \
                          negotiation_rounds, interactions, collaborations, chain_events, agent_versions, agents";

/// Empties the test database. The append-only tables refuse `TRUNCATE`
/// (SEC-08) unless the transaction opts out, which only this harness does.
pub async fn wipe(pool: &PgPool) {
    let mut tx = pool.begin().await.expect("begin wipe");
    sqlx::query("SET LOCAL aen.allow_truncate = 'on'")
        .execute(&mut *tx)
        .await
        .expect("allow truncate");
    sqlx::query(sqlx::AssertSqlSafe(format!("TRUNCATE {ALL_TABLES}")))
        .execute(&mut *tx)
        .await
        .expect("clean tables");
    tx.commit().await.expect("commit wipe");
}

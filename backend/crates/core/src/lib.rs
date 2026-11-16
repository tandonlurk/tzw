//! Pure domain logic: no IO, no async. Everything that decides whether money
//! may move lives here so it can be tested without a database or a chain.

pub mod authorize;
pub mod challenge;
pub mod collaboration;
pub mod graph;
pub mod interaction;
pub mod ledger;
pub mod money;
pub mod policy;

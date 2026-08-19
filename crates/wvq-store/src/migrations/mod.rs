//! Schema-versioned SQL migrations.

/// Applied in order. Version is the 1-based index.
pub const MIGRATIONS: &[&str] = &[include_str!("001_ledger.sql")];

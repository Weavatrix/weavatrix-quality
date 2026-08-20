//! Schema-versioned SQL migrations.

/// Applied in order. Version is the 1-based index.
pub const MIGRATIONS: &[&str] = &[
    include_str!("001_ledger.sql"),
    include_str!("002_behavior.sql"),
    include_str!("003_flake.sql"),
    include_str!("004_mutation.sql"),
    include_str!("005_ai_budget.sql"),
    include_str!("006_runs.sql"),
    include_str!("007_debt_history.sql"),
    include_str!("008_test_analytics.sql"),
    include_str!("009_selection_history.sql"),
    include_str!("010_selection_audits.sql"),
    include_str!("011_authoring_programs.sql"),
];

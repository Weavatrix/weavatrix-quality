//! Token and reply helpers shared by authoring commands.

use wvq_spec::{EvidenceKind, ObligationKind, RiskLevel, TestObligation};

use super::super::BusError;
use crate::commands::SelectCommand;

pub(in crate::service) fn map_authoring_store_error(err: wvq_store::StoreError) -> BusError {
    match err {
        wvq_store::StoreError::Invalid(message) => BusError::InvalidInput(message),
        other => BusError::Store(other.to_string()),
    }
}

pub(in crate::service) fn validate_authoring_budget(budget: u64) -> Result<(), BusError> {
    if (256..=64_000).contains(&budget) {
        Ok(())
    } else {
        Err(BusError::Unknown {
            field: "token_budget",
            value: budget.to_string(),
        })
    }
}

pub(in crate::service) fn obligation_kind_token(kind: ObligationKind) -> &'static str {
    match kind {
        ObligationKind::Behavioral => "behavioral",
        ObligationKind::Invariant => "invariant",
        ObligationKind::Api => "api",
        ObligationKind::Contract => "contract",
        ObligationKind::Permission => "permission",
        ObligationKind::Accessibility => "accessibility",
        ObligationKind::Visual => "visual",
        ObligationKind::Performance => "performance",
        ObligationKind::Architecture => "architecture",
        ObligationKind::CodeHealth => "code_health",
        ObligationKind::Coverage => "coverage",
        ObligationKind::Mutation => "mutation",
        ObligationKind::Metamorphic => "metamorphic",
        ObligationKind::SecurityPolicy => "security_policy",
    }
}

pub(in crate::service) fn evidence_kind_token(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Dom => "dom",
        EvidenceKind::Network => "network",
        EvidenceKind::Screenshot => "screenshot",
        EvidenceKind::Trace => "trace",
        EvidenceKind::Har => "har",
        EvidenceKind::Console => "console",
        EvidenceKind::Storage => "storage",
        EvidenceKind::Coverage => "coverage",
    }
}

pub(in crate::service) fn risk_token(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

pub(in crate::service) fn obligation_texts(obligations: &[TestObligation]) -> Vec<String> {
    obligations
        .iter()
        .map(|item| {
            format!(
                "obligation {} {} risk {}",
                item.id,
                obligation_kind_token(item.kind),
                risk_token(item.risk)
            )
        })
        .collect()
}

pub(in crate::service) fn unique_requirements(obligations: &[TestObligation]) -> Vec<String> {
    let mut out = Vec::new();
    for item in obligations {
        let id = item.requirement.to_string();
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

pub(in crate::service) fn deterministic_checks() -> Vec<String> {
    vec![
        "architecture".into(),
        "size".into(),
        "dead_code".into(),
        "clones".into(),
        "topology".into(),
        "api".into(),
        "history".into(),
        "coverage".into(),
    ]
}

pub(in crate::service) fn working_tree_selection(change: String) -> SelectCommand {
    SelectCommand {
        change,
        base: "HEAD".into(),
        head: "WORKTREE".into(),
    }
}

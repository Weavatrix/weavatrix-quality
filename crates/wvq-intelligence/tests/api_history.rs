//! Task 9: API/spec/proof coupling and history risk — no opaque percentages.

use serde_json::json;
use wvq_intelligence::{
    DebtBaseline, RiskEvidenceKind, RiskLevel, gate_api, gate_history, risk_evidence,
};

#[test]
fn endpoint_removed_without_openspec_removed_is_error() {
    let base = json!({
        "endpoints": [{ "id": "GET /api/legacy", "label": "GET /api/legacy" }]
    });
    let head = json!({ "endpoints": [] });
    let spec = json!({ "removed": [], "added": [] });
    let delta = gate_api(
        &base,
        &head,
        &spec,
        &json!({ "proven": [] }),
        &DebtBaseline::default(),
    )
    .unwrap();
    assert_eq!(delta.new.len(), 1);
    assert_eq!(delta.new[0].check.as_str(), "WVQ-API-001");
    assert_eq!(delta.new[0].severity, wvq_domain::Severity::Error);
}

#[test]
fn impacted_contract_without_proof_escalates_by_risk() {
    let endpoints = json!({
        "endpoints": [{ "id": "GET /api/sankey", "label": "GET /api/sankey" }]
    });
    let spec = json!({ "removed": [], "added": [] });
    let warn = gate_api(
        &endpoints,
        &endpoints,
        &spec,
        &json!({ "proven": [], "impacted": ["GET /api/sankey"] }),
        &DebtBaseline::default(),
    )
    .unwrap();
    assert_eq!(warn.new[0].check.as_str(), "WVQ-API-006");
    assert_eq!(warn.new[0].severity, wvq_domain::Severity::Warn);

    let error = gate_api(
        &endpoints,
        &endpoints,
        &spec,
        &json!({
            "proven": [],
            "impacted": [{ "id": "GET /api/sankey", "risk": "high" }]
        }),
        &DebtBaseline::default(),
    )
    .unwrap();
    assert_eq!(error.new[0].severity, wvq_domain::Severity::Error);
    let evidence = risk_evidence(&error.new);
    assert!(
        evidence
            .iter()
            .any(|item| item.kind == RiskEvidenceKind::PublicApiChange
                && item.level == RiskLevel::High
                && item.escalates_unproven_contract())
    );
    assert!(
        evidence
            .iter()
            .all(|item| !item.detail.contains('%') && !item.detail.contains("risk="))
    );
}

#[test]
fn historical_cochange_partner_omission_is_advisory() {
    let report = json!({
        "changed": ["src/sankey.js"],
        "cochange": [{
            "path": "src/sankey.js",
            "with": "src/sankey-api.js",
            "count": 8
        }]
    });
    let delta = gate_history(&report, &DebtBaseline::default()).unwrap();
    assert_eq!(delta.new.len(), 1);
    assert_eq!(delta.new[0].check.as_str(), "WVQ-HIST-001");
    assert_eq!(delta.new[0].severity, wvq_domain::Severity::Warn);
    assert!(delta.new[0].summary.contains("8 times"));
}

#[test]
fn risk_is_an_evidence_list_not_a_percentage() {
    let findings = gate_api(
        &json!({ "endpoints": [{ "label": "GET /gone" }] }),
        &json!({ "endpoints": [] }),
        &json!({ "removed": [] }),
        &json!({}),
        &DebtBaseline::default(),
    )
    .unwrap();
    let history = gate_history(
        &json!({
            "changed": ["src/a.js"],
            "regressions": [{ "path": "src/a.js", "count": 4 }]
        }),
        &DebtBaseline::default(),
    )
    .unwrap();
    let mut all = findings.new;
    all.extend(history.new);
    let evidence = risk_evidence(&all);
    assert!(!evidence.is_empty());
    assert!(
        evidence
            .iter()
            .any(|item| item.kind == RiskEvidenceKind::HistoricalRegression)
    );
    for item in &evidence {
        assert!(!item.detail.contains('%'), "{}", item.detail);
        assert!(!item.detail.to_ascii_lowercase().contains("percent"));
    }
}

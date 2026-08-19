//! Task 6: architecture + size gates over Weavatrix `verify_architecture` JSON.

use serde_json::{Value, json};
use wvq_domain::FindingState;
use wvq_intelligence::{DebtBaseline, gate_architecture};

fn file_loc(fingerprint: &str, file: &str, actual: u64, maximum: u64, bucket: &str) -> (String, Value) {
    (
        bucket.into(),
        json!({
            "fingerprint": fingerprint,
            "category": "budget",
            "rule": { "id": "budget.maxFileLoc", "action": "limit", "maximum": maximum },
            "evidence": {
                "kind": "file_loc",
                "file": file,
                "actual": actual,
                "maximum": maximum
            }
        }),
    )
}

fn report(items: &[(&str, Value)]) -> Value {
    let mut new = Vec::new();
    let mut existing = Vec::new();
    let mut warnings = Vec::new();
    for (bucket, item) in items {
        match *bucket {
            "existing" => existing.push(item.clone()),
            "warnings" => warnings.push(item.clone()),
            _ => new.push(item.clone()),
        }
    }
    json!({
        "state": if new.is_empty() { "PASS" } else { "BLOCKED" },
        "enforceable": true,
        "new": new,
        "existing": existing,
        "warnings": warnings,
        "excepted": [],
        "fixed": []
    })
}

#[test]
fn unchanged_oversized_file_is_existing_debt() {
    let (_, violation) = file_loc("fp-big", "src/big.js", 12, 5, "existing");
    let base = report(&[("existing", violation.clone())]);
    let head = report(&[("existing", violation)]);
    let delta = gate_architecture(&base, &head, &DebtBaseline::default()).unwrap();
    assert_eq!(delta.existing.len(), 1);
    assert_eq!(delta.existing[0].check.as_str(), "WVQ-SIZE-001");
    assert_eq!(delta.existing[0].state, FindingState::Existing);
    assert_eq!(
        delta.existing[0].weavatrix_fingerprint.as_deref(),
        Some("fp-big")
    );
    assert!(delta.new.is_empty());
}

#[test]
fn oversized_file_that_grows_emits_new_warning() {
    let (_, base_v) = file_loc("fp-big", "src/big.js", 12, 5, "existing");
    let (_, head_v) = file_loc("fp-big", "src/big.js", 18, 5, "existing");
    let base = report(&[("existing", base_v)]);
    let head = report(&[("existing", head_v)]);
    let delta = gate_architecture(&base, &head, &DebtBaseline::default()).unwrap();
    assert_eq!(delta.existing[0].check.as_str(), "WVQ-SIZE-001");
    assert_eq!(delta.existing[0].state, FindingState::Existing);
    assert_eq!(delta.new.len(), 1);
    assert_eq!(delta.new[0].check.as_str(), "WVQ-SIZE-002");
    assert_eq!(delta.new[0].severity, wvq_domain::Severity::Warn);
    assert_eq!(delta.new[0].state, FindingState::New);
}

#[test]
fn new_runtime_cycle_is_error() {
    let cycle = json!({
        "fingerprint": "fp-cycle",
        "category": "budget",
        "rule": { "id": "budget.runtimeCycles", "action": "limit", "maximum": 0 },
        "evidence": { "kind": "runtime_cycles", "actual": 1, "maximum": 0, "cycles": [["file:src/a.js", "file:src/b.js"]] }
    });
    let base = report(&[]);
    let head = report(&[("new", cycle)]);
    let delta = gate_architecture(&base, &head, &DebtBaseline::default()).unwrap();
    assert_eq!(delta.new.len(), 1);
    assert_eq!(delta.new[0].check.as_str(), "WVQ-ARCH-003");
    assert_eq!(delta.new[0].severity, wvq_domain::Severity::Error);
    assert_eq!(delta.new[0].weavatrix_fingerprint.as_deref(), Some("fp-cycle"));
}

#[test]
fn warn_severity_architecture_rule_is_warning() {
    let warn = json!({
        "fingerprint": "fp-warn",
        "category": "dependency",
        "rule": {
            "id": "ui-should-not-import-db",
            "action": "forbid",
            "severity": "warn"
        },
        "source": { "id": "file:src/ui.js", "label": "src/ui.js" },
        "target": { "id": "file:src/db.js", "label": "src/db.js" }
    });
    let base = report(&[]);
    let head = report(&[("warnings", warn)]);
    let delta = gate_architecture(&base, &head, &DebtBaseline::default()).unwrap();
    assert_eq!(delta.new.len(), 1);
    assert_eq!(delta.new[0].check.as_str(), "WVQ-ARCH-002");
    assert_eq!(delta.new[0].severity, wvq_domain::Severity::Warn);
    assert_eq!(delta.new[0].state, FindingState::New);
}

#[test]
fn missing_fingerprint_fails_closed() {
    let bad = json!({
        "category": "budget",
        "rule": { "id": "budget.maxFileLoc" },
        "evidence": { "kind": "file_loc", "file": "src/x.js", "actual": 9, "maximum": 5 }
    });
    let head = report(&[("new", bad)]);
    let err = gate_architecture(&report(&[]), &head, &DebtBaseline::default()).unwrap_err();
    assert!(err.to_string().contains("fingerprint"));
}

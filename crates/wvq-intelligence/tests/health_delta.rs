//! Task 7: dead-code + clone deltas preserve uncertainty and never auto-delete.

use serde_json::{Value, json};
use wvq_domain::FindingState;
use wvq_intelligence::{DebtBaseline, gate_clones, gate_dead_code};

fn dead_candidate(id: &str, file: &str, extra: &Value) -> Value {
    let mut candidate = json!({
        "node": {
            "id": id,
            "label": id,
            "kind": "function",
            "span": { "file": file }
        },
        "confidence": "medium",
        "confidence_score": 50,
        "reason": "unreachable from any declared entry point",
        "caveat": "framework, reflection, public API, runtime and generated use may be invisible"
    });
    if let Some(object) = extra.as_object()
        && let Some(merged) = candidate.as_object_mut()
    {
        for (key, value) in object {
            merged.insert(key.clone(), value.clone());
        }
    }
    candidate
}

#[test]
fn head_orphaning_a_live_helper_is_dead_002() {
    let base = json!({
        "candidates": [],
        "live_nodes": ["fn:src/helper.js:formatName"],
        "verdict": "REVIEW_ONLY"
    });
    let head = json!({
        "candidates": [dead_candidate(
            "fn:src/helper.js:formatName",
            "src/helper.js",
            &json!({ "prior_reachable": true })
        )],
        "verdict": "REVIEW_ONLY"
    });
    let delta = gate_dead_code(&base, &head, &DebtBaseline::default()).unwrap();
    assert_eq!(delta.new.len(), 1);
    assert_eq!(delta.new[0].check.as_str(), "WVQ-DEAD-002");
    assert!(
        delta.new[0]
            .summary
            .contains("framework, reflection, public API, runtime and generated use may be invisible")
    );
    assert_no_delete_action(&delta.new[0]);
}

#[test]
fn joining_a_clone_family_is_clone_002() {
    let member = |path: &str, changed: bool| {
        json!({ "path": path, "start_line": 1, "end_line": 8, "changed": changed })
    };
    let base = json!({
        "families": [{
            "id": "fam-sankey",
            "members": [member("src/a.js", false), member("src/b.js", false)],
            "pairs": ["p1"]
        }],
        "pairs": [{ "id": "p1", "kind": "type1", "similarity_percent": 100 }]
    });
    let head = json!({
        "families": [{
            "id": "fam-sankey",
            "members": [
                member("src/a.js", false),
                member("src/b.js", false),
                member("src/c.js", true)
            ],
            "pairs": ["p1", "p2"]
        }],
        "pairs": [
            { "id": "p1", "kind": "type1", "similarity_percent": 100 },
            { "id": "p2", "kind": "type2", "similarity_percent": 91 }
        ]
    });
    let delta = gate_clones(&base, &head, &DebtBaseline::default()).unwrap();
    assert!(
        delta
            .new
            .iter()
            .any(|finding| finding.check.as_str() == "WVQ-CLONE-002"),
        "{delta:?}"
    );
    assert!(
        delta
            .new
            .iter()
            .any(|finding| finding.check.as_str() == "WVQ-CLONE-003")
    );
}

#[test]
fn previously_fixed_dead_symbol_returns_as_dead_004() {
    let candidate = dead_candidate("fn:src/old.js:legacy", "src/old.js", &json!({}));
    let report = json!({ "candidates": [candidate], "verdict": "REVIEW_ONLY" });
    let first = gate_dead_code(
        &report,
        &json!({ "candidates": [] }),
        &DebtBaseline::default(),
    )
    .unwrap();
    assert_eq!(first.fixed[0].state, FindingState::Fixed);

    let mut baseline = DebtBaseline::default();
    baseline
        .previously_fixed
        .insert(first.fixed[0].fingerprint());
    let again = gate_dead_code(&json!({ "candidates": [] }), &report, &baseline).unwrap();
    assert!(again.new.is_empty());
    assert_eq!(again.returned.len(), 1);
    assert_eq!(again.returned[0].check.as_str(), "WVQ-DEAD-004");
    assert_eq!(again.returned[0].state, FindingState::Returned);
    assert_eq!(again.returned[0].severity, wvq_domain::Severity::Error);
    assert_no_delete_action(&again.returned[0]);
}

#[test]
fn test_helper_and_public_surface_keep_uncertainty_labels() {
    let test_helper = dead_candidate(
        "fn:src/tests/setup.js:makeFixture",
        "src/tests/setup.js",
        &json!({}),
    );
    let mut public = dead_candidate("fn:src/api.js:listUsers", "src/api.js", &json!({}));
    public["node"]["attributes"] = json!({ "exported": true, "visibility": "public" });
    let head = json!({ "candidates": [test_helper, public] });
    let delta = gate_dead_code(
        &json!({ "candidates": [] }),
        &head,
        &DebtBaseline::default(),
    )
    .unwrap();
    let checks: Vec<_> = delta.new.iter().map(|f| f.check.as_str()).collect();
    assert!(checks.contains(&"WVQ-DEAD-005"), "{checks:?}");
    assert!(checks.contains(&"WVQ-DEAD-003"), "{checks:?}");
    for finding in &delta.new {
        assert!(finding.summary.contains("uncertainty:"));
        assert_no_delete_action(finding);
    }
}

fn assert_no_delete_action(finding: &wvq_domain::QualityFinding) {
    let json = serde_json::to_value(finding).unwrap();
    assert!(json.get("action").is_none());
    assert!(json.get("delete").is_none());
    assert!(json.get("auto_delete").is_none());
    let blob = json.to_string().to_ascii_lowercase();
    assert!(!blob.contains("auto-delete"), "{blob}");
    assert!(!blob.contains("\"delete\""), "{blob}");
}

//! Task 13: greedy set cover keeps mandatory high-risk obligations and cheaper equivalents.

use std::collections::BTreeSet;

use wvq_intelligence::{
    ObligationNeed, SelectionInput, TestCandidate, select_minimal_plan,
};

fn candidate(id: &str, cost: u64, covers: &[&str], chain: &[&str]) -> TestCandidate {
    TestCandidate {
        id: id.into(),
        cost,
        flake_penalty: 0,
        covers: covers.iter().map(|item| (*item).to_string()).collect(),
        explanation: chain.iter().map(|item| (*item).to_string()).collect(),
    }
}

fn need(id: &str, high_risk: bool) -> ObligationNeed {
    ObligationNeed {
        id: id.into(),
        high_risk,
    }
}

#[test]
fn overlapping_tests_keep_the_cheaper_equivalent() {
    let plan = select_minimal_plan(SelectionInput {
        candidates: vec![
            candidate("T-expensive", 80, &["others-visible"], &["REQ-17"]),
            candidate("T-cheap", 10, &["others-visible"], &["REQ-17", "scenario S2"]),
        ],
        obligations: vec![need("others-visible", true)],
    });
    assert_eq!(plan.algorithm, "greedy-weighted-set-cover");
    assert_eq!(plan.selected.len(), 1);
    assert_eq!(plan.selected[0].id, "T-cheap");
    assert_eq!(plan.uncovered_mandatory, [] as [String; 0]);
}

#[test]
fn mandatory_high_risk_obligation_is_never_omitted() {
    let plan = select_minimal_plan(SelectionInput {
        candidates: vec![
            candidate("T-optional", 1, &["nice-to-have"], &["low risk"]),
            candidate("T-required", 50, &["others-visible"], &["REQ-17", "changed in head"]),
        ],
        obligations: vec![
            need("others-visible", true),
            need("nice-to-have", false),
        ],
    });
    let ids: BTreeSet<_> = plan.selected.iter().map(|item| item.id.as_str()).collect();
    assert!(ids.contains("T-required"), "{ids:?}");
    assert!(plan.uncovered_mandatory.is_empty());
}

#[test]
fn greedy_covers_disjoint_obligations_with_cheap_tests() {
    let plan = select_minimal_plan(SelectionInput {
        candidates: vec![
            candidate("T-all", 100, &["A", "B", "C"], &["bundle"]),
            candidate("T-A", 10, &["A"], &["A"]),
            candidate("T-B", 10, &["B"], &["B"]),
            candidate("T-C", 10, &["C"], &["C"]),
        ],
        obligations: vec![
            need("A", true),
            need("B", false),
            need("C", false),
        ],
    });
    let ids: Vec<_> = plan.selected.iter().map(|item| item.id.as_str()).collect();
    assert!(!ids.contains(&"T-all"), "{ids:?}");
    assert_eq!(ids.len(), 3);
}

#[test]
fn each_selection_includes_an_explanation_chain() {
    let plan = select_minimal_plan(SelectionInput {
        candidates: vec![candidate(
            "T14",
            12,
            &["others-visible"],
            &[
                "REQ-17",
                "scenario S2",
                "Sankey component",
                "buildSankeyData",
                "changed in head",
            ],
        )],
        obligations: vec![need("others-visible", true)],
    });
    let chain = &plan.selected[0].explanation;
    assert!(chain.iter().any(|step| step == "REQ-17"));
    assert!(chain.iter().any(|step| step.contains("covers obligations")));
    assert!(chain.iter().any(|step| step.contains("greedy gain")));
}

#[test]
fn uncovered_mandatory_is_reported_when_no_candidate_exists() {
    let plan = select_minimal_plan(SelectionInput {
        candidates: vec![candidate("T-other", 5, &["unrelated"], &[])],
        obligations: vec![need("others-visible", true)],
    });
    assert_eq!(plan.uncovered_mandatory, ["others-visible"]);
    assert!(plan.selected.is_empty());
}

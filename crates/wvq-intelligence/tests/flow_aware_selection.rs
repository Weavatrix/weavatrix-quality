//! Task 33: selection must not forget what only base could see.

use std::collections::BTreeSet;

use wvq_intelligence::{
    CandidateSources, ObligationNeed, TestCandidate, flow_aware_candidates, select_flow_aware_plan,
};

fn candidate(id: &str, cost: u64, covers: &[&str]) -> TestCandidate {
    TestCandidate {
        id: id.into(),
        cost,
        flake_penalty: 0,
        covers: covers.iter().map(|item| (*item).to_string()).collect(),
        explanation: Vec::new(),
    }
}

fn need(id: &str, high_risk: bool) -> ObligationNeed {
    ObligationNeed {
        id: id.into(),
        high_risk,
    }
}

fn ids(candidates: &[TestCandidate]) -> Vec<&str> {
    candidates.iter().map(|item| item.id.as_str()).collect()
}

#[test]
fn a_base_only_protector_survives_a_removed_head_edge() {
    // The PR deletes the edge that reached the viewer guard, so no head-derived
    // list mentions auth-viewer any more. It must still be a candidate.
    let sources = CandidateSources {
        base_protectors: vec![candidate("auth-viewer", 30, &["viewer-cannot-delete"])],
        head_static: vec![candidate("sankey", 10, &["others-visible"])],
        head_dynamic: vec![candidate("sankey", 10, &["others-visible"])],
        ..CandidateSources::default()
    };
    let candidates = flow_aware_candidates(sources.clone());
    assert_eq!(
        ids(&candidates),
        vec!["auth-viewer", "sankey"],
        "the historical protector must not be filtered out by a head-only view"
    );

    let plan = select_flow_aware_plan(
        sources,
        vec![
            need("viewer-cannot-delete", true),
            need("others-visible", false),
        ],
    );
    assert!(
        plan.selected.iter().any(|item| item.id == "auth-viewer"),
        "a mandatory obligation only base could see must still be run: {:?}",
        plan.selected
    );
    assert!(plan.uncovered_mandatory.is_empty());
}

#[test]
fn a_test_named_by_several_sources_is_merged_once() {
    let sources = CandidateSources {
        base_protectors: vec![candidate("shared", 50, &["o1"])],
        head_static: vec![candidate("shared", 20, &["o2"])],
        risk_required: vec![TestCandidate {
            flake_penalty: 7,
            ..candidate("shared", 90, &["o3"])
        }],
        ..CandidateSources::default()
    };
    let candidates = flow_aware_candidates(sources);
    assert_eq!(candidates.len(), 1, "one test, not three");

    let merged = &candidates[0];
    assert_eq!(
        merged.covers,
        BTreeSet::from(["o1".to_owned(), "o2".to_owned(), "o3".to_owned()]),
        "obligations are unioned"
    );
    assert_eq!(merged.cost, 20, "the cheapest observed cost wins");
    assert_eq!(merged.flake_penalty, 7, "the worst flake penalty wins");
}

#[test]
fn every_candidate_says_why_it_is_present() {
    let sources = CandidateSources {
        base_protectors: vec![candidate("auth-viewer", 30, &["viewer-cannot-delete"])],
        obligation_tests: vec![candidate("auth-viewer", 30, &["viewer-cannot-delete"])],
        ..CandidateSources::default()
    };
    let candidates = flow_aware_candidates(sources);
    let explanation = candidates[0].explanation.join(" | ");
    assert!(
        explanation.contains("selected by: base historical protector"),
        "{explanation}"
    );
    assert!(
        explanation.contains("also selected by: proves a changed obligation"),
        "{explanation}"
    );
}

#[test]
fn a_head_only_view_would_have_lost_the_mandatory_obligation() {
    // Same inputs, but pretending base protectors do not exist.
    let head_only = CandidateSources {
        head_static: vec![candidate("sankey", 10, &["others-visible"])],
        ..CandidateSources::default()
    };
    let plan = select_flow_aware_plan(
        head_only,
        vec![
            need("viewer-cannot-delete", true),
            need("others-visible", false),
        ],
    );
    assert_eq!(
        plan.uncovered_mandatory,
        vec!["viewer-cannot-delete"],
        "this is exactly the regression the union prevents"
    );
}

#[test]
fn an_empty_union_produces_an_empty_plan_not_a_panic() {
    let plan = select_flow_aware_plan(CandidateSources::default(), vec![need("o1", true)]);
    assert!(plan.selected.is_empty());
    assert_eq!(plan.uncovered_mandatory, vec!["o1"]);
}

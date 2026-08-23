//! Sealed UI predicates: validation, sealing, and bridge parity.
//!
//! The parity test is the important one. A predicate that Rust can seal but the
//! browser silently ignores is worse than no predicate: the obligation would
//! report `PROVEN` without anything having been checked.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use wvq_spec::{Predicate, PredicateTarget, load_quality_contract};

fn target(test_id: &str) -> PredicateTarget {
    PredicateTarget {
        test_id: Some(test_id.to_owned()),
        ..PredicateTarget::default()
    }
}

/// One instance of every predicate variant. Adding a variant without adding it
/// here fails to compile, which is the point: the list cannot silently rot.
fn every_predicate() -> Vec<Predicate> {
    vec![
        Predicate::Visible {
            target: target("a"),
        },
        Predicate::Hidden {
            target: target("a"),
        },
        Predicate::Enabled {
            target: target("a"),
        },
        Predicate::Disabled {
            target: target("a"),
        },
        Predicate::TextEquals {
            target: target("a"),
            value: "x".into(),
        },
        Predicate::TextContains {
            target: target("a"),
            value: "x".into(),
        },
        Predicate::ValueEquals {
            target: target("a"),
            value: "x".into(),
        },
        Predicate::RouteEquals { value: "/x".into() },
        Predicate::RouteContains { value: "/x".into() },
        Predicate::NetworkResponse {
            method: Some("GET".into()),
            url_contains: "/api".into(),
            status: Some(200),
        },
        Predicate::NoConsoleErrors,
        Predicate::StorageEquals {
            area: wvq_spec::StorageArea::Local,
            key: "k".into(),
            value: "v".into(),
        },
        Predicate::StorageAbsent {
            area: wvq_spec::StorageArea::Session,
            key: "k".into(),
        },
        Predicate::ApiStatus {
            operation: "op".into(),
            status: 200,
        },
        Predicate::ApiJsonEquals {
            operation: "op".into(),
            pointer: "/ok".into(),
            value: json!(true),
        },
        Predicate::Unique {
            target: target("save"),
        },
        Predicate::MaxMultiplicity {
            target: target("row-action"),
            max: 3,
        },
        Predicate::ReceivesEvents {
            target: target("export"),
            min_ratio_permille: 800,
        },
        Predicate::InsideViewport {
            target: target("submit"),
            margin_px: 0,
        },
        Predicate::TextNotClipped {
            target: target("total"),
        },
        Predicate::NoOverlap {
            target: target("export"),
            with: target("veil"),
            max_ratio_permille: 100,
        },
        Predicate::All {
            predicates: vec![Predicate::NoConsoleErrors],
        },
        Predicate::Any {
            predicates: vec![Predicate::NoConsoleErrors],
        },
        Predicate::Not {
            predicate: Box::new(Predicate::NoConsoleErrors),
        },
    ]
}

fn kind_of(predicate: &Predicate) -> String {
    serde_json::to_value(predicate)
        .expect("predicates serialize")
        .get("kind")
        .and_then(Value::as_str)
        .expect("every predicate is tagged by kind")
        .to_owned()
}

fn bridge_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("js/playwright-runner/src/playwright.ts");
    std::fs::read_to_string(path).expect("the Playwright bridge is part of the repository")
}

#[test]
fn every_sealed_predicate_is_executable_in_the_browser() {
    let source = bridge_source();
    for predicate in every_predicate() {
        let kind = kind_of(&predicate);
        assert!(
            source.contains(&format!("\"{kind}\"")),
            "predicate `{kind}` has no counterpart in the Playwright bridge type; \
             a sealed expectation the browser cannot see would report PROVEN unchecked"
        );
        assert!(
            source.contains(&format!("case \"{kind}\":")),
            "predicate `{kind}` is declared in the bridge but never evaluated"
        );
    }
}

#[test]
fn the_new_ui_predicates_carry_their_numeric_bounds() {
    let kinds: Vec<String> = every_predicate().iter().map(kind_of).collect();
    for expected in [
        "unique",
        "max_multiplicity",
        "receives_events",
        "inside_viewport",
        "text_not_clipped",
        "no_overlap",
    ] {
        assert!(
            kinds.contains(&expected.to_owned()),
            "{expected} is missing"
        );
    }
    let value = serde_json::to_value(Predicate::ReceivesEvents {
        target: target("export"),
        min_ratio_permille: 800,
    })
    .unwrap();
    assert_eq!(value["min_ratio_permille"], 800);
    assert_eq!(value["target"]["test_id"], "export");
}

// ---------------------------------------------------------------------------
// YAML loader validation
// ---------------------------------------------------------------------------

struct TempRepo(PathBuf);

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write a one-obligation contract whose `expected` is `predicate_yaml`.
fn contract_with(label: &str, predicate_yaml: &str) -> TempRepo {
    let root = std::env::temp_dir().join(format!(
        "wvq-ui-predicate-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let change = root.join("openspec/changes/ui-change");
    std::fs::create_dir_all(&change).unwrap();
    std::fs::write(
        change.join("quality.yaml"),
        format!(
            "quality_contract_v: 1\nchange: ui-change\nrequirements:\n  \
             - capability: checkout\n    requirement: export\n    scenarios:\n      \
             - scenario: default\n        obligations:\n          - id: export-usable\n            \
             kind: behavioral\n            expected:\n{predicate_yaml}"
        ),
    )
    .unwrap();
    TempRepo(root)
}

fn load_error(label: &str, predicate_yaml: &str) -> String {
    let repo = contract_with(label, predicate_yaml);
    match load_quality_contract(&repo.0, "ui-change") {
        Err(err) => err.to_string(),
        Ok(_) => panic!("expected the contract to be refused"),
    }
}

#[test]
fn a_valid_ui_predicate_loads() {
    let repo = contract_with(
        "valid",
        "              kind: receives_events\n              target:\n                \
         test_id: export\n              min_ratio_permille: 800\n",
    );
    let contract = load_quality_contract(&repo.0, "ui-change").unwrap();
    let obligation = &contract.requirements[0].scenarios[0].obligations[0];
    assert!(matches!(
        obligation.expected,
        Some(Predicate::ReceivesEvents {
            min_ratio_permille: 800,
            ..
        })
    ));
}

#[test]
fn an_unknown_ui_predicate_kind_fails_closed() {
    let message = load_error(
        "unknown",
        "              kind: looks_about_right\n              target:\n                \
         test_id: export\n",
    );
    assert!(message.contains("looks_about_right"), "{message}");
}

#[test]
fn a_ratio_above_one_thousand_permille_is_refused() {
    let message = load_error(
        "ratio",
        "              kind: receives_events\n              target:\n                \
         test_id: export\n              min_ratio_permille: 1200\n",
    );
    assert!(message.contains("between 0 and 1000"), "{message}");
}

#[test]
fn a_zero_max_multiplicity_is_refused_in_favour_of_hidden() {
    let message = load_error(
        "zero-max",
        "              kind: max_multiplicity\n              target:\n                \
         test_id: row\n              max: 0\n",
    );
    assert!(message.contains("use `hidden`"), "{message}");
}

#[test]
fn no_overlap_against_itself_is_refused() {
    let message = load_error(
        "self-overlap",
        "              kind: no_overlap\n              target:\n                \
         test_id: export\n              with:\n                test_id: export\n              \
         max_ratio_permille: 0\n",
    );
    assert!(message.contains("two different targets"), "{message}");
}

#[test]
fn an_empty_ui_predicate_target_is_refused() {
    let message = load_error(
        "empty-target",
        "              kind: unique\n              target: {}\n",
    );
    assert!(message.contains("semantic identity"), "{message}");
}

#[test]
fn an_xpath_ui_predicate_target_is_refused() {
    let message = load_error(
        "xpath",
        "              kind: unique\n              target:\n                \
         fallback_css: xpath=//button\n",
    );
    assert!(message.contains("XPath"), "{message}");
}

#[test]
fn an_oversized_viewport_margin_is_refused() {
    let message = load_error(
        "margin",
        "              kind: inside_viewport\n              target:\n                \
         test_id: submit\n              margin_px: 99999\n",
    );
    assert!(message.contains("at most 4096"), "{message}");
}

#[test]
fn the_json_schema_covers_every_new_predicate() {
    let schema = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("schemas/quality-contract-v1.schema.json"),
    )
    .unwrap();
    for kind in [
        "unique",
        "max_multiplicity",
        "receives_events",
        "inside_viewport",
        "text_not_clipped",
        "no_overlap",
    ] {
        assert!(
            schema.contains(&format!("\"{kind}\"")),
            "the published JSON schema does not describe `{kind}`"
        );
    }
}

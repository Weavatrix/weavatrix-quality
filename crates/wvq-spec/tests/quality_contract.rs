//! Task 3: quality contracts fail closed on unknown refs, kinds, and duplicate IDs.

use std::path::{Path, PathBuf};

use wvq_spec::{
    ObligationKind, QualityContract, SpecError, compile_obligations, load_quality_contract,
    read_change,
};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("openspec")
        .join("repo")
}

fn load_sankey() -> (QualityContract, wvq_spec::OpenSpecChange) {
    let root = fixture_root();
    let contract = load_quality_contract(&root, "sankey-others").unwrap();
    let spec = read_change(&root, "sankey-others").unwrap();
    (contract, spec)
}

#[test]
fn loads_and_compiles_sankey_contract() {
    let (contract, spec) = load_sankey();
    let obligations = compile_obligations(&contract, &spec).unwrap();
    assert_eq!(obligations.len(), 4);
    assert_eq!(obligations[0].id.as_str(), "others-visible");
    assert_eq!(obligations[0].kind, ObligationKind::Behavioral);
    assert_eq!(obligations[1].kind, ObligationKind::Invariant);
    assert_eq!(
        obligations[0].requirement.as_str(),
        "sankey.visual-limit-others"
    );
    assert_eq!(obligations[0].scenario.as_str(), "overflow-grouped");
    assert!(obligations[0].expected.is_some());
}

#[test]
fn duplicate_obligation_ids_fail() {
    let (mut contract, spec) = load_sankey();
    let dup = contract.requirements[0].scenarios[0].obligations[0].clone();
    contract.requirements[0].scenarios[0].obligations.push(dup);
    let err = compile_obligations(&contract, &spec).unwrap_err();
    match err {
        SpecError::InvalidSyntax { message, .. } => {
            assert!(message.contains("duplicate obligation id"));
        }
        other => panic!("unexpected {other}"),
    }
}

#[test]
fn unknown_scenario_reference_fails() {
    let (mut contract, spec) = load_sankey();
    contract.requirements[0].scenarios[0].scenario = "does-not-exist".into();
    let err = compile_obligations(&contract, &spec).unwrap_err();
    match err {
        SpecError::InvalidSyntax { message, .. } => {
            assert!(message.contains("unknown scenario"));
        }
        other => panic!("unexpected {other}"),
    }
}

#[test]
fn unknown_evidence_kind_fails() {
    let err = serde_yaml::from_str::<QualityContract>(
        r"
quality_contract_v: 1
change: sankey-others
requirements:
  - capability: sankey
    requirement: visual-limit-others
    scenarios:
      - scenario: overflow-grouped
        obligations:
          - id: others-visible
            kind: behavioral
        evidence:
          required: [laser]
",
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("laser") || message.contains("unknown variant"),
        "{message}"
    );
}

#[test]
fn unknown_predicate_kind_fails_closed() {
    let err = serde_yaml::from_str::<QualityContract>(
        r"
quality_contract_v: 1
change: sankey-others
requirements:
  - capability: sankey
    requirement: visual-limit-others
    scenarios:
      - scenario: overflow-grouped
        obligations:
          - id: others-visible
            kind: behavioral
            expected:
              kind: ask_a_model
",
    )
    .unwrap_err();
    assert!(err.to_string().contains("ask_a_model"));
}

#[test]
fn empty_predicate_target_fails_closed() {
    let root = fixture_root();
    let path = root
        .join("openspec")
        .join("changes")
        .join("sankey-others")
        .join("quality.yaml");
    let raw = std::fs::read_to_string(&path).unwrap();
    let invalid = raw.replace(
        "role: region\n                accessible_name: Others",
        "role: ''",
    );
    let temp = std::env::temp_dir().join(format!("wvq-empty-predicate-{}", std::process::id()));
    let contract_dir = temp.join("openspec/changes/sankey-others");
    std::fs::create_dir_all(&contract_dir).unwrap();
    std::fs::write(contract_dir.join("quality.yaml"), invalid).unwrap();
    let err = load_quality_contract(&temp, "sankey-others").unwrap_err();
    let _ = std::fs::remove_dir_all(&temp);
    assert!(err.to_string().contains("semantic identity"));
}

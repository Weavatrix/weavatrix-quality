//! Task 3: `OracleSeal` ignores implementation metadata and moves when intent moves.

use std::path::{Path, PathBuf};

use wvq_spec::{
    ObligationKind, compile_obligations, load_quality_contract, read_change, seal,
};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("openspec")
        .join("repo")
}

#[test]
fn implementation_metadata_does_not_alter_seal() {
    let root = fixture_root();
    let spec = read_change(&root, "sankey-others").unwrap();
    let mut contract = load_quality_contract(&root, "sankey-others").unwrap();
    let first = {
        let obligations = compile_obligations(&contract, &spec).unwrap();
        seal(&contract, &obligations, &spec).unwrap()
    };

    contract.ai.as_mut().unwrap().planning_tokens = 99_000;
    contract.ai.as_mut().unwrap().runtime_tokens = 0;
    if let Some(scenario) = contract.requirements[0].scenarios.get_mut(0) {
        if let Some(evidence) = scenario.evidence.as_mut() {
            evidence.on_failure.clear();
        }
        if let Some(mutation) = scenario.mutation.as_mut() {
            mutation.operators.clear();
        }
    }

    let obligations = compile_obligations(&contract, &spec).unwrap();
    let second = seal(&contract, &obligations, &spec).unwrap();
    assert_eq!(first.digest, second.digest);
    assert_eq!(first.id, second.id);
}

#[test]
fn expected_invariant_change_alters_seal() {
    let root = fixture_root();
    let spec = read_change(&root, "sankey-others").unwrap();
    let mut contract = load_quality_contract(&root, "sankey-others").unwrap();
    let before = {
        let obligations = compile_obligations(&contract, &spec).unwrap();
        seal(&contract, &obligations, &spec).unwrap()
    };

    let invariant = contract.requirements[0].scenarios[0]
        .obligations
        .iter_mut()
        .find(|item| item.kind == ObligationKind::Invariant)
        .expect("fixture has an invariant");
    invariant.kind = ObligationKind::Behavioral;

    let obligations = compile_obligations(&contract, &spec).unwrap();
    let after = seal(&contract, &obligations, &spec).unwrap();
    assert_ne!(before.digest, after.digest);
}

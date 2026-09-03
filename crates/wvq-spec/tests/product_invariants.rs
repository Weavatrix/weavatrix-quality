//! This repository's product `OpenSpec` change compiles without invented bindings.

use std::path::{Path, PathBuf};

use wvq_spec::{ObligationKind, compile_obligations, load_quality_contract, read_change};

fn product_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[test]
fn product_invariants_change_compiles_nine_invariants() {
    let root = product_root();
    let spec = read_change(&root, "wvq-invariants").unwrap();
    let contract = load_quality_contract(&root, "wvq-invariants").unwrap();
    let obligations = compile_obligations(&contract, &spec).unwrap();
    assert_eq!(spec.id.as_str(), "wvq-invariants");
    assert_eq!(obligations.len(), 9);
    assert!(
        obligations
            .iter()
            .all(|obligation| obligation.kind == ObligationKind::Invariant)
    );
    assert_eq!(
        contract.ai.as_ref().map(|hints| hints.runtime_tokens),
        Some(0)
    );
}

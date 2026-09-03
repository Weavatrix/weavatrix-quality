//! Product `test_bindings` name existing exact cases. They are not invented.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::*;
use wvq_spec::{compile_obligations, load_quality_contract, read_change};

fn product_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

#[test]
fn product_invariants_bindings_name_existing_exact_cases() {
    let root = product_root();
    let spec = read_change(&root, "wvq-invariants").unwrap();
    let contract = load_quality_contract(&root, "wvq-invariants").unwrap();
    let obligations = compile_obligations(&contract, &spec).unwrap();
    let bindings = load_test_bindings(&root).unwrap();
    assert_eq!(bindings.len(), obligations.len());

    let bound = bindings
        .iter()
        .flat_map(|binding| binding.obligations.iter().cloned())
        .collect::<BTreeSet<_>>();
    let needed = obligations
        .iter()
        .map(|obligation| obligation.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        bound, needed,
        "every compiled obligation must have one exact case"
    );

    for binding in &bindings {
        assert_eq!(binding.runner.as_deref(), Some("cargo-test"));
        assert!(binding.suite.is_some(), "{}", binding.path);
        let case = binding.case.as_deref().expect("exact case required");
        let path = root.join(&binding.path);
        assert!(path.is_file(), "missing {}", binding.path);
        let source = std::fs::read_to_string(&path).unwrap();
        let fn_name = case.rsplit("::").next().unwrap();
        assert!(
            source.contains(&format!("fn {fn_name}(")),
            "{} must contain fn {fn_name}",
            binding.path
        );
    }
}

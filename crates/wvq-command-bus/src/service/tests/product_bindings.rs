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

#[test]
fn product_invariants_impacted_run_uses_exact_cargo_cases_instead_of_widening() {
    let root = product_root();
    let bindings = load_test_bindings(&root).unwrap();
    let mut selected = bindings
        .iter()
        .map(|binding| binding.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    selected.sort();
    let selection = LiveSelection {
        explanations: vec![Vec::new(); selected.len()],
        selected,
        uncovered_mandatory: Vec::new(),
        uncovered_all: Vec::new(),
        bindings: bindings.clone(),
    };
    let targets = vec![ExecutorTarget {
        executor: wvq_runtime::ExecutorId::new("cargo-test").unwrap(),
        cwd: root.clone(),
    }];
    let (requests, scope, reason, executed) =
        build_execution_requests(&root, &targets, &selection, &BTreeSet::new(), "impacted");
    assert_eq!(scope, "impacted", "{reason}");
    assert_eq!(requests.len(), bindings.len(), "{reason}");
    assert!(
        requests.iter().all(|request| {
            request.filters.len() == 1 && request.target.executor.as_str() == "cargo-test"
        }),
        "each cargo-test process must carry exactly one case name"
    );
    let cases = requests
        .iter()
        .map(|request| request.filters[0].clone())
        .collect::<BTreeSet<_>>();
    let expected = bindings
        .iter()
        .map(|binding| binding.case.clone().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(cases, expected);
    assert_eq!(
        executed.as_ref().map(BTreeSet::len),
        Some(selection.selected.len())
    );
}

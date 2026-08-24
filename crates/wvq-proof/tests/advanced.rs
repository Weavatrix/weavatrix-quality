//! Task 22: mutation, metamorphic relations, cheap explorer.

use std::collections::BTreeSet;

use wvq_proof::{
    ExecutionEvidence, Explorer, ExplorerBudget, ExplorerDecision, MetaSample, MutantOracle,
    MutantStatus, MutationSummary, ProofVerdict, assemble, builtins, execute, go_mutants,
    plan_go_source_mutants, plan_ts_js_source_mutants, propose, run_selected_mutants,
    seal_relation, ts_js_mutants,
};
use wvq_spec::EvidenceKind;

use wvq_domain::{ObligationId, OracleSealId, ProofId, RequirementId, RevisionId, ScenarioId};

struct MapOracle {
    kills: BTreeSet<(String, String)>,
}

impl MutantOracle for MapOracle {
    fn test_fails(&self, mutant_id: &str, test_id: &str) -> bool {
        self.kills
            .contains(&(mutant_id.to_owned(), test_id.to_owned()))
    }
}

#[test]
fn ts_and_go_mutants_only_for_changed_regions() {
    assert!(ts_js_mutants("").is_empty());
    assert!(go_mutants("").is_empty());
    let ts = ts_js_mutants("src/sankey.ts:40-44");
    assert!(ts.len() >= 8);
    assert!(ts.iter().all(|item| item.region == "src/sankey.ts:40-44"));
    let go = go_mutants("add.go:12");
    assert!(go.len() >= 5);
    assert!(
        go.iter()
            .all(|item| item.ecosystem == wvq_proof::MutantEcosystem::Go)
    );
}

#[test]
fn mutation_runs_selected_tests_only() {
    let mutants = ts_js_mutants("src/add.ts:1");
    let selected = vec!["T-unit-add".into()];
    let oracle = MapOracle {
        kills: BTreeSet::from([(mutants[0].id.clone(), "T-full-suite".into())]),
    };
    let results = run_selected_mutants(&mutants, &selected, &oracle);
    assert!(results.iter().all(|item| item.tests_run == selected));
    assert!(
        results
            .iter()
            .all(|item| item.status == MutantStatus::Survived)
    );
    let summary = MutationSummary::from_results(&results);
    assert_eq!(summary.killed, 0);
    assert_eq!(summary.survived, u64::try_from(results.len()).unwrap());
    assert_eq!(summary.invalid, 0);
    assert!(!summary.unmeasured);
}

#[test]
fn source_mutants_edit_only_changed_lines_with_concrete_safe_replacements() {
    let ts = "const oldBoundary = value > 0;\nexport const allowed = value >= 5 && true;\n";
    let mutants = plan_ts_js_source_mutants(
        "src/limit.ts",
        ts,
        &BTreeSet::from([2]),
        &["boundary_flip".into(), "bool_flip".into()],
        16,
    )
    .unwrap();
    assert_eq!(mutants.len(), 2, "{mutants:#?}");
    assert!(mutants.iter().all(|mutant| mutant.line == 2));
    assert!(
        mutants
            .iter()
            .all(|mutant| mutant.apply(ts).unwrap().contains("value > 0;\nexport"))
    );
    assert!(mutants.iter().any(|mutant| {
        mutant.operator == "boundary_flip"
            && mutant.apply(ts).unwrap().contains("value > 5 && true")
    }));
    assert!(mutants.iter().any(|mutant| {
        mutant.operator == "bool_flip" && mutant.apply(ts).unwrap().contains("value >= 5 && false")
    }));

    let go = "package limit\nfunc allowed(value int) bool { return value >= 5 && true }\n";
    let mutants = plan_go_source_mutants(
        "limit.go",
        go,
        &BTreeSet::from([2]),
        &["boundary_flip".into(), "invert_bool".into()],
        16,
    )
    .unwrap();
    assert_eq!(mutants.len(), 2, "{mutants:#?}");
    assert!(mutants.iter().all(|mutant| mutant.line == 2));
    assert!(mutants.iter().any(|mutant| {
        mutant.operator == "boundary_flip"
            && mutant.apply(go).unwrap().contains("value > 5 && true")
    }));
}

#[test]
fn source_mutation_refuses_markup_comments_and_unrelated_error_line_comparisons() {
    let ts = "// value >= 5\nconst markup = <Button>ok</Button>\nconst text = 'value >= 5'\n";
    let mutants = plan_ts_js_source_mutants(
        "src/view.tsx",
        ts,
        &BTreeSet::from([1, 2, 3]),
        &["boundary_flip".into()],
        16,
    )
    .unwrap();
    assert!(mutants.is_empty(), "{mutants:#?}");

    let go = "if err != nil && count == 2 { return err }\n";
    let mutants = plan_go_source_mutants(
        "worker.go",
        go,
        &BTreeSet::from([1]),
        &["err_nil_flip".into()],
        16,
    )
    .unwrap();
    assert_eq!(mutants.len(), 1, "{mutants:#?}");
    let mutated = mutants[0].apply(go).unwrap();
    assert!(mutated.contains("err == nil"));
    assert!(mutated.contains("count == 2"));

    let compact = "export const allowed = (value) => value>=5\n";
    let mutants = plan_ts_js_source_mutants(
        "src/compact.ts",
        compact,
        &BTreeSet::from([1]),
        &["boundary_flip".into()],
        16,
    )
    .unwrap();
    assert_eq!(mutants.len(), 1, "{mutants:#?}");
    assert!(mutants[0].apply(compact).unwrap().contains("value>5"));
}

#[test]
fn every_declared_ts_js_and_go_operator_has_a_concrete_source_edit() {
    let ts = "if (value >= 5 && canDelete() && true) { items.sort(); callback(); throw error; }\nconst same = left === right\nconst page = items.slice(0, 5)\nconst offset = index +1\n";
    let ts_mutants =
        plan_ts_js_source_mutants("src/all.ts", ts, &BTreeSet::from([1, 2, 3, 4]), &[], 64)
            .unwrap();
    let ts_operators = ts_mutants
        .iter()
        .map(|mutant| mutant.operator.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ts_operators,
        BTreeSet::from([
            "boundary_flip",
            "equality_flip",
            "bool_flip",
            "logical_flip",
            "off_by_one",
            "remove_branch",
            "remove_sort",
            "wrong_permission",
            "omit_callback",
            "omit_error",
            "collection_boundary",
        ])
    );
    assert!(
        ts_mutants
            .iter()
            .all(|mutant| mutant.apply(ts).unwrap() != ts)
    );

    let go = "if err != nil { return 1 }\nif value >= 5 { return true }\nif ctx.Err() != nil { return false }\n";
    let go_mutants =
        plan_go_source_mutants("worker.go", go, &BTreeSet::from([1, 2, 3]), &[], 64).unwrap();
    let go_operators = go_mutants
        .iter()
        .map(|mutant| mutant.operator.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        go_operators,
        BTreeSet::from([
            "err_nil_flip",
            "boundary_flip",
            "return_zero",
            "skip_branch",
            "ignore_context",
            "invert_bool",
        ])
    );
    assert!(
        go_mutants
            .iter()
            .all(|mutant| mutant.apply(go).unwrap() != go)
    );
}

#[test]
fn survived_mutant_makes_proven_partial() {
    let (id, requirement, scenario, obligation, oracle_seal, revision) = (
        ProofId::new("proof-mut").unwrap(),
        RequirementId::new("sankey.visual-limit-others").unwrap(),
        ScenarioId::new("overflow-grouped").unwrap(),
        ObligationId::new("others-visible").unwrap(),
        OracleSealId::new("oseal-deadbeefdeadbee").unwrap(),
        RevisionId::new("rev-head").unwrap(),
    );
    let assembled = assemble(wvq_proof::AssemblyInput {
        id,
        requirement,
        scenario,
        obligation,
        oracle_seal,
        revision,
        program: None,
        run: None,
        observations: vec!["obs-1".into()],
        artifacts: vec![],
        required_evidence: vec![EvidenceKind::Dom, EvidenceKind::Network],
        execution: ExecutionEvidence::Passed {
            present: vec![EvidenceKind::Dom, EvidenceKind::Network],
        },
        spec_ambiguous: false,
        quality_debt: vec![],
        mutation: Some(MutationSummary {
            killed: 8,
            survived: 1,
            invalid: 0,
            unmeasured: false,
        }),
    });
    assert_eq!(assembled.proof.verdict, ProofVerdict::Partial);
    assert_eq!(assembled.proof.mutation.unwrap().survived, 1);
}

#[test]
fn builtins_run_model_less_and_unsealed_cannot() {
    let sample = MetaSample {
        values: vec![1, 2, 3],
        semantic: Some("above_visual_limit".into()),
    };
    for relation in builtins() {
        assert!(relation.sealed);
        assert!(execute(&relation, &sample).unwrap());
    }
    let proposed = propose(
        "agent-sum",
        wvq_proof::MetaTransform::Permute,
        wvq_proof::MetaExpectation::SumUnchanged,
    );
    assert!(!proposed.sealed);
    assert!(execute(&proposed, &sample).is_err());
    let sealed = seal_relation(proposed);
    assert!(execute(&sealed, &sample).unwrap());
}

fn control(id: &str, novel: bool, obligation: bool) -> wvq_proof::SemanticControl {
    wvq_proof::SemanticControl {
        id: id.into(),
        semantic: format!("button {id}"),
        setup_cost: 1,
        already_covered: false,
        uncovers_obligation: obligation,
        novel_state: novel,
        risk: 0,
        boundary: false,
        historical: false,
    }
}

#[test]
fn explorer_prefers_uncovered_obligation_and_packets_after_tarpit() {
    let cheap = control("covered", false, false);
    let mut cheap = cheap;
    cheap.already_covered = true;
    let useful = control("others", true, true);
    let mut explorer = Explorer::new(
        ExplorerBudget {
            max_actions: 8,
            tarpit_after: 2,
        },
        "state-0",
    );
    let first = explorer.step(
        &[cheap.clone(), useful.clone()],
        "state-1",
        Some("others-visible"),
    );
    assert_eq!(first, ExplorerDecision::Act("others".into()));
    assert!(wvq_proof::Explorer::score(&useful) > wvq_proof::Explorer::score(&cheap));

    let mut trap = Explorer::new(
        ExplorerBudget {
            max_actions: 10,
            tarpit_after: 2,
        },
        "s",
    );
    let idle = control("idle-a", false, false);
    let idle_b = control("idle-b", false, false);
    let _ = trap.step(&[idle.clone(), idle_b.clone()], "s", None);
    let _ = trap.step(&[idle.clone(), idle_b.clone()], "s", None);
    let done = trap.step(&[idle, idle_b], "s", None);
    match done {
        ExplorerDecision::Exhausted(packet) => {
            assert_eq!(packet.runtime_tokens, 0);
            assert!(!packet.failed_candidates.is_empty());
        }
        ExplorerDecision::Act(_) => panic!("tarpit must exhaust"),
    }
}

//! Task 22: mutation, metamorphic relations, cheap explorer.

use std::collections::BTreeSet;

use wvq_proof::{
    ExecutionEvidence, Explorer, ExplorerBudget, ExplorerDecision, MetaSample, MutantOracle,
    MutantStatus, MutationSummary, ProofVerdict, assemble, builtins, execute, go_mutants, propose,
    run_selected_mutants, seal_relation, ts_js_mutants,
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

//! Task 15: five-way Proof verdict; debt is a separate axis.

use wvq_domain::{
    CheckId, FindingState, ObligationId, OracleSealId, ProofId, QualityFinding, RequirementId,
    RevisionId, ScenarioId, Severity, SubjectRef,
};
use wvq_proof::{
    AssemblyInput, ExecutionEvidence, ProofVerdict, assemble, decide_verdict,
};
use wvq_proof::VerdictInput;
use wvq_spec::EvidenceKind;

fn ids() -> (ProofId, RequirementId, ScenarioId, ObligationId, OracleSealId, RevisionId) {
    (
        ProofId::new("proof-1").unwrap(),
        RequirementId::new("sankey.visual-limit-others").unwrap(),
        ScenarioId::new("overflow-grouped").unwrap(),
        ObligationId::new("others-visible").unwrap(),
        OracleSealId::new("oseal-deadbeefdeadbee").unwrap(),
        RevisionId::new("rev-head").unwrap(),
    )
}

fn base_input(execution: ExecutionEvidence) -> AssemblyInput {
    let (id, requirement, scenario, obligation, oracle_seal, revision) = ids();
    AssemblyInput {
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
        execution,
        spec_ambiguous: false,
        quality_debt: vec![],
        mutation: None,
    }
}

#[test]
fn passing_required_execution_is_proven() {
    let assembled = assemble(base_input(ExecutionEvidence::Passed {
        present: vec![EvidenceKind::Dom, EvidenceKind::Network],
    }));
    assert_eq!(assembled.proof.verdict, ProofVerdict::Proven);
    assert_eq!(assembled.proof.verdict.as_str(), "PROVEN");
    assert_eq!(assembled.proof.schema_v, 1);
    assert_eq!(assembled.proof.revision.as_str(), "rev-head");
}

#[test]
fn missing_required_runtime_evidence_is_unproven() {
    let assembled = assemble(base_input(ExecutionEvidence::Absent));
    assert_eq!(assembled.proof.verdict, ProofVerdict::Unproven);
    let partial_run = assemble(base_input(ExecutionEvidence::Passed {
        present: vec![EvidenceKind::Dom],
    }));
    assert_eq!(partial_run.proof.verdict, ProofVerdict::Partial);
}

#[test]
fn sealed_expectation_contradicted_is_contradicted() {
    let assembled = assemble(base_input(ExecutionEvidence::Failed {
        seal_contradicted: true,
        present: vec![EvidenceKind::Dom, EvidenceKind::Network],
    }));
    assert_eq!(assembled.proof.verdict, ProofVerdict::Contradicted);
}

#[test]
fn spec_ambiguity_is_human_required() {
    let mut input = base_input(ExecutionEvidence::Passed {
        present: vec![EvidenceKind::Dom, EvidenceKind::Network],
    });
    input.spec_ambiguous = true;
    let assembled = assemble(input);
    assert_eq!(assembled.proof.verdict, ProofVerdict::HumanRequired);
}

#[test]
fn quality_debt_does_not_change_the_verdict() {
    let debt = QualityFinding::new(
        CheckId::new("WVQ-ARCH-001").unwrap(),
        Severity::Error,
        SubjectRef::File("src/a.js".into()),
        "new architecture error",
    )
    .with_state(FindingState::New);
    let mut input = base_input(ExecutionEvidence::Passed {
        present: vec![EvidenceKind::Dom, EvidenceKind::Network],
    });
    input.quality_debt = vec![debt.clone()];
    let assembled = assemble(input);
    assert_eq!(assembled.proof.verdict, ProofVerdict::Proven);
    assert_eq!(assembled.debt, [debt]);
}

#[test]
fn contradiction_wins_over_ambiguity() {
    let verdict = decide_verdict(&VerdictInput {
        required_evidence: vec![EvidenceKind::Dom],
        present_evidence: vec![EvidenceKind::Dom],
        execution_passed: false,
        seal_contradicted: true,
        spec_ambiguous: true,
    });
    assert_eq!(verdict, ProofVerdict::Contradicted);
}

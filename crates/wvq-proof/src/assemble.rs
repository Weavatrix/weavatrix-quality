//! Assemble a revision-bound [`Proof`]. Debt stays on a parallel axis.

use wvq_domain::{
    ArtifactId, ObligationId, OracleSealId, ProgramId, ProofId, QualityFinding, RequirementId,
    RevisionId, RunId, ScenarioId,
};
use wvq_spec::EvidenceKind;

use crate::mutation::MutationSummary;
use crate::verdict::{ProofVerdict, VerdictInput, decide_verdict};

/// Observed execution for one obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionEvidence {
    /// Runner passed and listed collected evidence kinds.
    Passed {
        /// Evidence actually present.
        present: Vec<EvidenceKind>,
    },
    /// Runner failed. `seal_contradicted` if the failure hits a sealed oracle.
    Failed {
        /// Sealed expectation was violated.
        seal_contradicted: bool,
        /// Evidence present despite failure.
        present: Vec<EvidenceKind>,
    },
    /// No runtime was executed / no artifacts.
    Absent,
}

/// Input to [`assemble`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyInput {
    /// Proof identity.
    pub id: ProofId,
    /// Requirement.
    pub requirement: RequirementId,
    /// Scenario.
    pub scenario: ScenarioId,
    /// Obligation.
    pub obligation: ObligationId,
    /// Seal that must not be healed away.
    pub oracle_seal: OracleSealId,
    /// Repository revision.
    pub revision: RevisionId,
    /// Optional program.
    pub program: Option<ProgramId>,
    /// Optional run.
    pub run: Option<RunId>,
    /// Observation handles.
    pub observations: Vec<String>,
    /// Artifact ids (CAS handles).
    pub artifacts: Vec<ArtifactId>,
    /// Required evidence kinds from the obligation.
    pub required_evidence: Vec<EvidenceKind>,
    /// Execution / evidence.
    pub execution: ExecutionEvidence,
    /// Spec cannot be interpreted without a human.
    pub spec_ambiguous: bool,
    /// Debt findings. Recorded beside the proof, never mixed into the verdict.
    pub quality_debt: Vec<QualityFinding>,
    /// Mutation killed/survived. Survived relevant mutants weaken a green run.
    pub mutation: Option<MutationSummary>,
}

/// Spec §27 Proof record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    /// Schema version.
    pub schema_v: u32,
    /// Proof id.
    pub id: ProofId,
    /// Requirement.
    pub requirement: RequirementId,
    /// Scenario.
    pub scenario: ScenarioId,
    /// Obligation.
    pub obligation: ObligationId,
    /// Oracle seal.
    pub oracle_seal: OracleSealId,
    /// Revision.
    pub revision: RevisionId,
    /// Program.
    pub program: Option<ProgramId>,
    /// Run.
    pub run: Option<RunId>,
    /// Observations.
    pub observations: Vec<String>,
    /// Artifacts.
    pub artifacts: Vec<ArtifactId>,
    /// Mutation summary, when run.
    pub mutation: Option<MutationSummary>,
    /// Verdict. Not a percentage.
    pub verdict: ProofVerdict,
}

/// Proof plus the separate debt axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofAssembly {
    /// Revision-bound proof.
    pub proof: Proof,
    /// Quality debt, unchanged by the verdict.
    pub debt: Vec<QualityFinding>,
}

/// Assemble a proof. Debt is copied through and does not affect [`ProofVerdict`].
#[must_use]
pub fn assemble(input: AssemblyInput) -> ProofAssembly {
    let (execution_passed, seal_contradicted, present) = match &input.execution {
        ExecutionEvidence::Passed { present } => (true, false, present.clone()),
        ExecutionEvidence::Failed {
            seal_contradicted,
            present,
        } => (false, *seal_contradicted, present.clone()),
        ExecutionEvidence::Absent => (false, false, Vec::new()),
    };
    let mut verdict = decide_verdict(&VerdictInput {
        required_evidence: input.required_evidence,
        present_evidence: present,
        execution_passed,
        seal_contradicted,
        spec_ambiguous: input.spec_ambiguous,
    });
    if input
        .mutation
        .as_ref()
        .is_some_and(|summary| summary.survived > 0 || summary.invalid > 0 || summary.unmeasured)
        && verdict == ProofVerdict::Proven
    {
        verdict = ProofVerdict::Partial;
    }
    ProofAssembly {
        proof: Proof {
            schema_v: 1,
            id: input.id,
            requirement: input.requirement,
            scenario: input.scenario,
            obligation: input.obligation,
            oracle_seal: input.oracle_seal,
            revision: input.revision,
            program: input.program,
            run: input.run,
            observations: input.observations,
            artifacts: input.artifacts,
            mutation: input.mutation,
            verdict,
        },
        debt: input.quality_debt,
    }
}

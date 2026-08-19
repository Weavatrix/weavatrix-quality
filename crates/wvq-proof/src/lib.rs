//! Revision-bound Proof assembly. Never a single quality percentage.

#![forbid(unsafe_code)]

mod assemble;
mod differential;
mod explorer;
mod flake;
mod heal;
mod metamorphic;
mod mutation;
mod verdict;

pub use assemble::{AssemblyInput, ExecutionEvidence, Proof, ProofAssembly, assemble};
pub use differential::{
    CodeDelta, DeltaTriangle, SpecDelta, TriangleAxes, TriangleReading, classify_triangle,
    join_triangle, spec_delta,
};
pub use explorer::{Explorer, ExplorerBudget, ExplorerDecision, ExplorerPacket, SemanticControl};
pub use flake::{
    DecisionPacket, FailureEvidence, FailureSignal, FlakeClass, FlakeError, FlakeTriage,
    TimingBucket, fingerprint_id, triage,
};
pub use heal::{HealEdit, HealError, HealedProgram, apply_heal, recover_target};
pub use metamorphic::{
    MetaError, MetaExpectation, MetaSample, MetaTransform, MetamorphicRelation, RelationOrigin,
    builtins, execute, propose, seal_relation,
};
pub use mutation::{
    Mutant, MutantEcosystem, MutantOracle, MutantResult, MutantStatus, MutationSummary, go_mutants,
    run_selected_mutants, ts_js_mutants,
};
pub use verdict::{ProofVerdict, VerdictInput, decide_verdict};

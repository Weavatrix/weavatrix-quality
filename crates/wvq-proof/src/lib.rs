//! Revision-bound Proof assembly. Never a single quality percentage.

#![forbid(unsafe_code)]

mod assemble;
mod budget;
mod differential;
mod explorer;
mod flake;
mod heal;
mod metamorphic;
mod model;
mod mutation;
mod protection;
mod protection_checks;
mod protection_delta;
mod protection_view;
mod verdict;

pub use assemble::{AssemblyInput, ExecutionEvidence, Proof, ProofAssembly, assemble};
pub use budget::{
    AI_BUDGET_EXHAUSTED, AiBudget, AiCall, AiCallKind, AiCostFirewall, AiUsage, BudgetExhausted,
    BudgetLimit, TokenRatio,
};
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
pub use model::{
    LocalModelConfig, LocalModelReply, LocalModelRequest, ModelError, call_local_model,
};
pub use mutation::{
    Mutant, MutantEcosystem, MutantOracle, MutantResult, MutantStatus, MutationSummary, go_mutants,
    run_selected_mutants, ts_js_mutants,
};
pub use protection::{
    FlowProtection, HistoricalProof, ProtectionError, ProtectionSnapshot, ReusePolicy, may_reuse,
    snapshot,
};
pub use protection_checks::{
    ProtectionCheckInput, ProtectionFinding, ProtectionPolicy, ProtectionTrend, TestChange, blocks,
    gate_protection,
};
pub use protection_delta::{
    DeltaContext, ProtectionDelta, ProtectionDeltaState, ProtectionSummary, protection_delta,
    summarise,
};
pub use protection_view::{FlowView, ProtectionReport, ProtectionView, TestLineageView};
pub use verdict::{ProofVerdict, VerdictInput, decide_verdict};

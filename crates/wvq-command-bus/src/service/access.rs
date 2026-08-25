//! Non-cyclic imports for extracted helpers. Do not glob helper modules here.
#![allow(unused_imports)]

pub(in crate::service) use std::collections::{BTreeMap, BTreeSet};
pub(in crate::service) use std::path::{Path, PathBuf};
pub(in crate::service) use std::sync::atomic::AtomicBool;
pub(in crate::service) use std::sync::Arc;
pub(in crate::service) use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(in crate::service) use serde_json::{json, Value};
pub(in crate::service) use wvq_domain::{
    ArtifactId, ContentHash, OracleSealId, ProgramId, ProofId, RevisionId, RunId, Severity,
};
pub(in crate::service) use wvq_intelligence::{
    ApplicationSurfaceKind, CodeEvidenceProvider, CoverageMeasurement, GraphDelta, NodeCoverage,
    ObligationNeed, SelectionInput, SurfaceCoverageState, SurfaceDelta, TestCandidate,
    WeavatrixProvider, application_surface_graph, coverage_autopilot, impacted_surface,
    map_coverage_to_nodes, select_minimal_plan,
};
pub(in crate::service) use wvq_proof::{
    AiAxis, AiCallKind, AiCostFirewall, AiUsage, AssemblyInput, AxisState, ChangeQualityVerdict,
    DebtAxis, DebtItem, DeltaContext, DeltaFindingRef, DeltaTriangleAxis, ExecutionEvidence,
    FailureEvidence, FlakeClass, FlowProtection, FlowView, HealEdit, Limitation, LocalModelRequest,
    OracleReplacementReview, ProofOutcome, ProofVerdict, ProtectionAxis, ProtectionCheckInput,
    ProtectionDelta, ProtectionDeltaState, ProtectionFinding, ProtectionPolicy, ProtectionSnapshot,
    ProtectionView, StabilityAxis, TestChange, TestLineageView, TimingBucket, UiFindingRef,
    UiIntegrityAxis, VerdictInputs, apply_heal, assemble, call_local_model, compose, debt_rule_blocks,
    fingerprint_id, gate_protection, protection_delta, snapshot_with_executed_tests, summarise,
    triage,
};
pub(in crate::service) use wvq_runtime::{
    BehaviorState, BrowserAssertionStatus, BrowserProgramRun, BrowserRecordingRequest,
    BrowserRunConfig, BrowserViewport, CaptureWhen, CoverageArtifact, ExecutionResult,
    ExecutorRegistry, ExecutorTarget, NetworkMode, NetworkRunPolicy, NormalizedTestRun,
    PrepareRequest, ProgramOracle, Recorder, TestAction, TestProgram, TestStatus, default_limits,
    discover_executor_targets, parse_cargo_test, parse_go_coverprofile, parse_go_json, parse_junit,
    parse_lcov, promote, record_browser_session, run_browser_program, run_browser_program_at,
};
pub(in crate::service) use wvq_spec::{
    EvidenceKind, RiskLevel, SpecError, TestObligation, load_quality_contract, seal,
};
pub(in crate::service) use wvq_store::{
    HistoricalTestCandidate, Store, StoredAiUsage, StoredProof, StoredRun, StoredRunItem,
    StoredSelectionAudit, StoredTestCaseIdentity, StoredTestCaseResult,
};
pub(in crate::service) use wvq_ui::{
    LayoutSnapshot, ResponsiveProbe, UiFindingState, UiIntegrityDelta, UiIntegrityFinding,
    UiIntegrityPolicy, UiIntegritySnapshot, detect as detect_ui, next_responsive_probe,
    ratchet as ratchet_ui, responsive_failure_intervals, responsive_probe_plan,
};

pub(in crate::service) use crate::commands::{
    AuthorDraftCommand, AuthorHealCommand, AuthorHealEdit, AuthorPreviewCommand,
    AuthorPromoteCommand, AuthorValidateCommand, ChangesCommand, ContextCommand, DebtCommand,
    EvidenceCommand, ExplainCommand, InitCommand, ModelCommand, PlanCommand, RecordCommand,
    RecoveryCommand, RunCommand, SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
pub(in crate::service) use crate::replies::{
    AuthorDraftReply, AuthorHealReply, AuthorModelUsage, AuthorPreviewReply, AuthorPromoteReply,
    AuthorValidateReply, AuthoringObligation, ChangesReply, ContextReply, DebtReply, EvidenceReply,
    ExplainReply, InitReply, ModelReply, PlanReply, ProofSummary, RecordReply, RecoveryReply,
    RunReply, SelectReply, SelectionAuditReply, SpecSealReply, SpecValidateReply, StatusReply,
    VerifyReply, ApplicationSurfaceView, CheapestEvidencePlanView, SurfaceEvidenceMatrixView,
};
pub(in crate::service) use crate::source_mutation::{
    MutationBinding, MutationPolicy, MutationRunDocument, MutationRunRequest, execute_source_mutation,
};

pub(in crate::service) use super::BusError;
pub(in crate::service) use super::authoring::*;
pub(in crate::service) use super::delta::*;
pub(in crate::service) use super::git::*;
pub(in crate::service) use super::graph::*;
pub(in crate::service) use super::paths::*;
pub(in crate::service) use super::policy::*;
pub(in crate::service) use super::types::*;
pub(in crate::service) use super::validate::*;
pub(in crate::service) use super::{
    APPLICATION_SURFACE_GRAPH_KIND, CHEAPEST_EVIDENCE_PLAN_KIND, DELTA_TRIANGLE_KIND,
    MUTATION_RESULTS_KIND, ORACLE_REPLACEMENT_KIND, SURFACE_EVIDENCE_MATRIX_KIND,
    UI_INTEGRITY_DELTA_KIND,
};

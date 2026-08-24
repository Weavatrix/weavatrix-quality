//! Runner evidence normalization. WVQ does not invent replacement runners.

#![forbid(unsafe_code)]

mod behavior;
mod browser_bridge;
mod browser_protocol;
mod cargotest;
mod diff;
mod discovery;
mod executor;
mod gocover;
mod gojson;
mod junit;
mod lcov;
mod normalize;
mod process;
mod program;

pub use behavior::{
    BehaviorEdge, BehaviorState, BehaviorTrace, CoverageContribution, GraphMemory, RecordedEvent,
    Recorder, ReplayHost, coverage_contribution, promote, replay_program, replay_trace,
    semantic_target,
};
pub use browser_bridge::{
    ActionSpan, BrowserAssertionObservation, BrowserAssertionStatus, BrowserBridgeError,
    BrowserProgramRun, BrowserRecordedEvent, BrowserRecording, BrowserRecordingRequest,
    BrowserRunConfig, BrowserViewport, DuplicateMutationRequest, ProgramOracle,
    RecordedOracleOutcome, UiCollectionConfig, UiSnapshotEvidence, duplicate_mutation_requests,
    record_browser_session, run_browser_program, run_browser_program_at,
};
pub use browser_protocol::{
    BridgeReply, BridgeRequest, decode_request, encode_reply, observe_body,
};
pub use cargotest::parse_cargo_test;
pub use diff::{
    AxisDelta, BehaviorDelta, DiffAxis, StructuredView, behavior_delta, replay_base_head,
};
pub use discovery::{ExecutorTarget, discover_executor_targets};
pub use executor::{
    ExecutionResult, Executor, ExecutorCapabilities, ExecutorId, ExecutorRegistry, ExecutorSpec,
    PrepareRequest, PreparedRun, default_limits,
};
pub use gocover::parse_go_coverprofile;
pub use gojson::parse_go_json;
pub use junit::parse_junit;
pub use lcov::parse_lcov;
pub use normalize::{
    ArtifactDescriptor, CoverageArtifact, FileCoverage, LineRange, NormalizedTestRun, RuntimeError,
    TestCaseResult, TestStatus,
};
pub use process::ProcessLimits;
pub use program::{
    ApiOperation, CaptureWhen, EvidencePolicy, FaultSpec, NetworkRequestObservation, Observation,
    ProgramError, ProgramSource, Target, TestAction, TestProgram, WaitCondition,
    filter_observation,
};

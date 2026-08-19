//! Runner evidence normalization. WVQ does not invent replacement runners.

#![forbid(unsafe_code)]

mod browser_protocol;
mod executor;
mod gojson;
mod junit;
mod lcov;
mod normalize;
mod process;
mod program;

pub use browser_protocol::{
    BridgeReply, BridgeRequest, decode_request, encode_reply, observe_body,
};
pub use executor::{
    ExecutionResult, Executor, ExecutorCapabilities, ExecutorId, ExecutorRegistry, ExecutorSpec,
    PrepareRequest, PreparedRun, default_limits,
};
pub use gojson::parse_go_json;
pub use junit::parse_junit;
pub use lcov::parse_lcov;
pub use normalize::{
    ArtifactDescriptor, CoverageArtifact, FileCoverage, LineRange, NormalizedTestRun, RuntimeError,
    TestCaseResult, TestStatus,
};
pub use process::ProcessLimits;
pub use program::{
    CaptureWhen, EvidencePolicy, Observation, ProgramError, ProgramSource, Target, TestAction,
    TestProgram, WaitCondition, filter_observation,
};

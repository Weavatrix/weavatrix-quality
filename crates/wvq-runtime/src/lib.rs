//! Runner evidence normalization. WVQ does not invent replacement runners.

#![forbid(unsafe_code)]

mod executor;
mod gojson;
mod junit;
mod lcov;
mod normalize;
mod process;

pub use executor::{
    Executor, ExecutorCapabilities, ExecutorId, ExecutorRegistry, ExecutorSpec, ExecutionResult,
    PrepareRequest, PreparedRun, default_limits,
};
pub use gojson::parse_go_json;
pub use junit::parse_junit;
pub use lcov::parse_lcov;
pub use normalize::{
    ArtifactDescriptor, CoverageArtifact, FileCoverage, LineRange, NormalizedTestRun,
    RuntimeError, TestCaseResult, TestStatus,
};
pub use process::ProcessLimits;

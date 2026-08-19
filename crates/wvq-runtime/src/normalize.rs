//! Normalized runner evidence types.

use thiserror::Error;

/// One normalized runner invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedTestRun {
    /// Test cases in source order.
    pub cases: Vec<TestCaseResult>,
    /// Optional mapped coverage.
    pub coverage: Option<CoverageArtifact>,
    /// Raw artifact handles (paths/kinds), not file bodies.
    pub raw_artifacts: Vec<ArtifactDescriptor>,
}

/// One test case after runner-specific parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCaseResult {
    /// Case name.
    pub name: String,
    /// Suite / package / file.
    pub suite: String,
    /// Outcome.
    pub status: TestStatus,
    /// Duration in milliseconds when the runner reported it.
    pub duration_ms: Option<u64>,
    /// Failure/skip message, if any.
    pub message: Option<String>,
}

/// Normalized case status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    /// Passed.
    Pass,
    /// Failed assertion.
    Fail,
    /// Skipped / ignored.
    Skip,
    /// Runner/infrastructure error.
    Error,
}

/// Coverage mapped to source line ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageArtifact {
    /// Per-file ranges.
    pub files: Vec<FileCoverage>,
}

/// Coverage of one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCoverage {
    /// Repository-relative path.
    pub path: String,
    /// Covered inclusive line ranges.
    pub covered: Vec<LineRange>,
    /// Uncovered inclusive line ranges.
    pub uncovered: Vec<LineRange>,
}

/// Inclusive 1-based line range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// First line.
    pub start: u32,
    /// Last line, inclusive.
    pub end: u32,
}

/// Handle for a raw runner artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDescriptor {
    /// `junit`, `lcov`, or `go-json`.
    pub kind: String,
    /// Optional filesystem path.
    pub path: Option<String>,
}

/// Why runner evidence could not be normalized.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeError {
    /// XML/JSON/LCOV could not be parsed.
    #[error("malformed {kind} evidence: {message}")]
    Malformed {
        /// Artifact kind.
        kind: String,
        /// Parser detail.
        message: String,
    },
    /// Stream ended before a complete record.
    #[error("truncated {kind} evidence")]
    Truncated {
        /// Artifact kind.
        kind: String,
    },
}

impl NormalizedTestRun {
    /// Attach coverage to an already-normalized case list.
    #[must_use]
    pub fn with_coverage(mut self, coverage: CoverageArtifact) -> Self {
        self.coverage = Some(coverage);
        self
    }
}

/// Parse a runner-reported seconds field into milliseconds.
pub(crate) fn seconds_to_ms(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (whole, frac) = raw.split_once('.').unwrap_or((raw, "0"));
    let secs: u64 = whole.parse().ok()?;
    let frac = frac.chars().take(3).collect::<String>();
    let millis: u64 = format!("{frac:0<3}").parse().ok()?;
    Some(secs.saturating_mul(1000).saturating_add(millis))
}

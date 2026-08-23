//! Deterministic UI integrity for Weavatrix Quality.
//!
//! This crate is pure analysis. It never opens a browser, never touches the
//! DOM, and never calls a model: it takes a bounded [`LayoutSnapshot`] that the
//! existing Playwright bridge collected, and turns it into measured findings, a
//! base/head ratchet, and a policy decision.
//!
//! Keeping it separate is what stops the detector logic being written twice.
//! Collection lives in `js/playwright-runner`, orchestration in
//! `wvq-command-bus`, evidence in `wvq-store`, sealed expectations in
//! `wvq-spec`. Only this crate decides whether a duplicate, an occlusion, or an
//! overflow is a problem.
//!
//! # Cost
//!
//! Zero LLM tokens and zero vision calls. Every answer here is arithmetic over
//! geometry and hit-test results.

#![forbid(unsafe_code)]

mod detect;
mod finding;
mod policy;
mod ratchet;
mod snapshot;
mod spatial;

pub use detect::{DetectionOutput, detect};
pub use finding::{UiCheck, UiEvidence, UiIntegrityFinding, sort_findings};
pub use policy::{
    AcceptedTruncation, AllowedOverlap, DEFAULT_GEOMETRY_TOLERANCE_PX, DEFAULT_MAX_NODES,
    DEFAULT_OCCLUSION_FAILURE_PERMILLE, NodeMatcher, UiException, UiIntegrityPolicy, parse_policy,
};
pub use ratchet::{UiFindingState, UiIntegrityDelta, UiIntegritySnapshot, ratchet};
pub use snapshot::{
    DocumentMetrics, HitTestSample, LAYOUT_SNAPSHOT_SCHEMA_V, LayoutSnapshot, MAX_HIT_TEST_SAMPLES,
    MAX_LABEL_CHARS, MAX_NODES, MAX_ROUTE_CHARS, Point, Rect, SnapshotIndex, UiNode, UiNodeId,
    UiStateKey, Viewport,
};
pub use spatial::{CandidatePairs, MAX_CANDIDATE_PAIRS, overlapping_pairs};

use thiserror::Error;

/// Why UI-integrity evidence or policy was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UiError {
    /// Snapshot schema version is not [`LAYOUT_SNAPSHOT_SCHEMA_V`].
    #[error("unknown layout_snapshot schema_v {0}")]
    UnknownSchema(u32),
    /// Snapshot is structurally invalid.
    #[error("malformed layout snapshot: {0}")]
    Malformed(String),
    /// A hard bound was exceeded.
    #[error("layout snapshot exceeds a hard bound: {0}")]
    Bounded(String),
    /// Local policy is invalid. Unknown fields fail closed.
    #[error("invalid ui_integrity policy: {0}")]
    Policy(String),
}

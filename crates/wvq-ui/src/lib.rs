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
mod responsive;
mod snapshot;
mod spatial;
mod visual;
mod visual_pixels;

pub use detect::{DetectionOutput, detect};
pub use finding::{UiCheck, UiEvidence, UiIntegrityFinding, sort_findings};
pub use policy::{
    AcceptedTruncation, AllowedOverlap, DEFAULT_GEOMETRY_TOLERANCE_PX, DEFAULT_MAX_NODES,
    DEFAULT_OCCLUSION_FAILURE_PERMILLE, DEFAULT_RESPONSIVE_HEIGHT, DEFAULT_RESPONSIVE_MAX_PROBES,
    DEFAULT_RESPONSIVE_MAX_WIDTH, DEFAULT_RESPONSIVE_MIN_WIDTH, NodeMatcher, ResponsivePolicy,
    UiException, UiIntegrityPolicy, parse_policy,
};
pub use ratchet::{UiFindingState, UiIntegrityDelta, UiIntegritySnapshot, ratchet};
pub use responsive::{
    ResponsiveFailureInterval, ResponsiveProbe, ResponsiveProbePlan, next_responsive_probe,
    responsive_failure_intervals, responsive_probe_plan,
};
pub use snapshot::{
    DocumentMetrics, HitTestSample, LAYOUT_SNAPSHOT_SCHEMA_V, LayoutSnapshot, MAX_HIT_TEST_SAMPLES,
    MAX_LABEL_CHARS, MAX_NODES, MAX_RESPONSIVE_BREAKPOINTS, MAX_ROUTE_CHARS, Point, Rect,
    SnapshotIndex, UiNode, UiNodeId, UiStateKey, Viewport,
};
pub use spatial::{CandidatePairs, MAX_CANDIDATE_PAIRS, overlapping_pairs};
pub use visual::{
    MAX_VISUAL_REGIONS, VisualRegion, VisualRegionDiff, VisualRegionKind, region_visual_diff,
};
pub use visual_pixels::{MAX_CROP_PIXELS, PixelFrame, encode_rgba_png};

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

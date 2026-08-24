//! UI-integrity findings and their stable ratchet identity.
//!
//! A finding is a measurement, not an opinion. Every one carries the numbers
//! that produced it — hit-test samples, occlusion ratio, overflow pixels,
//! scroll versus client size — so `quality_explain` can show a reviewer exactly
//! what was observed instead of reporting a "possible overlap".

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wvq_domain::Severity;

use crate::snapshot::UiStateKey;

/// The deterministic P0 detector catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiCheck {
    /// `WVQ-UI-DUP-001` — two rendered nodes share one non-empty `id`.
    DuplicateDomId,
    /// `WVQ-UI-DUP-002` — two rendered nodes share one stable test id.
    DuplicateTestId,
    /// `WVQ-UI-DUP-003` — two interactive nodes are indistinguishable in scope.
    AmbiguousInteractive,
    /// `WVQ-UI-LAYOUT-001` — an enabled control does not receive events.
    InteractiveOcclusion,
    /// `WVQ-UI-LAYOUT-002` — a node leaves the effective viewport.
    ViewportOverflow,
    /// `WVQ-UI-LAYOUT-003` — text is clipped with no accepted truncation.
    TextClipping,
    /// `WVQ-UI-LAYOUT-004` — two interactive nodes overlap by policy-forbidden
    /// geometry confirmed by hit testing.
    ForbiddenOverlap,
}

impl UiCheck {
    /// Stable catalogue identity.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::DuplicateDomId => "WVQ-UI-DUP-001",
            Self::DuplicateTestId => "WVQ-UI-DUP-002",
            Self::AmbiguousInteractive => "WVQ-UI-DUP-003",
            Self::InteractiveOcclusion => "WVQ-UI-LAYOUT-001",
            Self::ViewportOverflow => "WVQ-UI-LAYOUT-002",
            Self::TextClipping => "WVQ-UI-LAYOUT-003",
            Self::ForbiddenOverlap => "WVQ-UI-LAYOUT-004",
        }
    }

    /// Every P0 detector, in catalogue order.
    #[must_use]
    pub fn all() -> [Self; 7] {
        [
            Self::DuplicateDomId,
            Self::DuplicateTestId,
            Self::AmbiguousInteractive,
            Self::InteractiveOcclusion,
            Self::ViewportOverflow,
            Self::TextClipping,
            Self::ForbiddenOverlap,
        ]
    }

    /// Whether the finding can change as viewport width changes.
    #[must_use]
    pub fn is_responsive(self) -> bool {
        matches!(
            self,
            Self::InteractiveOcclusion
                | Self::ViewportOverflow
                | Self::TextClipping
                | Self::ForbiddenOverlap
        )
    }
}

impl std::fmt::Display for UiCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

/// Quantified evidence behind one finding. Integers only, so a finding
/// compares and hashes exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiEvidence {
    /// Hit-test points probed on the target.
    pub sample_count: u32,
    /// Points where the target (or a descendant) was topmost.
    pub received_event_samples: u32,
    /// Share of probed points the target lost, in permille.
    pub failure_ratio_permille: u16,
    /// Overlapping area as a share of the target, in permille.
    pub overlap_ratio_permille: u16,
    /// How far the node leaves the effective viewport, in whole pixels.
    pub overflow_px: i64,
    /// `scrollWidth` when text metrics were collected.
    pub scroll_width: i64,
    /// `clientWidth` when text metrics were collected.
    pub client_width: i64,
    /// `scrollHeight` when text metrics were collected.
    pub scroll_height: i64,
    /// `clientHeight` when text metrics were collected.
    pub client_height: i64,
    /// How many nodes shared the duplicated identity.
    pub duplicate_count: u32,
}

/// One measured UI-integrity problem at one route, state, and viewport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiIntegrityFinding {
    /// Detector that produced it.
    pub check: UiCheck,
    /// Gate severity before the ratchet classifies it.
    pub severity: Severity,
    /// Base/head comparison key.
    pub state: UiStateKey,
    /// Route the finding was measured on.
    pub route: String,
    /// Viewport it was measured at.
    pub viewport: String,
    /// Semantic identity of the target.
    pub subject: String,
    /// Occluding, overlapping, or duplicate counterpart, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterpart: Option<String>,
    /// Component the target came from, when the app exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_hint: Option<String>,
    /// Collector node identities involved, for artifact drill-down.
    #[serde(default)]
    pub nodes: Vec<String>,
    /// Quantified evidence.
    pub evidence: UiEvidence,
    /// One sentence a reviewer can act on. Always numeric.
    pub detail: String,
}

impl UiIntegrityFinding {
    /// Stable ratchet identity.
    ///
    /// Built from the detector, the measurement point, and the semantic
    /// identities involved — never from counts or wording, so the same problem
    /// keeps one fingerprint while its numbers move. Route, viewport, and state
    /// are part of the identity: the same duplicate at two viewports is two
    /// findings, because fixing one does not fix the other.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        for part in [
            self.check.id(),
            self.state.as_str(),
            self.subject.as_str(),
            self.counterpart.as_deref().unwrap_or(""),
        ] {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }
        let digest = hasher
            .finalize()
            .iter()
            .take(16)
            .fold(String::new(), |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            });
        format!("ui:{}:{digest}", self.check.id())
    }

    /// Stable identity across viewport widths for responsive interval search.
    #[must_use]
    pub fn responsive_identity(&self) -> String {
        format!(
            "{}\0{}\0{}\0{}",
            self.check.id(),
            self.state.without_viewport(),
            self.subject,
            self.counterpart.as_deref().unwrap_or("")
        )
    }

    /// Sort key that keeps artifacts and replies byte-stable.
    #[must_use]
    pub fn order_key(&self) -> (&'static str, &str, &str, &str) {
        (
            self.check.id(),
            self.state.as_str(),
            self.subject.as_str(),
            self.counterpart.as_deref().unwrap_or(""),
        )
    }
}

/// Sort findings into the one deterministic order every surface uses.
pub fn sort_findings(findings: &mut [UiIntegrityFinding]) {
    findings.sort_by(|left, right| left.order_key().cmp(&right.order_key()));
}

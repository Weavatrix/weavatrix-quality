//! Bounded diagnostic reel for a Playwright failure.
//!
//! Assembled only after a step fails. The frames are evidence for a reviewer,
//! never a [`crate::diff::DiffAxis`] and never a triangle input. A passing
//! program produces no reel and pays no capture cost.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::program::Target;

/// Schema of [`FailureReel`].
pub const FAILURE_REEL_SCHEMA_V: u32 = 1;

/// At most before, highlight, and after.
pub const MAX_FAILURE_REEL_FRAMES: usize = 3;

/// Ceiling on one diagnostic PNG. Larger frames are dropped, not hashed.
pub const MAX_FAILURE_REEL_FRAME_BYTES: u64 = 2 * 1024 * 1024;

/// Ceiling on the stored cause sentence.
pub const MAX_FAILURE_REEL_CAUSE_CHARS: usize = 512;

/// Sealed predicates whose failure is geometric, not a textual assertion miss.
const GEOMETRIC_PREDICATES: [&str; 4] = [
    "no_overlap",
    "inside_viewport",
    "text_not_clipped",
    "receives_events",
];

/// Files captured around one failed step, still on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FailureReelCapture {
    /// Program identity.
    pub program: String,
    /// Zero-based failed step.
    pub step: usize,
    /// `TestAction` tag (`activate`, `assert`, …).
    pub action: String,
    /// Semantic target of the failed action, when the IR named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Stable failure text from the bridge.
    pub failure: String,
    /// Pre-action frame, copied off the last observation screenshot when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_path: Option<PathBuf>,
    /// Post-action page with the semantic target outlined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight_path: Option<PathBuf>,
    /// Post-action page with no overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_path: Option<PathBuf>,
    /// Honest gaps: missing before-frame, missing target, dropped size, …
    #[serde(default)]
    pub limitations: Vec<String>,
}

impl FailureReelCapture {
    /// Every local PNG the persist layer must import or delete.
    #[must_use]
    pub fn frame_paths(&self) -> Vec<&Path> {
        [&self.before_path, &self.highlight_path, &self.after_path]
            .into_iter()
            .filter_map(Option::as_deref)
            .collect()
    }
}

/// Why the step failed. Diagnostic only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCauseKind {
    /// A sealed `assert` step missed its expectation.
    Assertion,
    /// A sealed spatial predicate missed (overlap, clip, viewport, events).
    Geometric,
    /// The action itself failed (timeout, missing target, network, …).
    Action,
}

/// Bounded explanation stored next to the frames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureCause {
    /// Assertion, geometric, or action.
    pub kind: FailureCauseKind,
    /// One sentence, truncated.
    pub text: String,
    /// Obligation id when the failure named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation: Option<String>,
    /// Geometric predicate tag when [`FailureCauseKind::Geometric`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
}

/// Handles (or local names, before persist) for the three diagnostic frames.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureReelFrames {
    /// Observation immediately before the failing action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Same page with the semantic target outlined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    /// Page after the failing action, no overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

impl FailureReelFrames {
    /// How many slots are filled.
    #[must_use]
    pub fn count(&self) -> usize {
        usize::from(self.before.is_some())
            + usize::from(self.highlight.is_some())
            + usize::from(self.after.is_some())
    }
}

/// CAS document. `diagnostic` is always true; `join_triangle` must ignore it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureReel {
    /// [`FAILURE_REEL_SCHEMA_V`].
    pub schema_v: u32,
    /// Always `true`. A reel is never a verdict source.
    pub diagnostic: bool,
    /// Program identity.
    pub program: String,
    /// Zero-based failed step.
    pub step: usize,
    /// `TestAction` tag.
    pub action: String,
    /// Compact semantic identity of the failed target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Assertion, geometric, or action cause.
    pub cause: FailureCause,
    /// At most three frame handles.
    pub frames: FailureReelFrames,
    /// Honest gaps.
    #[serde(default)]
    pub limitations: Vec<String>,
    /// Green-path and diagnostic path both spend zero model tokens.
    pub runtime_llm_tokens: u32,
}

/// Copy one observation screenshot into a reel-owned name so persist can
/// delete the observation file independently.
///
/// # Errors
///
/// Returns [`None`] when the source cannot be copied. The caller records a limitation.
#[must_use]
pub fn copy_reel_frame(
    source: &Path,
    evidence_dir: &Path,
    program: &str,
    step: usize,
    slot: &str,
) -> Option<PathBuf> {
    let name = format!(
        "{}-reel-{step}-{slot}.png",
        safe_file_token(program)
    );
    let dest = evidence_dir.join(name);
    std::fs::copy(source, &dest).ok()?;
    Some(dest)
}

/// Build the diagnostic document, or [`None`] on the green path.
///
/// A passing program never produces a reel, even if a capture leaked in.
/// Missing frames become limitations; they are never invented.
#[must_use]
pub fn assemble_failure_reel(
    passed: bool,
    capture: Option<&FailureReelCapture>,
) -> Option<FailureReel> {
    if passed {
        return None;
    }
    let capture = capture?;
    if capture.program.trim().is_empty() {
        return None;
    }
    let mut limitations = capture.limitations.clone();
    let before = accept_frame(capture.before_path.as_deref(), "before", &mut limitations);
    let highlight = accept_frame(
        capture.highlight_path.as_deref(),
        "highlight",
        &mut limitations,
    );
    let after = accept_frame(capture.after_path.as_deref(), "after", &mut limitations);
    if before.is_none() && !limitations.iter().any(|item| item == "before_frame_unmeasured") {
        limitations.push("before_frame_unmeasured".into());
    }
    if capture.target.is_some()
        && highlight.is_none()
        && !limitations
            .iter()
            .any(|item| item == "target_not_located" || item == "highlight_unmeasured")
    {
        limitations.push("highlight_unmeasured".into());
    }
    if capture.target.is_none()
        && !limitations
            .iter()
            .any(|item| item == "target_not_applicable")
    {
        limitations.push("target_not_applicable".into());
    }
    limitations.sort();
    limitations.dedup();
    let frames = FailureReelFrames {
        before,
        highlight,
        after,
    };
    debug_assert!(frames.count() <= MAX_FAILURE_REEL_FRAMES);
    Some(FailureReel {
        schema_v: FAILURE_REEL_SCHEMA_V,
        diagnostic: true,
        program: capture.program.clone(),
        step: capture.step,
        action: capture.action.clone(),
        target: capture.target.as_ref().map(summarize_target),
        cause: failure_cause(&capture.failure),
        frames,
        limitations,
        runtime_llm_tokens: 0,
    })
}

/// Classify the bridge failure text. No model call.
#[must_use]
pub fn failure_cause(failure: &str) -> FailureCause {
    let text = truncate_cause(failure);
    let geometric = GEOMETRIC_PREDICATES
        .iter()
        .copied()
        .find(|marker| failure.contains(marker));
    if let Some(check) = geometric {
        return FailureCause {
            kind: FailureCauseKind::Geometric,
            obligation: obligation_from_failure(failure),
            check: Some(check.into()),
            text,
        };
    }
    if failure.starts_with("assertion_failed:")
        || failure.starts_with("condition_not_established:")
    {
        return FailureCause {
            kind: FailureCauseKind::Assertion,
            obligation: obligation_from_failure(failure),
            check: None,
            text,
        };
    }
    FailureCause {
        kind: FailureCauseKind::Action,
        obligation: None,
        check: None,
        text,
    }
}

/// Compact identity used in the CAS document. Never a locator.
#[must_use]
pub fn summarize_target(target: &Target) -> String {
    if let Some(test_id) = target.test_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        return format!("testid:{test_id}");
    }
    match (
        target.role.as_deref().map(str::trim).filter(|v| !v.is_empty()),
        target
            .accessible_name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    ) {
        (Some(role), Some(name)) => format!("{role}:{name}"),
        (Some(role), None) => format!("role:{role}"),
        (None, Some(name)) => format!("name:{name}"),
        (None, None) => {
            if let Some(label) = target.label.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                format!("label:{label}")
            } else if let Some(hint) = target
                .component_hint
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                format!("component:{hint}")
            } else {
                "semantic-target".into()
            }
        }
    }
}

fn accept_frame(path: Option<&Path>, slot: &str, limitations: &mut Vec<String>) -> Option<String> {
    let path = path?;
    let meta = match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta,
        _ => {
            limitations.push(format!("frame_unreadable:{slot}"));
            return None;
        }
    };
    if meta.len() == 0 {
        limitations.push(format!("frame_empty:{slot}"));
        return None;
    }
    if meta.len() > MAX_FAILURE_REEL_FRAME_BYTES {
        limitations.push(format!("frame_exceeded_bound:{slot}"));
        return None;
    }
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

fn obligation_from_failure(failure: &str) -> Option<String> {
    let rest = failure
        .strip_prefix("assertion_failed:")
        .or_else(|| failure.strip_prefix("condition_not_established:"))?;
    let obligation = rest.split_once(':').map_or(rest, |(id, _)| id).trim();
    if obligation.is_empty() {
        None
    } else {
        Some(obligation.to_owned())
    }
}

fn truncate_cause(failure: &str) -> String {
    let trimmed = failure.trim();
    if trimmed.chars().count() <= MAX_FAILURE_REEL_CAUSE_CHARS {
        return trimmed.to_owned();
    }
    trimmed.chars().take(MAX_FAILURE_REEL_CAUSE_CHARS).collect()
}

fn safe_file_token(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes().take(100) {
        if byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'-' {
            out.push(byte as char);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "program".into()
    } else {
        out
    }
}

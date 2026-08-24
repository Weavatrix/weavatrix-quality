//! Bounded, versioned layout evidence.
//!
//! A `LayoutSnapshot` is the only thing the detectors read. It is deliberately
//! not a DOM: there is no `innerHTML`, no form values, no cookies, no response
//! bodies, and no unbounded visible text. What survives collection is geometry,
//! semantic identity, and hit-test results — enough to prove a duplicate or an
//! occlusion, and not enough to leak a user's data into an artifact store.
//!
//! Every bound is explicit. A snapshot that hit one of them carries
//! `truncated = true`, and a truncated snapshot is never treated as clean.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use wvq_domain::{ContentHash, RevisionId};

use crate::UiError;

/// Only schema version the detectors accept. Unknown versions fail closed.
pub const LAYOUT_SNAPSHOT_SCHEMA_V: u32 = 2;

/// Hard ceiling on collected nodes, whatever local policy asks for.
pub const MAX_NODES: usize = 20_000;

/// Hard ceiling on hit-test samples in one snapshot.
pub const MAX_HIT_TEST_SAMPLES: usize = 40_000;

/// Maximum CSS/container width breakpoints carried by one snapshot.
pub const MAX_RESPONSIVE_BREAKPOINTS: usize = 128;

/// Longest accessible name, label, or text hint kept in evidence.
pub const MAX_LABEL_CHARS: usize = 120;

/// Longest route recorded.
pub const MAX_ROUTE_CHARS: usize = 512;

/// Axis-aligned rectangle in CSS pixels, relative to the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width. Never negative.
    pub width: f64,
    /// Height. Never negative.
    pub height: f64,
}

impl Rect {
    /// Right edge.
    #[must_use]
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Area in square pixels.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Whether the rectangle has any extent at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// Overlapping region, when the two rectangles genuinely intersect.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            return None;
        }
        Some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    /// Whether `self` fully contains `other` within `tolerance` pixels.
    #[must_use]
    pub fn contains(&self, other: &Self, tolerance: f64) -> bool {
        self.x <= other.x + tolerance
            && self.y <= other.y + tolerance
            && self.right() + tolerance >= other.right()
            && self.bottom() + tolerance >= other.bottom()
    }

    fn validate(&self, label: &str) -> Result<(), UiError> {
        let finite = self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite();
        if !finite {
            return Err(UiError::Malformed(format!("{label} has a non-finite rect")));
        }
        if self.width < 0.0 || self.height < 0.0 {
            return Err(UiError::Malformed(format!(
                "{label} has a negative rect extent"
            )));
        }
        Ok(())
    }
}

/// One point in viewport coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    /// Horizontal offset.
    pub x: f64,
    /// Vertical offset.
    pub y: f64,
}

/// Collector-assigned node identity. Stable only within one snapshot.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UiNodeId(String);

impl UiNodeId {
    /// Parse a non-empty identity.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Malformed`] when `raw` is empty or has whitespace.
    pub fn new(raw: impl AsRef<str>) -> Result<Self, UiError> {
        let raw = raw.as_ref();
        if raw.is_empty() || raw.chars().any(char::is_whitespace) {
            return Err(UiError::Malformed(
                "ui node id must be a non-empty string without whitespace".into(),
            ));
        }
        Ok(Self(raw.to_owned()))
    }

    /// Borrow the identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UiNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One collected element. Bounded, redacted, and geometry-first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct UiNode {
    /// Collector-assigned identity.
    pub id: UiNodeId,

    /// `id` attribute, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dom_id: Option<String>,
    /// Configured stable test attribute, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,

    /// ARIA role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Accessible name, bounded and redacted by the collector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessible_name: Option<String>,
    /// Associated label, bounded and redacted by the collector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Lowercase HTML tag. Structural evidence only; never markup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Lowercase input type, only for an `input` element. Never its value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    /// This exact semantic node is named by a sealed predicate.
    #[serde(default)]
    pub required_by_oracle: bool,

    /// Browser facts needed by standards-derived accessibility rules. `None`
    /// means an older collector did not measure the fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focusable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_associated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modal: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains_focus: Option<bool>,

    /// Raw bounded ARIA state tokens. Rust owns role/state interpretation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria_disabled: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria_checked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria_selected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria_pressed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aria_expanded: Option<String>,

    /// Framework component name, when the app exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_hint: Option<String>,
    /// Row, record, or dialog identity that scopes repeated controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_key: Option<String>,

    /// Client rectangles. A wrapped inline element has more than one.
    #[serde(default)]
    pub rects: Vec<Rect>,
    /// Nearest clipping ancestor's rectangle, when the node is clipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_rect: Option<Rect>,

    /// Rendered and not `visibility: hidden` / `display: none` / zero-sized.
    pub visible: bool,
    /// Accepts pointer or keyboard interaction by role or tag.
    pub interactive: bool,
    /// Not disabled or `aria-disabled`.
    pub enabled: bool,
    /// Computed `pointer-events` is not `none`.
    pub pointer_events: bool,
    /// Computed `overflow` makes this node a scroll container.
    #[serde(default)]
    pub scrollable: bool,
    /// Purely decorative: `aria-hidden`, or a presentational layer.
    #[serde(default)]
    pub decorative: bool,

    /// Computed `position`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    /// Resolved `z-index`, when not `auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i32>,
    /// Identity of the nearest stacking context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stacking_context: Option<String>,

    /// Parent node in the collected subset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<UiNodeId>,

    /// `scrollWidth`, when text metrics were collected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_scroll_width: Option<f64>,
    /// `clientWidth`, when text metrics were collected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_client_width: Option<f64>,
    /// `scrollHeight`, when text metrics were collected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_scroll_height: Option<f64>,
    /// `clientHeight`, when text metrics were collected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_client_height: Option<f64>,
}

impl UiNode {
    /// Union of every client rectangle, or `None` for a node with no box.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        let mut iter = self.rects.iter().filter(|rect| !rect.is_empty());
        let first = *iter.next()?;
        Some(iter.fold(first, |acc, rect| {
            let x = acc.x.min(rect.x);
            let y = acc.y.min(rect.y);
            let right = acc.right().max(rect.right());
            let bottom = acc.bottom().max(rect.bottom());
            Rect {
                x,
                y,
                width: right - x,
                height: bottom - y,
            }
        }))
    }

    /// Bounds after the nearest clipping ancestor is applied.
    #[must_use]
    pub fn visible_bounds(&self) -> Option<Rect> {
        let bounds = self.bounds()?;
        match &self.clip_rect {
            Some(clip) => bounds.intersection(clip),
            None => Some(bounds),
        }
    }

    /// Whether this node can actually be operated by a user.
    #[must_use]
    pub fn is_actionable(&self) -> bool {
        self.visible && self.interactive && self.enabled && self.pointer_events
    }

    /// Human-facing semantic identity used as a finding subject.
    #[must_use]
    pub fn semantic_identity(&self) -> String {
        if let Some(test_id) = non_empty(self.test_id.as_deref()) {
            return format!("testid:{test_id}");
        }
        if let Some(dom_id) = non_empty(self.dom_id.as_deref()) {
            return format!("#{dom_id}");
        }
        match (
            non_empty(self.role.as_deref()),
            non_empty(self.accessible_name.as_deref()),
        ) {
            (Some(role), Some(name)) => format!("{role}:{name}"),
            (Some(role), None) => format!("{role}:<unnamed>"),
            (None, Some(name)) => format!("text:{name}"),
            (None, None) => non_empty(self.component_hint.as_deref()).map_or_else(
                || format!("node:{}", self.id),
                |hint| format!("component:{hint}"),
            ),
        }
    }

    fn validate(&self) -> Result<(), UiError> {
        for (index, rect) in self.rects.iter().enumerate() {
            rect.validate(&format!("node {} rect {index}", self.id))?;
        }
        if let Some(clip) = &self.clip_rect {
            clip.validate(&format!("node {} clip_rect", self.id))?;
        }
        for (field, value) in [
            ("accessible_name", self.accessible_name.as_deref()),
            ("label", self.label.as_deref()),
            ("component_hint", self.component_hint.as_deref()),
            ("entity_key", self.entity_key.as_deref()),
            ("role", self.role.as_deref()),
            ("dom_id", self.dom_id.as_deref()),
            ("test_id", self.test_id.as_deref()),
            ("tag", self.tag.as_deref()),
            ("input_type", self.input_type.as_deref()),
            ("aria_disabled", self.aria_disabled.as_deref()),
            ("aria_checked", self.aria_checked.as_deref()),
            ("aria_selected", self.aria_selected.as_deref()),
            ("aria_pressed", self.aria_pressed.as_deref()),
            ("aria_expanded", self.aria_expanded.as_deref()),
        ] {
            if value.is_some_and(|text| text.chars().count() > MAX_LABEL_CHARS) {
                return Err(UiError::Malformed(format!(
                    "node {} {field} exceeds {MAX_LABEL_CHARS} characters; \
                     the collector must bound and redact it before it becomes evidence",
                    self.id
                )));
            }
        }
        for (field, value) in [
            ("text_scroll_width", self.text_scroll_width),
            ("text_client_width", self.text_client_width),
            ("text_scroll_height", self.text_scroll_height),
            ("text_client_height", self.text_client_height),
        ] {
            if value.is_some_and(|metric| !metric.is_finite() || metric < 0.0) {
                return Err(UiError::Malformed(format!(
                    "node {} {field} is not a non-negative finite number",
                    self.id
                )));
            }
        }
        Ok(())
    }
}

impl Default for UiNodeId {
    fn default() -> Self {
        Self("node".to_owned())
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

/// One hit test against one interactive target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTestSample {
    /// Node the sample was taken for.
    pub target: UiNodeId,
    /// Viewport point that was probed.
    pub point: Point,
    /// Node `elementsFromPoint` reported first, when anything was there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topmost: Option<UiNodeId>,
    /// Full paint order at the point, outermost last.
    #[serde(default)]
    pub stack: Vec<UiNodeId>,
}

/// Rendered viewport size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    /// CSS pixel width.
    pub width: u32,
    /// CSS pixel height.
    pub height: u32,
}

impl Viewport {
    /// Stable `WxH` label used in fingerprints and findings.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    /// Viewport as a rectangle.
    #[must_use]
    pub fn rect(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: f64::from(self.width),
            height: f64::from(self.height),
        }
    }
}

/// Document-level scroll metrics, used to separate a page-wide horizontal
/// overflow from one element sticking out of an intentional scroll container.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DocumentMetrics {
    /// `documentElement.scrollWidth`.
    pub scroll_width: f64,
    /// `documentElement.clientWidth`.
    pub client_width: f64,
    /// `documentElement.scrollHeight`.
    pub scroll_height: f64,
    /// `documentElement.clientHeight`.
    pub client_height: f64,
}

/// One route/state/viewport of one revision, as collected by the browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutSnapshot {
    /// Schema version. Only [`LAYOUT_SNAPSHOT_SCHEMA_V`].
    pub schema_v: u32,
    /// Exact revision the snapshot belongs to.
    pub revision: RevisionId,
    /// Program that drove the browser to this point.
    pub program: String,
    /// Zero-based `TestProgram` step index the snapshot follows.
    pub step: u32,
    /// Application route.
    pub route: String,
    /// Accessibility digest of the rendered state. Provenance, not identity:
    /// it changes whenever the DOM changes, which is exactly what a regression
    /// does, so it is never the base/head comparison key.
    pub state_digest: ContentHash,
    /// Rendered viewport.
    pub viewport: Viewport,
    /// Width transitions discovered from the browser's parsed CSSOM.
    #[serde(default)]
    pub responsive_breakpoints: Vec<u32>,
    /// False when an applied stylesheet could not be inspected.
    #[serde(default)]
    pub responsive_breakpoints_complete: bool,
    /// Document scroll metrics.
    #[serde(default)]
    pub document: DocumentMetrics,
    /// Bounded candidate node set.
    pub nodes: Vec<UiNode>,
    /// Hit-test samples for interactive targets.
    #[serde(default)]
    pub hit_tests: Vec<HitTestSample>,
    /// True when any bound was hit. A truncated snapshot is never clean.
    #[serde(default)]
    pub truncated: bool,
}

/// The base/head comparison key.
///
/// Deliberately built from the program, step, route, and viewport rather than
/// from the DOM digest: base and head must line up on the same measurement
/// point even though the change under review altered the markup.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UiStateKey(String);

impl UiStateKey {
    /// Borrow the key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Program/step/route identity with the viewport suffix removed.
    #[must_use]
    pub fn without_viewport(&self) -> &str {
        self.0
            .rsplit_once('@')
            .map_or(self.0.as_str(), |(state, _)| state)
    }
}

impl std::fmt::Display for UiStateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Node lookup and ancestry for one snapshot.
///
/// Built once per analysis. Ancestor questions are asked thousands of times by
/// the detectors, so rebuilding the map per call would turn an `O(n log n)`
/// pass into an `O(n² log n)` one.
#[derive(Debug, Clone)]
pub struct SnapshotIndex<'a> {
    nodes: BTreeMap<&'a UiNodeId, &'a UiNode>,
    depth_limit: usize,
}

impl<'a> SnapshotIndex<'a> {
    /// Index a snapshot in one pass.
    #[must_use]
    pub fn new(snapshot: &'a LayoutSnapshot) -> Self {
        Self {
            nodes: snapshot.nodes.iter().map(|node| (&node.id, node)).collect(),
            // The node count bounds every ancestor walk, so a malformed parent
            // cycle terminates instead of spinning.
            depth_limit: snapshot.nodes.len(),
        }
    }

    /// Node by collector identity.
    #[must_use]
    pub fn node(&self, id: &UiNodeId) -> Option<&'a UiNode> {
        self.nodes.get(id).copied()
    }

    /// Whether `candidate` is `ancestor` itself or sits inside it.
    #[must_use]
    pub fn is_self_or_descendant(&self, candidate: &UiNodeId, ancestor: &UiNodeId) -> bool {
        if candidate == ancestor {
            return true;
        }
        let mut current = self
            .nodes
            .get(candidate)
            .and_then(|node| node.parent.as_ref());
        for _ in 0..self.depth_limit {
            let Some(parent) = current else { return false };
            if parent == ancestor {
                return true;
            }
            current = self.nodes.get(parent).and_then(|node| node.parent.as_ref());
        }
        false
    }

    /// Nearest `entity_key` on the node or one of its ancestors.
    #[must_use]
    pub fn scope_of(&self, id: &UiNodeId) -> Option<String> {
        let mut current = Some(id);
        for _ in 0..=self.depth_limit {
            let node = self.nodes.get(current?)?;
            if let Some(entity) = non_empty(node.entity_key.as_deref()) {
                return Some(entity.to_owned());
            }
            current = node.parent.as_ref();
        }
        None
    }

    /// Whether anything above `node` scrolls.
    #[must_use]
    pub fn has_scrollable_ancestor(&self, node: &UiNode) -> bool {
        let mut current = node.parent.as_ref();
        for _ in 0..self.depth_limit {
            let Some(parent) = current.and_then(|id| self.node(id)) else {
                return false;
            };
            if parent.scrollable {
                return true;
            }
            current = parent.parent.as_ref();
        }
        false
    }
}

impl LayoutSnapshot {
    /// Base/head comparison key for this snapshot.
    #[must_use]
    pub fn state_key(&self) -> UiStateKey {
        UiStateKey(format!(
            "{}#{}@{}@{}",
            self.program,
            self.step,
            self.route,
            self.viewport.label()
        ))
    }

    /// Index this snapshot for repeated node and ancestry lookups.
    #[must_use]
    pub fn index(&self) -> SnapshotIndex<'_> {
        SnapshotIndex::new(self)
    }

    /// Reject a snapshot the detectors must not reason about.
    ///
    /// # Errors
    ///
    /// Unknown schema version, a bound exceeded, duplicate or dangling node
    /// identities, a non-finite rectangle, or an over-long label.
    pub fn validate(&self) -> Result<(), UiError> {
        if self.schema_v != LAYOUT_SNAPSHOT_SCHEMA_V {
            return Err(UiError::UnknownSchema(self.schema_v));
        }
        if self.program.trim().is_empty() {
            return Err(UiError::Malformed(
                "layout snapshot must name the program that produced it".into(),
            ));
        }
        if self.route.chars().count() > MAX_ROUTE_CHARS {
            return Err(UiError::Malformed(format!(
                "layout snapshot route exceeds {MAX_ROUTE_CHARS} characters"
            )));
        }
        if self.viewport.width == 0 || self.viewport.height == 0 {
            return Err(UiError::Malformed(
                "layout snapshot viewport must have a non-zero extent".into(),
            ));
        }
        if self.responsive_breakpoints.len() > MAX_RESPONSIVE_BREAKPOINTS {
            return Err(UiError::Bounded(format!(
                "layout snapshot has {} responsive breakpoints, the hard ceiling is {MAX_RESPONSIVE_BREAKPOINTS}",
                self.responsive_breakpoints.len()
            )));
        }
        if self
            .responsive_breakpoints
            .iter()
            .any(|width| !(1..=16_384).contains(width))
        {
            return Err(UiError::Malformed(
                "layout snapshot responsive breakpoints must be between 1 and 16384 pixels".into(),
            ));
        }
        if self.nodes.len() > MAX_NODES {
            return Err(UiError::Bounded(format!(
                "layout snapshot has {} nodes, the hard ceiling is {MAX_NODES}",
                self.nodes.len()
            )));
        }
        if self.hit_tests.len() > MAX_HIT_TEST_SAMPLES {
            return Err(UiError::Bounded(format!(
                "layout snapshot has {} hit-test samples, the hard ceiling is \
                 {MAX_HIT_TEST_SAMPLES}",
                self.hit_tests.len()
            )));
        }
        let mut seen = BTreeSet::new();
        for node in &self.nodes {
            if !seen.insert(node.id.clone()) {
                return Err(UiError::Malformed(format!(
                    "layout snapshot repeats node identity {}",
                    node.id
                )));
            }
            node.validate()?;
        }
        for node in &self.nodes {
            if let Some(parent) = &node.parent
                && !seen.contains(parent)
            {
                return Err(UiError::Malformed(format!(
                    "node {} names parent {parent}, which is not in the snapshot",
                    node.id
                )));
            }
        }
        for sample in &self.hit_tests {
            if !seen.contains(&sample.target) {
                return Err(UiError::Malformed(format!(
                    "hit test names target {}, which is not in the snapshot",
                    sample.target
                )));
            }
            for member in sample.topmost.iter().chain(&sample.stack) {
                if !seen.contains(member) {
                    return Err(UiError::Malformed(format!(
                        "hit test stack names {member}, which is not in the snapshot"
                    )));
                }
            }
        }
        Ok(())
    }
}

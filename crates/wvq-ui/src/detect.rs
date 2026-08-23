//! The deterministic P0 detectors.
//!
//! Every detector here is arithmetic over one [`LayoutSnapshot`]. There is no
//! model call, no vision, and no heuristic phrasing: a finding is emitted only
//! when the collected geometry or hit-test result forces it, and it carries the
//! numbers that did.
//!
//! Controlling false positives is most of the work. Repeated row actions,
//! tooltips over their triggers, badges over avatars, icons inside inputs,
//! children inside parents, backdrops under dialogs, and intentional scroll
//! containers are all normal UI. Each is excluded by a specific rule below
//! rather than by a confidence threshold.

use std::collections::BTreeMap;

use wvq_domain::Severity;

use crate::finding::{UiCheck, UiEvidence, UiIntegrityFinding, sort_findings};
use crate::policy::UiIntegrityPolicy;
use crate::snapshot::{LayoutSnapshot, Rect, SnapshotIndex, UiNode, UiNodeId};
use crate::spatial::overlapping_pairs;

/// Everything one snapshot produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DetectionOutput {
    /// Findings, in deterministic order.
    pub findings: Vec<UiIntegrityFinding>,
    /// True when the snapshot or the candidate search hit a bound.
    pub truncated: bool,
}

/// Run every P0 detector over one validated snapshot.
///
/// # Errors
///
/// Returns [`crate::UiError`] when the snapshot itself is not analysable.
pub fn detect(
    snapshot: &LayoutSnapshot,
    policy: &UiIntegrityPolicy,
) -> Result<DetectionOutput, crate::UiError> {
    snapshot.validate()?;
    let mut out = DetectionOutput {
        truncated: snapshot.truncated,
        ..DetectionOutput::default()
    };
    if !policy.enabled {
        return Ok(out);
    }
    if snapshot.nodes.len() > policy.max_nodes as usize {
        // The collector should have stopped first; if it did not, the snapshot
        // is over budget and must not be reported as a clean measurement.
        out.truncated = true;
    }
    duplicate_identity(snapshot, UiCheck::DuplicateDomId, &mut out.findings);
    duplicate_identity(snapshot, UiCheck::DuplicateTestId, &mut out.findings);
    let index = snapshot.index();
    ambiguous_interactive(snapshot, &index, &mut out.findings);
    let hits = hit_index(snapshot);
    interactive_occlusion(snapshot, &index, policy, &hits, &mut out.findings);
    viewport_overflow(snapshot, &index, policy, &mut out.findings);
    text_clipping(snapshot, policy, &mut out.findings);
    let overlap_truncated = forbidden_overlap(snapshot, &index, policy, &hits, &mut out.findings);
    out.truncated |= overlap_truncated;
    sort_findings(&mut out.findings);
    Ok(out)
}

fn base(
    snapshot: &LayoutSnapshot,
    check: UiCheck,
    severity: Severity,
    subject: String,
    detail: String,
) -> UiIntegrityFinding {
    UiIntegrityFinding {
        check,
        severity,
        state: snapshot.state_key(),
        route: snapshot.route.clone(),
        viewport: snapshot.viewport.label(),
        subject,
        counterpart: None,
        component_hint: None,
        nodes: Vec::new(),
        evidence: UiEvidence::default(),
        detail,
    }
}

// ---------------------------------------------------------------------------
// WVQ-UI-DUP-001 / WVQ-UI-DUP-002 — duplicate machine identity
// ---------------------------------------------------------------------------

/// A rendered `id` or stable test id must resolve to exactly one node.
///
/// Only *visible* nodes count. A hidden template or an off-screen `<template>`
/// clone sharing an id is not what breaks a selector at runtime.
fn duplicate_identity(
    snapshot: &LayoutSnapshot,
    check: UiCheck,
    out: &mut Vec<UiIntegrityFinding>,
) {
    let mut groups: BTreeMap<&str, Vec<&UiNode>> = BTreeMap::new();
    for node in snapshot.nodes.iter().filter(|node| node.visible) {
        let key = match check {
            UiCheck::DuplicateDomId => node.dom_id.as_deref(),
            _ => node.test_id.as_deref(),
        };
        if let Some(key) = key.map(str::trim).filter(|text| !text.is_empty()) {
            groups.entry(key).or_default().push(node);
        }
    }
    for (identity, nodes) in groups {
        if nodes.len() < 2 {
            continue;
        }
        let subject = match check {
            UiCheck::DuplicateDomId => format!("#{identity}"),
            _ => format!("testid:{identity}"),
        };
        let count = u32::try_from(nodes.len()).unwrap_or(u32::MAX);
        let mut finding = base(
            snapshot,
            check,
            Severity::Error,
            subject,
            format!(
                "{count} visible nodes share the identity `{identity}`; \
                 a selector for it resolves ambiguously"
            ),
        );
        finding.evidence.duplicate_count = count;
        finding.nodes = nodes.iter().map(|node| node.id.to_string()).collect();
        finding.component_hint = nodes.iter().find_map(|node| node.component_hint.clone());
        out.push(finding);
    }
}

// ---------------------------------------------------------------------------
// WVQ-UI-DUP-003 — ambiguous interactive semantic identity
// ---------------------------------------------------------------------------

/// Two controls a user cannot tell apart, in one scope.
///
/// The scope is what stops this firing on every list: a `Delete` button in row
/// 1 and a `Delete` button in row 2 share a role and a name but not an
/// `entity_key`, so they are two unambiguous controls. Two `Save` buttons
/// inside one dialog share all three and are genuinely ambiguous.
///
/// When no scope can be resolved the finding drops to a warning. WVQ does not
/// know whether it is looking at one dialog or two rows, and refusing to guess
/// is cheaper than blocking a healthy change.
fn ambiguous_interactive(
    snapshot: &LayoutSnapshot,
    index: &SnapshotIndex<'_>,
    out: &mut Vec<UiIntegrityFinding>,
) {
    let mut groups: BTreeMap<(String, String, Option<String>), Vec<&UiNode>> = BTreeMap::new();
    for node in snapshot.nodes.iter().filter(|node| node.is_actionable()) {
        let (Some(role), Some(name)) = (
            node.role
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty()),
            node.accessible_name
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty()),
        ) else {
            continue;
        };
        // A node that carries its own entity key is its own scope; otherwise the
        // nearest ancestor that declares one wins.
        let scope = index.scope_of(&node.id);
        groups
            .entry((role.to_owned(), name.to_owned(), scope))
            .or_default()
            .push(node);
    }
    for ((role, name, scope), nodes) in groups {
        if nodes.len() < 2 {
            continue;
        }
        let count = u32::try_from(nodes.len()).unwrap_or(u32::MAX);
        let (severity, detail) = match &scope {
            Some(scope) => (
                Severity::Error,
                format!(
                    "{count} enabled `{role}` controls named `{name}` share the scope `{scope}`; \
                     no semantic locator can pick one"
                ),
            ),
            None => (
                Severity::Warn,
                format!(
                    "{count} enabled `{role}` controls named `{name}` were found and no row, \
                     record, or dialog scope could be resolved; add an entity key to tell them \
                     apart or confirm they are one control"
                ),
            ),
        };
        let mut finding = base(
            snapshot,
            UiCheck::AmbiguousInteractive,
            severity,
            format!("{role}:{name}"),
            detail,
        );
        finding.counterpart = scope.map(|scope| format!("scope:{scope}"));
        finding.evidence.duplicate_count = count;
        finding.nodes = nodes.iter().map(|node| node.id.to_string()).collect();
        finding.component_hint = nodes.iter().find_map(|node| node.component_hint.clone());
        out.push(finding);
    }
}

// ---------------------------------------------------------------------------
// WVQ-UI-LAYOUT-001 — interactive occlusion
// ---------------------------------------------------------------------------

/// Hit-test samples grouped by the target they were taken for.
type HitIndex<'a> = BTreeMap<&'a UiNodeId, Vec<&'a crate::snapshot::HitTestSample>>;

fn hit_index(snapshot: &LayoutSnapshot) -> HitIndex<'_> {
    let mut index: HitIndex<'_> = BTreeMap::new();
    for sample in &snapshot.hit_tests {
        index.entry(&sample.target).or_default().push(sample);
    }
    index
}

/// Whether the node on top of `target` at a sample point is an acceptable one.
///
/// The target itself and anything inside it are the normal case — a button's
/// own icon or label is what `elementsFromPoint` reports. A layer that has
/// `pointer-events: none` never intercepts the click at all. Anything the
/// project explicitly allowed to paint over this node is intentional.
fn blocker_is_allowed(
    index: &SnapshotIndex<'_>,
    policy: &UiIntegrityPolicy,
    target: &UiNode,
    topmost: Option<&UiNodeId>,
) -> bool {
    let Some(topmost) = topmost else {
        // Nothing was reported at the point: the browser found no element, so
        // there is no occluder to blame.
        return true;
    };
    if index.is_self_or_descendant(topmost, &target.id) {
        return true;
    }
    let Some(blocker) = index.node(topmost) else {
        return true;
    };
    if !blocker.pointer_events || blocker.decorative {
        return true;
    }
    // A parent painting "over" its own child is containment, not occlusion.
    if index.is_self_or_descendant(&target.id, topmost) {
        return true;
    }
    policy.overlap_allowed(blocker, target)
}

fn interactive_occlusion(
    snapshot: &LayoutSnapshot,
    index: &SnapshotIndex<'_>,
    policy: &UiIntegrityPolicy,
    hits: &HitIndex<'_>,
    out: &mut Vec<UiIntegrityFinding>,
) {
    for node in snapshot.nodes.iter().filter(|node| node.is_actionable()) {
        let Some(samples) = hits.get(&node.id) else {
            continue;
        };
        if samples.is_empty() {
            continue;
        }
        let mut received = 0_u32;
        let mut blockers: BTreeMap<&UiNodeId, u32> = BTreeMap::new();
        for sample in samples {
            if blocker_is_allowed(index, policy, node, sample.topmost.as_ref()) {
                received += 1;
            } else if let Some(topmost) = &sample.topmost {
                *blockers.entry(topmost).or_default() += 1;
            }
        }
        let total = u32::try_from(samples.len()).unwrap_or(u32::MAX);
        let lost = total.saturating_sub(received);
        let failure_permille = permille(u64::from(lost), u64::from(total));
        if failure_permille < policy.occlusion_failure_permille {
            continue;
        }
        // The blocker that shadowed the most points is the one to report.
        let blocker = blockers
            .into_iter()
            .max_by_key(|(id, count)| (*count, std::cmp::Reverse(id.as_str().to_owned())))
            .map(|(id, _)| id);
        let blocker_node = blocker.and_then(|id| index.node(id));
        let overlap = blocker_node
            .and_then(UiNode::visible_bounds)
            .zip(node.visible_bounds())
            .and_then(|(top, bottom)| {
                top.intersection(&bottom)
                    .map(|shared| ratio_permille(shared.area(), bottom.area()))
            })
            .unwrap_or_default();
        let mut finding = base(
            snapshot,
            UiCheck::InteractiveOcclusion,
            Severity::Error,
            node.semantic_identity(),
            format!(
                "{lost} of {total} hit-test points on this enabled control were intercepted by \
                 {}; only {received} would deliver the event",
                blocker_node.map_or_else(
                    || "another element".to_owned(),
                    |item| format!("`{}`", item.semantic_identity())
                )
            ),
        );
        finding.counterpart = blocker_node.map(UiNode::semantic_identity);
        finding.component_hint.clone_from(&node.component_hint);
        finding.nodes = std::iter::once(node.id.to_string())
            .chain(blocker.map(ToString::to_string))
            .collect();
        finding.evidence = UiEvidence {
            sample_count: total,
            received_event_samples: received,
            failure_ratio_permille: failure_permille,
            overlap_ratio_permille: overlap,
            ..UiEvidence::default()
        };
        out.push(finding);
    }
}

// ---------------------------------------------------------------------------
// WVQ-UI-LAYOUT-002 — viewport overflow
// ---------------------------------------------------------------------------

/// A control the user cannot reach, and a page that scrolls sideways.
///
/// An element inside a scroll container is not overflowing: it is content the
/// user scrolls to, which is the whole point of the container. A `fixed` or
/// `sticky` node is positioned against the viewport, so leaving it is a real
/// defect and is reported with its position named.
fn viewport_overflow(
    snapshot: &LayoutSnapshot,
    index: &SnapshotIndex<'_>,
    policy: &UiIntegrityPolicy,
    out: &mut Vec<UiIntegrityFinding>,
) {
    let viewport = snapshot.viewport.rect();
    let tolerance = policy.tolerance();
    for node in snapshot.nodes.iter().filter(|node| node.is_actionable()) {
        if index.has_scrollable_ancestor(node) {
            continue;
        }
        let Some(bounds) = node.visible_bounds() else {
            continue;
        };
        let left = (viewport.x - bounds.x).max(0.0);
        let right = (bounds.right() - viewport.right()).max(0.0);
        let top = (viewport.y - bounds.y).max(0.0);
        let bottom = (bounds.bottom() - viewport.bottom()).max(0.0);
        let worst = left.max(right).max(top).max(bottom);
        if worst <= tolerance {
            continue;
        }
        let reachable = bounds.intersection(&viewport).is_some();
        let mut finding = base(
            snapshot,
            UiCheck::ViewportOverflow,
            Severity::Error,
            node.semantic_identity(),
            format!(
                "this enabled control extends {worst:.0}px outside the {} viewport and is {}; \
                 no ancestor scrolls to bring it back{}",
                snapshot.viewport.label(),
                if reachable {
                    "only partly reachable"
                } else {
                    "entirely unreachable"
                },
                node.position
                    .as_deref()
                    .filter(|position| matches!(*position, "fixed" | "sticky"))
                    .map_or_else(String::new, |position| format!(" ({position} positioned)"))
            ),
        );
        finding.component_hint.clone_from(&node.component_hint);
        finding.nodes = vec![node.id.to_string()];
        finding.evidence.overflow_px = round_px(worst);
        out.push(finding);
    }

    // Whole-page horizontal overflow is a different defect from one stray
    // control, so it gets its own subject and a warning rather than a gate.
    let document = snapshot.document;
    let horizontal = document.scroll_width - document.client_width;
    if document.client_width > 0.0 && horizontal > tolerance {
        let mut finding = base(
            snapshot,
            UiCheck::ViewportOverflow,
            Severity::Warn,
            "document".into(),
            format!(
                "the page scrolls {horizontal:.0}px horizontally at {} \
                 (scrollWidth {:.0} vs clientWidth {:.0})",
                snapshot.viewport.label(),
                document.scroll_width,
                document.client_width
            ),
        );
        finding.evidence.overflow_px = round_px(horizontal);
        finding.evidence.scroll_width = round_px(document.scroll_width);
        finding.evidence.client_width = round_px(document.client_width);
        out.push(finding);
    }
}

// ---------------------------------------------------------------------------
// WVQ-UI-LAYOUT-003 — text clipping
// ---------------------------------------------------------------------------

/// Text the box is too small to show.
///
/// An ellipsis is a deliberate design choice most of the time, so this is a
/// warning by default and only becomes a gate for an actionable control whose
/// full value is not available to assistive technology either — at that point
/// nobody, sighted or not, can read what the button does.
fn text_clipping(
    snapshot: &LayoutSnapshot,
    policy: &UiIntegrityPolicy,
    out: &mut Vec<UiIntegrityFinding>,
) {
    let tolerance = policy.tolerance();
    for node in snapshot.nodes.iter().filter(|node| node.visible) {
        let (Some(scroll_w), Some(client_w)) = (node.text_scroll_width, node.text_client_width)
        else {
            continue;
        };
        let scroll_h = node.text_scroll_height.unwrap_or_default();
        let client_h = node.text_client_height.unwrap_or_default();
        let horizontal = scroll_w - client_w;
        let vertical = scroll_h - client_h;
        if horizontal <= tolerance && vertical <= tolerance {
            continue;
        }
        // A scroll container is supposed to hold more than it shows.
        if node.scrollable {
            continue;
        }
        if policy.truncation_accepted(node) {
            continue;
        }
        let accessible_full_value = node
            .accessible_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        let critical = node.is_actionable() && !accessible_full_value;
        let mut finding = base(
            snapshot,
            UiCheck::TextClipping,
            if critical {
                Severity::Error
            } else {
                Severity::Warn
            },
            node.semantic_identity(),
            if critical {
                format!(
                    "this control's label is clipped ({scroll_w:.0}px of text in {client_w:.0}px \
                     of box) and no accessible name carries the full value"
                )
            } else {
                format!(
                    "text is clipped: scrollWidth {scroll_w:.0} vs clientWidth {client_w:.0}, \
                     scrollHeight {scroll_h:.0} vs clientHeight {client_h:.0}"
                )
            },
        );
        finding.component_hint.clone_from(&node.component_hint);
        finding.nodes = vec![node.id.to_string()];
        finding.evidence = UiEvidence {
            overflow_px: round_px(horizontal.max(vertical)),
            scroll_width: round_px(scroll_w),
            client_width: round_px(client_w),
            scroll_height: round_px(scroll_h),
            client_height: round_px(client_h),
            ..UiEvidence::default()
        };
        out.push(finding);
    }
}

// ---------------------------------------------------------------------------
// WVQ-UI-LAYOUT-004 — explicit forbidden overlap
// ---------------------------------------------------------------------------

/// Two controls that overlap, confirmed by hit testing.
///
/// This is deliberately *not* "any two rectangles intersect". Overlap is how
/// most interfaces are built: an icon sits inside its input, a badge sits on an
/// avatar, a dialog sits on its backdrop, a tooltip sits on its trigger. What
/// is reported here is narrower — two separately operable controls covering
/// each other, where the browser confirms one paints above the other, and no
/// declared allowance covers the pair.
///
/// Returns whether the candidate search hit its bound.
fn forbidden_overlap(
    snapshot: &LayoutSnapshot,
    index: &SnapshotIndex<'_>,
    policy: &UiIntegrityPolicy,
    hits: &HitIndex<'_>,
    out: &mut Vec<UiIntegrityFinding>,
) -> bool {
    let interactive: Vec<&UiNode> = snapshot
        .nodes
        .iter()
        .filter(|node| node.is_actionable())
        .collect();
    let rects: Vec<Option<Rect>> = interactive
        .iter()
        .map(|node| node.visible_bounds())
        .collect();
    let candidates = overlapping_pairs(&rects);
    for (left_index, right_index) in candidates.pairs {
        let left = interactive[left_index];
        let right = interactive[right_index];
        // Containment is structure, not collision.
        if index.is_self_or_descendant(&left.id, &right.id)
            || index.is_self_or_descendant(&right.id, &left.id)
        {
            continue;
        }
        let (Some(left_rect), Some(right_rect)) = (rects[left_index], rects[right_index]) else {
            continue;
        };
        let Some(shared) = left_rect.intersection(&right_rect) else {
            continue;
        };
        // Which one does the browser actually paint on top?
        let Some((top, bottom)) = confirmed_order(index, hits, left, right) else {
            // Geometry alone is not evidence. Without a hit test saying one
            // covers the other, this stays unreported rather than becoming a
            // guess.
            continue;
        };
        if policy.overlap_allowed(top, bottom) {
            continue;
        }
        let bottom_rect = if std::ptr::eq(bottom, left) {
            left_rect
        } else {
            right_rect
        };
        let overlap = ratio_permille(shared.area(), bottom_rect.area());
        let mut finding = base(
            snapshot,
            UiCheck::ForbiddenOverlap,
            Severity::Warn,
            bottom.semantic_identity(),
            format!(
                "`{}` paints over {}permille of this control's box; hit testing confirms the \
                 order and no declared overlap allows the pair",
                top.semantic_identity(),
                overlap
            ),
        );
        finding.counterpart = Some(top.semantic_identity());
        finding.component_hint.clone_from(&bottom.component_hint);
        finding.nodes = vec![bottom.id.to_string(), top.id.to_string()];
        finding.evidence.overlap_ratio_permille = overlap;
        out.push(finding);
    }
    candidates.truncated
}

/// `(top, bottom)` when a hit-test sample proves which node paints above.
fn confirmed_order<'a>(
    index: &SnapshotIndex<'_>,
    hits: &HitIndex<'_>,
    left: &'a UiNode,
    right: &'a UiNode,
) -> Option<(&'a UiNode, &'a UiNode)> {
    for (target, other) in [(left, right), (right, left)] {
        for sample in hits.get(&target.id).into_iter().flatten() {
            let Some(topmost) = &sample.topmost else {
                continue;
            };
            if index.is_self_or_descendant(topmost, &other.id)
                && !index.is_self_or_descendant(topmost, &target.id)
            {
                return Some((other, target));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------

fn permille(part: u64, whole: u64) -> u16 {
    if whole == 0 {
        return 0;
    }
    u16::try_from(part.saturating_mul(1000) / whole).unwrap_or(1000)
}

fn ratio_permille(part: f64, whole: f64) -> u16 {
    if whole <= 0.0 || !part.is_finite() || !whole.is_finite() {
        return 0;
    }
    let scaled = (part / whole).clamp(0.0, 1.0) * 1000.0;
    // Clamped to `0.0 ..= 1000.0` above, so the cast is exact.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        scaled.round() as u16
    }
}

/// Round a CSS-pixel measurement to a whole number of pixels.
///
/// Geometry is bounded by the viewport and document size, so the clamp below is
/// defensive rather than load-bearing; it exists so a malformed snapshot can
/// never produce a saturating or wrapping cast.
fn round_px(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    let clamped = value.round().clamp(-1e12, 1e12);
    // Clamped well inside the range i64 represents exactly.
    #[allow(clippy::cast_possible_truncation)]
    {
        clamped as i64
    }
}

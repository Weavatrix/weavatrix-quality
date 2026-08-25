//! Region-guided visual comparison.
//!
//! Full-page SHA-256 names whether any screenshot byte changed. This module
//! names *which* visual surface changed: match nodes by semantic identity,
//! take their clipped rectangles, and compare exact pixels only inside those
//! crops. There is no perceptual kernel and no vision call.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::snapshot::{LayoutSnapshot, Rect, UiNode};
use crate::visual_pixels::{PixelFrame, crop_matches};

/// Hard ceiling on named visual surfaces in one comparison.
pub const MAX_VISUAL_REGIONS: usize = 64;

/// Why a named surface is in the region set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualRegionKind {
    /// Present only on head.
    Added,
    /// Present only on base.
    Removed,
    /// Same identity, rectangle moved or resized beyond tolerance.
    GeometryChanged,
    /// Same identity and rectangle; PNG crops differ.
    PixelsChanged,
}

/// One named visual surface that differed between revisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualRegion {
    /// Semantic identity (`testid:…`, `role:name`, `#id`).
    pub surface: String,
    /// How the surface changed.
    pub kind: VisualRegionKind,
    /// Clipped bounds on base, when the node existed.
    pub base_rect: Option<Rect>,
    /// Clipped bounds on head, when the node existed.
    pub head_rect: Option<Rect>,
    /// Pixels that differed inside the crop. Absent when pixels were not compared.
    pub mismatched_pixels: Option<u64>,
    /// Pixels examined inside the crop.
    pub compared_pixels: Option<u64>,
}

/// Geometry-first, then exact-pixel, visual comparison of one state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct VisualRegionDiff {
    /// Named surfaces that changed, sorted by kind then identity.
    pub regions: Vec<VisualRegion>,
    /// True when [`MAX_VISUAL_REGIONS`] stopped the list.
    pub truncated: bool,
    /// Why a crop or pairing could not be measured.
    pub limitations: Vec<String>,
}

impl VisualRegionDiff {
    /// Whether any named visual surface changed.
    #[must_use]
    pub fn changed(&self) -> bool {
        !self.regions.is_empty()
    }
}

/// Pair two snapshots of the same program/step/route/viewport.
///
/// `base_png` / `head_png` are optional full-page screenshots. Without them the
/// reading is geometry-only: added, removed, and moved surfaces. With both, a
/// same-geometry surface is pixel-compared in its crop.
#[must_use]
pub fn region_visual_diff(
    base: &LayoutSnapshot,
    head: &LayoutSnapshot,
    base_png: Option<&[u8]>,
    head_png: Option<&[u8]>,
    geometry_tolerance_px: f64,
) -> VisualRegionDiff {
    let mut diff = VisualRegionDiff::default();
    let base_nodes = unique_surfaces(base, &mut diff.limitations, "base");
    let head_nodes = unique_surfaces(head, &mut diff.limitations, "head");
    let mut keys = BTreeSet::new();
    keys.extend(base_nodes.keys().cloned());
    keys.extend(head_nodes.keys().cloned());

    let css_width = if base.viewport.width == 0 {
        diff.limitations
            .push("viewport width is zero; pixel crops are unmeasured".into());
        0
    } else if base.viewport.width != head.viewport.width {
        diff.limitations.push(format!(
            "viewport widths differ ({} vs {}); pixel crops are unmeasured",
            base.viewport.width, head.viewport.width
        ));
        0
    } else {
        base.viewport.width
    };

    let frames = match (base_png, head_png) {
        (Some(base_bytes), Some(head_bytes)) => match (
            PixelFrame::decode(base_bytes),
            PixelFrame::decode(head_bytes),
        ) {
            (Ok(base_frame), Ok(head_frame)) => Some((base_frame, head_frame)),
            (Err(error), _) | (_, Err(error)) => {
                diff.limitations.push(error);
                None
            }
        },
        (None, None) => None,
        _ => {
            diff.limitations
                .push("only one revision captured a screenshot; pixel crops are unmeasured".into());
            None
        }
    };

    for key in keys {
        if diff.regions.len() >= MAX_VISUAL_REGIONS {
            diff.truncated = true;
            diff.limitations.push(format!(
                "visual region set hit the {MAX_VISUAL_REGIONS}-surface ceiling"
            ));
            break;
        }
        let base_node = base_nodes.get(&key).copied();
        let head_node = head_nodes.get(&key).copied();
        let base_rect = base_node.and_then(UiNode::visible_bounds);
        let head_rect = head_node.and_then(UiNode::visible_bounds);
        let Some(region) = classify_region(
            &key,
            base_rect,
            head_rect,
            geometry_tolerance_px,
            frames.as_ref(),
            css_width,
            &mut diff.limitations,
        ) else {
            continue;
        };
        diff.regions.push(region);
    }
    diff.regions.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.surface.cmp(&right.surface))
    });
    diff
}

fn unique_surfaces<'a>(
    snapshot: &'a LayoutSnapshot,
    limitations: &mut Vec<String>,
    side: &str,
) -> BTreeMap<String, &'a UiNode> {
    let mut counts = BTreeMap::<String, u32>::new();
    for node in &snapshot.nodes {
        if !eligible(node) {
            continue;
        }
        *counts.entry(node.semantic_identity()).or_default() += 1;
    }
    let mut out = BTreeMap::new();
    for node in &snapshot.nodes {
        if !eligible(node) {
            continue;
        }
        let key = node.semantic_identity();
        if counts.get(&key) != Some(&1) {
            continue;
        }
        out.insert(key, node);
    }
    for (key, count) in counts {
        if count > 1 {
            limitations.push(format!(
                "{side} surface `{key}` is ambiguous ({count} nodes); skipped"
            ));
        }
    }
    out
}

fn eligible(node: &UiNode) -> bool {
    node.visible && !node.decorative && node.visible_bounds().is_some()
}

fn classify_region(
    surface: &str,
    base_rect: Option<Rect>,
    head_rect: Option<Rect>,
    tolerance: f64,
    frames: Option<&(PixelFrame, PixelFrame)>,
    css_width: u32,
    limitations: &mut Vec<String>,
) -> Option<VisualRegion> {
    match (base_rect, head_rect) {
        (None, Some(head_rect)) => Some(VisualRegion {
            surface: surface.to_owned(),
            kind: VisualRegionKind::Added,
            base_rect: None,
            head_rect: Some(head_rect),
            mismatched_pixels: None,
            compared_pixels: None,
        }),
        (Some(base_rect), None) => Some(VisualRegion {
            surface: surface.to_owned(),
            kind: VisualRegionKind::Removed,
            base_rect: Some(base_rect),
            head_rect: None,
            mismatched_pixels: None,
            compared_pixels: None,
        }),
        (Some(base_rect), Some(head_rect)) => {
            if !rects_match(&base_rect, &head_rect, tolerance) {
                return Some(VisualRegion {
                    surface: surface.to_owned(),
                    kind: VisualRegionKind::GeometryChanged,
                    base_rect: Some(base_rect),
                    head_rect: Some(head_rect),
                    mismatched_pixels: None,
                    compared_pixels: None,
                });
            }
            let (base_frame, head_frame) = frames?;
            if css_width == 0 {
                return None;
            }
            match crop_matches(
                base_frame,
                head_frame,
                &base_rect,
                &head_rect,
                f64::from(css_width),
            ) {
                Ok((compared, mismatched)) if mismatched > 0 => Some(VisualRegion {
                    surface: surface.to_owned(),
                    kind: VisualRegionKind::PixelsChanged,
                    base_rect: Some(base_rect),
                    head_rect: Some(head_rect),
                    mismatched_pixels: Some(mismatched),
                    compared_pixels: Some(compared),
                }),
                Ok(_) => None,
                Err(limitation) => {
                    limitations.push(format!("{surface}: {limitation}"));
                    None
                }
            }
        }
        (None, None) => None,
    }
}

fn rects_match(left: &Rect, right: &Rect, tolerance: f64) -> bool {
    (left.x - right.x).abs() <= tolerance
        && (left.y - right.y).abs() <= tolerance
        && (left.width - right.width).abs() <= tolerance
        && (left.height - right.height).abs() <= tolerance
}

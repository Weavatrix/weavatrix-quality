//! Region-guided visual diff names the surface, not the whole PNG.

use wvq_ui::{
    DocumentMetrics, LAYOUT_SNAPSHOT_SCHEMA_V, LayoutSnapshot, Rect, UiNode, UiNodeId, Viewport,
    VisualRegionKind, encode_rgba_png, region_visual_diff,
};

fn id(raw: &str) -> UiNodeId {
    UiNodeId::new(raw).unwrap()
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn control(node_id: &str, test_id: &str, bounds: Rect) -> UiNode {
    UiNode {
        id: id(node_id),
        test_id: Some(test_id.into()),
        role: Some("button".into()),
        accessible_name: Some(test_id.into()),
        rects: vec![bounds],
        visible: true,
        interactive: true,
        enabled: true,
        pointer_events: true,
        ..UiNode::default()
    }
}

fn snapshot(nodes: Vec<UiNode>, width: u32, height: u32) -> LayoutSnapshot {
    LayoutSnapshot {
        schema_v: LAYOUT_SNAPSHOT_SCHEMA_V,
        revision: wvq_domain::RevisionId::new("rev-head").unwrap(),
        program: "checkout".into(),
        step: 3,
        route: "/checkout".into(),
        state_digest: wvq_domain::ContentHash::new("ab".repeat(32)).unwrap(),
        viewport: Viewport { width, height },
        responsive_breakpoints: Vec::new(),
        responsive_breakpoints_complete: true,
        document: DocumentMetrics {
            scroll_width: f64::from(width),
            client_width: f64::from(width),
            scroll_height: f64::from(height),
            client_height: f64::from(height),
        },
        nodes,
        hit_tests: Vec::new(),
        truncated: false,
    }
}

fn fill(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    pixel
        .into_iter()
        .cycle()
        .take((width * height * 4) as usize)
        .collect()
}

fn put(rgba: &mut [u8], width: u32, x: u32, y: u32, pixel: [u8; 4]) {
    let index = ((y * width + x) * 4) as usize;
    rgba[index..index + 4].copy_from_slice(&pixel);
}

fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    encode_rgba_png(width, height, rgba).unwrap()
}

#[test]
fn an_added_surface_is_named_without_pixels() {
    let base = snapshot(Vec::new(), 16, 8);
    let head = snapshot(
        vec![control("n1", "pay", rect(2.0, 2.0, 4.0, 4.0))],
        16,
        8,
    );
    let diff = region_visual_diff(&base, &head, None, None, 1.0);
    assert_eq!(diff.regions.len(), 1);
    assert_eq!(diff.regions[0].surface, "testid:pay");
    assert_eq!(diff.regions[0].kind, VisualRegionKind::Added);
}

#[test]
fn a_moved_surface_is_geometry_not_pixels() {
    let base = snapshot(
        vec![control("n1", "pay", rect(2.0, 2.0, 4.0, 4.0))],
        16,
        8,
    );
    let head = snapshot(
        vec![control("n1", "pay", rect(8.0, 2.0, 4.0, 4.0))],
        16,
        8,
    );
    let diff = region_visual_diff(&base, &head, None, None, 1.0);
    assert_eq!(diff.regions[0].kind, VisualRegionKind::GeometryChanged);
}

#[test]
fn a_pixel_change_inside_the_named_crop_is_attributed() {
    let nodes = vec![control("n1", "pay", rect(2.0, 2.0, 4.0, 4.0))];
    let layout = snapshot(nodes, 16, 8);
    let base_rgba = fill(16, 8, [255, 0, 0, 255]);
    let mut head_rgba = fill(16, 8, [255, 0, 0, 255]);
    put(&mut head_rgba, 16, 3, 3, [0, 0, 255, 255]);
    let diff = region_visual_diff(
        &layout,
        &layout,
        Some(&png(16, 8, &base_rgba)),
        Some(&png(16, 8, &head_rgba)),
        1.0,
    );
    assert_eq!(diff.regions.len(), 1);
    assert_eq!(diff.regions[0].surface, "testid:pay");
    assert_eq!(diff.regions[0].kind, VisualRegionKind::PixelsChanged);
    assert_eq!(diff.regions[0].mismatched_pixels, Some(1));
}

#[test]
fn a_pixel_change_outside_named_surfaces_is_not_a_region() {
    let nodes = vec![control("n1", "pay", rect(2.0, 2.0, 4.0, 4.0))];
    let layout = snapshot(nodes, 16, 8);
    let base_rgba = fill(16, 8, [255, 0, 0, 255]);
    let mut head_rgba = fill(16, 8, [255, 0, 0, 255]);
    put(&mut head_rgba, 16, 0, 0, [0, 255, 0, 255]);
    let diff = region_visual_diff(
        &layout,
        &layout,
        Some(&png(16, 8, &base_rgba)),
        Some(&png(16, 8, &head_rgba)),
        1.0,
    );
    assert!(
        diff.regions.is_empty(),
        "chrome pixels must not name the pay surface: {:?}",
        diff.regions
    );
}

#[test]
fn an_ambiguous_identity_is_skipped_not_guessed() {
    let layout = snapshot(
        vec![
            control("n1", "pay", rect(2.0, 2.0, 2.0, 2.0)),
            control("n2", "pay", rect(8.0, 2.0, 2.0, 2.0)),
        ],
        16,
        8,
    );
    let diff = region_visual_diff(&layout, &layout, None, None, 1.0);
    assert!(diff.regions.is_empty());
    assert!(
        diff.limitations
            .iter()
            .any(|item| item.contains("ambiguous")),
        "{:?}",
        diff.limitations
    );
}

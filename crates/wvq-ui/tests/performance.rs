//! Measured cost of the candidate search and a full detection pass.
//!
//! These are measurements, not claims. The assertions bound *work* — candidate
//! pairs compared, not wall-clock time — because a timing threshold on shared
//! CI hardware is a flake generator. Wall time is printed so a human can read
//! it from the test log without the suite depending on it.

use std::time::Instant;

use wvq_ui::{
    DocumentMetrics, LAYOUT_SNAPSHOT_SCHEMA_V, LayoutSnapshot, Rect, UiIntegrityPolicy, UiNode,
    UiNodeId, Viewport, detect, overlapping_pairs,
};

/// A grid layout of `count` non-overlapping controls, the realistic shape: a
/// page is mostly a tiling of boxes, not a pile.
fn grid(count: usize) -> Vec<UiNode> {
    let columns = 20;
    (0..count)
        .map(|index| {
            let column = index % columns;
            let row = index / columns;
            UiNode {
                id: UiNodeId::new(format!("n{index}")).unwrap(),
                role: Some("button".into()),
                accessible_name: Some(format!("Item {index}")),
                rects: vec![Rect {
                    #[allow(clippy::cast_precision_loss)]
                    x: (column * 64) as f64,
                    #[allow(clippy::cast_precision_loss)]
                    y: (row * 32) as f64,
                    width: 60.0,
                    height: 28.0,
                }],
                visible: true,
                interactive: true,
                enabled: true,
                pointer_events: true,
                ..UiNode::default()
            }
        })
        .collect()
}

fn snapshot(nodes: Vec<UiNode>) -> LayoutSnapshot {
    LayoutSnapshot {
        schema_v: LAYOUT_SNAPSHOT_SCHEMA_V,
        revision: wvq_domain::RevisionId::new("rev-head").unwrap(),
        program: "grid".into(),
        step: 0,
        route: "/grid".into(),
        state_digest: wvq_domain::ContentHash::new("cd".repeat(32)).unwrap(),
        viewport: Viewport {
            width: 1280,
            height: 720,
        },
        responsive_breakpoints: Vec::new(),
        responsive_breakpoints_complete: true,
        document: DocumentMetrics {
            scroll_width: 1280.0,
            client_width: 1280.0,
            scroll_height: 720.0,
            client_height: 720.0,
        },
        nodes,
        hit_tests: Vec::new(),
        truncated: false,
    }
}

#[test]
fn the_sweep_compares_far_fewer_pairs_than_a_full_scan() {
    let nodes = grid(5_000);
    let rects: Vec<Option<Rect>> = nodes.iter().map(UiNode::bounds).collect();
    let started = Instant::now();
    let candidates = overlapping_pairs(&rects);
    let elapsed = started.elapsed();

    let naive = nodes.len() * (nodes.len() - 1) / 2;
    println!(
        "sweep over {} nodes: {} intersecting pairs in {:?} (a full scan would compare {naive})",
        nodes.len(),
        candidates.pairs.len(),
        elapsed
    );
    assert!(!candidates.truncated);
    assert!(
        candidates.pairs.len() < naive / 100,
        "a tiled layout must not degenerate into a pairwise scan: {} of {naive}",
        candidates.pairs.len()
    );
}

#[test]
fn a_full_detection_pass_at_the_default_ceiling_is_measured() {
    let state = snapshot(grid(5_000));
    let policy = UiIntegrityPolicy {
        enabled: true,
        ..UiIntegrityPolicy::default()
    };
    let started = Instant::now();
    let output = detect(&state, &policy).unwrap();
    let elapsed = started.elapsed();
    println!(
        "detection over {} nodes: {} findings in {:?}",
        state.nodes.len(),
        output.findings.len(),
        elapsed
    );
    assert!(!output.truncated);
}

#[test]
fn a_dense_pile_is_bounded_rather_than_silently_truncated() {
    // Every box on top of every other: the worst case the sweep cannot avoid.
    let nodes: Vec<UiNode> = (0..800)
        .map(|index| UiNode {
            id: UiNodeId::new(format!("n{index}")).unwrap(),
            rects: vec![Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            }],
            visible: true,
            interactive: true,
            enabled: true,
            pointer_events: true,
            ..UiNode::default()
        })
        .collect();
    let rects: Vec<Option<Rect>> = nodes.iter().map(UiNode::bounds).collect();
    let candidates = overlapping_pairs(&rects);
    println!(
        "dense pile of {}: {} pairs, truncated={}",
        nodes.len(),
        candidates.pairs.len(),
        candidates.truncated
    );
    // 800 fully stacked boxes really are 319 600 pairs; the point is that the
    // ceiling reports itself instead of dropping candidates in silence.
    assert_eq!(candidates.truncated, candidates.pairs.len() >= 200_000);
}

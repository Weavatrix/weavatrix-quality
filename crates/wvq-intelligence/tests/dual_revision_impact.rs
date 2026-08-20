//! Task 29: the impacted surface spans both revisions, never head alone.

use wvq_intelligence::{
    FlowEntry, FlowState, GraphDelta, ImpactedFlow, SurfaceDelta, fingerprint, impacted_surface,
    match_flows,
};

fn flow(id: &str, revision: &str, surface: &str, nodes: &[&str], edges: &[&str]) -> ImpactedFlow {
    ImpactedFlow {
        id: id.into(),
        revision: revision.into(),
        entry: FlowEntry::Endpoint(surface.into()),
        graph_nodes: nodes.iter().map(|item| (*item).to_string()).collect(),
        graph_edges: edges.iter().map(|item| (*item).to_string()).collect(),
        public_surfaces: vec![surface.into()],
        requirements: Vec::new(),
    }
}

fn names(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

#[test]
fn a_removed_base_edge_stays_in_the_impacted_surface() {
    // base: UI -> controller -> validation -> service
    // head: UI -> service
    let surface = impacted_surface(
        &[
            "UI".into(),
            "controller".into(),
            "validation".into(),
            "service".into(),
        ],
        &["UI".into(), "service".into()],
        &GraphDelta {
            removed_nodes: vec!["controller".into(), "validation".into()],
            removed_edges: vec!["UI->controller".into(), "controller->validation".into()],
        },
        &SurfaceDelta::default(),
    );

    assert_eq!(names(&surface.base_only), vec!["controller", "validation"]);
    assert_eq!(names(&surface.shared), vec!["UI", "service"]);
    assert!(surface.has_removals());
    assert!(
        surface.all_nodes().contains(&"validation".to_owned()),
        "the validation step must remain visible after it is deleted"
    );
    assert_eq!(
        surface.missed_by_head_only(),
        2,
        "a head-only algorithm would lose both removed steps"
    );
}

#[test]
fn a_removed_endpoint_is_represented() {
    let surface = impacted_surface(
        &["handler".into()],
        &[],
        &GraphDelta {
            removed_nodes: vec!["handler".into()],
            removed_edges: Vec::new(),
        },
        &SurfaceDelta {
            added: Vec::new(),
            removed: vec!["GET /api/sankey/others".into()],
        },
    );
    assert_eq!(
        names(&surface.removed_surfaces),
        vec!["GET /api/sankey/others"]
    );
    assert!(surface.has_removals());
}

#[test]
fn a_head_only_flow_is_represented() {
    let surface = impacted_surface(
        &["service".into()],
        &["service".into(), "newHandler".into()],
        &GraphDelta::default(),
        &SurfaceDelta {
            added: vec!["GET /api/sankey/others".into()],
            removed: Vec::new(),
        },
    );
    assert_eq!(names(&surface.head_only), vec!["newHandler"]);
    assert!(!surface.has_removals());
    assert_eq!(surface.missed_by_head_only(), 0);
}

#[test]
fn a_shared_flow_is_matched_across_a_refactor() {
    // The implementation moved, but the endpoint and most nodes are the same.
    let base = vec![flow(
        "b1",
        "rev-base",
        "GET /api/sankey",
        &["handler", "service", "repo"],
        &["handler->service", "service->repo"],
    )];
    let head = vec![flow(
        "h1",
        "rev-head",
        "GET /api/sankey",
        &["handler", "service", "repo"],
        &["handler->service", "service->repo"],
    )];
    let matches = match_flows(&base, &head);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].state, FlowState::Unchanged);
    assert_eq!(matches[0].matched_on, "entry_surface");
}

#[test]
fn same_nodes_with_a_different_path_is_rewired() {
    let base = vec![flow(
        "b1",
        "rev-base",
        "GET /api/sankey",
        &["ui", "validation", "service"],
        &["ui->validation", "validation->service"],
    )];
    let head = vec![flow(
        "h1",
        "rev-head",
        "GET /api/sankey",
        &["ui", "validation", "service"],
        &["ui->service"],
    )];
    let matches = match_flows(&base, &head);
    assert_eq!(
        matches[0].state,
        FlowState::Rewired,
        "the nodes survived but the path through them did not"
    );
}

#[test]
fn a_flow_whose_surface_disappears_is_removed_not_forgotten() {
    let base = vec![flow(
        "b1",
        "rev-base",
        "GET /api/legacy",
        &["legacyHandler"],
        &[],
    )];
    let head = vec![flow("h1", "rev-head", "GET /api/new", &["newHandler"], &[])];
    let matches = match_flows(&base, &head);
    let removed = matches
        .iter()
        .find(|item| item.base.as_deref() == Some("b1"))
        .expect("the base flow is still reported");
    assert_eq!(removed.state, FlowState::Removed);
    assert!(
        matches
            .iter()
            .any(|item| item.state == FlowState::Added && item.head.as_deref() == Some("h1"))
    );
}

#[test]
fn lineage_survives_a_rename_through_a_shared_requirement() {
    let mut base = flow("b1", "rev-base", "GET /api/old", &["a"], &[]);
    base.requirements = vec!["sankey.visual-limit".into()];
    let mut head = flow("h1", "rev-head", "GET /api/renamed", &["z"], &[]);
    head.requirements = vec!["sankey.visual-limit".into()];

    let matches = match_flows(&[base], &[head]);
    assert_eq!(matches.len(), 1, "a rename must not look like remove + add");
    assert_eq!(matches[0].matched_on, "requirement");
    assert_eq!(matches[0].state, FlowState::Modified);
}

#[test]
fn one_base_flow_becoming_two_is_a_split() {
    let base = vec![flow("b1", "rev-base", "GET /api/x", &["a", "b"], &[])];
    let head = vec![
        flow("h1", "rev-head", "GET /api/x", &["a"], &[]),
        flow("h2", "rev-head", "GET /api/x", &["b"], &[]),
    ];
    let matches = match_flows(&base, &head);
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().all(|item| item.state == FlowState::Split));
}

#[test]
fn a_fingerprint_ignores_file_paths() {
    let mut moved = flow("h1", "rev-head", "GET /api/x", &["a", "b"], &[]);
    moved.requirements = vec!["req-1".into()];
    let base = {
        let mut base = flow("b1", "rev-base", "GET /api/x", &["b", "a"], &[]);
        base.requirements = vec!["req-1".into()];
        base
    };
    let left = fingerprint(&base, Some("sankey".into()));
    let right = fingerprint(&moved, Some("sankey".into()));
    assert_eq!(
        left, right,
        "node order and revision must not change the fingerprint"
    );
    assert_eq!(left.entry_surface, "GET /api/x");
}

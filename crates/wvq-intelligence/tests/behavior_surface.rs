//! Behavior combinations exist only with evidence. Facts are never crossed.

use serde_json::json;
use wvq_intelligence::{
    application_surface_graph, behavior_surface_graph, BehaviorSurfaceFact, BehaviorSurfaceOrigin,
    MAX_BEHAVIOR_SURFACES,
};

fn checkout_graph() -> wvq_intelligence::ApplicationSurfaceGraph {
    application_surface_graph(&json!({
        "nodes": [
            {"id": "route:/checkout", "kind": "route", "label": "/checkout"},
            {"id": "route:/idle", "kind": "route", "label": "/idle"}
        ]
    }))
}

fn fact(
    surface: &str,
    role: Option<&str>,
    state: Option<&str>,
    action: Option<&str>,
    origin: BehaviorSurfaceOrigin,
) -> BehaviorSurfaceFact {
    BehaviorSurfaceFact {
        surface: surface.into(),
        role: role.map(str::to_owned),
        state: state.map(str::to_owned),
        action: action.map(str::to_owned),
        flag: None,
        origin,
    }
}

#[test]
fn two_facts_are_not_crossed_into_a_third_combination() {
    let graph = checkout_graph();
    let projected = behavior_surface_graph(
        &graph,
        &[
            fact(
                "route:/checkout",
                Some("admin"),
                None,
                None,
                BehaviorSurfaceOrigin::Recorded,
            ),
            fact(
                "route:/checkout",
                None,
                Some("empty_cart"),
                None,
                BehaviorSurfaceOrigin::Observed,
            ),
        ],
    );
    assert_eq!(projected.behaviors.len(), 2);
    assert!(
        projected
            .behaviors
            .iter()
            .all(|item| item.role.is_none() || item.state.is_none()),
        "admin × empty_cart must not be invented: {:?}",
        projected.behaviors
    );
    assert!(projected
        .behaviors
        .iter()
        .any(|item| item.id == "route:/checkout|role:admin"));
    assert!(projected
        .behaviors
        .iter()
        .any(|item| item.id == "route:/checkout|state:empty_cart"));
}

#[test]
fn a_fact_for_an_unknown_surface_is_dropped() {
    let graph = checkout_graph();
    let projected = behavior_surface_graph(
        &graph,
        &[fact(
            "route:/missing",
            Some("admin"),
            None,
            None,
            BehaviorSurfaceOrigin::Declared,
        )],
    );
    assert!(projected.behaviors.is_empty());
}

#[test]
fn a_surface_only_fact_is_not_a_behavior_combination() {
    let graph = checkout_graph();
    let projected = behavior_surface_graph(
        &graph,
        &[fact(
            "route:/checkout",
            None,
            None,
            None,
            BehaviorSurfaceOrigin::Observed,
        )],
    );
    assert!(projected.behaviors.is_empty());
}

#[test]
fn duplicate_facts_merge_origins_instead_of_duplicating() {
    let graph = checkout_graph();
    let projected = behavior_surface_graph(
        &graph,
        &[
            fact(
                "route:/checkout",
                Some("admin"),
                None,
                Some("activate"),
                BehaviorSurfaceOrigin::Recorded,
            ),
            fact(
                "route:/checkout",
                Some("admin"),
                None,
                Some("activate"),
                BehaviorSurfaceOrigin::Observed,
            ),
        ],
    );
    assert_eq!(projected.behaviors.len(), 1);
    assert_eq!(
        projected.behaviors[0].id,
        "route:/checkout|role:admin|action:activate"
    );
    assert_eq!(
        projected.behaviors[0].origins,
        [
            BehaviorSurfaceOrigin::Observed,
            BehaviorSurfaceOrigin::Recorded
        ]
    );
}

#[test]
fn the_behavior_ceiling_truncates_instead_of_exploding() {
    let graph = checkout_graph();
    let facts = (0..=MAX_BEHAVIOR_SURFACES)
        .map(|index| {
            fact(
                "route:/checkout",
                Some(&format!("role-{index}")),
                None,
                None,
                BehaviorSurfaceOrigin::Recorded,
            )
        })
        .collect::<Vec<_>>();
    let projected = behavior_surface_graph(&graph, &facts);
    assert!(projected.truncated);
    assert_eq!(projected.behaviors.len(), MAX_BEHAVIOR_SURFACES);
}

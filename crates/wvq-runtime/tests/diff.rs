//! Task 20: same `TestProgram` on base/head; structured axes before visual digest.

use std::collections::BTreeMap;

use wvq_domain::{ObligationId, ProgramId};
use wvq_runtime::{
    DiffAxis, Observation, ProgramError, ProgramSource, ReplayHost, StructuredView, TestAction,
    TestProgram, behavior_delta, replay_base_head,
};

fn program() -> TestProgram {
    TestProgram {
        schema_v: 1,
        id: ProgramId::new("sankey-others-replay").unwrap(),
        source: ProgramSource::Recorded,
        obligations: vec![ObligationId::new("others-visible").unwrap()],
        preconditions: Vec::new(),
        steps: vec![
            TestAction::Navigate {
                route: "/sankey".into(),
            },
            TestAction::Assert {
                obligation: ObligationId::new("others-visible").unwrap(),
            },
        ],
        data: BTreeMap::new(),
        faults: BTreeMap::new(),
        api_operations: BTreeMap::new(),
        evidence_policy: wvq_runtime::EvidencePolicy::default(),
        deterministic_seed: Some(7),
    }
}

struct ScriptedHost {
    states: Vec<wvq_runtime::BehaviorState>,
    index: usize,
}

impl ReplayHost for ScriptedHost {
    fn apply(&mut self, _action: &TestAction) -> Result<wvq_runtime::BehaviorState, ProgramError> {
        let state = self
            .states
            .get(self.index)
            .cloned()
            .ok_or_else(|| ProgramError::Invalid("host exhausted".into()))?;
        self.index += 1;
        Ok(state)
    }
}

fn state(route: &str) -> wvq_runtime::BehaviorState {
    wvq_runtime::BehaviorState {
        route: route.into(),
        ..wvq_runtime::BehaviorState::default()
    }
}

fn view(route: &str, digest: Option<&str>) -> StructuredView {
    let obs = Observation {
        route: Some(route.into()),
        visual_digest: digest.map(ToOwned::to_owned),
        visual_surface: digest.map(|_| "screenshot_png".into()),
        ..Observation::default()
    };
    StructuredView::from_replay(&obs, None)
}

#[test]
fn same_program_replays_on_base_and_head() {
    let mut base = ScriptedHost {
        states: vec![state("/sankey"), state("/sankey")],
        index: 0,
    };
    let mut head = ScriptedHost {
        states: vec![state("/sankey-v2"), state("/sankey-v2")],
        index: 0,
    };
    let (base_states, head_states) =
        replay_base_head(&program(), Some(7), &mut base, &mut head).unwrap();
    assert_eq!(base_states.len(), 2);
    assert_eq!(head_states.len(), 2);
    assert_ne!(base_states[0].route, head_states[0].route);
}

#[test]
fn structured_route_change_skips_visual_digest() {
    let base = view("/sankey", Some("aaa"));
    let head = view("/sankey-v2", Some("bbb"));
    let delta = behavior_delta(&base, &head);
    assert_eq!(delta.first_structured, Some(DiffAxis::Route));
    assert!(!delta.visual_compared);
    assert!(
        delta
            .axes
            .iter()
            .all(|item| item.axis != DiffAxis::VisualDigest)
    );
}

#[test]
fn visual_digest_is_compared_only_when_structured_matches() {
    let base = view("/sankey", Some("aaa"));
    let head = view("/sankey", Some("bbb"));
    let delta = behavior_delta(&base, &head);
    assert!(delta.visual_compared);
    assert_eq!(delta.first_structured, None);
    assert_eq!(delta.axes[0].axis, DiffAxis::VisualDigest);
    assert!(delta.axes[0].base.starts_with("screenshot_png:"));
}

#[test]
fn identical_visual_bytes_are_not_a_change_even_if_handles_would_differ() {
    let mut base = view("/sankey", Some("same-digest"));
    let mut head = view("/sankey", Some("same-digest"));
    base.visual_surface = Some("screenshot_png".into());
    head.visual_surface = Some("screenshot_png".into());
    let delta = behavior_delta(&base, &head);
    assert!(delta.visual_compared);
    assert!(!delta.changed());
}

#[test]
fn stored_pixel_token_still_reads_as_visual_digest() {
    let axis: DiffAxis = serde_json::from_str("\"pixel\"").unwrap();
    assert_eq!(axis, DiffAxis::VisualDigest);
    assert_eq!(
        serde_json::to_string(&DiffAxis::VisualDigest).unwrap(),
        "\"visual_digest\""
    );
}

#[test]
fn screenshot_handles_without_a_digest_are_not_visual_evidence() {
    let base = Observation {
        route: Some("/sankey".into()),
        screenshot_handle: Some("cas:base".into()),
        ..Observation::default()
    };
    let head = Observation {
        route: Some("/sankey".into()),
        screenshot_handle: Some("cas:head".into()),
        ..Observation::default()
    };
    let delta = behavior_delta(
        &StructuredView::from_replay(&base, None),
        &StructuredView::from_replay(&head, None),
    );
    assert!(!delta.visual_compared);
    assert!(!delta.changed());
}

#[test]
fn network_order_does_not_count_as_behavior_change() {
    let mut base = view("/sankey", None);
    base.network = vec!["GET /a".into(), "GET /b".into()];
    let mut head = view("/sankey", None);
    head.network = vec!["GET /b".into(), "GET /a".into()];
    let delta = behavior_delta(&base, &head);
    assert!(!delta.changed());
}

#[test]
fn preview_origins_do_not_create_a_network_delta() {
    let base = Observation {
        network: vec!["POST http://127.0.0.1:41001/api/save 204".into()],
        ..Observation::default()
    };
    let head = Observation {
        network: vec!["POST http://127.0.0.1:52002/api/save 204".into()],
        ..Observation::default()
    };
    let delta = behavior_delta(
        &StructuredView::from_replay(&base, None),
        &StructuredView::from_replay(&head, None),
    );
    assert!(
        !delta.changed(),
        "preview origin is not application behavior"
    );
}

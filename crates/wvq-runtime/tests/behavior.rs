//! Task 19: `BehaviorGraph` hash, recorder, coverage contribution, promote, replay.

use std::collections::BTreeMap;

use wvq_domain::{ObligationId, ProgramId};
use wvq_runtime::{
    BehaviorState, GraphMemory, ProgramError, ProgramSource, Recorder, ReplayHost, TestAction,
    coverage_contribution, promote, replay_program, replay_trace, semantic_target,
};

fn dash() -> BehaviorState {
    BehaviorState {
        route: "/analytics/dashboard/42".into(),
        actor: Some("admin".into()),
        component: Some("sankey".into()),
        modal: Some("closed".into()),
        network_phase: Some("idle".into()),
        data_class: Some("above_visual_limit".into()),
        feature_flags: [("new_sankey".into(), "true".into())].into(),
        a11y_digest: Some("a11y-1".into()),
        viewport: Some("1280x720".into()),
    }
}

struct ScriptedHost {
    states: Vec<BehaviorState>,
    index: usize,
}

impl ReplayHost for ScriptedHost {
    fn apply(&mut self, _action: &TestAction) -> Result<BehaviorState, ProgramError> {
        let state = self
            .states
            .get(self.index)
            .cloned()
            .ok_or_else(|| ProgramError::Invalid("host exhausted".into()))?;
        self.index += 1;
        Ok(state)
    }
}

#[test]
fn state_digest_is_order_independent() {
    let mut flipped = dash();
    flipped.feature_flags = [("new_sankey".into(), "true".into())].into();
    let mut other_insert = BehaviorState {
        route: dash().route,
        actor: dash().actor,
        component: dash().component,
        modal: dash().modal,
        network_phase: dash().network_phase,
        data_class: dash().data_class,
        a11y_digest: dash().a11y_digest,
        viewport: dash().viewport,
        feature_flags: BTreeMap::new(),
    };
    other_insert
        .feature_flags
        .insert("new_sankey".into(), "true".into());
    assert_eq!(dash().digest().unwrap(), flipped.digest().unwrap());
    assert_eq!(dash().digest().unwrap(), other_insert.digest().unwrap());
}

#[test]
fn different_route_changes_digest() {
    let mut other = dash();
    other.route = "/settings".into();
    assert_ne!(dash().digest().unwrap(), other.digest().unwrap());
}

#[test]
fn screenshot_is_not_part_of_state_identity() {
    let json = serde_json::to_string(&dash()).unwrap();
    assert!(!json.contains("screenshot"));
}

#[test]
fn recorder_uses_semantic_targets_and_rejects_xpath_programs() {
    let mut rec = Recorder::new("sess-1", Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Others"),
        },
        dash(),
    )
    .unwrap();
    let xpath = r#"{
        "schema_v": 1,
        "id": "p",
        "source": "recorded",
        "obligations": ["others-visible"],
        "steps": [{"action":"activate","target":{"xpath":"//button"}}]
    }"#;
    assert!(wvq_runtime::TestProgram::from_json(xpath).is_err());
}

#[test]
fn coverage_contribution_and_redundant_steps() {
    let mut rec = Recorder::new("sess-2", Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    let mut others = dash();
    others.modal = Some("others".into());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Others"),
        },
        others.clone(),
    )
    .unwrap();
    rec.step(
        TestAction::Press {
            target: None,
            key: "Escape".into(),
        },
        others.clone(),
    )
    .unwrap();
    rec.link_obligation(ObligationId::new("others-visible").unwrap());
    rec.link_obligation(ObligationId::new("overflow-grouped").unwrap());
    rec.link_api("GET /api/sankey");
    rec.link_coverage("src/sankey.ts");
    let trace = rec.finish().unwrap();
    let mut memory = GraphMemory::default();
    memory.known_obligations.insert("others-visible".into());
    memory
        .known_states
        .insert(dash().digest().unwrap().to_string());
    let contrib = coverage_contribution(&trace, &memory).unwrap();
    assert_eq!(
        contrib.existing_obligations,
        vec!["others-visible".to_owned()]
    );
    assert_eq!(contrib.new_obligations, vec!["overflow-grouped".to_owned()]);
    assert_eq!(contrib.new_behavior_states, 1);
    assert_eq!(
        contrib.new_api_operations,
        vec!["GET /api/sankey".to_owned()]
    );
    assert_eq!(contrib.new_code_coverage, vec!["src/sankey.ts".to_owned()]);
    assert_eq!(contrib.redundant_steps, 1);
}

#[test]
fn promote_drops_redundant_steps_and_keeps_seed() {
    let mut rec = Recorder::new("sess-3", Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    let mut others = dash();
    others.component = Some("others-panel".into());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Others"),
        },
        others.clone(),
    )
    .unwrap();
    rec.step(
        TestAction::Press {
            target: None,
            key: "Escape".into(),
        },
        others,
    )
    .unwrap();
    rec.link_obligation(ObligationId::new("others-visible").unwrap());
    let trace = rec.finish().unwrap();
    let program = promote(&trace, ProgramId::new("sankey-others-replay").unwrap()).unwrap();
    assert_eq!(program.source, ProgramSource::Recorded);
    assert_eq!(program.deterministic_seed, Some(7));
    assert_eq!(program.obligations[0].as_str(), "others-visible");
    assert!(matches!(program.steps[0], TestAction::Activate { .. }));
    assert!(matches!(
        program.steps.last(),
        Some(TestAction::Assert { .. })
    ));
    assert_eq!(
        program.steps.len(),
        2,
        "redundant Escape dropped, assert added"
    );
}

#[test]
fn replay_requires_matching_seed_and_fixture() {
    let mut rec = Recorder::new("sess-4", Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    let mut others = dash();
    others.component = Some("others-panel".into());
    rec.step(
        TestAction::Navigate {
            route: "/analytics/dashboard/42".into(),
        },
        others.clone(),
    )
    .unwrap();
    rec.link_obligation(ObligationId::new("others-visible").unwrap());
    let trace = rec.finish().unwrap();
    let err = replay_trace(
        &trace,
        Some("other-fixture"),
        Some(7),
        &mut ScriptedHost {
            states: vec![others.clone()],
            index: 0,
        },
    )
    .unwrap_err();
    assert!(matches!(err, ProgramError::Invalid(message) if message.contains("fixture")));
    let seed_err = replay_trace(
        &trace,
        Some("admin-above-limit"),
        Some(9),
        &mut ScriptedHost {
            states: vec![others.clone()],
            index: 0,
        },
    )
    .unwrap_err();
    assert!(matches!(seed_err, ProgramError::Invalid(message) if message.contains("seed")));
    let states = replay_trace(
        &trace,
        Some("admin-above-limit"),
        Some(7),
        &mut ScriptedHost {
            states: vec![others.clone()],
            index: 0,
        },
    )
    .unwrap();
    assert_eq!(states.len(), 1);
    let program = promote(&trace, ProgramId::new("replay-1").unwrap()).unwrap();
    let replayed = replay_program(
        &program,
        Some(7),
        &mut ScriptedHost {
            states: vec![others, dash()],
            index: 0,
        },
    )
    .unwrap();
    assert_eq!(replayed.len(), program.steps.len());
}

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

/// A recorder session that moves through two distinct states, so the
/// redundancy filter keeps both interactions.
fn two_step_recorder(session: &str, obligations: &[&str]) -> wvq_runtime::BehaviorTrace {
    let mut rec = Recorder::new(session, Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    let mut opened = dash();
    opened.component = Some("others-panel".into());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Others"),
        },
        opened.clone(),
    )
    .unwrap();
    let mut grouped = opened;
    grouped.modal = Some("grouped".into());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Group"),
        },
        grouped,
    )
    .unwrap();
    for obligation in obligations {
        rec.link_obligation(ObligationId::new(obligation).unwrap());
    }
    rec.finish().unwrap()
}

fn asserted_obligations(program: &wvq_runtime::TestProgram) -> Vec<String> {
    program
        .steps
        .iter()
        .filter_map(|step| match step {
            TestAction::Assert { obligation } => Some(obligation.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_declared_obligation_is_asserted_after_promotion() {
    let trace = two_step_recorder("sess-multi", &["others-visible", "overflow-grouped"]);
    let program = promote(&trace, ProgramId::new("multi-obligation").unwrap()).unwrap();
    assert_eq!(
        asserted_obligations(&program),
        vec!["others-visible".to_owned(), "overflow-grouped".to_owned()],
        "both declared obligations must have an exact assertion"
    );
    assert_eq!(program.steps.len(), 4, "two actions plus two assertions");
    program.validate().unwrap();
}

#[test]
fn an_existing_assertion_is_kept_and_only_the_missing_one_is_added() {
    let mut rec = Recorder::new("sess-partial", Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    let mut opened = dash();
    opened.component = Some("others-panel".into());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Others"),
        },
        opened.clone(),
    )
    .unwrap();
    // A recorded assertion does not change the state digest; it must survive the
    // redundancy filter instead of being dropped and re-appended.
    rec.step(
        TestAction::Assert {
            obligation: ObligationId::new("others-visible").unwrap(),
        },
        opened,
    )
    .unwrap();
    rec.link_obligation(ObligationId::new("others-visible").unwrap());
    rec.link_obligation(ObligationId::new("overflow-grouped").unwrap());
    let trace = rec.finish().unwrap();
    let program = promote(&trace, ProgramId::new("partial-assertions").unwrap()).unwrap();
    assert_eq!(
        asserted_obligations(&program),
        vec!["others-visible".to_owned(), "overflow-grouped".to_owned()],
        "the recorded assertion keeps its position; only the missing one is appended"
    );
    assert!(matches!(program.steps[1], TestAction::Assert { .. }));
    assert_eq!(program.steps.len(), 3, "no assertion is added twice");
    program.validate().unwrap();
}

#[test]
fn a_repeated_recorded_assertion_is_deduplicated() {
    let mut rec = Recorder::new("sess-dupe", Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    let mut opened = dash();
    opened.component = Some("others-panel".into());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Others"),
        },
        opened.clone(),
    )
    .unwrap();
    for _ in 0..2 {
        rec.step(
            TestAction::Assert {
                obligation: ObligationId::new("others-visible").unwrap(),
            },
            opened.clone(),
        )
        .unwrap();
    }
    rec.link_obligation(ObligationId::new("others-visible").unwrap());
    let trace = rec.finish().unwrap();
    let program = promote(&trace, ProgramId::new("deduped-assertions").unwrap()).unwrap();
    assert_eq!(
        asserted_obligations(&program),
        vec!["others-visible".to_owned()]
    );
    assert_eq!(program.steps.len(), 2);
}

#[test]
fn promotion_refuses_an_assertion_for_an_undeclared_obligation() {
    let mut rec = Recorder::new("sess-undeclared", Some("admin-above-limit".into()), Some(7));
    rec.start(dash());
    let mut opened = dash();
    opened.component = Some("others-panel".into());
    rec.step(
        TestAction::Activate {
            target: semantic_target("button", "Others"),
        },
        opened.clone(),
    )
    .unwrap();
    rec.step(
        TestAction::Assert {
            obligation: ObligationId::new("never-declared").unwrap(),
        },
        opened,
    )
    .unwrap();
    rec.link_obligation(ObligationId::new("others-visible").unwrap());
    let trace = rec.finish().unwrap();
    let err = promote(&trace, ProgramId::new("undeclared").unwrap()).unwrap_err();
    assert!(
        matches!(&err, ProgramError::Invalid(message) if message.contains("undeclared obligation")),
        "{err:?}"
    );
}

#[test]
fn promotion_is_deterministic_and_follows_declared_obligation_order() {
    let mut trace = two_step_recorder("sess-stable", &["a-first", "b-second"]);
    let first = promote(&trace, ProgramId::new("stable").unwrap()).unwrap();
    let second = promote(&trace, ProgramId::new("stable").unwrap()).unwrap();
    assert_eq!(first, second, "the same trace always promotes identically");
    assert_eq!(
        asserted_obligations(&first),
        vec!["a-first".to_owned(), "b-second".to_owned()]
    );
    // Appended assertions follow `trace.obligations` rather than being re-sorted,
    // so a caller that supplies its own order keeps it.
    trace.obligations.reverse();
    let reversed = promote(&trace, ProgramId::new("stable").unwrap()).unwrap();
    assert_eq!(
        asserted_obligations(&reversed),
        vec!["b-second".to_owned(), "a-first".to_owned()]
    );
}

#[test]
fn validate_rejects_a_declared_obligation_with_no_assertion() {
    let raw = r#"{
        "schema_v": 1,
        "id": "missing-assert",
        "source": "authored",
        "obligations": ["others-visible", "overflow-grouped"],
        "steps": [
            {"action":"navigate","route":"/analytics"},
            {"action":"assert","obligation":"others-visible"}
        ]
    }"#;
    let err = wvq_runtime::TestProgram::from_json(raw).unwrap_err();
    assert!(
        matches!(&err, ProgramError::Invalid(message)
            if message.contains("overflow-grouped") && message.contains("never asserted")),
        "{err:?}"
    );
}

#[test]
fn validate_rejects_an_assertion_for_an_undeclared_obligation() {
    let raw = r#"{
        "schema_v": 1,
        "id": "stray-assert",
        "source": "authored",
        "obligations": ["others-visible"],
        "steps": [
            {"action":"navigate","route":"/analytics"},
            {"action":"assert","obligation":"others-visible"},
            {"action":"assert","obligation":"not-declared"}
        ]
    }"#;
    let err = wvq_runtime::TestProgram::from_json(raw).unwrap_err();
    assert!(
        matches!(&err, ProgramError::Invalid(message)
            if message.contains("undeclared obligation") && message.contains("not-declared")),
        "{err:?}"
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

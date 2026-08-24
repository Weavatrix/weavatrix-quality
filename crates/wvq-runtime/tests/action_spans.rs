use wvq_runtime::{
    ActionSpan, BrowserProgramRun, NetworkRequestObservation, Observation, Target, TestAction,
    duplicate_mutation_requests,
};

fn request(sequence: u64, method: &str, url: &str) -> NetworkRequestObservation {
    NetworkRequestObservation {
        sequence,
        method: method.into(),
        url: url.into(),
        status: Some(200),
        resource_type: Some("fetch".into()),
    }
}

fn activate() -> TestAction {
    TestAction::Activate {
        target: Target {
            role: Some("button".into()),
            accessible_name: Some("Save".into()),
            ..Target::default()
        },
    }
}

fn run(observations: Vec<Observation>, action_spans: Vec<ActionSpan>) -> BrowserProgramRun {
    BrowserProgramRun {
        program: "save-widget".into(),
        passed: true,
        asserted: Vec::new(),
        contradicted: Vec::new(),
        assertions: Vec::new(),
        observations,
        action_spans,
        screenshot_paths: Vec::new(),
        trace_path: None,
        ui_snapshots: Vec::new(),
        network_profile: None,
        network_limitations: Vec::new(),
        failure: None,
    }
}

#[test]
fn two_mutations_inside_one_action_are_a_duplicate() {
    let observations = vec![
        Observation::default(),
        Observation {
            network_requests: vec![
                request(1, "POST", "https://example.invalid/api/widgets?tenant=7"),
                request(2, "POST", "https://example.invalid/api/widgets?tenant=7"),
            ],
            ..Observation::default()
        },
    ];
    let action = activate();
    let duplicates = duplicate_mutation_requests(&run(
        observations,
        vec![ActionSpan {
            step: 0,
            action: action.clone(),
            start_observation: 0,
            end_observation: 1,
        }],
    ));

    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].step, 0);
    assert_eq!(duplicates[0].action, action);
    assert_eq!(duplicates[0].method, "POST");
    assert_eq!(duplicates[0].url, "/api/widgets?tenant=7");
    assert_eq!(duplicates[0].sequences, [1, 2]);
}

#[test]
fn the_same_mutation_in_two_action_spans_is_two_user_intents() {
    let first = Observation {
        network_requests: vec![request(1, "POST", "https://example.invalid/api/widgets")],
        ..Observation::default()
    };
    let second = Observation {
        network_requests: vec![
            request(1, "POST", "https://example.invalid/api/widgets"),
            request(2, "POST", "https://example.invalid/api/widgets"),
        ],
        ..Observation::default()
    };
    let action = activate();
    let spans = vec![
        ActionSpan {
            step: 0,
            action: action.clone(),
            start_observation: 0,
            end_observation: 1,
        },
        ActionSpan {
            step: 1,
            action,
            start_observation: 1,
            end_observation: 2,
        },
    ];

    assert!(
        duplicate_mutation_requests(&run(vec![Observation::default(), first, second], spans))
            .is_empty()
    );
}

#[test]
fn reads_and_truncated_journals_never_become_duplicate_mutation_claims() {
    let action = activate();
    let spans = vec![ActionSpan {
        step: 0,
        action,
        start_observation: 0,
        end_observation: 1,
    }];
    let reads = Observation {
        network_requests: vec![
            request(1, "GET", "https://example.invalid/api/widgets"),
            request(2, "GET", "https://example.invalid/api/widgets"),
        ],
        ..Observation::default()
    };
    assert!(
        duplicate_mutation_requests(&run(vec![Observation::default(), reads], spans.clone()))
            .is_empty()
    );

    let truncated = Observation {
        network_requests: vec![
            request(1, "DELETE", "https://example.invalid/api/widgets/1"),
            request(2, "DELETE", "https://example.invalid/api/widgets/1"),
        ],
        network_requests_truncated: true,
        ..Observation::default()
    };
    assert!(
        duplicate_mutation_requests(&run(vec![Observation::default(), truncated], spans))
            .is_empty()
    );
}

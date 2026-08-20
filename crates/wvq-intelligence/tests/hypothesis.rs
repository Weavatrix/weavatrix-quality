//! Deterministic defect hypotheses: concrete probes, and no busywork.

use wvq_intelligence::{
    ChangeSignal, DefectHypothesis, HypothesisWeight, blocking_questions, hypothesise,
};

fn find<'a>(hypotheses: &'a [DefectHypothesis], id: &str) -> &'a DefectHypothesis {
    hypotheses
        .iter()
        .find(|item| item.id == id)
        .unwrap_or_else(|| {
            panic!(
                "no {id} in {:?}",
                hypotheses.iter().map(|i| i.id).collect::<Vec<_>>()
            )
        })
}

#[test]
fn a_change_with_no_recognised_shape_asks_nothing() {
    assert!(
        hypothesise(&[]).is_empty(),
        "an unremarkable change must not generate questions"
    );
}

#[test]
fn a_flipped_default_asks_about_the_absent_value() {
    let hypotheses = hypothesise(&[ChangeSignal::DefaultSensitivityFlipped {
        subject: "showCentreLabel".into(),
        before: "x !== false".into(),
        after: "x === true".into(),
    }]);
    let found = find(&hypotheses, "WVQ-HYP-001");
    assert_eq!(found.weight, HypothesisWeight::High);
    assert!(found.question.contains("absent"));
    assert!(
        found.probes.iter().any(|item| item.contains("undefined")),
        "the undefined case is the whole point: {:?}",
        found.probes
    );
    assert!(
        found
            .probes
            .iter()
            .any(|item| item.contains("declared default")),
        "and it must send the reviewer to the declared default"
    );
}

#[test]
fn a_membership_guard_enumerates_what_falls_outside_it() {
    let hypotheses = hypothesise(&[ChangeSignal::MembershipGuardAdded {
        subject: "calculation".into(),
        members: vec!["SUM".into(), "COUNT".into()],
        domain: vec![
            "SUM".into(),
            "AVG".into(),
            "MIN".into(),
            "MAX".into(),
            "MEDIAN".into(),
            "COUNT".into(),
        ],
    }]);
    let found = find(&hypotheses, "WVQ-HYP-002");
    let probes = found.probes.join(" | ");
    for outside in ["AVG", "MIN", "MAX", "MEDIAN"] {
        assert!(
            probes.contains(outside),
            "{outside} must be probed: {probes}"
        );
    }
    assert!(
        !probes.contains("= SUM"),
        "members inside the set are not the risk"
    );
    assert!(
        probes.contains("absent"),
        "`has(undefined)` is false, and that is easy to get wrong"
    );
    assert!(
        probes.contains("added to the domain later"),
        "a new enum member silently falls outside the set"
    );
}

#[test]
fn retiring_a_persisted_key_asks_about_records_that_still_have_it() {
    let hypotheses = hypothesise(&[ChangeSignal::PersistedKeyRetired {
        key: "centreLabelMode".into(),
        scope: "the widget normalisation list".into(),
    }]);
    let found = find(&hypotheses, "WVQ-HYP-003");
    assert_eq!(found.weight, HypothesisWeight::High);
    let probes = found.probes.join(" | ");
    assert!(probes.contains("existing record"));
    assert!(
        probes.contains("migration"),
        "the reviewer must decide migrate-or-accept: {probes}"
    );
}

#[test]
fn moving_a_derivation_asks_whether_the_two_sources_disagree() {
    let hypotheses = hypothesise(&[ChangeSignal::DerivationSourceMoved {
        derived: "centre label mode".into(),
        from_key: "centreLabelMode".into(),
        to_key: "calculation".into(),
    }]);
    let found = find(&hypotheses, "WVQ-HYP-004");
    assert!(found.question.contains("disagree"));
    assert!(
        found
            .probes
            .iter()
            .any(|item| item.contains("imply different")),
        "the disagreeing record is the defect case: {:?}",
        found.probes
    );
    assert!(
        found
            .probes
            .iter()
            .any(|item| item.contains("carrying neither")),
        "and the empty case must be asked too"
    );
}

#[test]
fn a_permission_change_asks_who_used_to_be_denied() {
    let hypotheses = hypothesise(&[ChangeSignal::PermissionPredicateChanged {
        subject: "canDelete".into(),
    }]);
    let found = find(&hypotheses, "WVQ-HYP-006");
    assert_eq!(found.weight, HypothesisWeight::High);
    assert!(found.question.contains("deny"));
    assert!(
        found.probes.iter().any(|item| item.contains("deny path")),
        "the deny branch is the one that loses coverage silently"
    );
}

#[test]
fn a_fold_asks_whether_the_aggregate_is_additive() {
    let hypotheses = hypothesise(&[ChangeSignal::AggregationIntroduced {
        subject: "rollupTail".into(),
    }]);
    let found = find(&hypotheses, "WVQ-HYP-007");
    let probes = found.probes.join(" | ");
    assert!(probes.contains("additive"));
    assert!(probes.contains("exactly at the fold threshold"));
}

#[test]
fn the_heaviest_question_is_read_first() {
    let hypotheses = hypothesise(&[
        ChangeSignal::BoundaryChanged {
            subject: "limit".into(),
        },
        ChangeSignal::PersistedKeyRetired {
            key: "mode".into(),
            scope: "config".into(),
        },
        ChangeSignal::TestMovedWithImplementation {
            test: "widget.test.js".into(),
        },
    ]);
    assert_eq!(hypotheses[0].weight, HypothesisWeight::High);
    assert_eq!(hypotheses[0].id, "WVQ-HYP-003");

    let blocking = blocking_questions(&hypotheses);
    assert_eq!(blocking.len(), 1, "only the high-weight question blocks");
    assert_eq!(blocking[0].id, "WVQ-HYP-003");
}

#[test]
fn every_hypothesis_says_which_signal_produced_it() {
    let hypotheses = hypothesise(&[
        ChangeSignal::DefaultSensitivityFlipped {
            subject: "flag".into(),
            before: "a".into(),
            after: "b".into(),
        },
        ChangeSignal::AggregationIntroduced {
            subject: "fold".into(),
        },
    ]);
    assert!(!hypotheses.is_empty());
    for item in &hypotheses {
        assert!(
            !item.because.is_empty(),
            "a question with no stated cause is noise"
        );
        assert!(
            !item.probes.is_empty(),
            "a question with no probe cannot be settled"
        );
    }
}

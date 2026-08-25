//! Continuous observation journal: `OBSERVED_ONLY`, fail closed, never a seal.

use wvq_runtime::{ContinuousJournal, ProgramError};

fn valid_journal() -> String {
    r#"{
        "schema_v": 1,
        "source": "continuous",
        "observed_only": true,
        "session_id": "staging-checkout",
        "data": { "name": "Ada" },
        "initial": { "route": "/checkout" },
        "events": [
            {
                "action": { "action": "fill", "target": { "test_id": "name" }, "value": "name" },
                "after": { "route": "/checkout" }
            },
            {
                "action": { "action": "activate", "target": { "test_id": "pay" } },
                "after": { "route": "/checkout/done" }
            }
        ]
    }"#
    .into()
}

#[test]
fn a_valid_journal_becomes_a_trace_without_obligations() {
    let journal = ContinuousJournal::from_json(&valid_journal()).unwrap();
    assert!(journal.observed_only);
    let trace = journal.to_trace().unwrap();
    assert_eq!(trace.session_id, "staging-checkout");
    assert_eq!(trace.events.len(), 2);
    assert!(trace.obligations.is_empty());
    assert!(trace.api_operations.is_empty());
}

#[test]
fn observed_only_false_is_refused_not_upgraded() {
    let raw = valid_journal().replace("\"observed_only\": true", "\"observed_only\": false");
    let err = ContinuousJournal::from_json(&raw).unwrap_err();
    assert!(matches!(err, ProgramError::Invalid(message) if message.contains("observed_only")));
}

#[test]
fn unknown_schema_fails_closed() {
    let raw = valid_journal().replace("\"schema_v\": 1", "\"schema_v\": 2");
    let err = ContinuousJournal::from_json(&raw).unwrap_err();
    assert!(matches!(err, ProgramError::UnknownSchema(2)));
}

#[test]
fn xpath_and_assert_and_upload_are_refused() {
    for (needle, replacement) in [
        (
            r#"{ "action": "activate", "target": { "test_id": "pay" } }"#,
            r#"{ "action": "activate", "target": { "xpath": "//button" } }"#,
        ),
        (
            r#"{ "action": "activate", "target": { "test_id": "pay" } }"#,
            r#"{ "action": "assert", "obligation": "paid" }"#,
        ),
        (
            r#"{ "action": "activate", "target": { "test_id": "pay" } }"#,
            r#"{ "action": "upload", "target": { "test_id": "file" }, "fixture": "invoice" }"#,
        ),
    ] {
        let raw = valid_journal().replace(needle, replacement);
        let err = ContinuousJournal::from_json(&raw).unwrap_err();
        assert!(
            matches!(err, ProgramError::Invalid(_) | ProgramError::Malformed(_)),
            "{err}"
        );
    }
}

#[test]
fn a_path_shaped_session_id_is_refused() {
    let raw = valid_journal().replace("staging-checkout", "../secret");
    let err = ContinuousJournal::from_json(&raw).unwrap_err();
    assert!(matches!(err, ProgramError::Invalid(message) if message.contains("path")));
}

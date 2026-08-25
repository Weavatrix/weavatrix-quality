//! Continuous journal ingest is `OBSERVED_ONLY` and never a seal.

use super::*;
use crate::IngestJournalCommand;

fn journal(observed_only: bool) -> String {
    format!(
        r#"{{
            "schema_v": 1,
            "source": "continuous",
            "observed_only": {observed_only},
            "session_id": "staging-checkout",
            "initial": {{ "route": "/checkout" }},
            "events": [
                {{
                    "action": {{ "action": "activate", "target": {{ "test_id": "pay" }} }},
                    "after": {{ "route": "/checkout/done" }}
                }}
            ]
        }}"#
    )
}

#[test]
fn ingest_journal_is_observed_only_and_cannot_seal() {
    let reply = FakeService::default()
        .ingest_journal(&IngestJournalCommand {
            change: "sankey-others".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            journal: journal(true),
        })
        .unwrap();
    assert!(reply.observed_only);
    assert!(!reply.seal_eligible);
    assert_eq!(reply.runtime_llm_tokens, 0);
    assert!(reply.trace_handle.is_some());
}

#[test]
fn ingest_journal_refuses_observed_only_false() {
    let err = FakeService::default()
        .ingest_journal(&IngestJournalCommand {
            change: "sankey-others".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            journal: journal(false),
        })
        .unwrap_err();
    assert!(err.to_string().contains("observed_only"), "{err}");
}

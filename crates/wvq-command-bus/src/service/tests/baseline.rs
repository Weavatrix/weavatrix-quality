//! Existing debt may become `OBSERVED_ONLY` baseline evidence. New debt cannot.

use super::*;
use crate::{BaselineCommand, DebtCommand};

#[test]
fn baseline_is_observed_only_and_does_not_swallow_new_debt() {
    let fake = FakeService::default();
    let before = fake
        .debt(&DebtCommand {
            change: "sankey-others".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
        })
        .unwrap();
    assert_eq!(before.existing, 1);
    assert_eq!(before.new, 1);
    assert_eq!(before.excepted, 0);

    let reply = fake
        .baseline(&BaselineCommand {
            change: "sankey-others".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            decision: "observed_only".into(),
        })
        .unwrap();
    assert!(reply.observed_only);
    assert!(!reply.seal_eligible);
    assert_eq!(reply.runtime_llm_tokens, 0);
    assert_eq!(reply.fingerprints, ["legacy-clone"]);
    assert_eq!(reply.recorded, 1);
    assert_eq!(reply.new_unbaselined, 1);

    let after = fake
        .debt(&DebtCommand {
            change: "sankey-others".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
        })
        .unwrap();
    assert_eq!(after.existing, 0);
    assert_eq!(after.excepted, 1);
    assert_eq!(after.new, 1, "new debt must remain new");
}

#[test]
fn baseline_refuses_a_seal_decision() {
    let err = FakeService::default()
        .baseline(&BaselineCommand {
            change: "sankey-others".into(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
            decision: "accept_as_intended".into(),
        })
        .unwrap_err();
    assert!(err.to_string().contains("decision"), "{err}");
    assert!(err.to_string().contains("accept_as_intended"), "{err}");
}

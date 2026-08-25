//! Brownfield recovery desk from an exact Git range.

use wvq_spec_recovery::{
    NarrativeInput, RecoveryDesk, RecoveryInput, TestIntentSummary, VerifyContext, cluster, narrate,
};

use super::super::access::*;
use super::super::protection_snapshot::ensure_complete_diff;
use super::super::recovery::{
    recovery_candidates, recovery_code_delta, recovery_commits, recovery_evidence,
    recovery_existing_requirements,
};
use super::LiveService;

impl LiveService {
    /// Build a live brownfield recovery desk from an exact Git range and
    /// revision-bound Weavatrix evidence. Recovered candidates remain proposals;
    /// this method never seals them.
    ///
    /// # Errors
    ///
    /// Fails closed when refs, graph evidence, or Git provenance are unavailable.
    pub fn recovery_desk(
        &self,
        change: &str,
        base: &str,
        head: &str,
    ) -> Result<RecoveryDesk, BusError> {
        let range = self.revision_range(base, head)?;
        let revision = self.revision()?;
        let diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        ensure_complete_diff(&diff)?;
        let files = changed_files(&self.repo, &range)?;
        let (code_delta, surfaces) = recovery_code_delta(&diff);
        if files.is_empty() && code_delta.changed_symbols.is_empty() {
            return Err(BusError::Intelligence(format!(
                "revision range `{base}` -> `{head}` contains no recoverable change"
            )));
        }

        let head_revision = if head == "WORKTREE" {
            format!("WORKTREE@{}", revision.as_str())
        } else {
            range.head_commit.clone()
        };
        let existing_requirements = recovery_existing_requirements(&self.repo, change)?;
        let evidence = recovery_evidence(
            &self.repo,
            &range,
            &code_delta,
            &files,
            &existing_requirements,
        )?;
        let commits = recovery_commits(
            &self.repo,
            &range,
            &head_revision,
            &code_delta.components,
            !files.is_empty(),
        )?;
        let clusters = cluster(&commits);
        let narrative = narrate(NarrativeInput {
            change_cluster: change.to_owned(),
            base_revision: range.merge_base.clone(),
            head_revision,
            evidence: evidence.clone(),
            code_delta: code_delta.clone(),
            tests_delta: files.tests_delta(),
            behavior_delta: Vec::new(),
        });
        let recover_changed_symbols =
            !files.changed_tests().is_empty() && !files.changes_openspec_change(change);
        let candidates =
            recovery_candidates(&surfaces, &code_delta, &evidence, recover_changed_symbols);
        let test_intent = files
            .changed_tests()
            .into_iter()
            .map(|test| TestIntentSummary {
                appears_to_expect: format!(
                    "the assertions in `{test}` remain valid on both revisions"
                ),
                test,
                changed_with_implementation: true,
            })
            .collect();
        let mut desk = RecoveryDesk::new(change);
        desk.recover(RecoveryInput {
            narrative,
            clusters,
            surface_delta: surfaces.clone(),
            test_intent,
            candidates,
            context: VerifyContext {
                existing_requirements,
                removed_endpoints: surfaces.removed,
                observed: Vec::new(),
            },
        });
        Ok(desk)
    }
}

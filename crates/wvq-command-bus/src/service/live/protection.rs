//! Live protection continuity view.

use super::super::access::*;
use super::super::protection_snapshot::ensure_complete_diff;
use super::super::protection_view::{build_protection_view, expectation_change};
use super::LiveService;

impl LiveService {
    /// Replay measured coverage on base and head and build the live protection
    /// continuity view used by MCP and Studio.
    ///
    /// # Errors
    ///
    /// Missing revision-bound coverage, a failed runner, or incomplete graph
    /// evidence is refused rather than converted into an unprotected result.
    pub fn protection_view(
        &self,
        change: &str,
        base: &str,
        head: &str,
    ) -> Result<ProtectionView, BusError> {
        let compiled = self.compiled(change)?;
        let head_oracle = oracle_identity(&self.repo, &compiled)?;
        let range = self.revision_range(base, head)?;
        let files = changed_files(&self.repo, &range)?;
        let all_files = files.all();
        if all_files.is_empty() {
            return Err(BusError::Intelligence(format!(
                "revision range `{base}` -> `{head}` has no files to measure"
            )));
        }
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
        let head_graph = protection_graph_for_files(&self.repo, &revision, &all_files)?;
        let head_run = self.run(&RunCommand {
            change: compiled.change.clone(),
            base: range.base_ref.clone(),
            head: range.head_ref.clone(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })?;
        if head_run.outcome != "passed" {
            return Err(BusError::Runtime(format!(
                "head protection run {} did not pass ({})",
                head_run.run_id, head_run.outcome
            )));
        }
        let head_snapshot = self.stored_protection_snapshot(&head_run.run_id)?;
        let (base_snapshot, base_graph, base_compiled, base_oracle) =
            self.measure_base_protection(&range, &all_files, &compiled.change)?;
        // Replaying the base suite is the one expensive part of protection, so
        // the measurement is persisted against the head run. `quality_verify`
        // then composes a real protection axis from stored evidence without
        // executing anything itself.
        self.persist_base_protection(&head_run.run_id, &base_snapshot)?;
        let oracle_replacement =
            if base_oracle.id == head_oracle.id && base_oracle.digest == head_oracle.digest {
                None
            } else {
                let (changed_obligations, obligation_replacements) =
                    expectation_change(&base_compiled.obligations, &compiled.obligations, true);
                let document = OracleReplacementDocument {
                    schema_v: 1,
                    change: compiled.change.clone(),
                    base_revision: range.merge_base.clone(),
                    head_revision: range.head_commit.clone(),
                    head_content_revision: revision.to_string(),
                    merge_base: range.merge_base.clone(),
                    base_seal: base_oracle.id,
                    base_seal_digest: base_oracle.digest,
                    head_seal: head_oracle.id,
                    head_seal_digest: head_oracle.digest,
                    changed_obligations,
                    obligation_replacements,
                };
                Some(self.persist_oracle_replacement(&head_run.run_id, &document)?)
            };
        Ok(build_protection_view(
            &compiled.obligations,
            &diff,
            (&base_snapshot, &head_snapshot),
            (&base_graph, &head_graph),
            &files,
            oracle_replacement,
        ))
    }
}

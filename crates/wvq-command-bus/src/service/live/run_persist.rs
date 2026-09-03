//! Persist-core half of run_controlled.

use super::super::access::*;
use super::super::persist_browser::persist_browser_runs;
use super::super::persist_run::{obligation_execution_map, put_json_run_artifact, put_run_artifact};
use super::super::persist_surface::persist_application_surface_graph;

use super::super::persist_ui::persist_ui_integrity;
use super::super::runner_coverage::stdout_kind;
use super::super::selection_audit::live_selection_report;
use super::LiveService;
use super::run_types::{ExecutedControlledRun, PersistedControlledRun, PreparedControlledRun};

impl LiveService {
    pub(in crate::service) fn persist_controlled_run(
        &self,
        cmd: &RunCommand,
        prepared: &PreparedControlledRun,
        executed: ExecutedControlledRun<'_>,
    ) -> Result<RunReply, BusError> {
        let persisted = self.persist_run_core_artifacts(cmd, prepared, &executed)?;
        self.finish_controlled_run(cmd, prepared, executed, persisted)
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn persist_run_core_artifacts(
        &self,
        cmd: &RunCommand,
        prepared: &PreparedControlledRun,
        executed: &ExecutedControlledRun<'_>,
    ) -> Result<PersistedControlledRun, BusError> {
        let store = &prepared.store;
        let range = &prepared.range;
        let before = &prepared.before;
        let live_selection = &prepared.live_selection;
        let historical_selection = &prepared.historical_selection;
        let protection_graph = &prepared.protection_graph;
        let graph_diff = &prepared.graph_diff;
        let impact = &prepared.impact;
        let static_selection = &prepared.static_selection;
        let records = &executed.records;
        let mutation_document = &executed.mutation_document;
        let ui_policy = &executed.ui_policy;
        let browser_runs = &executed.browser_runs;
        let run_id = &executed.run_id;

        let mut handles = Vec::new();
        put_json_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-revision-range", run_id.as_str()),
            "revision-range",
            &json!({
                "schema_v": 2,
                "base": {"ref": range.base_ref, "commit": range.base_commit},
                "head": {
                    "ref": range.head_ref,
                    "commit": range.head_commit,
                    "content_revision": before.as_str()
                },
                "merge_base": range.merge_base
            }),
            &mut handles,
        )?;
        put_json_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-selection-decision", run_id.as_str()),
            "selection-decision",
            &live_selection_report(live_selection, historical_selection.len()),
            &mut handles,
        )?;
        put_json_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-protection-graph", run_id.as_str()),
            "weavatrix-protection-graph",
            protection_graph,
            &mut handles,
        )?;
        persist_application_surface_graph(
            store,
            run_id,
            before,
            protection_graph,
            records,
            &mut handles,
        )?;
        put_json_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-graph-diff", run_id.as_str()),
            "weavatrix-graph-diff",
            graph_diff,
            &mut handles,
        )?;
        let obligation_execution = obligation_execution_map(
            &self.repo,
            &live_selection.bindings,
            records,
            run_id,
            browser_runs,
            &cmd.evidence_policy,
        )?;
        put_json_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-obligation-execution", run_id.as_str()),
            "obligation-execution-map",
            &obligation_execution,
            &mut handles,
        )?;
        if let Some(mutation) = mutation_document {
            put_json_run_artifact(
                store,
                run_id,
                &format!("artifact-{}-mutation-results", run_id.as_str()),
                MUTATION_RESULTS_KIND,
                mutation,
                &mut handles,
            )?;
            for result in &mutation.results {
                let stored_result_id = format!("{}--{}", run_id.as_str(), result.id);
                store
                    .put_mutation_result(
                        &stored_result_id,
                        &result.operator,
                        &format!("{}:{}:{}", result.path, result.line, result.column),
                        &result.status,
                    )
                    .map_err(|err| BusError::Store(err.to_string()))?;
            }
        }
        put_json_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-impact", run_id.as_str()),
            "impacted-surface",
            impact,
            &mut handles,
        )?;
        put_json_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-selection", run_id.as_str()),
            "weavatrix-test-selection",
            static_selection,
            &mut handles,
        )?;
        for (index, record) in records.iter().enumerate() {
            store
                .put_run_item(&StoredRunItem {
                    id: format!("{}-item-{index}", run_id.as_str()),
                    run_id: run_id.clone(),
                    executor: record.executor.clone(),
                    status_code: record.status_code,
                    passed: record.passed,
                })
                .map_err(|err| BusError::Store(err.to_string()))?;
            let keep_raw_streams = cmd.evidence_policy == "standard"
                || (cmd.evidence_policy == "minimal" && !record.passed);
            if keep_raw_streams && !record.stdout.is_empty() {
                put_run_artifact(
                    store,
                    run_id,
                    &format!("artifact-{}-{index}-stdout", run_id.as_str()),
                    stdout_kind(&record.executor),
                    &record.stdout,
                    &mut handles,
                )?;
            }
            if keep_raw_streams && !record.stderr.is_empty() {
                put_run_artifact(
                    store,
                    run_id,
                    &format!("artifact-{}-{index}-stderr", run_id.as_str()),
                    "stderr",
                    &record.stderr,
                    &mut handles,
                )?;
            }
            for (artifact_index, artifact) in record.artifacts.iter().enumerate() {
                let keep = cmd.evidence_policy == "standard"
                    || (cmd.evidence_policy == "minimal"
                        && matches!(artifact.kind.as_str(), "normalized-test-run" | "coverage"));
                if !keep {
                    continue;
                }
                put_run_artifact(
                    store,
                    run_id,
                    &format!(
                        "artifact-{}-{index}-produced-{artifact_index}",
                        run_id.as_str()
                    ),
                    &artifact.kind,
                    &artifact.bytes,
                    &mut handles,
                )?;
            }
        }
        persist_browser_runs(
            store,
            run_id,
            browser_runs,
            &cmd.evidence_policy,
            &mut handles,
        )?;
        let head_ui = persist_ui_integrity(
            store,
            run_id,
            before,
            ui_policy,
            browser_runs,
            &mut handles,
        )?;
        Ok(PersistedControlledRun { handles, head_ui })
    }
}

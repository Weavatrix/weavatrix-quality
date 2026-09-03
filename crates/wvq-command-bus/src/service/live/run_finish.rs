//! Finish half of run_controlled: UI delta, protection, summary, reply.

use super::super::access::*;
use super::super::analytics::persist_test_analytics;
use super::super::persist_behavior::persist_browser_behavior;
use super::super::persist_matrix::{
    load_continuous_journals, persist_surface_evidence_from, SurfaceEvidenceSources,
};
use super::super::persist_plan::persist_cheapest_evidence_from;
use super::super::persist_run::{put_json_run_artifact, put_run_artifact};
use super::super::persist_surface::persist_behavior_surface_graph;
use super::super::persist_ui_analyse::analyse_ui_snapshots;
use super::super::protection_snapshot::{
    live_protection_snapshot, persist_dynamic_coverage_history,
};
use super::super::runner::execution_summary;
use super::run_types::{ExecutedControlledRun, PersistedControlledRun, PreparedControlledRun};
use super::LiveService;

impl LiveService {
    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn finish_controlled_run(
        &self,
        cmd: &RunCommand,
        prepared: &PreparedControlledRun,
        executed: ExecutedControlledRun<'_>,
        persisted: PersistedControlledRun,
    ) -> Result<RunReply, BusError> {
        let store = &prepared.store;
        let compiled = &prepared.compiled;
        let range = &prepared.range;
        let changed = &prepared.changed;
        let before = &prepared.before;
        let protection_graph = &prepared.protection_graph;
        let graph_diff = &prepared.graph_diff;
        let live_selection = &prepared.live_selection;
        let available_test_count = prepared.available_test_count;
        let effective_scope = &prepared.effective_scope;
        let scope_reason = &prepared.scope_reason;
        let executed_tests = &prepared.executed_tests;
        let records = &executed.records;
        let ui_policy = &executed.ui_policy;
        let browser_runs = &executed.browser_runs;
        let outcome = executed.outcome;
        let run_id = &executed.run_id;
        let mut handles = persisted.handles;
        let head_ui = persisted.head_ui;
        let base_browser_replay = executed.base_browser_replay;

        if let (Some(head_ui), Some(base_replay)) = (head_ui.as_ref(), base_browser_replay.as_ref())
        {
            let base_ui = match base_replay {
                Ok(base) if base.runs.len() == browser_runs.len() => {
                    let borrowed = browser_runs
                        .iter()
                        .zip(&base.runs)
                        .map(|((configured, _), result)| (*configured, result.clone()))
                        .collect::<Vec<_>>();
                    analyse_ui_snapshots(&base.revision, ui_policy, &borrowed)?.snapshot
                }
                Ok(base) => UiIntegritySnapshot {
                    revision: base.revision.to_string(),
                    truncated: true,
                    ..UiIntegritySnapshot::default()
                },
                Err(_) => UiIntegritySnapshot {
                    revision: range.merge_base.clone(),
                    truncated: true,
                    ..UiIntegritySnapshot::default()
                },
            };
            let previously_fixed = store
                .previously_fixed_debt()
                .map_err(|err| BusError::Store(err.to_string()))?
                .into_iter()
                .filter(|item| item.starts_with("ui:"))
                .collect::<BTreeSet<_>>();
            let mut delta = ratchet_ui(&base_ui, head_ui, &previously_fixed, ui_policy);
            if ui_policy.responsive.enabled && base_replay.is_ok() {
                let (intervals, truncated) = self.measure_responsive_ui(
                    range,
                    compiled,
                    ui_policy,
                    &base_ui,
                    head_ui,
                    &previously_fixed,
                )?;
                delta.responsive_intervals = intervals;
                delta.responsive_truncated = truncated;
            }
            let fixed = delta.fixed_fingerprints();
            if !fixed.is_empty() {
                store
                    .remember_fixed_debt(&fixed, before)
                    .map_err(|err| BusError::Store(err.to_string()))?;
            }
            Self::persist_ui_delta_with_handles(store, run_id, &base_ui, &delta, &mut handles)?;
        }
        persist_dynamic_coverage_history(store, run_id, before, protection_graph, records)?;
        let mut code_flows = Vec::new();
        let protection = live_protection_snapshot(
            &self.repo,
            before,
            protection_graph,
            records,
            &live_selection.bindings,
        )?;
        if let Some(snapshot) = &protection {
            code_flows.extend(snapshot.flows.iter().cloned());
            put_json_run_artifact(
                store,
                run_id,
                &format!("artifact-{}-protection", run_id.as_str()),
                "protection-snapshot",
                snapshot,
                &mut handles,
            )?;
        }
        let journals = load_continuous_journals(store)?;
        let evidence_sources = SurfaceEvidenceSources {
            graph: protection_graph,
            records,
            bindings: &live_selection.bindings,
            mutation: executed.mutation_document.as_ref(),
            browser_runs,
            ui: head_ui.as_ref(),
            protection: protection.as_ref(),
            journals: &journals,
        };
        persist_surface_evidence_from(store, run_id, before, &evidence_sources, &mut handles)?;
        persist_cheapest_evidence_from(store, run_id, before, &evidence_sources, &mut handles)?;
        persist_behavior_surface_graph(
            store,
            run_id,
            before,
            protection_graph,
            &journals,
            &mut handles,
        )?;
        let bound_files = live_selection
            .bindings
            .iter()
            .map(|binding| binding.path.clone())
            .collect::<Vec<_>>();
        let bound_graph = protection_graph_for_files(&self.repo, before, &bound_files)?;
        code_flows.extend(declared_code_flows(
            before.as_str(),
            &live_selection.bindings,
            &bound_graph,
        ));
        if let Some(base_replay) = base_browser_replay {
            persist_delta_triangle(
                store,
                run_id,
                compiled,
                changed,
                graph_diff,
                &code_flows,
                browser_runs,
                base_replay,
                &cmd.evidence_policy,
                &mut handles,
            )?;
        }
        let behavior = persist_browser_behavior(store, run_id, before, browser_runs, &mut handles)?;
        let test_analytics = persist_test_analytics(store, run_id, before, records, browser_runs)?;
        put_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-test-analytics", run_id.as_str()),
            "test-analytics",
            &test_analytics.bytes,
            &mut handles,
        )?;
        let summary = execution_summary(
            run_id,
            &compiled.change,
            before,
            range,
            &cmd.scope,
            effective_scope,
            scope_reason,
            &cmd.evidence_policy,
            outcome,
            records,
            browser_runs,
        )?;
        put_run_artifact(
            store,
            run_id,
            &format!("artifact-{}-summary", run_id.as_str()),
            "execution-summary",
            &summary,
            &mut handles,
        )?;
        handles.sort();

        let selected_test_count = executed_tests
            .as_ref()
            .map_or(available_test_count, BTreeSet::len);

        let state = RunState {
            id: run_id.to_string(),
            status: "complete".into(),
            outcome: outcome.into(),
            handles: handles.clone(),
        };
        *self.lock() = Some(state);
        Ok(RunReply {
            run_id: run_id.to_string(),
            change: compiled.change.clone(),
            base: range.base_ref.clone(),
            head: range.head_ref.clone(),
            base_commit: range.base_commit.clone(),
            head_commit: range.head_commit.clone(),
            merge_base: range.merge_base.clone(),
            requested_scope: cmd.scope.clone(),
            scope: effective_scope.clone(),
            scope_reason: scope_reason.clone(),
            status: "complete".into(),
            executed: true,
            outcome: outcome.into(),
            selected_test_count: u64::try_from(selected_test_count).unwrap_or(u64::MAX),
            available_test_count: u64::try_from(available_test_count).unwrap_or(u64::MAX),
            executor_invocations: u64::try_from(records.len()).unwrap_or(u64::MAX),
            browser_programs: u64::try_from(browser_runs.len()).unwrap_or(u64::MAX),
            behavior_state_count: behavior.states,
            new_behavior_state_count: behavior.new_states,
            behavior_edge_count: behavior.edges,
            new_behavior_edge_count: behavior.new_edges,
            recorded_test_count: test_analytics.recorded_test_count,
            failed_test_count: test_analytics.failed_test_count,
            flaky_test_count: test_analytics.flaky_test_count,
            unknown_failure_count: test_analytics.unknown_failure_count,
            artifact_handles: handles,
        })
    }
}

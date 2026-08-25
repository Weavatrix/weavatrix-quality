//! Prepare half of run_controlled.

use super::super::access::*;
use super::super::execute::{available_test_paths, build_execution_requests};
use super::super::impact::live_impacted_surface;
use super::super::runner::clear_generated_runner_artifacts;
use super::super::selection_build::{build_live_selection, historical_selection_candidates};
use super::LiveService;
use super::run_types::PreparedControlledRun;

impl LiveService {
    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn prepare_controlled_run(
        &self,
        cmd: &RunCommand,
    ) -> Result<PreparedControlledRun, BusError> {
        validate_scope(&cmd.scope)?;
        validate_evidence_policy(&cmd.evidence_policy)?;
        let compiled = self.compiled(&cmd.change)?;
        let mutation_policy =
            MutationPolicy::from_contract(&load_quality_contract(&self.repo, &compiled.change)?)
                .map_err(BusError::Runtime)?;
        if let Some(err) = &self.executor_init_error {
            return Err(BusError::Runtime(format!(
                "registered executor initialization failed: {err}"
            )));
        }
        let targets = discover_executor_targets(&self.repo)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let configured_browser = load_browser_policy(&self.repo, &compiled.obligations)?;
        if targets.is_empty()
            && configured_browser
                .as_ref()
                .is_none_or(|policy| policy.programs.is_empty())
        {
            let store = self.store()?;
            let promoted = store
                .latest_program_revisions_for_change(&compiled.change)
                .map_err(|err| BusError::Store(err.to_string()))?;
            if configured_browser.is_none() || promoted.is_empty() {
                return Err(BusError::Runtime(
                    "no supported registered executor or browser TestProgram was discovered".into(),
                ));
            }
        }
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let changed = changed_files(&self.repo, &range)?;
        let store = self.store()?;
        let browser = load_live_browser_policy(&self.repo, &compiled, &store)?;
        if targets.is_empty()
            && browser
                .as_ref()
                .is_none_or(|policy| policy.programs.is_empty())
        {
            return Err(BusError::Runtime(
                "no supported registered executor or browser TestProgram was discovered".into(),
            ));
        }
        for target in &targets {
            clear_generated_runner_artifacts(&target.cwd)?;
        }
        let before = self.revision()?;
        let protection_graph = protection_graph_for_files(&self.repo, &before, &changed.all())?;
        let graph_diff = self.weavatrix_operation(
            &before,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        let change_impact = self.weavatrix_operation(
            &before,
            "change_impact",
            &json!({
                "base_ref": range.merge_base,
                "depth": 6,
                "max_nodes": 100_000,
                "precision": "graph"
            }),
        )?;
        let static_selection = self.weavatrix_operation(
            &before,
            "select_tests",
            &json!({
                "base_ref": range.merge_base,
                "depth": 6,
                "max_nodes": 2000,
                "max_tests": 500,
                "precision": "graph"
            }),
        )?;
        let obligation_needs: Vec<_> = compiled
            .obligations
            .iter()
            .map(|item| ObligationNeed {
                id: item.id.to_string(),
                high_risk: matches!(item.risk, RiskLevel::High | RiskLevel::Critical),
            })
            .collect();
        let browser_bindings = browser
            .as_ref()
            .map_or_else(Vec::new, browser_test_bindings);
        let impact = live_impacted_surface(&graph_diff, &change_impact)?;
        let historical_selection = historical_selection_candidates(&store, &impact)?;
        let live_selection = build_live_selection(
            &self.repo,
            &static_selection,
            &graph_diff,
            &impact,
            &obligation_needs,
            &browser_bindings,
            &historical_selection,
        )?;
        let browser_paths = browser_bindings
            .iter()
            .map(|binding| binding.path.clone())
            .collect::<BTreeSet<_>>();
        let available_test_count =
            available_test_paths(&self.repo, &targets, &browser_paths)?.len();
        let (execution_requests, effective_scope, scope_reason, executed_tests) =
            build_execution_requests(
                &self.repo,
                &targets,
                &live_selection,
                &browser_paths,
                &cmd.scope,
            );
        Ok(PreparedControlledRun {
            compiled,
            mutation_policy,
            range,
            changed,
            store,
            browser,
            before,
            protection_graph,
            graph_diff,
            static_selection,
            impact,
            historical_selection,
            live_selection,
            available_test_count,
            execution_requests,
            effective_scope,
            scope_reason,
            executed_tests,
        })
    }
}

//! Execute half of run_controlled.

use super::super::access::*;
use super::super::persist_evidence::cap_browser_evidence;
use super::super::persist_run::make_run_id;
use super::super::runner::{attach_normalized_artifacts, clear_generated_runner_artifacts};
use super::LiveService;
use super::run_types::{ExecutedControlledRun, PreparedControlledRun};

impl LiveService {
    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn execute_controlled_run<'a>(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
        prepared: &'a PreparedControlledRun,
    ) -> Result<ExecutedControlledRun<'a>, BusError> {
        let compiled = &prepared.compiled;
        let mutation_policy = &prepared.mutation_policy;
        let range = &prepared.range;
        let changed = &prepared.changed;
        let store = &prepared.store;
        let browser = &prepared.browser;
        let before = prepared.before.clone();
        let live_selection = &prepared.live_selection;
        let execution_requests = &prepared.execution_requests;
        let executed_tests = &prepared.executed_tests;
        let mut records = Vec::new();
        for request in execution_requests {
            let target = &request.target;
            std::fs::create_dir_all(target.cwd.join(".weavatrix-quality")).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot prepare runner evidence directory in {}: {err}",
                    target.cwd.display()
                ))
            })?;
            clear_generated_runner_artifacts(&target.cwd)?;
            let prepared = self
                .executors
                .prepare(PrepareRequest {
                    executor: target.executor.clone(),
                    cwd: target.cwd.clone(),
                    filters: request.filters.clone(),
                    exact_case: None,
                    extra: BTreeMap::new(),
                    limits: default_limits(),
                    cancel: Arc::clone(&cancel),
                })
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            let started = SystemTime::now();
            let mut record = match self.executors.execute(&prepared) {
                Ok(ExecutionResult {
                    status_code,
                    stdout,
                    stderr,
                }) => ExecutorRecord {
                    executor: target.executor.as_str().to_owned(),
                    cwd: relative_or_display(&self.repo, &target.cwd),
                    selection: request.selected_tests.clone(),
                    status_code,
                    passed: status_code == Some(0),
                    error: None,
                    stdout,
                    stderr,
                    artifacts: Vec::new(),
                },
                Err(err) => ExecutorRecord {
                    executor: target.executor.as_str().to_owned(),
                    cwd: relative_or_display(&self.repo, &target.cwd),
                    selection: request.selected_tests.clone(),
                    status_code: None,
                    passed: false,
                    error: Some(err.to_string()),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    artifacts: Vec::new(),
                },
            };
            attach_normalized_artifacts(&self.repo, &target.cwd, started, &mut record);
            clear_generated_runner_artifacts(&target.cwd)?;
            records.push(record);
        }

        let mut mutation_bindings = Vec::new();
        for binding in &live_selection.bindings {
            let Some(runner) = binding.runner.clone() else {
                continue;
            };
            let Some(case) = binding.case.clone() else {
                continue;
            };
            let known_flaky = binding.flake_penalty > 0
                || store
                    .test_case_stats(
                        &runner,
                        binding.suite.as_deref().unwrap_or(&binding.path),
                        &case,
                    )
                    .map_err(|err| BusError::Store(err.to_string()))?
                    .flaky;
            mutation_bindings.push(MutationBinding {
                path: binding.path.clone(),
                runner,
                case,
                obligations: binding.obligations.clone(),
                known_flaky,
            });
        }
        let mutation_document = mutation_policy
            .as_ref()
            .map(|policy| {
                if records.is_empty() || records.iter().any(|record| !record.passed) {
                    Ok(MutationRunDocument::unmeasured(
                        policy,
                        "the selected baseline suite did not pass before mutation".into(),
                    ))
                } else {
                    execute_source_mutation(&MutationRunRequest {
                        repo: &self.repo,
                        head_commit: &range.head_commit,
                        merge_base: &range.merge_base,
                        head_is_worktree: range.head_ref == "WORKTREE",
                        added_files: &changed.added,
                        changed_files: &changed.changed,
                        bindings: &mutation_bindings,
                        policy,
                        executors: &self.executors,
                        cancel: Arc::clone(&cancel),
                    })
                    .map_err(BusError::Runtime)
                }
            })
            .transpose()?;

        let ui_policy = load_ui_integrity_policy(&self.repo)?;
        let mut browser_runs = Vec::new();
        if let Some(policy) = browser {
            for configured in policy.programs.iter().filter(|program| {
                executed_tests
                    .as_ref()
                    .is_none_or(|selected| selected.contains(&program.path))
            }) {
                let mut executable = configured.program.clone();
                cap_browser_evidence(&mut executable, &cmd.evidence_policy);
                let evidence_dir = self
                    .repo
                    .join(".weavatrix-quality")
                    .join("browser-evidence")
                    .join(safe_file_token(configured.program.id.as_str()));
                let result = run_browser_program_at(
                    &BrowserRunConfig {
                        base_url: policy.base_url.clone(),
                        browser: policy.browser.clone(),
                        headless: policy.headless,
                        timeout: policy.timeout,
                        module_root: policy.module_root.clone(),
                        runtime_dir: self
                            .repo
                            .join(".weavatrix-quality/runtime/playwright-runner"),
                        evidence_dir,
                        viewport: None,
                        ui_integrity: ui_collection_config(&ui_policy, &configured.oracles),
                        network: policy.network.clone(),
                        cancel: Arc::clone(&cancel),
                    },
                    &executable,
                    &configured.oracles,
                    before.as_str(),
                )
                .map_err(|err| BusError::Runtime(err.to_string()))?;
                browser_runs.push((configured, result));
            }
        }

        // Differential replay is part of the normal browser run, not an
        // opt-in reporting view. A failure to obtain the base side is retained
        // as unmeasured evidence after the head run itself is stored.
        let base_browser_replay = browser.as_ref().and_then(|policy| {
            (!browser_runs.is_empty()).then(|| {
                self.replay_base_browser_programs(
                    range,
                    &compiled.change,
                    policy,
                    &browser_runs,
                    &ui_policy,
                    &cmd.evidence_policy,
                )
            })
        });

        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during execution: `{before}` -> `{after}`"
            )));
        }
        let outcome = if records.iter().any(|record| record.error.is_some()) {
            "error"
        } else if records.iter().all(|record| record.passed)
            && browser_runs.iter().all(|(_, run)| run.passed)
        {
            "passed"
        } else {
            "failed"
        };
        let run_id = make_run_id(&compiled.change, &before)?;
        store
            .put_run(&StoredRun {
                id: run_id.clone(),
                change_id: compiled.change.clone(),
                revision: before.clone(),
                status: "complete".into(),
                passed: outcome == "passed",
                outcome: outcome.into(),
            })
            .map_err(|err| BusError::Store(err.to_string()))?;

        Ok(ExecutedControlledRun {
            records,
            mutation_document,
            ui_policy,
            browser_runs,
            base_browser_replay,
            outcome,
            run_id,
        })
    }
}

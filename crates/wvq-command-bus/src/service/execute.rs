use super::access::*;
use super::runner::{attach_normalized_artifacts, clear_generated_runner_artifacts};

pub(in crate::service) fn build_execution_requests(
    repo: &Path,
    targets: &[ExecutorTarget],
    selection: &LiveSelection,
    browser_paths: &BTreeSet<String>,
    requested_scope: &str,
) -> (
    Vec<ExecutionRequest>,
    String,
    String,
    Option<BTreeSet<String>>,
) {
    pub(in crate::service) const MAX_FILTERED_PROCESSES: usize = 16;
    if requested_scope != "impacted" {
        return full_execution_requests(targets, "full scope requested by caller");
    }
    if !selection.complete() {
        return full_execution_requests(
            targets,
            &format!(
                "impacted selection widened: uncovered obligations: {}",
                selection.uncovered_all.join(", ")
            ),
        );
    }
    if selection.selected.is_empty() {
        return full_execution_requests(
            targets,
            "impacted selection widened: no executable tests were selected",
        );
    }
    let mut grouped = FilterGroups::new();
    let mut executed = BTreeSet::new();
    for selected in &selection.selected {
        if browser_paths.contains(selected) {
            executed.insert(selected.clone());
            continue;
        }
        let absolute = repo.join(selected);
        if !absolute.is_file() {
            return full_execution_requests(
                targets,
                &format!("impacted selection widened: selected test `{selected}` is missing"),
            );
        }
        let mut matching = targets
            .iter()
            .filter(|target| absolute.starts_with(&target.cwd))
            .collect::<Vec<_>>();
        matching.sort_by_key(|target| std::cmp::Reverse(target.cwd.components().count()));
        let Some(target) = matching
            .into_iter()
            .find(|target| target_accepts_filter(target, selected))
        else {
            return full_execution_requests(
                targets,
                &format!(
                    "impacted selection widened: selected test `{selected}` has no filterable registered executor"
                ),
            );
        };
        let filter = absolute
            .strip_prefix(&target.cwd)
            .ok()
            .map(|path| normalize_path(&path.to_string_lossy()))
            .filter(|path| !path.is_empty());
        let Some(filter) = filter else {
            return full_execution_requests(
                targets,
                &format!(
                    "impacted selection widened: selected test `{selected}` cannot be expressed as a safe runner filter"
                ),
            );
        };
        let cwd = target.cwd.display().to_string();
        grouped
            .entry((target.executor.as_str().to_owned(), cwd))
            .or_insert_with(|| (target.clone(), Vec::new()))
            .1
            .push((filter, selected.clone()));
        executed.insert(selected.clone());
    }
    let requests = batch_filter_groups(grouped);
    if requests.len() > MAX_FILTERED_PROCESSES {
        return full_execution_requests(
            targets,
            &format!(
                "impacted selection widened: {} batched processes exceed the safe process-amplification limit {MAX_FILTERED_PROCESSES}",
                requests.len()
            ),
        );
    }
    if requests.is_empty() && executed.is_empty() {
        full_execution_requests(
            targets,
            "impacted selection widened: selection produced no runnable requests",
        )
    } else {
        let process_count = requests.len();
        let reason = format!(
            "complete selection mapped {} test paths to {process_count} bounded runner {}",
            executed.len(),
            if process_count == 1 {
                "process"
            } else {
                "processes"
            }
        );
        (requests, "impacted".into(), reason, Some(executed))
    }
}

pub(in crate::service) fn batch_filter_groups(grouped: FilterGroups) -> Vec<ExecutionRequest> {
    pub(in crate::service) const MAX_FILTERS_PER_PROCESS: usize = 128;
    pub(in crate::service) const MAX_FILTER_BYTES_PER_PROCESS: usize = 24 * 1024;

    let mut requests = Vec::new();
    for (_, (target, pairs)) in grouped {
        let mut filters = Vec::new();
        let mut selected_tests = Vec::new();
        let mut filter_bytes = 0;
        for (filter, selected) in pairs {
            let next_bytes = filter_bytes + filter.len() + 1;
            if !filters.is_empty()
                && (filters.len() >= MAX_FILTERS_PER_PROCESS
                    || next_bytes > MAX_FILTER_BYTES_PER_PROCESS)
            {
                requests.push(ExecutionRequest {
                    target: target.clone(),
                    filters: std::mem::take(&mut filters),
                    selected_tests: std::mem::take(&mut selected_tests),
                });
                filter_bytes = 0;
            }
            filter_bytes += filter.len() + 1;
            filters.push(filter);
            selected_tests.push(selected);
        }
        if !filters.is_empty() {
            requests.push(ExecutionRequest {
                target,
                filters,
                selected_tests,
            });
        }
    }
    requests
}

pub(in crate::service) fn supports_path_filters(executor: &str) -> bool {
    matches!(
        executor,
        "vitest" | "storybook-vitest" | "storybook-vitest-v8" | "jest" | "bun-test" | "playwright"
    )
}

pub(in crate::service) fn target_accepts_filter(target: &ExecutorTarget, path: &str) -> bool {
    if is_story_path(path) {
        matches!(
            target.executor.as_str(),
            "storybook-vitest" | "storybook-vitest-v8"
        )
    } else {
        !matches!(
            target.executor.as_str(),
            "storybook-vitest" | "storybook-vitest-v8"
        ) && supports_path_filters(target.executor.as_str())
    }
}

pub(in crate::service) fn available_test_paths(
    repo: &Path,
    targets: &[ExecutorTarget],
    browser_paths: &BTreeSet<String>,
) -> Result<BTreeSet<String>, BusError> {
    let raw = String::from_utf8(git_output(
        repo,
        &[
            "ls-files".into(),
            "--cached".into(),
            "--others".into(),
            "--exclude-standard".into(),
            "-z".into(),
        ],
    )?)
    .map_err(|err| BusError::Intelligence(format!("Git paths are not UTF-8: {err}")))?;
    let mut paths = raw
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(normalize_path)
        .filter(|path| is_test_path(path))
        .filter(|path| repo.join(path).is_file())
        .filter(|path| {
            let absolute = repo.join(path);
            targets.iter().any(|target| {
                target_accepts_filter(target, path) && absolute.starts_with(&target.cwd)
            })
        })
        .collect::<BTreeSet<_>>();
    paths.extend(browser_paths.iter().cloned());
    Ok(paths)
}

pub(in crate::service) fn full_execution_requests(
    targets: &[ExecutorTarget],
    reason: &str,
) -> (
    Vec<ExecutionRequest>,
    String,
    String,
    Option<BTreeSet<String>>,
) {
    (
        targets
            .iter()
            .filter(|target| {
                !matches!(
                    target.executor.as_str(),
                    "storybook-vitest" | "storybook-vitest-v8"
                ) || !targets.iter().any(|candidate| {
                    candidate.cwd == target.cwd && candidate.executor.as_str() == "vitest"
                })
            })
            .cloned()
            .map(|target| ExecutionRequest {
                target,
                filters: Vec::new(),
                selected_tests: Vec::new(),
            })
            .collect(),
        "all".into(),
        reason.into(),
        None,
    )
}

pub(in crate::service) fn execute_full_targets(
    executors: &ExecutorRegistry,
    repo: &Path,
    targets: &[ExecutorTarget],
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<ExecutorRecord>, BusError> {
    let mut records = Vec::new();
    for target in targets {
        std::fs::create_dir_all(target.cwd.join(".weavatrix-quality")).map_err(|err| {
            BusError::Runtime(format!(
                "cannot prepare runner evidence directory in {}: {err}",
                target.cwd.display()
            ))
        })?;
        clear_generated_runner_artifacts(&target.cwd)?;
        let prepared = executors
            .prepare(PrepareRequest {
                executor: target.executor.clone(),
                cwd: target.cwd.clone(),
                filters: Vec::new(),
                exact_case: None,
                extra: BTreeMap::new(),
                limits: default_limits(),
                cancel: Arc::clone(cancel),
            })
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let started = SystemTime::now();
        let mut record = match executors.execute(&prepared) {
            Ok(ExecutionResult {
                status_code,
                stdout,
                stderr,
            }) => ExecutorRecord {
                executor: target.executor.as_str().to_owned(),
                cwd: relative_or_display(repo, &target.cwd),
                selection: Vec::new(),
                status_code,
                passed: status_code == Some(0),
                error: None,
                stdout,
                stderr,
                artifacts: Vec::new(),
            },
            Err(err) => ExecutorRecord {
                executor: target.executor.as_str().to_owned(),
                cwd: relative_or_display(repo, &target.cwd),
                selection: Vec::new(),
                status_code: None,
                passed: false,
                error: Some(err.to_string()),
                stdout: Vec::new(),
                stderr: Vec::new(),
                artifacts: Vec::new(),
            },
        };
        attach_normalized_artifacts(repo, &target.cwd, started, &mut record);
        clear_generated_runner_artifacts(&target.cwd)?;
        records.push(record);
    }
    Ok(records)
}

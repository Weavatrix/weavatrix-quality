//! Extracted command-bus helper.

use super::access::*;
use super::persist_run::put_json_run_artifact;

pub(in crate::service) struct SelectionAuditArtifactInput<'a> {
    pub(in crate::service) missed: &'a [StoredTestCaseIdentity],
    pub(in crate::service) learned_paths: &'a BTreeSet<String>,
    pub(in crate::service) impact_nodes_total: usize,
    pub(in crate::service) impact_nodes_considered: usize,
    pub(in crate::service) learning_truncated: bool,
}

pub(in crate::service) fn audit_live_selection(
    repo: &Path,
    store: &Store,
    impacted_raw: &str,
    full_raw: &str,
) -> Result<SelectionAuditReply, BusError> {
    let impacted_id =
        RunId::new(impacted_raw).map_err(|err| BusError::Identity(err.to_string()))?;
    let full_id = RunId::new(full_raw).map_err(|err| BusError::Identity(err.to_string()))?;
    if impacted_id == full_id {
        return Err(BusError::InvalidInput(
            "selection audit requires two distinct runs".into(),
        ));
    }
    if let Some(existing) = store
        .selection_audit_for_runs(&impacted_id, &full_id)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        return stored_selection_audit_reply(store, &existing);
    }
    let (impacted, _full) = load_shadow_runs(store, &impacted_id, &full_id)?;
    let impacted_summary = read_single_run_json(store, &impacted_id, "execution-summary")?;
    let full_summary = read_single_run_json(store, &full_id, "execution-summary")?;
    validate_shadow_scopes(&impacted_summary, &full_summary)?;
    let reduced = impacted_summary
        .get("effective_scope")
        .and_then(Value::as_str)
        == Some("impacted");
    let impacted_cases = store
        .test_case_results_for_run(&impacted_id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let full_cases = store
        .test_case_results_for_run(&full_id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let missed = if reduced {
        missed_failure_identities(&impacted_cases, &full_cases)
    } else {
        Vec::new()
    };
    let status = if !reduced {
        "not_reduced"
    } else if full_cases.is_empty() {
        "unmeasured"
    } else if missed.is_empty() {
        "corroborated"
    } else {
        "contradicted"
    };
    let impact = read_single_run_json(store, &impacted_id, "impacted-surface")?;
    let all_nodes = impact_nodes_from_artifact(&impact)?;
    let mut learned_paths = missed
        .iter()
        .filter_map(|case| resolve_observed_test_path(repo, &case.suite))
        .collect::<BTreeSet<_>>();
    let missed_count = u64::try_from(missed.len()).unwrap_or(u64::MAX);
    let all_node_count = all_nodes.len();
    let learning_truncated = learned_paths.len() > 500 || all_nodes.len() > 2_000;
    learned_paths = learned_paths.into_iter().take(500).collect();
    let learning_nodes = all_nodes.into_iter().take(2_000).collect::<Vec<_>>();
    let learned_count = u64::try_from(learned_paths.len()).unwrap_or(u64::MAX);
    let audit_id = format!(
        "selection-audit-{}",
        &sha256_hex(format!("{impacted_id}\0{full_id}").as_bytes())[..16]
    );
    let audit = StoredSelectionAudit {
        id: audit_id.clone(),
        impacted_run: impacted_id,
        full_run: full_id.clone(),
        change_id: impacted.change_id,
        revision: impacted.revision.clone(),
        status: status.into(),
        missed_failures: missed_count,
        learned_tests: learned_count,
    };
    store
        .put_selection_audit(&audit)
        .map_err(|err| BusError::Store(err.to_string()))?;
    for path in &learned_paths {
        store
            .observe_selection_miss(&audit_id, path, &learning_nodes, &impacted.revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    let artifact = SelectionAuditArtifactInput {
        missed: &missed,
        learned_paths: &learned_paths,
        impact_nodes_total: all_node_count,
        impact_nodes_considered: learning_nodes.len(),
        learning_truncated,
    };
    let handle = persist_selection_audit_artifact(store, &full_id, &audit, &artifact)?;
    Ok(SelectionAuditReply {
        audit_id,
        status: status.into(),
        missed_failure_count: missed_count,
        learned_test_count: learned_count,
        evidence_handle: Some(handle),
    })
}

pub(in crate::service) fn load_shadow_runs(
    store: &Store,
    impacted_id: &RunId,
    full_id: &RunId,
) -> Result<(StoredRun, StoredRun), BusError> {
    let impacted = store
        .get_run(impacted_id)
        .map_err(|err| BusError::Store(err.to_string()))?
        .ok_or_else(|| BusError::NotFound(format!("run {impacted_id}")))?;
    let full = store
        .get_run(full_id)
        .map_err(|err| BusError::Store(err.to_string()))?
        .ok_or_else(|| BusError::NotFound(format!("run {full_id}")))?;
    if impacted.change_id != full.change_id || impacted.revision != full.revision {
        return Err(BusError::Ambiguous(
            "selection audit runs do not share one change and revision".into(),
        ));
    }
    Ok((impacted, full))
}

pub(in crate::service) fn persist_selection_audit_artifact(
    store: &Store,
    full_run: &RunId,
    audit: &StoredSelectionAudit,
    input: &SelectionAuditArtifactInput<'_>,
) -> Result<String, BusError> {
    let handle = format!("artifact-{}", audit.id);
    put_json_run_artifact(
        store,
        full_run,
        &handle,
        "selection-audit",
        &json!({
            "schema_v": 1,
            "audit_id": audit.id,
            "impacted_run": audit.impacted_run.as_str(),
            "full_run": audit.full_run.as_str(),
            "change": audit.change_id,
            "revision": audit.revision.as_str(),
            "status": audit.status,
            "missed_failure_count": audit.missed_failures,
            "missed_failures": input.missed.iter().take(500).map(|case| json!({
                "executor": case.executor,
                "suite": case.suite,
                "name": case.name,
                "status": case.status,
            })).collect::<Vec<_>>(),
            "learned_test_paths": input.learned_paths,
            "impact_nodes_total": input.impact_nodes_total,
            "impact_nodes_considered": input.impact_nodes_considered,
            "learning_truncated": input.learning_truncated,
            "runtime_llm_tokens": 0,
        }),
        &mut Vec::new(),
    )?;
    Ok(handle)
}

pub(in crate::service) fn stored_selection_audit_reply(
    store: &Store,
    audit: &StoredSelectionAudit,
) -> Result<SelectionAuditReply, BusError> {
    let handle = format!("artifact-{}", audit.id);
    let artifact = ArtifactId::new(&handle).map_err(|err| BusError::Identity(err.to_string()))?;
    let present = store
        .get_artifact(&artifact)
        .map_err(|err| BusError::Store(err.to_string()))?
        .is_some();
    Ok(SelectionAuditReply {
        audit_id: audit.id.clone(),
        status: audit.status.clone(),
        missed_failure_count: audit.missed_failures,
        learned_test_count: audit.learned_tests,
        evidence_handle: present.then_some(handle),
    })
}

pub(in crate::service) fn validate_shadow_scopes(impacted: &Value, full: &Value) -> Result<(), BusError> {
    let impacted_requested = impacted.get("requested_scope").and_then(Value::as_str);
    let full_requested = full.get("requested_scope").and_then(Value::as_str);
    let full_effective = full.get("effective_scope").and_then(Value::as_str);
    if impacted_requested != Some("impacted")
        || full_requested != Some("all")
        || full_effective != Some("all")
    {
        return Err(BusError::InvalidInput(
            "selection audit requires an impacted run followed by an effective full run".into(),
        ));
    }
    Ok(())
}

pub(in crate::service) fn missed_failure_identities(
    impacted: &[StoredTestCaseIdentity],
    full: &[StoredTestCaseIdentity],
) -> Vec<StoredTestCaseIdentity> {
    let impacted_failures = impacted
        .iter()
        .filter(|case| matches!(case.status.as_str(), "fail" | "error"))
        .map(|case| (&case.executor, &case.suite, &case.name))
        .collect::<BTreeSet<_>>();
    full.iter()
        .filter(|case| matches!(case.status.as_str(), "fail" | "error"))
        .filter(|case| !impacted_failures.contains(&(&case.executor, &case.suite, &case.name)))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(in crate::service) fn impact_nodes_from_artifact(impact: &Value) -> Result<Vec<String>, BusError> {
    let mut nodes = BTreeSet::new();
    for field in ["base_only", "head_only", "shared", "removed_nodes"] {
        let values = impact
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| BusError::Store(format!("impacted-surface omitted {field}")))?;
        for value in values {
            let node = value.as_str().ok_or_else(|| {
                BusError::Store(format!("impacted-surface {field} contains a non-string"))
            })?;
            nodes.insert(node.to_owned());
        }
    }
    Ok(nodes.into_iter().collect())
}

pub(in crate::service) fn resolve_observed_test_path(repo: &Path, suite: &str) -> Option<String> {
    let normalized = normalize_path(suite);
    if !is_test_path(&normalized) {
        return None;
    }
    let root = std::fs::canonicalize(repo).ok()?;
    let absolute = std::fs::canonicalize(repo.join(&normalized)).ok()?;
    let relative = absolute.strip_prefix(root).ok()?;
    absolute
        .is_file()
        .then(|| normalize_path(&relative.to_string_lossy()))
}

pub(in crate::service) fn read_single_run_json(store: &Store, run: &RunId, kind: &str) -> Result<Value, BusError> {
    let mut found = None;
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, bytes) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind != kind {
            continue;
        }
        if found.is_some() {
            return Err(BusError::Store(format!(
                "run {run} has more than one {kind} artifact"
            )));
        }
        found =
            Some(serde_json::from_slice(&bytes).map_err(|err| {
                BusError::Store(format!("run {run} has malformed {kind}: {err}"))
            })?);
    }
    found.ok_or_else(|| BusError::Store(format!("run {run} has no {kind} artifact")))
}

pub(in crate::service) fn live_selection_report(selection: &LiveSelection, historical_candidates: usize) -> Value {
    let selected = selection
        .selected
        .iter()
        .zip(&selection.explanations)
        .map(|(path, explanation)| {
            json!({
                "path": path,
                "explanation": explanation,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_v": 2,
        "algorithm": "weavatrix-base-head-history-union+greedy-weighted-set-cover",
        "selected": selected,
        "historical_candidates": historical_candidates,
        "minimum_history_observations": 2,
        "uncovered_mandatory": selection.uncovered_mandatory,
        "uncovered_obligations": selection.uncovered_all,
    })
}

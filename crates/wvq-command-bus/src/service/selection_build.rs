use super::access::*;

pub(in crate::service) fn static_and_base_tests(static_report: &Value, diff: &Value) -> (Vec<String>, Vec<Vec<String>>) {
    let mut reasons = BTreeMap::<String, BTreeSet<String>>::new();
    for test in static_report
        .get("tests")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = test.get("path").and_then(Value::as_str) else {
            continue;
        };
        let entry = reasons.entry(normalize_path(path)).or_default();
        entry.insert("selected by Weavatrix head static impact".into());
        for reason in test
            .get("reasons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            entry.insert(format!("head evidence: {reason}"));
        }
    }

    for node in diff
        .pointer("/nodes/removed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(path) = node
            .get("label")
            .and_then(Value::as_str)
            .filter(|path| is_test_path(path))
        {
            reasons
                .entry(normalize_path(path))
                .or_default()
                .insert("base-only test preserved from graph_diff removed nodes".into());
        }
    }
    for edge in diff
        .pointer("/edges/removed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for key in ["source", "target"] {
            if let Some(path) = edge
                .get(key)
                .and_then(Value::as_str)
                .and_then(test_path_from_node_id)
            {
                reasons
                    .entry(path)
                    .or_default()
                    .insert("base-only test preserved from graph_diff removed edge".into());
            }
        }
    }

    let selected = reasons.keys().cloned().collect::<Vec<_>>();
    let explanations = reasons
        .into_values()
        .map(|items| items.into_iter().collect())
        .collect();
    (selected, explanations)
}

pub(in crate::service) fn historical_selection_candidates(
    store: &Store,
    impact: &wvq_intelligence::ImpactedSurface,
) -> Result<Vec<HistoricalTestCandidate>, BusError> {
    let mut candidates = store
        .historical_tests_for_nodes(&impact.all_nodes(), 2, 100_000)
        .map_err(|err| BusError::Store(err.to_string()))?;
    candidates.sort_by(|left, right| {
        right
            .defensive_misses
            .cmp(&left.defensive_misses)
            .then_with(|| right.matched_nodes.len().cmp(&left.matched_nodes.len()))
            .then_with(|| right.minimum_observations.cmp(&left.minimum_observations))
            .then_with(|| left.test_path.cmp(&right.test_path))
    });
    candidates.truncate(500);
    Ok(candidates)
}

pub(in crate::service) fn merge_historical_selection(
    repo: &Path,
    historical: &[HistoricalTestCandidate],
    selected: &mut Vec<String>,
    explanations: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for candidate in historical.iter().filter(|candidate| {
        repo.join(&candidate.test_path).is_file() && is_test_path(&candidate.test_path)
    }) {
        let path = normalize_path(&candidate.test_path);
        let reasons = explanations.entry(path.clone()).or_default();
        if candidate.minimum_observations > 0 {
            reasons.insert(format!(
                "selected by repeated measured coverage of {} impacted graph node(s), minimum {} observations, evidence revision {}",
                candidate.matched_nodes.len(),
                candidate.minimum_observations,
                candidate.last_revision
            ));
        }
        if candidate.defensive_misses > 0 {
            reasons.insert(format!(
                "selected after {} defensive full-run miss(es) across {} impacted graph node(s), evidence revision {}",
                candidate.defensive_misses,
                candidate.matched_nodes.len(),
                candidate.last_revision
            ));
        }
        selected.push(path);
    }
}

pub(in crate::service) fn merge_impacted_stories(
    repo: &Path,
    impact: &wvq_intelligence::ImpactedSurface,
    selected: &mut Vec<String>,
    explanations: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for path in impact
        .all_nodes()
        .iter()
        .filter_map(|node| test_path_from_node_id(node))
        .filter(|path| is_story_path(path) && repo.join(path).is_file())
    {
        explanations
            .entry(path.clone())
            .or_default()
            .insert("selected as a Storybook state in the base/head Weavatrix impact union".into());
        selected.push(path);
    }
}

pub(in crate::service) fn build_live_selection(
    repo: &Path,
    static_report: &Value,
    diff: &Value,
    impact: &wvq_intelligence::ImpactedSurface,
    obligations: &[ObligationNeed],
    additional_bindings: &[TestBinding],
    historical: &[HistoricalTestCandidate],
) -> Result<LiveSelection, BusError> {
    let (mut static_selected, static_explanations) = static_and_base_tests(static_report, diff);
    let mut explanations = static_selected
        .iter()
        .cloned()
        .zip(static_explanations)
        .map(|(path, reasons)| (path, reasons.into_iter().collect::<BTreeSet<_>>()))
        .collect::<BTreeMap<_, _>>();
    merge_historical_selection(repo, historical, &mut static_selected, &mut explanations);
    merge_impacted_stories(repo, impact, &mut static_selected, &mut explanations);
    let known = obligations
        .iter()
        .map(|obligation| obligation.id.clone())
        .collect::<BTreeSet<_>>();
    let bindings = merged_test_bindings(repo, &known, additional_bindings)?;
    let candidates = selection_candidates(repo, &bindings);
    let plan = select_minimal_plan(SelectionInput {
        candidates,
        obligations: obligations.to_owned(),
    });
    let mut selected = static_selected.into_iter().collect::<BTreeSet<_>>();
    for test in plan.selected {
        selected.insert(test.id.clone());
        explanations
            .entry(test.id)
            .or_default()
            .extend(test.explanation);
    }
    let covered = bindings
        .iter()
        .filter(|binding| {
            binding.case.is_some()
                && selected.contains(&binding.path)
                && repo.join(&binding.path).is_file()
        })
        .flat_map(|binding| binding.obligations.iter().cloned())
        .collect::<BTreeSet<_>>();
    let uncovered_all = known.difference(&covered).cloned().collect::<Vec<_>>();
    let selected = selected.into_iter().collect::<Vec<_>>();
    let explanations = selected
        .iter()
        .map(|path| {
            explanations
                .remove(path)
                .unwrap_or_else(|| BTreeSet::from(["selected by live policy".into()]))
                .into_iter()
                .collect()
        })
        .collect();
    Ok(LiveSelection {
        selected,
        explanations,
        uncovered_mandatory: plan.uncovered_mandatory,
        uncovered_all,
        bindings,
    })
}

pub(in crate::service) fn merged_test_bindings(
    repo: &Path,
    known: &BTreeSet<String>,
    additional_bindings: &[TestBinding],
) -> Result<Vec<TestBinding>, BusError> {
    let mut merged =
        BTreeMap::<(String, Option<String>, Option<String>, Option<String>), TestBinding>::new();
    let mut configured_bindings = load_test_bindings(repo)?;
    configured_bindings.extend(additional_bindings.iter().cloned());
    for binding in configured_bindings {
        if let Some(unknown) = binding
            .obligations
            .iter()
            .find(|obligation| !known.contains(*obligation))
        {
            return Err(BusError::Runtime(format!(
                "test binding {} names unknown obligation {unknown}",
                binding.path
            )));
        }
        let key = (
            binding.path.clone(),
            binding.runner.clone(),
            binding.suite.clone(),
            binding.case.clone(),
        );
        let entry = merged.entry(key).or_insert_with(|| TestBinding {
            path: binding.path.clone(),
            runner: binding.runner.clone(),
            suite: binding.suite.clone(),
            case: binding.case.clone(),
            obligations: BTreeSet::new(),
            cost: binding.cost,
            flake_penalty: binding.flake_penalty,
        });
        entry.obligations.extend(binding.obligations);
        entry.cost = entry.cost.min(binding.cost);
        entry.flake_penalty = entry.flake_penalty.max(binding.flake_penalty);
    }
    Ok(merged.into_values().collect())
}

pub(in crate::service) fn selection_candidates(repo: &Path, bindings: &[TestBinding]) -> Vec<TestCandidate> {
    let mut candidates = BTreeMap::<String, TestCandidate>::new();
    for binding in bindings
        .iter()
        .filter(|binding| binding.case.is_some() && repo.join(&binding.path).is_file())
    {
        let entry = candidates
            .entry(binding.path.clone())
            .or_insert_with(|| TestCandidate {
                id: binding.path.clone(),
                cost: binding.cost,
                flake_penalty: binding.flake_penalty,
                covers: BTreeSet::new(),
                explanation: Vec::new(),
            });
        entry.cost = entry.cost.min(binding.cost);
        entry.flake_penalty = entry.flake_penalty.max(binding.flake_penalty);
        entry.covers.extend(binding.obligations.iter().cloned());
    }
    for candidate in candidates.values_mut() {
        candidate.explanation.push(format!(
            "quality policy binds exact test case evidence to: {}",
            candidate
                .covers
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    candidates.into_values().collect()
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

pub(in crate::service) struct SelectionAuditArtifactInput<'a> {
    pub(in crate::service) missed: &'a [StoredTestCaseIdentity],
    pub(in crate::service) learned_paths: &'a BTreeSet<String>,
    pub(in crate::service) impact_nodes_total: usize,
    pub(in crate::service) impact_nodes_considered: usize,
    pub(in crate::service) learning_truncated: bool,
}

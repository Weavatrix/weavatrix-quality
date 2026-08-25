//! Extracted command-bus helper.

use super::access::*;
use super::persist_browser::{BEHAVIOR_PROGRAM_SAMPLE_LIMIT, BEHAVIOR_SAMPLE_LIMIT};
use super::persist_run::put_json_run_artifact;

pub(in crate::service) fn persist_browser_behavior(
    store: &Store,
    run_id: &RunId,
    revision: &RevisionId,
    browser_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
    handles: &mut Vec<String>,
) -> Result<BehaviorContributionSummary, BusError> {
    if browser_runs.is_empty() {
        return Ok(BehaviorContributionSummary::default());
    }

    let mut all_states = BTreeSet::new();
    let mut all_new_states = BTreeSet::new();
    let mut all_edges = BTreeSet::new();
    let mut all_new_edges = BTreeSet::new();
    let mut all_api_operations = BTreeSet::new();
    let mut programs = Vec::new();

    for (configured, result) in browser_runs {
        let contribution = persist_program_behavior(store, configured, result)?;
        all_states.extend(contribution.states.iter().cloned());
        all_new_states.extend(contribution.new_states.iter().cloned());
        all_edges.extend(contribution.edges.iter().cloned());
        all_new_edges.extend(contribution.new_edges.iter().cloned());
        all_api_operations.extend(contribution.api_operations.iter().cloned());
        programs.push(contribution.artifact);
    }

    let summary = BehaviorContributionSummary {
        states: u64::try_from(all_states.len()).unwrap_or(u64::MAX),
        new_states: u64::try_from(all_new_states.len()).unwrap_or(u64::MAX),
        edges: u64::try_from(all_edges.len()).unwrap_or(u64::MAX),
        new_edges: u64::try_from(all_new_edges.len()).unwrap_or(u64::MAX),
    };
    let (state_digests, state_digests_truncated) = bounded_set(&all_states, BEHAVIOR_SAMPLE_LIMIT);
    let (new_state_digests, new_state_digests_truncated) =
        bounded_set(&all_new_states, BEHAVIOR_SAMPLE_LIMIT);
    let (api_operations, api_operations_truncated) =
        bounded_set(&all_api_operations, BEHAVIOR_SAMPLE_LIMIT);
    let artifact = json!({
        "schema_v": 1,
        "run_id": run_id,
        "revision": revision,
        "state_count": summary.states,
        "new_state_count": summary.new_states,
        "edge_count": summary.edges,
        "new_edge_count": summary.new_edges,
        "state_digests": state_digests,
        "new_state_digests": new_state_digests,
        "api_operations": api_operations,
        "programs": programs,
        "coverage_status": "unmeasured",
        "coverage_nodes": [],
        "truncated": state_digests_truncated
            || new_state_digests_truncated
            || api_operations_truncated,
        "runtime_llm_tokens": 0,
    });
    put_json_run_artifact(
        store,
        run_id,
        &format!("artifact-{}-behavior-contribution", run_id.as_str()),
        "behavior-contribution",
        &artifact,
        handles,
    )?;
    Ok(summary)
}

pub(in crate::service) fn persist_program_behavior(
    store: &Store,
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
) -> Result<ProgramBehaviorContribution, BusError> {
    let mut states = BTreeSet::new();
    let mut new_states = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut new_edges = BTreeSet::new();
    let mut api_operations = BTreeSet::new();
    let mut observation_states = BTreeMap::new();

    for (index, observation) in result.observations.iter().enumerate() {
        api_operations.extend(
            observation
                .network
                .iter()
                .map(|operation| bounded_network_operation(operation))
                .filter(|operation| !operation.is_empty()),
        );
        let Some((digest, body)) = normalized_behavior_state(observation)? else {
            continue;
        };
        let digest_text = digest.to_string();
        states.insert(digest_text.clone());
        if store
            .put_behavior_state(&digest, &body)
            .map_err(|err| BusError::Store(err.to_string()))?
        {
            new_states.insert(digest_text);
        }
        observation_states.insert(index, digest);
    }
    for span in &result.action_spans {
        if let (Some(previous_digest), Some(digest)) = (
            observation_states.get(&span.start_observation),
            observation_states.get(&span.end_observation),
        ) {
            let (key, inserted) =
                persist_behavior_edge(store, previous_digest, digest, &span.action)?;
            edges.insert(key.clone());
            if inserted {
                new_edges.insert(key);
            }
        }
        if let TestAction::ApiCall { operation, .. } = &span.action {
            api_operations.insert(operation.clone());
        }
    }

    let artifact = program_behavior_artifact(
        configured,
        result,
        &states,
        &new_states,
        &edges,
        &new_edges,
        &api_operations,
    );
    Ok(ProgramBehaviorContribution {
        states,
        new_states,
        edges,
        new_edges,
        api_operations,
        artifact,
    })
}

pub(in crate::service) fn normalized_behavior_state(
    observation: &wvq_runtime::Observation,
) -> Result<Option<(ContentHash, Vec<u8>)>, BusError> {
    let Some(route) = observation
        .route
        .as_deref()
        .map(str::trim)
        .filter(|route| !route.is_empty())
    else {
        return Ok(None);
    };
    let state = BehaviorState {
        route: route.to_owned(),
        a11y_digest: observation.a11y_digest.clone(),
        viewport: observation.viewport.clone(),
        ..BehaviorState::default()
    };
    let body = state
        .canonical_json()
        .map_err(|err| BusError::Runtime(err.to_string()))?;
    let digest = state
        .digest()
        .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(Some((digest, body)))
}

pub(in crate::service) fn persist_behavior_edge(
    store: &Store,
    previous: &ContentHash,
    current: &ContentHash,
    action: &TestAction,
) -> Result<(String, bool), BusError> {
    let action = serde_json::to_string(action).map_err(|err| BusError::Runtime(err.to_string()))?;
    let key = format!("{previous}\0{action}\0{current}");
    let inserted = store
        .put_behavior_edge(previous, current, &action)
        .map_err(|err| BusError::Store(err.to_string()))?;
    Ok((key, inserted))
}

pub(in crate::service) fn program_behavior_artifact(
    configured: &ConfiguredBrowserProgram,
    result: &BrowserProgramRun,
    states: &BTreeSet<String>,
    new_states: &BTreeSet<String>,
    edges: &BTreeSet<String>,
    new_edges: &BTreeSet<String>,
    api_operations: &BTreeSet<String>,
) -> Value {
    let (state_digests, state_digests_truncated) =
        bounded_set(states, BEHAVIOR_PROGRAM_SAMPLE_LIMIT);
    let (new_state_digests, new_state_digests_truncated) =
        bounded_set(new_states, BEHAVIOR_PROGRAM_SAMPLE_LIMIT);
    let (api_operations, api_operations_truncated) =
        bounded_set(api_operations, BEHAVIOR_PROGRAM_SAMPLE_LIMIT);
    let duplicate_mutations = wvq_runtime::duplicate_mutation_requests(result);
    let action_spans = result
        .action_spans
        .iter()
        .take(BEHAVIOR_PROGRAM_SAMPLE_LIMIT)
        .collect::<Vec<_>>();
    let duplicate_samples = duplicate_mutations
        .iter()
        .take(BEHAVIOR_PROGRAM_SAMPLE_LIMIT)
        .collect::<Vec<_>>();
    json!({
        "program": configured.program.id,
        "passed": result.passed,
        "obligations": configured.program.obligations,
        "state_count": states.len(),
        "new_state_count": new_states.len(),
        "edge_count": edges.len(),
        "new_edge_count": new_edges.len(),
        "state_digests": state_digests,
        "new_state_digests": new_state_digests,
        "api_operations": api_operations,
        "action_span_count": result.action_spans.len(),
        "action_spans": action_spans,
        "duplicate_mutation_request_count": duplicate_mutations.len(),
        "duplicate_mutation_requests": duplicate_samples,
        "network_request_evidence_truncated": result.observations.iter().any(|observation| observation.network_requests_truncated),
        "coverage_status": "unmeasured",
        "coverage_nodes": [],
        "truncated": state_digests_truncated
            || new_state_digests_truncated
            || api_operations_truncated
            || result.action_spans.len() > BEHAVIOR_PROGRAM_SAMPLE_LIMIT
            || duplicate_mutations.len() > BEHAVIOR_PROGRAM_SAMPLE_LIMIT
            || result.observations.iter().any(|observation| observation.network_requests_truncated),
    })
}

pub(in crate::service) fn bounded_set(values: &BTreeSet<String>, limit: usize) -> (Vec<String>, bool) {
    (
        values.iter().take(limit).cloned().collect(),
        values.len() > limit,
    )
}

pub(in crate::service) fn bounded_network_operation(operation: &str) -> String {
    let mut parts = operation.split_whitespace();
    let Some(method) = parts.next() else {
        return String::new();
    };
    let Some(raw_url) = parts.next() else {
        return operation.chars().take(512).collect();
    };
    let url = raw_url
        .split(['?', '#'])
        .next()
        .unwrap_or(raw_url)
        .chars()
        .take(400)
        .collect::<String>();
    let status = parts.next().unwrap_or_default();
    format!(
        "{} {} {}",
        method.chars().take(16).collect::<String>(),
        url,
        status
    )
    .trim_end()
    .to_owned()
}

pub(in crate::service) fn recorded_api_operation(request: &wvq_runtime::NetworkRequestObservation) -> String {
    let path = request
        .url
        .split_once("://")
        .and_then(|(_, authority_and_path)| {
            authority_and_path
                .find('/')
                .map(|at| &authority_and_path[at..])
        })
        .unwrap_or(&request.url)
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .chars()
        .take(400)
        .collect::<String>();
    format!(
        "{} {}",
        request.method.chars().take(16).collect::<String>(),
        if path.is_empty() { "/" } else { &path }
    )
}

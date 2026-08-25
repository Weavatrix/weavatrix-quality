//! Capture half of record_controlled.

use super::super::access::*;
use super::super::persist_behavior::recorded_api_operation;
use super::LiveService;
use super::record::CapturedRecording;

impl LiveService {
    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn capture_controlled_record(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<CapturedRecording, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let _range = self.revision_range(&cmd.base, &cmd.head)?;
        let before = self.revision()?;
        let policy = load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
            BusError::Runtime(
                "passive recording requires a browser runtime in .weavatrix-quality/config.yaml"
                    .into(),
            )
        })?;
        let mut oracles = Vec::new();
        for obligation in &compiled.obligations {
            let Some(expected) = &obligation.expected else {
                continue;
            };
            oracles.push(ProgramOracle {
                obligation: obligation.id.clone(),
                condition: obligation
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                expected: serde_json::to_value(expected)
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
            });
        }
        let session_id = author_preview_token("recording")?;
        let idle_timeout = Duration::from_millis(cmd.idle_timeout_ms);
        let bridge_timeout = policy
            .timeout
            .max(idle_timeout.saturating_add(Duration::from_secs(15)))
            .min(Duration::from_secs(120));
        let evidence_dir = self
            .repo
            .join(".weavatrix-quality")
            .join("recording-evidence")
            .join(&session_id);
        let recording = record_browser_session(
            &BrowserRunConfig {
                base_url: policy.base_url,
                browser: policy.browser,
                headless: cmd.headless.unwrap_or(false),
                timeout: bridge_timeout,
                module_root: policy.module_root,
                runtime_dir: self
                    .repo
                    .join(".weavatrix-quality/runtime/playwright-runner"),
                evidence_dir: evidence_dir.clone(),
                viewport: None,
                ui_integrity: None,
                network: NetworkRunPolicy {
                    mode: NetworkMode::Record,
                    profile: None,
                    redact_json_keys: policy.network.redact_json_keys,
                    max_entries: policy.network.max_entries,
                    max_body_bytes: policy.network.max_body_bytes,
                    max_total_bytes: policy.network.max_total_bytes,
                },
                cancel: Arc::clone(&cancel),
            },
            &BrowserRecordingRequest {
                session: session_id.clone(),
                route: cmd.route.clone(),
                fixture_values: cmd.fixture_values.clone(),
                idle_timeout,
                max_events: cmd.max_events,
            },
            &oracles,
        )
        .map_err(|err| BusError::Runtime(err.to_string()))?;
        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during passive recording: `{before}` -> `{after}`"
            )));
        }
        let _ = std::fs::remove_dir(&evidence_dir);

        let initial = BehaviorState::from_observation(&recording.initial).ok_or_else(|| {
            BusError::Runtime("passive recorder initial observation omitted its route".into())
        })?;
        let mut recorder = Recorder::new(&session_id, None, None);
        recorder.start(initial);
        for (name, value) in &cmd.fixture_values {
            recorder.link_fixture(name, Value::String(value.clone()));
        }
        for event in &recording.events {
            let state = BehaviorState::from_observation(&event.observation).ok_or_else(|| {
                BusError::Runtime("passive recorder event omitted its route".into())
            })?;
            recorder
                .step(event.action.clone(), state)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
        }
        for outcome in &recording.obligations {
            if outcome.status == "passed" {
                recorder.link_obligation(
                    wvq_domain::ObligationId::new(&outcome.obligation)
                        .map_err(|err| BusError::Identity(err.to_string()))?,
                );
            }
        }
        let api_operations = recording
            .events
            .iter()
            .flat_map(|event| &event.observation.network_requests)
            .filter(|request| {
                request
                    .resource_type
                    .as_deref()
                    .is_some_and(|kind| matches!(kind, "fetch" | "xhr" | "websocket"))
            })
            .map(recorded_api_operation)
            .collect::<BTreeSet<_>>();
        for operation in &api_operations {
            recorder.link_api(operation);
        }
        let trace = recorder
            .finish()
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let store = self.store()?;
        let mut new_behavior_states = 0_u64;
        for digest in trace
            .state_digests()
            .map_err(|err| BusError::Runtime(err.to_string()))?
        {
            if !store
                .has_behavior_state(&digest)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_behavior_states = new_behavior_states.saturating_add(1);
            }
        }
        let mut new_behavior_edges = 0_u64;
        for edge in trace
            .edges()
            .map_err(|err| BusError::Runtime(err.to_string()))?
            .into_iter()
            .filter(|edge| edge.src != edge.dst)
        {
            let action = serde_json::to_string(&edge.action)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            if !store
                .has_behavior_edge(&edge.src, &edge.dst, &action)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_behavior_edges = new_behavior_edges.saturating_add(1);
            }
        }
        let linked_obligations = trace
            .obligations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut new_obligations = Vec::new();
        for obligation in &linked_obligations {
            if !store
                .has_behavior_obligation(obligation)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_obligations.push(obligation.clone());
            }
        }
        let api_operations = api_operations.into_iter().collect::<Vec<_>>();
        let mut new_api_operations = Vec::new();
        for operation in &api_operations {
            if !store
                .has_behavior_api_operation(operation)
                .map_err(|err| BusError::Store(err.to_string()))?
            {
                new_api_operations.push(operation.clone());
            }
        }
        let useful = new_behavior_states != 0
            || new_behavior_edges != 0
            || !new_obligations.is_empty()
            || !new_api_operations.is_empty();
        Ok(CapturedRecording {
            compiled,
            before,
            session_id,
            recording,
            trace,
            store,
            new_behavior_states,
            new_behavior_edges,
            linked_obligations,
            new_obligations,
            api_operations,
            new_api_operations,
            useful,
        })
    }
}

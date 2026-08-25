//! Persist half of record_controlled.

use super::super::access::*;
use super::LiveService;
use super::record::CapturedRecording;

impl LiveService {
    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn persist_controlled_record(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
        captured: CapturedRecording,
    ) -> Result<RecordReply, BusError> {
        let CapturedRecording {
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
        } = captured;
        if !useful {
            return Ok(RecordReply {
                session_id,
                change: compiled.change,
                revision: before.to_string(),
                captured_events: u64::try_from(trace.events.len()).unwrap_or(u64::MAX),
                useful: false,
                discarded: true,
                discard_reason: Some("no_new_behavior_or_protection".into()),
                new_behavior_states: 0,
                new_behavior_edges: 0,
                linked_obligations,
                new_obligations,
                api_operations,
                new_api_operations,
                limitations: recording.limitations,
                candidate: None,
                preview: None,
                trace_handle: None,
                network_profile_handle: None,
                runtime_llm_tokens: 0,
            });
        }

        let (candidate, preview) = if trace.obligations.is_empty() {
            (None, None)
        } else {
            let program_id = ProgramId::new(format!(
                "recorded-{}",
                &sha256_hex(session_id.as_bytes())[..16]
            ))
            .map_err(|err| BusError::Identity(err.to_string()))?;
            let program =
                promote(&trace, program_id).map_err(|err| BusError::Runtime(err.to_string()))?;
            let candidate =
                serde_json::to_value(&program).map_err(|err| BusError::Runtime(err.to_string()))?;
            let preview = self.author_preview_controlled(
                &AuthorPreviewCommand {
                    change: compiled.change.clone(),
                    base: cmd.base.clone(),
                    head: cmd.head.clone(),
                    program: candidate.clone(),
                    screenshot: false,
                    trace: false,
                },
                Arc::clone(&cancel),
            )?;
            (Some(candidate), Some(preview))
        };

        let mut event_rows = Vec::new();
        for event in &trace.events {
            let action = serde_json::to_string(&event.action)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            let digest = event
                .after
                .digest()
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            event_rows.push((action, digest));
        }
        for state in
            std::iter::once(&trace.initial).chain(trace.events.iter().map(|event| &event.after))
        {
            let body = state
                .canonical_json()
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            let digest = state
                .digest()
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            store
                .put_behavior_state(&digest, &body)
                .map_err(|err| BusError::Store(err.to_string()))?;
        }
        for edge in trace
            .edges()
            .map_err(|err| BusError::Runtime(err.to_string()))?
        {
            let action = serde_json::to_string(&edge.action)
                .map_err(|err| BusError::Runtime(err.to_string()))?;
            store
                .put_behavior_edge(&edge.src, &edge.dst, &action)
                .map_err(|err| BusError::Store(err.to_string()))?;
        }
        let trace_body =
            serde_json::to_vec(&trace).map_err(|err| BusError::Runtime(err.to_string()))?;
        let preview_id = preview.as_ref().map(|preview| preview.preview_id.as_str());
        store
            .put_recorded_session(
                &session_id,
                trace.seed,
                trace.fixture.as_deref(),
                before.as_str(),
                preview_id,
                &trace_body,
                &event_rows,
                &linked_obligations,
                &api_operations,
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        let trace_handle = format!("artifact-session-{session_id}-trace");
        let trace_artifact =
            ArtifactId::new(&trace_handle).map_err(|err| BusError::Identity(err.to_string()))?;
        store
            .put_artifact(&trace_artifact, "behavior-trace", &trace_body)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let network_profile_handle = recording
            .network_profile
            .as_ref()
            .filter(|profile| !profile.entries.is_empty())
            .map(|profile| {
                let handle = format!("artifact-session-{session_id}-network");
                let artifact =
                    ArtifactId::new(&handle).map_err(|err| BusError::Identity(err.to_string()))?;
                let body = serde_json::to_vec(profile)
                    .map_err(|err| BusError::Runtime(err.to_string()))?;
                store
                    .put_artifact(&artifact, "network-replay-profile", &body)
                    .map_err(|err| BusError::Store(err.to_string()))?;
                Ok::<_, BusError>(handle)
            })
            .transpose()?;
        Ok(RecordReply {
            session_id,
            change: compiled.change,
            revision: before.to_string(),
            captured_events: u64::try_from(trace.events.len()).unwrap_or(u64::MAX),
            useful: true,
            discarded: false,
            discard_reason: None,
            new_behavior_states,
            new_behavior_edges,
            linked_obligations,
            new_obligations,
            api_operations,
            new_api_operations,
            limitations: recording.limitations,
            candidate,
            preview,
            trace_handle: Some(trace_handle),
            network_profile_handle,
            runtime_llm_tokens: 0,
        })
    }
}

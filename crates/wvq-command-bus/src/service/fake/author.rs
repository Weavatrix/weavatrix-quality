//! Inherent FakeService methods for authoring and recording.
#![allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::unused_self
)]

#[allow(clippy::wildcard_imports)]
use super::super::access::*;
use super::FakeService;

impl FakeService {
    pub(in crate::service) fn author_draft(
        &self,
        cmd: &AuthorDraftCommand,
    ) -> Result<AuthorDraftReply, BusError> {
        validate_authoring_budget(cmd.token_budget)?;
        let model_usage = cmd.use_model.then(|| AuthorModelUsage {
            model: "fake-local-model".into(),
            input_tokens: 1,
            output_tokens: 1,
            cost_micros: 0,
        });
        Ok(AuthorDraftReply {
            change: cmd.change.clone(),
            revision: "fake-revision".into(),
            base: cmd.base.clone(),
            head: cmd.head.clone(),
            changed_files: vec!["src/widget.ts".into()],
            context: vec!["changed file src/widget.ts".into()],
            obligations: vec![AuthoringObligation {
                id: "others-visible".into(),
                requirement: "sankey.visual-limit".into(),
                scenario: "overflow-grouped".into(),
                kind: "behavioral".into(),
                risk: "high".into(),
                condition: None,
                expected: Some(json!({"kind": "visible", "target": {"test_id": "others"}})),
                required_evidence: vec!["dom".into()],
            }],
            truncated: false,
            tokens_used: 32,
            token_budget: cmd.token_budget,
            candidate: None,
            model_usage,
        })
    }

    pub(in crate::service) fn author_validate(
        &self,
        cmd: &AuthorValidateCommand,
    ) -> Result<AuthorValidateReply, BusError> {
        let id = cmd
            .program
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| BusError::InvalidInput("authoring candidate omitted id".into()))?;
        Ok(AuthorValidateReply {
            change: cmd.change.clone(),
            seal_id: "oseal-fake".into(),
            program_id: id.into(),
            program: cmd.program.clone(),
            obligations: vec!["others-visible".into()],
            valid: true,
            persisted: false,
        })
    }

    pub(in crate::service) fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        let _ = cancel;
        let validated = self.author_validate(&AuthorValidateCommand {
            change: cmd.change.clone(),
            program: cmd.program.clone(),
        })?;
        Ok(AuthorPreviewReply {
            preview_id: format!("preview-{}", validated.program_id),
            change: validated.change,
            revision: "fake-revision".into(),
            program_id: validated.program_id,
            passed: true,
            asserted: validated.obligations,
            contradicted: Vec::new(),
            failure: None,
            observation_handles: vec!["artifact-fake-author-observation-0".into()],
            screenshot_handles: if cmd.screenshot {
                vec!["artifact-fake-author-screenshot-0".into()]
            } else {
                Vec::new()
            },
            trace_handle: cmd.trace.then(|| "artifact-fake-author-trace".into()),
            program_persisted: false,
        })
    }

    pub(in crate::service) fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        let _ = cancel;
        Ok(RecordReply {
            session_id: "recording-fake".into(),
            change: cmd.change.clone(),
            revision: "fake-revision".into(),
            captured_events: 2,
            useful: true,
            discarded: false,
            discard_reason: None,
            new_behavior_states: 2,
            new_behavior_edges: 1,
            linked_obligations: vec!["others-visible".into()],
            new_obligations: vec!["others-visible".into()],
            api_operations: Vec::new(),
            new_api_operations: Vec::new(),
            limitations: Vec::new(),
            candidate: Some(json!({"id": "recorded-fake"})),
            preview: Some(AuthorPreviewReply {
                preview_id: "preview-recorded-fake".into(),
                change: cmd.change.clone(),
                revision: "fake-revision".into(),
                program_id: "recorded-fake".into(),
                passed: true,
                asserted: vec!["others-visible".into()],
                contradicted: Vec::new(),
                failure: None,
                observation_handles: Vec::new(),
                screenshot_handles: Vec::new(),
                trace_handle: None,
                program_persisted: false,
            }),
            trace_handle: Some("artifact-session-recording-fake-trace".into()),
            network_profile_handle: Some("artifact-session-recording-fake-network".into()),
            runtime_llm_tokens: 0,
        })
    }

    pub(in crate::service) fn author_promote(
        &self,
        cmd: &AuthorPromoteCommand,
    ) -> Result<AuthorPromoteReply, BusError> {
        let validated = self.author_validate(&AuthorValidateCommand {
            change: cmd.change.clone(),
            program: cmd.program.clone(),
        })?;
        Ok(AuthorPromoteReply {
            change: validated.change,
            revision: "fake-revision".into(),
            seal_id: validated.seal_id,
            program_id: validated.program_id,
            program_revision: 1,
            persisted: true,
            created: true,
        })
    }

    pub(in crate::service) fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        let _ = cancel;
        if cmd.program_id.trim().is_empty() || cmd.expected_program_revision == 0 {
            return Err(BusError::InvalidInput(
                "healing requires a program id and positive expected revision".into(),
            ));
        }
        Ok(AuthorHealReply {
            preview_id: format!("preview-heal-{}", cmd.program_id),
            change: cmd.change.clone(),
            revision: "fake-revision".into(),
            seal_id: "oseal-fake".into(),
            program_id: cmd.program_id.clone(),
            previous_program_revision: cmd.expected_program_revision,
            program_revision: Some(cmd.expected_program_revision.saturating_add(1)),
            passed: true,
            asserted: vec!["others-visible".into()],
            contradicted: Vec::new(),
            failure: None,
            observation_handles: vec!["artifact-fake-heal-observation-0".into()],
            screenshot_handles: if cmd.screenshot {
                vec!["artifact-fake-heal-screenshot-0".into()]
            } else {
                Vec::new()
            },
            trace_handle: cmd.trace.then(|| "artifact-fake-heal-trace".into()),
            persisted: true,
            created: true,
        })
    }

    pub(in crate::service) fn ingest_journal(
        &self,
        cmd: &IngestJournalCommand,
    ) -> Result<IngestJournalReply, BusError> {
        let journal = ContinuousJournal::from_json(&cmd.journal)
            .map_err(|err| BusError::InvalidInput(err.to_string()))?;
        Ok(IngestJournalReply {
            session_id: journal.session_id,
            change: cmd.change.clone(),
            revision: "fake-revision".into(),
            captured_events: u64::try_from(journal.events.len()).unwrap_or(u64::MAX),
            useful: true,
            discarded: false,
            discard_reason: None,
            new_behavior_states: 1,
            new_behavior_edges: 1,
            observed_only: true,
            seal_eligible: false,
            trace_handle: Some("artifact-session-journal-fake-trace".into()),
            journal_handle: Some("artifact-session-journal-fake-journal".into()),
            runtime_llm_tokens: 0,
        })
    }

    pub(in crate::service) fn ingest_cassette(
        &self,
        cmd: &IngestCassetteCommand,
    ) -> Result<IngestCassetteReply, BusError> {
        let admitted = ingest_har(&cmd.har, &cmd.origin)
            .map_err(|err| BusError::InvalidInput(err.to_string()))?;
        let useful = admitted.captured_entries != 0;
        Ok(IngestCassetteReply {
            origin: cmd.origin.clone(),
            revision: "fake-revision".into(),
            captured_entries: admitted.captured_entries,
            omitted: admitted.omitted,
            useful,
            discarded: !useful,
            discard_reason: (!useful).then(|| "no_json_same_origin_responses".into()),
            limitations: admitted.limitations,
            replay_enabled: false,
            seal_eligible: false,
            profile_handle: useful.then(|| "artifact-cassette-fake-profile".into()),
            runtime_llm_tokens: 0,
        })
    }
}

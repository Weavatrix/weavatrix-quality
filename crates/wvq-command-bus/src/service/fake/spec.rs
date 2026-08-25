//! Inherent FakeService methods for context, plan, run, verify, and recovery.
#![allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::unused_self
)]

#[allow(clippy::wildcard_imports)]
use super::super::access::*;
use super::super::verify_debt::verify_from_token;
use super::FakeService;

impl FakeService {
    pub(in crate::service) fn context(
        &self,
        cmd: &ContextCommand,
    ) -> Result<ContextReply, BusError> {
        validate_purpose(&cmd.purpose)?;
        let items = self.lock().context_items.clone();
        Ok(pack_context(
            &cmd.change,
            &cmd.purpose,
            cmd.token_budget,
            items,
        ))
    }

    pub(in crate::service) fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError> {
        Ok(PlanReply {
            change: cmd.change.clone(),
            requirements: vec!["sankey.visual-limit-others".into()],
            obligations: vec!["others-visible".into()],
            risk: vec!["requirement_criticality high sankey.visual-limit-others".into()],
            existing_proofs: Vec::new(),
            gaps: vec!["others-visible: no runtime evidence".into()],
            checks: deterministic_checks(),
            executed: false,
        })
    }

    pub(in crate::service) fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError> {
        self.run_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }

    pub(in crate::service) fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        let _ = cancel;
        validate_scope(&cmd.scope)?;
        validate_evidence_policy(&cmd.evidence_policy)?;
        let mut inner = self.lock();
        inner.run_executed = true;
        let state = RunState {
            id: "run-fake".into(),
            status: "complete".into(),
            outcome: "passed".into(),
            handles: inner.evidence.keys().cloned().collect(),
        };
        inner.last_run = Some(state.clone());
        Ok(RunReply {
            run_id: state.id,
            change: cmd.change.clone(),
            base: cmd.base.clone(),
            head: cmd.head.clone(),
            base_commit: "fake-base-commit".into(),
            head_commit: "fake-head-commit".into(),
            merge_base: "fake-merge-base".into(),
            requested_scope: cmd.scope.clone(),
            scope: cmd.scope.clone(),
            scope_reason: format!("{} scope requested by caller", cmd.scope),
            status: "complete".into(),
            executed: true,
            outcome: state.outcome,
            selected_test_count: 1,
            available_test_count: 2,
            executor_invocations: 0,
            browser_programs: 0,
            behavior_state_count: 0,
            new_behavior_state_count: 0,
            behavior_edge_count: 0,
            new_behavior_edge_count: 0,
            recorded_test_count: 0,
            failed_test_count: 0,
            flaky_test_count: 0,
            unknown_failure_count: 0,
            artifact_handles: state.handles,
        })
    }

    pub(in crate::service) fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError> {
        if impacted_run.is_empty() || full_run.is_empty() {
            return Err(BusError::Identity("selection audit run id is empty".into()));
        }
        Ok(SelectionAuditReply {
            audit_id: format!("audit-{impacted_run}-{full_run}"),
            status: "unmeasured".into(),
            missed_failure_count: 0,
            learned_test_count: 0,
            evidence_handle: None,
        })
    }

    pub(in crate::service) fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError> {
        let inner = self.lock();
        match (&cmd.run_id, &inner.last_run) {
            (Some(want), Some(run)) if want != &run.id => {
                Err(BusError::NotFound(format!("run {want}")))
            }
            (_, Some(run)) => Ok(StatusReply {
                run_id: Some(run.id.clone()),
                status: run.status.clone(),
                outcome: Some(run.outcome.clone()),
                handles: run.handles.clone(),
            }),
            (Some(want), None) => Err(BusError::NotFound(format!("run {want}"))),
            (None, None) => Ok(StatusReply {
                run_id: None,
                status: "idle".into(),
                outcome: None,
                handles: Vec::new(),
            }),
        }
    }

    pub(in crate::service) fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError> {
        let inner = self.lock();
        let verdict = inner.verdict.clone();
        let proofs = inner.proofs.clone();
        drop(inner);
        let mut reply = verify_from_token(&cmd.change, &verdict);
        if !proofs.is_empty() {
            reply.proofs = proofs;
        }
        Ok(reply)
    }

    pub(in crate::service) fn explain(
        &self,
        cmd: &ExplainCommand,
    ) -> Result<ExplainReply, BusError> {
        let inner = self.lock();
        inner
            .explanations
            .get(&cmd.id)
            .cloned()
            .ok_or_else(|| BusError::NotFound(format!("id {}", cmd.id)))
    }

    pub(in crate::service) fn evidence(
        &self,
        cmd: &EvidenceCommand,
    ) -> Result<EvidenceReply, BusError> {
        let inner = self.lock();
        let bytes = inner
            .evidence
            .get(&cmd.handle)
            .ok_or_else(|| BusError::NotFound(format!("handle {}", cmd.handle)))?;
        Ok(evidence_from_bytes(&cmd.handle, bytes))
    }

    pub(in crate::service) fn spec_validate(
        &self,
        cmd: &SpecCommand,
    ) -> Result<SpecValidateReply, BusError> {
        Ok(SpecValidateReply {
            change: cmd.change.clone(),
            requirements: 1,
            obligations: 1,
            ok: true,
        })
    }

    pub(in crate::service) fn spec_seal(
        &self,
        cmd: &SpecCommand,
    ) -> Result<SpecSealReply, BusError> {
        Ok(SpecSealReply {
            change: cmd.change.clone(),
            seal_id: "oseal-fake".into(),
            digest: "ab".repeat(32),
            obligations: 1,
        })
    }

    pub(in crate::service) fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError> {
        Ok(empty_debt(&cmd.base, &cmd.head))
    }

    pub(in crate::service) fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        Ok(SelectReply {
            base: cmd.base.clone(),
            head: cmd.head.clone(),
            revision: None,
            algorithm: "greedy-weighted-set-cover".into(),
            selected: Vec::new(),
            uncovered_mandatory: vec!["others-visible".into()],
            explanations: Vec::new(),
            executed: false,
            selection_complete: false,
        })
    }

    pub(in crate::service) fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError> {
        let kind = parse_model_kind(&cmd.kind)?;
        let (planning_tokens, runtime_tokens, browser_escape_calls, vision_calls) = match kind {
            AiCallKind::Planning => (1, 0, 0, 0),
            AiCallKind::Runtime => (0, 1, 0, 0),
            AiCallKind::BrowserEscape => (0, 1, 1, 0),
            AiCallKind::Vision => (0, 1, 0, 1),
        };
        Ok(ModelReply {
            change: cmd.change.clone(),
            kind: cmd.kind.clone(),
            model: "fake-local-model".into(),
            text: "fake model decision".into(),
            input_tokens: planning_tokens + runtime_tokens,
            output_tokens: 0,
            cost_micros: browser_escape_calls + vision_calls,
        })
    }

    pub(in crate::service) fn changes(
        &self,
        cmd: &ChangesCommand,
    ) -> Result<ChangesReply, BusError> {
        let _ = cmd;
        Ok(ChangesReply {
            changes: vec!["sankey-others".into()],
        })
    }

    pub(in crate::service) fn recovery(
        &self,
        cmd: &RecoveryCommand,
    ) -> Result<RecoveryReply, BusError> {
        Err(BusError::NotFound(format!(
            "fake recovery is not configured for {}",
            cmd.change
        )))
    }
}

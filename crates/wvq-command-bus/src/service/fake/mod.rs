//! In-memory QualityService for tests. No filesystem, no subprocesses.

mod spec;
mod author;

use std::sync::Mutex;

#[allow(clippy::wildcard_imports)]
use super::access::*;
use crate::service::BusError;
use crate::service::QualityService;

/// In-memory provider for tests. No filesystem, no subprocesses.
#[derive(Debug)]
pub struct FakeService {
    inner: Mutex<FakeInner>,
}

#[derive(Debug)]
pub(in crate::service) struct FakeInner {
    pub(in crate::service) context_items: Vec<String>,
    pub(in crate::service) verdict: String,
    pub(in crate::service) evidence: BTreeMap<String, Vec<u8>>,
    pub(in crate::service) run_executed: bool,
    pub(in crate::service) last_run: Option<RunState>,
    pub(in crate::service) explanations: BTreeMap<String, ExplainReply>,
    pub(in crate::service) proofs: Vec<ProofSummary>,
    pub(in crate::service) application_surface: ApplicationSurfaceView,
}

impl Default for FakeService {
    fn default() -> Self {
        Self {
            inner: Mutex::new(FakeInner {
                context_items: vec![
                    "requirement: others remain visible".into(),
                    "obligation: others-visible".into(),
                    "heuristic: 0 runtime LLM tokens".into(),
                    "coverage: unmeasured is not uncovered".into(),
                ],
                verdict: "UNPROVEN".into(),
                evidence: BTreeMap::new(),
                run_executed: false,
                last_run: None,
                explanations: BTreeMap::new(),
                proofs: Vec::new(),
                application_surface: ApplicationSurfaceView::absent(),
            }),
        }
    }
}

impl FakeService {
    /// Programmable overall verify verdict (`PROVEN`, `CONTRADICTED`, …).
    pub fn set_verdict(&self, verdict: impl Into<String>) {
        self.lock().verdict = verdict.into();
    }

    /// Items that [`QualityService::context`] will bound.
    pub fn set_context_items(&self, items: Vec<String>) {
        self.lock().context_items = items;
    }

    /// Store a blob behind a handle.
    pub fn put_evidence(&self, handle: impl Into<String>, bytes: Vec<u8>) {
        self.lock().evidence.insert(handle.into(), bytes);
    }

    /// Register an explanation.
    pub fn put_explain(&self, reply: ExplainReply) {
        self.lock().explanations.insert(reply.id.clone(), reply);
    }

    /// Per-obligation proofs [`QualityService::verify`] should return.
    ///
    /// Empty (the default) keeps the single placeholder proof.
    pub fn set_proofs(&self, proofs: Vec<ProofSummary>) {
        self.lock().proofs = proofs;
    }

    /// Read-only Application Surface Graph projection [`QualityService::verify`] returns.
    pub fn set_application_surface(&self, view: ApplicationSurfaceView) {
        self.lock().application_surface = view;
    }

    /// Whether [`QualityService::run`] was invoked.
    #[must_use]
    pub fn run_was_executed(&self) -> bool {
        self.lock().run_executed
    }

    pub(in crate::service) fn lock(&self) -> std::sync::MutexGuard<'_, FakeInner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl QualityService for FakeService {
    fn context(&self, cmd: &ContextCommand) -> Result<ContextReply, BusError> {
        FakeService::context(self, cmd)
    }
    fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError> {
        FakeService::plan(self, cmd)
    }
    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError> {
        FakeService::run(self, cmd)
    }
    fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        FakeService::run_controlled(self, cmd, cancel)
    }
    fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError> {
        FakeService::audit_selection(self, impacted_run, full_run)
    }
    fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError> {
        FakeService::status(self, cmd)
    }
    fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError> {
        FakeService::verify(self, cmd)
    }
    fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError> {
        FakeService::explain(self, cmd)
    }
    fn evidence(&self, cmd: &EvidenceCommand) -> Result<EvidenceReply, BusError> {
        FakeService::evidence(self, cmd)
    }
    fn spec_validate(&self, cmd: &SpecCommand) -> Result<SpecValidateReply, BusError> {
        FakeService::spec_validate(self, cmd)
    }
    fn spec_seal(&self, cmd: &SpecCommand) -> Result<SpecSealReply, BusError> {
        FakeService::spec_seal(self, cmd)
    }
    fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError> {
        FakeService::debt(self, cmd)
    }
    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        FakeService::select(self, cmd)
    }
    fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError> {
        FakeService::model(self, cmd)
    }
    fn author_draft(&self, cmd: &AuthorDraftCommand) -> Result<AuthorDraftReply, BusError> {
        FakeService::author_draft(self, cmd)
    }
    fn author_validate(
        &self,
        cmd: &AuthorValidateCommand,
    ) -> Result<AuthorValidateReply, BusError> {
        FakeService::author_validate(self, cmd)
    }
    fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        FakeService::author_preview_controlled(self, cmd, cancel)
    }
    fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        FakeService::record_controlled(self, cmd, cancel)
    }
    fn author_promote(&self, cmd: &AuthorPromoteCommand) -> Result<AuthorPromoteReply, BusError> {
        FakeService::author_promote(self, cmd)
    }
    fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        FakeService::author_heal_controlled(self, cmd, cancel)
    }
    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        FakeService::changes(self, cmd)
    }
    fn recovery(&self, cmd: &RecoveryCommand) -> Result<RecoveryReply, BusError> {
        FakeService::recovery(self, cmd)
    }
    fn init(&self, cmd: &InitCommand) -> Result<InitReply, BusError> {
        let _ = cmd;
        Ok(InitReply {
            created: vec![
                ".weavatrix-quality/config.yaml".into(),
                ".weavatrix-quality/.gitignore".into(),
            ],
            skipped: Vec::new(),
            config: ".weavatrix-quality/config.yaml".into(),
            runtime_llm_tokens: 0,
        })
    }
}

//! Shared QualityService trait and command dispatch.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::BusError;
use crate::commands::{
    AuthorDraftCommand, AuthorHealCommand, AuthorPreviewCommand, AuthorPromoteCommand,
    AuthorValidateCommand, ChangesCommand, Command, ContextCommand, DebtCommand, EvidenceCommand,
    ExplainCommand, InitCommand, IngestJournalCommand, ModelCommand, PlanCommand, RecordCommand,
    RecoveryCommand, RunCommand, SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
use crate::replies::{
    AuthorDraftReply, AuthorHealReply, AuthorPreviewReply, AuthorPromoteReply, AuthorValidateReply,
    ChangesReply, ContextReply, DebtReply, EvidenceReply, ExplainReply, InitReply,
    IngestJournalReply, ModelReply, PlanReply, RecordReply, RecoveryReply, Reply, RunReply,
    SelectReply, SelectionAuditReply, SpecSealReply, SpecValidateReply, StatusReply, VerifyReply,
};

/// Shared facade for every host.
pub trait QualityService: Send + Sync {
    /// Bounded neighbouring context.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be resolved or the purpose is unknown.
    fn context(&self, cmd: &ContextCommand) -> Result<ContextReply, BusError>;
    /// Deterministic plan. Must not execute runners.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be compiled.
    fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError>;
    /// Execute a bounded run through a registered executor producer. No arbitrary shell.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on unknown scope/policy, missing change, or an unavailable executor
    /// producer.
    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError>;
    /// Execute with a cooperative cancellation flag supplied by a transport.
    ///
    /// The default preserves compatibility for embedded services. Live
    /// execution overrides this and passes the flag into every bounded process.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::run`].
    fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        let _ = cancel;
        self.run(cmd)
    }
    /// Compare an impacted run with its defensive full run and persist learning.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when runs are missing, incomparable, or evidence is malformed.
    fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError>;
    /// Compact run progress and handles.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when a requested run is unknown.
    fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError>;
    /// Assemble revision-bound proofs.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be compiled.
    fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError>;
    /// Explain one identity with provenance.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when the id is unknown.
    fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError>;
    /// Evidence metadata; large bodies remain handles.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when the handle is unknown.
    fn evidence(&self, cmd: &EvidenceCommand) -> Result<EvidenceReply, BusError>;
    /// Validate `OpenSpec` + contract.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::Spec`] on malformed intent.
    fn spec_validate(&self, cmd: &SpecCommand) -> Result<SpecValidateReply, BusError>;
    /// Seal compiled obligations.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::Spec`] on malformed intent.
    fn spec_seal(&self, cmd: &SpecCommand) -> Result<SpecSealReply, BusError>;
    /// Debt-ratchet buckets.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be resolved.
    fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError>;
    /// Minimal impacted selection. Does not execute.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the change cannot be compiled.
    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError>;
    /// Execute one explicit loopback model call through the per-change AI Cost Firewall.
    ///
    /// Normal verification never invokes this method.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when model policy, transport, usage evidence, or budget is invalid.
    fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError>;
    /// Build a bounded revision- and seal-bound browser-test authoring packet.
    ///
    /// The optional model call is explicit and charged through the AI Cost Firewall.
    /// This method never persists a candidate or changes an oracle.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when revision, graph, intent, budget, or model evidence is invalid.
    fn author_draft(&self, cmd: &AuthorDraftCommand) -> Result<AuthorDraftReply, BusError>;
    /// Validate one candidate `TestProgram` against an existing `OracleSeal`.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on malformed IR, unknown obligations, or missing sealed predicates.
    fn author_validate(&self, cmd: &AuthorValidateCommand)
    -> Result<AuthorValidateReply, BusError>;
    /// Execute a validated candidate through actual Playwright and return evidence handles.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on invalid IR/runtime/revision evidence.
    fn author_preview(&self, cmd: &AuthorPreviewCommand) -> Result<AuthorPreviewReply, BusError> {
        self.author_preview_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }
    /// Execute an authoring preview with transport-owned cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::author_preview`].
    fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        let _ = cancel;
        self.author_preview(cmd)
    }
    /// Passively capture natural app use, discard redundant traces, and preview a useful replay.
    ///
    /// Promotion remains an explicit, same-seal operation; recording cannot mutate an oracle.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] on invalid revision, browser, seal, or evidence.
    fn record(&self, cmd: &RecordCommand) -> Result<RecordReply, BusError> {
        self.record_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }
    /// Passive recording with transport-owned cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::record`].
    fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        let _ = cancel;
        self.record(cmd)
    }
    /// Persist a passing preview as revision 1 of a canonical program.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the preview, program, seal, or repository revision differs.
    fn author_promote(&self, cmd: &AuthorPromoteCommand) -> Result<AuthorPromoteReply, BusError>;
    /// Apply locator/wait-only repair, replay it, and append a version only on pass.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] for stale versions, changed seals, illegal edits, or runtime failures.
    fn author_heal(&self, cmd: &AuthorHealCommand) -> Result<AuthorHealReply, BusError> {
        self.author_heal_controlled(cmd, Arc::new(AtomicBool::new(false)))
    }
    /// Safe-healing replay with transport-owned cooperative cancellation.
    ///
    /// # Errors
    ///
    /// Returns the same failures as [`QualityService::author_heal`].
    fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        let _ = cancel;
        self.author_heal(cmd)
    }
    /// Known `OpenSpec` changes.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::NotFound`] when `openspec/changes` cannot be read.
    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError>;
    /// Build a revision-bound recovery packet without sealing recovered intent.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when revision or graph evidence is incomplete.
    fn recovery(&self, cmd: &RecoveryCommand) -> Result<RecoveryReply, BusError>;
    /// Write a fail-closed `.weavatrix-quality/config.yaml` if one is missing.
    ///
    /// Does not invent test bindings, a browser origin, or a model endpoint.
    /// Does not open the evidence ledger.
    ///
    /// # Errors
    ///
    /// Returns [`BusError::InvalidInput`] when a policy already exists and
    /// `force` is false, or when the repository path is not a directory.
    fn init(&self, cmd: &InitCommand) -> Result<InitReply, BusError>;
    /// Admit a continuous observation journal as `OBSERVED_ONLY` graph evidence.
    ///
    /// Never previews, promotes, or seals. Unknown schema and illegal actions fail closed.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] when the journal is malformed or the repository revision is ambiguous.
    fn ingest_journal(&self, cmd: &IngestJournalCommand) -> Result<IngestJournalReply, BusError>;
}

/// Dispatch a [`Command`] through any [`QualityService`].
///
/// # Errors
///
/// Propagates [`BusError`] from the service method.
pub fn dispatch(service: &dyn QualityService, command: Command) -> Result<Reply, BusError> {
    match command {
        Command::Context(cmd) => service.context(&cmd).map(Reply::Context),
        Command::Analyze(cmd) => service.context(&cmd).map(Reply::Analyze),
        Command::Plan(cmd) => service.plan(&cmd).map(Reply::Plan),
        Command::Run(cmd) => service.run(&cmd).map(Reply::Run),
        Command::Status(cmd) => service.status(&cmd).map(Reply::Status),
        Command::Verify(cmd) => service
            .verify(&cmd)
            .map(|reply| Reply::Verify(Box::new(reply))),
        Command::Explain(cmd) => service.explain(&cmd).map(Reply::Explain),
        Command::Evidence(cmd) => service.evidence(&cmd).map(Reply::Evidence),
        Command::SpecValidate(cmd) => service.spec_validate(&cmd).map(Reply::SpecValidate),
        Command::SpecSeal(cmd) => service.spec_seal(&cmd).map(Reply::SpecSeal),
        Command::Debt(cmd) => service.debt(&cmd).map(Reply::Debt),
        Command::Select(cmd) => service.select(&cmd).map(Reply::Select),
        Command::Model(cmd) => service.model(&cmd).map(Reply::Model),
        Command::AuthorDraft(cmd) => service.author_draft(&cmd).map(Reply::AuthorDraft),
        Command::AuthorValidate(cmd) => service.author_validate(&cmd).map(Reply::AuthorValidate),
        Command::AuthorPreview(cmd) => service.author_preview(&cmd).map(Reply::AuthorPreview),
        Command::Record(cmd) => service
            .record(&cmd)
            .map(|reply| Reply::Record(Box::new(reply))),
        Command::AuthorPromote(cmd) => service.author_promote(&cmd).map(Reply::AuthorPromote),
        Command::AuthorHeal(cmd) => service.author_heal(&cmd).map(Reply::AuthorHeal),
        Command::Changes(cmd) => service.changes(&cmd).map(Reply::Changes),
        Command::Recovery(cmd) => service
            .recovery(&cmd)
            .map(|reply| Reply::Recovery(Box::new(reply))),
        Command::Init(cmd) => service.init(&cmd).map(Reply::Init),
        Command::IngestJournal(cmd) => service.ingest_journal(&cmd).map(Reply::IngestJournal),
    }
}


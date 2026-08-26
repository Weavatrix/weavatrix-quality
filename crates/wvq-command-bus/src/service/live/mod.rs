//! Filesystem-backed live service inherent helpers.
#![allow(clippy::needless_pass_by_value)]

use std::sync::Mutex;

use super::access::*;
use super::QualityService;

mod repo;
mod recovery;
mod protection;
mod protection_store;
mod protection_axes;
mod ui;
mod ui_replay;
mod ui_responsive;
mod spec;
mod verify;
mod explain;
mod debt;
mod model;
mod author;
mod promote;
mod record;
mod record_capture;
mod record_persist;
mod ingest_journal;
mod ingest_cassette;
mod baseline;
mod run;
mod run_types;
mod run_prepare;
mod run_execute;
mod run_persist;
mod run_finish;
mod init;

/// Filesystem-backed service with registered bounded executors and a persistent evidence ledger.
#[derive(Debug)]
pub struct LiveService {
    pub(in crate::service) repo: PathBuf,
    pub(in crate::service) state: Mutex<Option<RunState>>,
    pub(in crate::service) executors: ExecutorRegistry,
    pub(in crate::service) executor_init_error: Option<String>,
}

impl LiveService {
    /// `repo` is the repository root that contains `openspec/`.
    #[must_use]
    pub fn new(repo: impl AsRef<Path>) -> Self {
        let (executors, executor_init_error) = match ExecutorRegistry::production() {
            Ok(registry) => (registry, None),
            Err(err) => (ExecutorRegistry::new(), Some(err.to_string())),
        };
        Self {
            repo: canonical_repo_path(repo.as_ref()),
            state: Mutex::new(None),
            executors,
            executor_init_error,
        }
    }

    /// Construct a live service with an explicit registered executor set.
    /// Intended for controlled embedding and integration tests.
    #[must_use]
    pub fn with_executors(repo: impl AsRef<Path>, executors: ExecutorRegistry) -> Self {
        Self {
            repo: canonical_repo_path(repo.as_ref()),
            state: Mutex::new(None),
            executors,
            executor_init_error: None,
        }
    }

    pub(in crate::service) fn lock(&self) -> std::sync::MutexGuard<'_, Option<RunState>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(in crate::service) fn compiled(&self, change: &str) -> Result<Compiled, BusError> {
        compile_repository(&self.repo, change)
    }

    pub(in crate::service) fn store(&self) -> Result<Store, BusError> {
        Store::open(&self.repo).map_err(|err| BusError::Store(err.to_string()))
    }

    pub(in crate::service) fn revision(&self) -> Result<RevisionId, BusError> {
        WeavatrixProvider
            .analyze(&self.repo)
            .map(|evidence| evidence.revision)
            .map_err(|err| BusError::Intelligence(err.to_string()))
    }
}

impl QualityService for LiveService {
    fn context(&self, cmd: &ContextCommand) -> Result<ContextReply, BusError> {
        LiveService::context(self, cmd)
    }
    fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError> {
        LiveService::plan(self, cmd)
    }
    fn run(&self, cmd: &RunCommand) -> Result<RunReply, BusError> {
        LiveService::run(self, cmd)
    }
    fn run_controlled(
        &self,
        cmd: &RunCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RunReply, BusError> {
        LiveService::run_controlled(self, cmd, cancel)
    }
    fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError> {
        LiveService::audit_selection(self, impacted_run, full_run)
    }
    fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError> {
        LiveService::status(self, cmd)
    }
    fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError> {
        LiveService::verify(self, cmd)
    }
    fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError> {
        LiveService::explain(self, cmd)
    }
    fn evidence(&self, cmd: &EvidenceCommand) -> Result<EvidenceReply, BusError> {
        LiveService::evidence(self, cmd)
    }
    fn spec_validate(&self, cmd: &SpecCommand) -> Result<SpecValidateReply, BusError> {
        LiveService::spec_validate(self, cmd)
    }
    fn spec_seal(&self, cmd: &SpecCommand) -> Result<SpecSealReply, BusError> {
        LiveService::spec_seal(self, cmd)
    }
    fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError> {
        LiveService::debt(self, cmd)
    }
    fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        LiveService::select(self, cmd)
    }
    fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError> {
        LiveService::model(self, cmd)
    }
    fn author_draft(&self, cmd: &AuthorDraftCommand) -> Result<AuthorDraftReply, BusError> {
        LiveService::author_draft(self, cmd)
    }
    fn author_validate(
        &self,
        cmd: &AuthorValidateCommand,
    ) -> Result<AuthorValidateReply, BusError> {
        LiveService::author_validate(self, cmd)
    }
    fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        LiveService::author_preview_controlled(self, cmd, cancel)
    }
    fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        LiveService::record_controlled(self, cmd, cancel)
    }
    fn author_promote(&self, cmd: &AuthorPromoteCommand) -> Result<AuthorPromoteReply, BusError> {
        LiveService::author_promote(self, cmd)
    }
    fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        LiveService::author_heal_controlled(self, cmd, cancel)
    }
    fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        LiveService::changes(self, cmd)
    }
    fn recovery(&self, cmd: &RecoveryCommand) -> Result<RecoveryReply, BusError> {
        LiveService::recovery(self, cmd)
    }
    fn init(&self, cmd: &InitCommand) -> Result<InitReply, BusError> {
        LiveService::init(self, cmd)
    }
    fn ingest_journal(&self, cmd: &IngestJournalCommand) -> Result<IngestJournalReply, BusError> {
        LiveService::ingest_journal(self, cmd)
    }
    fn ingest_cassette(
        &self,
        cmd: &IngestCassetteCommand,
    ) -> Result<IngestCassetteReply, BusError> {
        LiveService::ingest_cassette(self, cmd)
    }
    fn baseline(&self, cmd: &BaselineCommand) -> Result<BaselineReply, BusError> {
        LiveService::baseline(self, cmd)
    }
}

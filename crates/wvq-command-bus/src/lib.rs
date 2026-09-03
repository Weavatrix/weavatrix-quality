//! Shared command bus for CLI, MCP, and (later) HTTP.
//!
//! Transport adapters live in `apps/`. This crate has no MCP or argv types.

#![forbid(unsafe_code)]

mod commands;
mod replies;
mod service;
mod source_mutation;

pub use commands::{
    AnalyzeCommand, AuthorDraftCommand, AuthorHealCommand, AuthorHealEdit, AuthorPreviewCommand,
    AuthorPromoteCommand, AuthorValidateCommand, BaselineCommand, ChangesCommand, Command,
    ContextCommand, DebtCommand, EvidenceCommand, ExplainCommand, InitCommand, IngestCassetteCommand,
    IngestJournalCommand, ModelCommand, PlanCommand, RecordCommand, RecoveryCommand, RunCommand,
    SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
pub use replies::{
    AnalyzeReply, AuthorDraftReply, AuthorHealReply, AuthorModelUsage, AuthorPreviewReply,
    AuthorPromoteReply, AuthorValidateReply, AuthoringObligation, BaselineReply, ChangesReply,
    ContextReply, DebtReply, EvidenceReply, ExplainReply, INLINE_LIMIT, InitReply, IngestCassetteReply,
    IngestJournalReply, ModelReply, PlanReply, ProofSummary, RecordReply, RecoveryReply, Reply,
    RunReply, SelectReply, SelectionAuditReply, SpecSealReply, SpecValidateReply, StatusReply,
    VerifyReply, estimate_tokens, ApplicationSurfaceView, CheapestEvidencePlanView,
    SurfaceEvidenceMatrixView,
};
pub use service::{BusError, FakeService, LiveService, QualityService, dispatch};
pub use wvq_intelligence::{
    ApplicationSurfaceKind, EvidenceCell, EvidenceColumn, EvidenceNeed, EvidencePlan,
    EvidenceProducer, ProducerOffer, SurfaceEvidenceRow,
};

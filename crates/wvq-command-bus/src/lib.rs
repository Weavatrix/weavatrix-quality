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
    ContextCommand, DebtCommand, DoctorCommand, EvidenceCommand, ExplainCommand,
    IngestCassetteCommand, IngestJournalCommand, InitCommand, ModelCommand, PlanCommand,
    RecordCommand, RecoveryCommand, RunCommand, SelectCommand, SpecCommand, StatusCommand,
    VerifyCommand,
};
pub use replies::{
    AnalyzeReply, ApplicationSurfaceView, AuthorDraftReply, AuthorHealReply, AuthorModelUsage,
    AuthorPreviewReply, AuthorPromoteReply, AuthorValidateReply, AuthoringObligation,
    BaselineReply, ChangesReply, CheapestEvidencePlanView, ContextReply, DebtReply, DoctorBinding,
    DoctorReply, DoctorRunner, EvidenceReply, ExplainReply, INLINE_LIMIT, IngestCassetteReply,
    IngestJournalReply, InitReply, ModelReply, PlanReply, ProofSummary, RecordReply, RecoveryReply,
    Reply, RunReply, SelectReply, SelectionAuditReply, SpecSealReply, SpecValidateReply,
    StatusReply, SurfaceEvidenceMatrixView, VerifyReply, estimate_tokens,
};
pub use service::{BusError, FakeService, LiveService, QualityService, dispatch};
pub use wvq_intelligence::{
    ApplicationSurfaceKind, EvidenceCell, EvidenceColumn, EvidenceNeed, EvidencePlan,
    EvidenceProducer, ProducerOffer, SurfaceEvidenceRow,
};

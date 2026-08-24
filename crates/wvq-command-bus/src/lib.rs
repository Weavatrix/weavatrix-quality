//! Shared command bus for CLI, MCP, and (later) HTTP.
//!
//! Transport adapters live in `apps/`. This crate has no MCP or argv types.

#![forbid(unsafe_code)]

mod commands;
mod replies;
mod service;

pub use commands::{
    AnalyzeCommand, AuthorDraftCommand, AuthorHealCommand, AuthorHealEdit, AuthorPreviewCommand,
    AuthorPromoteCommand, AuthorValidateCommand, ChangesCommand, Command, ContextCommand,
    DebtCommand, EvidenceCommand, ExplainCommand, ModelCommand, PlanCommand, RecoveryCommand,
    RunCommand, SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
pub use replies::{
    AnalyzeReply, AuthorDraftReply, AuthorHealReply, AuthorModelUsage, AuthorPreviewReply,
    AuthorPromoteReply, AuthorValidateReply, AuthoringObligation, ChangesReply, ContextReply,
    DebtReply, EvidenceReply, ExplainReply, INLINE_LIMIT, ModelReply, PlanReply, ProofSummary,
    RecoveryReply, Reply, RunReply, SelectReply, SelectionAuditReply, SpecSealReply,
    SpecValidateReply, StatusReply, VerifyReply, estimate_tokens,
};
pub use service::{BusError, FakeService, LiveService, QualityService, dispatch};

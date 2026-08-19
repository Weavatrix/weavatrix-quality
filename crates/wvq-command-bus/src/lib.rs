//! Shared command bus for CLI, MCP, and (later) HTTP.
//!
//! Transport adapters live in `apps/`. This crate has no MCP or argv types.

#![forbid(unsafe_code)]

mod commands;
mod replies;
mod service;

pub use commands::{
    AnalyzeCommand, Command, ContextCommand, DebtCommand, EvidenceCommand, ExplainCommand,
    PlanCommand, RunCommand, SelectCommand, SpecCommand, StatusCommand, VerifyCommand,
};
pub use replies::{
    AnalyzeReply, ContextReply, DebtReply, EvidenceReply, ExplainReply, INLINE_LIMIT, PlanReply,
    Reply, RunReply, SelectReply, SpecSealReply, SpecValidateReply, StatusReply, VerifyReply,
    estimate_tokens,
};
pub use service::{BusError, FakeService, LiveService, QualityService, dispatch};

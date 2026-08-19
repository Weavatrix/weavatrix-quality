//! Bounded replies. Large artifacts are handles, never dumped into context.

use serde::Serialize;

/// Maximum UTF-8 bytes allowed inline in [`EvidenceReply`]. Larger stays a handle.
pub const INLINE_LIMIT: usize = 4_096;

/// Approximate LLM tokens: four Unicode scalars per token, rounded up.
#[must_use]
pub fn estimate_tokens(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    if chars == 0 { 0 } else { chars.div_ceil(4) }
}

/// Keep `items` under `budget` tokens. Sets `truncated` when anything is dropped.
#[must_use]
pub fn bound_items(items: Vec<String>, budget: u64) -> (Vec<String>, u64, bool) {
    let mut out = Vec::new();
    let mut used = 0_u64;
    let mut truncated = false;
    for item in items {
        let cost = estimate_tokens(&item).max(1);
        if used.saturating_add(cost) > budget {
            truncated = true;
            if out.is_empty() {
                let prefix = truncate_to_budget(&item, budget);
                used = estimate_tokens(&prefix);
                out.push(prefix);
            }
            break;
        }
        used = used.saturating_add(cost);
        out.push(item);
    }
    (out, used, truncated)
}

fn truncate_to_budget(text: &str, budget: u64) -> String {
    let mut used = 0_u64;
    let mut end = 0;
    for (idx, ch) in text.char_indices() {
        let next = estimate_tokens(ch.encode_utf8(&mut [0; 4])).max(1);
        if used.saturating_add(next) > budget {
            break;
        }
        used = used.saturating_add(next);
        end = idx + ch.len_utf8();
    }
    let mut prefix = text[..end].to_owned();
    prefix.push('…');
    prefix
}

/// Bounded `QualityContextPacket`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextReply {
    /// Change that was resolved.
    pub change: String,
    /// Requested purpose.
    pub purpose: String,
    /// Neighbouring requirements (truncated to budget).
    pub requirements: Vec<String>,
    /// Compiled obligations.
    pub obligations: Vec<String>,
    /// Deterministic quality heuristics.
    pub heuristics: Vec<String>,
    /// Coverage notes. Unmeasured is not uncovered.
    pub coverage: Vec<String>,
    /// Whether anything was dropped to honour the budget.
    pub truncated: bool,
    /// Approximate tokens in this packet.
    pub tokens_used: u64,
    /// Budget that was applied.
    pub token_budget: u64,
}

/// Plan: requirements, obligations, risk, proofs, gaps. Never execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanReply {
    /// Change.
    pub change: String,
    /// Requirement identities.
    pub requirements: Vec<String>,
    /// Obligation identities and kinds.
    pub obligations: Vec<String>,
    /// Named risk evidence. Never an opaque percentage.
    pub risk: Vec<String>,
    /// Existing proof ids, if any.
    pub existing_proofs: Vec<String>,
    /// Unproven / missing-evidence gaps.
    pub gaps: Vec<String>,
    /// Deterministic check families that will run later.
    pub checks: Vec<String>,
    /// Always false. Plan does not execute.
    pub executed: bool,
}

/// Run acknowledgement with handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunReply {
    /// Run identity.
    pub run_id: String,
    /// Change.
    pub change: String,
    /// Scope that was accepted.
    pub scope: String,
    /// `complete` or `queued`.
    pub status: String,
    /// True when a run was recorded (still no arbitrary shell).
    pub executed: bool,
    /// Artifact handles produced (possibly empty).
    pub artifact_handles: Vec<String>,
}

/// Compact progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusReply {
    /// Latest or requested run.
    pub run_id: Option<String>,
    /// `idle`, `queued`, or `complete`.
    pub status: String,
    /// Handles currently known.
    pub handles: Vec<String>,
}

/// Multi-axis verdict. Not a quality percentage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyReply {
    /// Change.
    pub change: String,
    /// Combined `Proof` verdict token.
    pub verdict: String,
    /// True when the verdict must fail CI (`CONTRADICTED`).
    pub blocking: bool,
    /// Per-obligation proof summaries.
    pub proofs: Vec<ProofSummary>,
}

impl VerifyReply {
    /// Process exit code: `0` proven, `2` blocking, `1` otherwise.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        if self.blocking {
            2
        } else {
            i32::from(self.verdict != "PROVEN")
        }
    }
}

/// One assembled proof, compact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProofSummary {
    /// Proof id.
    pub id: String,
    /// Requirement the obligation belongs to. Drives Studio drill-down.
    pub requirement: String,
    /// Obligation.
    pub obligation: String,
    /// Verdict token.
    pub verdict: String,
}

impl ProofSummary {
    /// Whether this proof is ordinary pass-noise the dashboard should hide.
    #[must_use]
    pub fn is_passing(&self) -> bool {
        self.verdict == "PROVEN"
    }
}

/// Known `OpenSpec` changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangesReply {
    /// Change identities, sorted.
    pub changes: Vec<String>,
}

/// Provenance-bearing explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplainReply {
    /// Requested id.
    pub id: String,
    /// `obligation`, `proof`, `finding`, or `selection`.
    pub kind: String,
    /// Short summary.
    pub summary: String,
    /// File/line or selection chain.
    pub provenance: Vec<String>,
}

/// Bounded evidence. `inline_text` is absent for large or binary artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceReply {
    /// Handle the caller used or was given.
    pub handle: String,
    /// Artifact kind (`text`, `junit`, `screenshot`, …).
    pub kind: String,
    /// Byte length of the stored blob.
    pub byte_len: u64,
    /// Content hash when known.
    pub content_hash: Option<String>,
    /// Small UTF-8 payload only.
    pub inline_text: Option<String>,
}

/// `wvq spec validate` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecValidateReply {
    /// Change.
    pub change: String,
    /// Requirement count.
    pub requirements: u64,
    /// Compiled obligation count.
    pub obligations: u64,
    /// Always true on success (errors are [`crate::BusError`]).
    pub ok: bool,
}

/// `wvq spec seal` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecSealReply {
    /// Change.
    pub change: String,
    /// `OracleSeal` id.
    pub seal_id: String,
    /// Canonical digest.
    pub digest: String,
    /// Obligation count sealed.
    pub obligations: u64,
}

/// Alias of [`ContextReply`] for `wvq analyze`.
pub type AnalyzeReply = ContextReply;

/// Debt-ratchet bucket counts. Empty when no findings were supplied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebtReply {
    /// Present on base and head.
    pub existing: u64,
    /// Introduced on head.
    pub new: u64,
    /// Gone on head.
    pub fixed: u64,
    /// Previously fixed, back on head.
    pub returned: u64,
    /// Explicit exceptions.
    pub excepted: u64,
    /// Compact finding summaries.
    pub findings: Vec<String>,
}

/// Minimal selection. Does not execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectReply {
    /// Algorithm id.
    pub algorithm: String,
    /// Selected test ids.
    pub selected: Vec<String>,
    /// High-risk obligations no candidate covered.
    pub uncovered_mandatory: Vec<String>,
    /// Explanation chains, aligned with `selected`.
    pub explanations: Vec<Vec<String>>,
    /// Always false.
    pub executed: bool,
}

/// Tagged reply for CLI JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "command", content = "body")]
pub enum Reply {
    /// [`ContextReply`].
    #[serde(rename = "context")]
    Context(ContextReply),
    /// [`PlanReply`].
    #[serde(rename = "plan")]
    Plan(PlanReply),
    /// [`RunReply`].
    #[serde(rename = "run")]
    Run(RunReply),
    /// [`StatusReply`].
    #[serde(rename = "status")]
    Status(StatusReply),
    /// [`VerifyReply`].
    #[serde(rename = "verify")]
    Verify(VerifyReply),
    /// [`ExplainReply`].
    #[serde(rename = "explain")]
    Explain(ExplainReply),
    /// [`EvidenceReply`].
    #[serde(rename = "evidence")]
    Evidence(EvidenceReply),
    /// [`SpecValidateReply`].
    #[serde(rename = "spec_validate")]
    SpecValidate(SpecValidateReply),
    /// [`SpecSealReply`].
    #[serde(rename = "spec_seal")]
    SpecSeal(SpecSealReply),
    /// [`AnalyzeReply`].
    #[serde(rename = "analyze")]
    Analyze(AnalyzeReply),
    /// [`DebtReply`].
    #[serde(rename = "debt")]
    Debt(DebtReply),
    /// [`SelectReply`].
    #[serde(rename = "select")]
    Select(SelectReply),
    /// [`ChangesReply`].
    #[serde(rename = "changes")]
    Changes(ChangesReply),
}

impl Reply {
    /// Blocking verify verdict, if this is a verify reply.
    #[must_use]
    pub fn verify_exit_code(&self) -> Option<i32> {
        match self {
            Self::Verify(reply) => Some(reply.exit_code()),
            _ => None,
        }
    }
}

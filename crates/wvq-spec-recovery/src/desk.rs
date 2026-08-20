//! The reviewed recovery workflow, in one place.
//!
//! CLI, MCP and Studio all drive this desk rather than re-implementing the
//! order of operations. The order is fixed by spec §65–§66: recover, check,
//! review, escalate if needed, and only then seal.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;
use wvq_domain::{ContentHash, HumanDecision};

use crate::cluster::CapabilityCluster;
use crate::narrative::ChangeNarrative;
use crate::packet::{
    PublicSurfaceDelta, Questions, RecoveryPacket, TestIntentSummary, packet, preview_patch,
    questions,
};
use crate::review::{CandidateReview, CandidateState, ReviewError, SealApproval};
use crate::verify::{CandidateRequirement, VerifierReport, VerifyContext, verify_candidates};

/// Everything needed to open a recovery desk for one change.
///
/// There is deliberately no `Default`: a narrative without revisions would be a
/// recovery with no provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryInput {
    /// Deterministic narrative.
    pub narrative: ChangeNarrative,
    /// Capability clusters with commit provenance.
    pub clusters: Vec<CapabilityCluster>,
    /// Added and removed public surfaces.
    pub surface_delta: PublicSurfaceDelta,
    /// What the changed tests appear to assert.
    pub test_intent: Vec<TestIntentSummary>,
    /// Candidate requirements drafted from the packet.
    pub candidates: Vec<CandidateRequirement>,
    /// Context for the deterministic checks.
    pub context: VerifyContext,
}

/// One candidate as a reviewer sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateView {
    /// Candidate identity.
    pub id: String,
    /// Proposed sentence.
    pub text: String,
    /// Where it is in the mandatory verification path.
    pub state: &'static str,
    /// Whether product approval is mandatory.
    pub requires_product_approval: bool,
    /// Digest the reviewer must decide against.
    pub digest: String,
    /// What the deterministic checks objected to.
    pub findings: Vec<String>,
    /// Evidence, strongest tier first.
    pub evidence: Vec<String>,
}

/// The whole review screen for one change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewSnapshot {
    /// Change under recovery.
    pub change: String,
    /// Candidates and their states.
    pub candidates: Vec<CandidateView>,
    /// Whether anything still blocks review.
    pub blocked: bool,
    /// Whether anything is escalated to product.
    pub needs_product: bool,
}

/// Recovery workflow state for one change.
#[derive(Debug, Clone)]
pub struct RecoveryDesk {
    change: String,
    candidates: Vec<CandidateRequirement>,
    report: VerifierReport,
    reviews: BTreeMap<String, CandidateReview>,
    packet: Option<RecoveryPacket>,
}

impl RecoveryDesk {
    /// Open an empty desk for `change`.
    #[must_use]
    pub fn new(change: impl Into<String>) -> Self {
        Self {
            change: change.into(),
            candidates: Vec::new(),
            report: VerifierReport {
                findings: Vec::new(),
                confidence: BTreeMap::new(),
            },
            reviews: BTreeMap::new(),
            packet: None,
        }
    }

    /// Build the packet, run the deterministic checks, and open a review per
    /// candidate. Candidates the checks still object to stay short of
    /// `QA_REVIEW`, so nobody is asked to review known-broken wording.
    pub fn recover(&mut self, input: RecoveryInput) -> &RecoveryPacket {
        let neighbours = input.context.existing_requirements.clone();
        self.report = verify_candidates(&input.candidates, &input.context);
        self.candidates = input.candidates;
        self.reviews = self
            .candidates
            .iter()
            .map(|candidate| {
                let mut review =
                    CandidateReview::new(&candidate.id, candidate_digest(&candidate.text));
                let _ = review.propose();
                if review.auto_check(&self.report).is_ok() {
                    let _ = review.submit_for_review();
                }
                (candidate.id.clone(), review)
            })
            .collect();
        &*self.packet.insert(packet(
            input.narrative,
            input.clusters,
            input.surface_delta,
            input.test_intent,
            neighbours,
        ))
    }

    /// The packet an agent receives instead of the repository.
    #[must_use]
    pub fn packet(&self) -> Option<&RecoveryPacket> {
        self.packet.as_ref()
    }

    /// The review screen.
    #[must_use]
    pub fn review(&self) -> ReviewSnapshot {
        let questions = self.questions();
        let candidates = self
            .candidates
            .iter()
            .map(|candidate| {
                let review = &self.reviews[&candidate.id];
                CandidateView {
                    id: candidate.id.clone(),
                    text: candidate.text.clone(),
                    state: review.state().as_str(),
                    requires_product_approval: review.requires_product_approval(),
                    digest: review.digest().to_string(),
                    findings: self
                        .report
                        .findings
                        .iter()
                        .filter(|item| item.candidate == candidate.id)
                        .map(|item| format!("{}: {}", item.kind.as_str(), item.detail))
                        .collect(),
                    evidence: candidate
                        .evidence
                        .iter()
                        .map(|item| {
                            format!(
                                "[{}] {} ({})",
                                item.tier().as_str(),
                                item.text,
                                item.provenance
                            )
                        })
                        .collect(),
                }
            })
            .collect();
        ReviewSnapshot {
            change: self.change.clone(),
            candidates,
            blocked: self.report.blocks_review(),
            needs_product: questions.needs_product(),
        }
    }

    /// Adaptive questions for QA and product.
    #[must_use]
    pub fn questions(&self) -> Questions {
        questions(&self.candidates, &self.report)
    }

    /// The `OpenSpec` patch a reviewer would be offered. Always a proposal.
    #[must_use]
    pub fn preview_patch(&self) -> String {
        preview_patch(&self.change, &self.candidates, &self.questions())
    }

    /// Record one human decision against one candidate.
    ///
    /// # Errors
    ///
    /// Propagates every refusal of the mandatory verification path, including an
    /// unknown candidate, a stale digest, and the wrong reviewer role.
    pub fn decide(&mut self, decision: &HumanDecision) -> Result<CandidateState, ReviewError> {
        let review = self.reviews.get_mut(&decision.subject).ok_or_else(|| {
            ReviewError::SubjectMismatch {
                subject: decision.subject.clone(),
                candidate: self.change.clone(),
            }
        })?;
        if review.state() == CandidateState::ProductDecisionRequired {
            review.record_product(decision)?;
        } else {
            review.record_qa(decision)?;
        }
        Ok(review.state())
    }

    /// Seal one candidate once every mandatory approval is present.
    ///
    /// # Errors
    ///
    /// Propagates [`ReviewError`]; in particular a candidate that no human
    /// verified can never reach a seal.
    pub fn seal(&mut self, candidate: &str) -> Result<SealApproval, ReviewError> {
        let review =
            self.reviews
                .get_mut(candidate)
                .ok_or_else(|| ReviewError::SubjectMismatch {
                    subject: candidate.to_owned(),
                    candidate: self.change.clone(),
                })?;
        review.mark_seal_eligible()?;
        review.seal()
    }

    /// State of one candidate, if the desk knows it.
    #[must_use]
    pub fn state_of(&self, candidate: &str) -> Option<CandidateState> {
        self.reviews.get(candidate).map(CandidateReview::state)
    }
}

/// Digest of the exact sentence a reviewer sees, so an edit invalidates approval.
fn candidate_digest(text: &str) -> ContentHash {
    let mut hex = String::with_capacity(64);
    for byte in text.bytes().take(32) {
        let _ = write!(hex, "{byte:02x}");
    }
    // A short candidate still needs a fixed-width even-length hex digest.
    while hex.len() < 64 {
        hex.push('0');
    }
    ContentHash::new(hex).unwrap_or_else(|_| unreachable!("bytes render as lowercase hex pairs"))
}

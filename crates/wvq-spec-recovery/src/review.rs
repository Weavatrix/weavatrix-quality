//! Spec §66 mandatory QA verification.
//!
//! A requirement recovered from implementation is never normative until a human
//! says so. The state machine below is the only path to `SEAL_ELIGIBLE`, and it
//! refuses every shortcut: no auto-seal, no `OBSERVED_ONLY` promotion, no
//! sealing over an unresolved escalation, and no decision that was made against
//! a candidate the reviewer can no longer have seen.

use serde::Serialize;
use thiserror::Error;
use wvq_domain::{ContentHash, HumanDecision, HumanDecisionId, HumanRole, VerificationDecision};

use crate::evidence::Confidence;
use crate::verify::VerifierReport;

/// Spec §66 candidate lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateState {
    /// Extracted from implementation evidence.
    Recovered,
    /// Written up as a candidate requirement.
    Proposed,
    /// Deterministic checks passed.
    AutoChecked,
    /// Waiting for a human.
    QaReview,
    /// A quality analyst confirmed the intent.
    QaVerified,
    /// QA escalated; only product can answer.
    ProductDecisionRequired,
    /// Product answered the escalation.
    ProductApproved,
    /// Everything required is present.
    SealEligible,
    /// Sealed into an `OracleSeal`.
    Sealed,
    /// A human rejected the candidate. Terminal.
    Rejected,
    /// Behaviour is observed but intent is unconfirmed. Never normative.
    ObservedOnly,
}

impl CandidateState {
    /// Stable token for transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recovered => "RECOVERED",
            Self::Proposed => "PROPOSED",
            Self::AutoChecked => "AUTO_CHECKED",
            Self::QaReview => "QA_REVIEW",
            Self::QaVerified => "QA_VERIFIED",
            Self::ProductDecisionRequired => "PRODUCT_DECISION_REQUIRED",
            Self::ProductApproved => "PRODUCT_APPROVED",
            Self::SealEligible => "SEAL_ELIGIBLE",
            Self::Sealed => "SEALED",
            Self::Rejected => "REJECTED",
            Self::ObservedOnly => "OBSERVED_ONLY",
        }
    }

    /// Whether this state may be used as a normative oracle.
    ///
    /// Only a sealed candidate may. In particular `ObservedOnly` may become
    /// baseline evidence but never a normative expectation.
    #[must_use]
    pub fn is_normative(self) -> bool {
        self == Self::Sealed
    }
}

/// Why a transition was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReviewError {
    /// The transition is not legal from the current state.
    #[error("cannot {action} from {}", .from.as_str())]
    IllegalTransition {
        /// Current state.
        from: CandidateState,
        /// Attempted action.
        action: &'static str,
    },
    /// Deterministic checks still object.
    #[error("deterministic checks still block review")]
    ChecksBlockReview,
    /// The candidate was never verified by QA.
    #[error("a recovered requirement cannot seal without QA verification")]
    QaVerificationMissing,
    /// QA escalated and product has not answered.
    #[error("product decision is required before sealing")]
    ProductApprovalRequired,
    /// The reviewer saw a different version of the candidate.
    #[error("decision was made against digest {seen}, candidate is now {current}")]
    DigestChanged {
        /// Digest the decision carries.
        seen: String,
        /// Digest the candidate has now.
        current: String,
    },
    /// A decision was recorded against another candidate.
    #[error("decision subject `{subject}` does not match candidate `{candidate}`")]
    SubjectMismatch {
        /// Subject on the decision.
        subject: String,
        /// Candidate under review.
        candidate: String,
    },
    /// Wrong role for this step.
    #[error("{step} needs a {expected} reviewer, got {got}")]
    WrongRole {
        /// Which step refused.
        step: &'static str,
        /// Role required.
        expected: &'static str,
        /// Role supplied.
        got: &'static str,
    },
}

/// Everything needed to seal, per spec §66.5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SealApproval {
    /// Mandatory QA verification.
    pub qa_verification: HumanDecisionId,
    /// Product approval, present whenever the escalation rules demanded it.
    pub product_approval: Option<HumanDecisionId>,
}

/// One candidate moving through mandatory verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateReview {
    candidate: String,
    state: CandidateState,
    digest: ContentHash,
    qa: Option<HumanDecisionId>,
    product: Option<HumanDecisionId>,
    requires_product: bool,
}

impl CandidateReview {
    /// Start at `RECOVERED` with the digest of what will be shown to a reviewer.
    #[must_use]
    pub fn new(candidate: impl Into<String>, digest: ContentHash) -> Self {
        Self {
            candidate: candidate.into(),
            state: CandidateState::Recovered,
            digest,
            qa: None,
            product: None,
            requires_product: false,
        }
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> CandidateState {
        self.state
    }

    /// Digest of the artifact a reviewer would see right now.
    #[must_use]
    pub fn digest(&self) -> &ContentHash {
        &self.digest
    }

    /// Whether product approval is mandatory for this candidate.
    #[must_use]
    pub fn requires_product_approval(&self) -> bool {
        self.requires_product
    }

    /// Force product approval, e.g. when a sealed expectation is being changed.
    pub fn require_product_approval(&mut self) {
        self.requires_product = true;
    }

    /// `RECOVERED` → `PROPOSED`.
    ///
    /// # Errors
    ///
    /// [`ReviewError::IllegalTransition`] from any other state.
    pub fn propose(&mut self) -> Result<(), ReviewError> {
        self.expect(CandidateState::Recovered, "propose")?;
        self.state = CandidateState::Proposed;
        Ok(())
    }

    /// `PROPOSED` → `AUTO_CHECKED`, refusing candidates the checks still object to.
    ///
    /// A weak oracle does not block the transition but does make product
    /// approval mandatory: a normative expectation reconstructed only from the
    /// implementation is exactly the case spec §66.5 escalates.
    ///
    /// # Errors
    ///
    /// [`ReviewError::ChecksBlockReview`] when a blocking finding remains.
    pub fn auto_check(&mut self, report: &VerifierReport) -> Result<(), ReviewError> {
        self.expect(CandidateState::Proposed, "auto-check")?;
        if report.blocks_review() {
            return Err(ReviewError::ChecksBlockReview);
        }
        if report
            .confidence
            .get(&self.candidate)
            .is_some_and(Confidence::oracle_independence_is_weak)
        {
            self.requires_product = true;
        }
        self.state = CandidateState::AutoChecked;
        Ok(())
    }

    /// `AUTO_CHECKED` → `QA_REVIEW`.
    ///
    /// # Errors
    ///
    /// [`ReviewError::IllegalTransition`] from any other state.
    pub fn submit_for_review(&mut self) -> Result<(), ReviewError> {
        self.expect(CandidateState::AutoChecked, "submit for review")?;
        self.state = CandidateState::QaReview;
        Ok(())
    }

    /// Record the QA decision.
    ///
    /// # Errors
    ///
    /// Refuses a stale digest, a decision about another subject, a non-QA
    /// reviewer, or a call from the wrong state.
    pub fn record_qa(&mut self, decision: &HumanDecision) -> Result<(), ReviewError> {
        self.expect(CandidateState::QaReview, "record a QA decision")?;
        self.check_decision(decision, "QA review", HumanRole::Qa)?;
        self.qa = Some(decision.id.clone());
        self.state = match decision.decision {
            VerificationDecision::ObservedOnly => CandidateState::ObservedOnly,
            VerificationDecision::Reject
            | VerificationDecision::MarkDuplicate
            | VerificationDecision::MarkNonBehavioral => CandidateState::Rejected,
            VerificationDecision::RequestProductDecision
            | VerificationDecision::RequestDeveloperClarification => {
                self.requires_product = true;
                CandidateState::ProductDecisionRequired
            }
            VerificationDecision::AcceptAsIntended
            | VerificationDecision::Edit
            | VerificationDecision::AddScenario => CandidateState::QaVerified,
        };
        Ok(())
    }

    /// Record the product answer to an escalation.
    ///
    /// # Errors
    ///
    /// Refuses a stale digest, the wrong subject, a non-product reviewer, or a
    /// call outside `PRODUCT_DECISION_REQUIRED`.
    pub fn record_product(&mut self, decision: &HumanDecision) -> Result<(), ReviewError> {
        self.expect(
            CandidateState::ProductDecisionRequired,
            "record a product decision",
        )?;
        self.check_decision(decision, "product approval", HumanRole::Product)?;
        self.product = Some(decision.id.clone());
        self.state = if decision.decision.seal_eligible() {
            CandidateState::ProductApproved
        } else {
            CandidateState::Rejected
        };
        Ok(())
    }

    /// Replace the candidate text, invalidating every decision taken so far.
    ///
    /// An edited candidate is a different artifact, so review restarts and any
    /// decision still carrying the old digest is refused.
    pub fn edit(&mut self, digest: ContentHash) {
        self.digest = digest;
        self.qa = None;
        self.product = None;
        self.state = CandidateState::Proposed;
    }

    /// Move to `SEAL_ELIGIBLE` once every mandatory approval is present.
    ///
    /// # Errors
    ///
    /// [`ReviewError::QaVerificationMissing`] without QA,
    /// [`ReviewError::ProductApprovalRequired`] with an unresolved escalation.
    pub fn mark_seal_eligible(&mut self) -> Result<(), ReviewError> {
        if self.qa.is_none() {
            return Err(ReviewError::QaVerificationMissing);
        }
        match self.state {
            CandidateState::QaVerified if !self.requires_product => {}
            CandidateState::ProductApproved => {}
            CandidateState::QaVerified | CandidateState::ProductDecisionRequired => {
                return Err(ReviewError::ProductApprovalRequired);
            }
            CandidateState::ObservedOnly | CandidateState::Rejected => {
                return Err(ReviewError::IllegalTransition {
                    from: self.state,
                    action: "seal",
                });
            }
            other => {
                return Err(ReviewError::IllegalTransition {
                    from: other,
                    action: "become seal-eligible",
                });
            }
        }
        self.state = CandidateState::SealEligible;
        Ok(())
    }

    /// Seal, returning the approvals that authorised it.
    ///
    /// # Errors
    ///
    /// [`ReviewError::IllegalTransition`] unless the candidate is seal-eligible.
    pub fn seal(&mut self) -> Result<SealApproval, ReviewError> {
        self.expect(CandidateState::SealEligible, "seal")?;
        let qa_verification = self.qa.clone().ok_or(ReviewError::QaVerificationMissing)?;
        if self.requires_product && self.product.is_none() {
            return Err(ReviewError::ProductApprovalRequired);
        }
        self.state = CandidateState::Sealed;
        Ok(SealApproval {
            qa_verification,
            product_approval: self.product.clone(),
        })
    }

    fn expect(&self, want: CandidateState, action: &'static str) -> Result<(), ReviewError> {
        if self.state == want {
            Ok(())
        } else {
            Err(ReviewError::IllegalTransition {
                from: self.state,
                action,
            })
        }
    }

    fn check_decision(
        &self,
        decision: &HumanDecision,
        step: &'static str,
        expected: HumanRole,
    ) -> Result<(), ReviewError> {
        if decision.subject != self.candidate {
            return Err(ReviewError::SubjectMismatch {
                subject: decision.subject.clone(),
                candidate: self.candidate.clone(),
            });
        }
        if decision.artifact_digest != self.digest {
            return Err(ReviewError::DigestChanged {
                seen: decision.artifact_digest.to_string(),
                current: self.digest.to_string(),
            });
        }
        if decision.role != expected {
            return Err(ReviewError::WrongRole {
                step,
                expected: expected.as_str(),
                got: decision.role.as_str(),
            });
        }
        Ok(())
    }
}

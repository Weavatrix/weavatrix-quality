//! Provenance-bearing human verification. Spec §66.2 and §66.5.
//!
//! A decision always names exactly one subject. There is deliberately no
//! representation for an implicit "accept all": bulk approval cannot mutate a
//! baseline or a sealed expectation by accident.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::{ContentHash, HumanDecisionId};

/// Why a decision was rejected before it could be recorded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecisionError {
    /// Subject was a wildcard, a list, or empty.
    #[error("human decision needs one concrete subject; bulk accept-all is refused")]
    BulkSubject,
    /// Reviewer identity was missing.
    #[error("human decision needs a reviewer identity")]
    MissingReviewer,
}

/// Who reviewed. Product approval is a distinct role, not a stronger QA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanRole {
    /// Quality analyst.
    Qa,
    /// Product owner. Required to resolve an escalation.
    Product,
    /// Implementing developer. Clarifies, never approves intent alone.
    Developer,
}

impl HumanRole {
    /// Stable token for storage and transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Qa => "qa",
            Self::Product => "product",
            Self::Developer => "developer",
        }
    }
}

/// Spec §66.2 QA actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationDecision {
    /// Behaviour is confirmed intended.
    AcceptAsIntended,
    /// Reviewer rewrote the candidate.
    Edit,
    /// Candidate is wrong.
    Reject,
    /// Behaviour is observed but intent is unconfirmed.
    ObservedOnly,
    /// Reviewer added a missing case.
    AddScenario,
    /// Duplicate of an existing requirement.
    MarkDuplicate,
    /// Refactor or other non-behavioural change.
    MarkNonBehavioral,
    /// Escalated to product.
    RequestProductDecision,
    /// Escalated to the developer.
    RequestDeveloperClarification,
}

impl VerificationDecision {
    /// Stable token for storage and transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AcceptAsIntended => "accept_as_intended",
            Self::Edit => "edit",
            Self::Reject => "reject",
            Self::ObservedOnly => "observed_only",
            Self::AddScenario => "add_scenario",
            Self::MarkDuplicate => "mark_duplicate",
            Self::MarkNonBehavioral => "mark_non_behavioral",
            Self::RequestProductDecision => "request_product_decision",
            Self::RequestDeveloperClarification => "request_developer_clarification",
        }
    }

    /// Whether this decision may carry a candidate towards `SEAL_ELIGIBLE`.
    ///
    /// `ObservedOnly` may become baseline evidence but never a normative oracle,
    /// and both escalations block sealing until the escalation is resolved.
    #[must_use]
    pub fn seal_eligible(self) -> bool {
        matches!(
            self,
            Self::AcceptAsIntended | Self::Edit | Self::AddScenario
        )
    }

    /// Whether this decision blocks sealing until someone else answers.
    #[must_use]
    pub fn escalates(self) -> bool {
        matches!(
            self,
            Self::RequestProductDecision | Self::RequestDeveloperClarification
        )
    }
}

/// One recorded human verification. Spec §66.5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecision {
    /// Decision identity.
    pub id: HumanDecisionId,
    /// Reviewer identity, as supplied by the host.
    pub reviewer: String,
    /// Reviewer role.
    pub role: HumanRole,
    /// Exactly one requirement, obligation, proof, or finding.
    pub subject: String,
    /// Digest of the artifact the reviewer actually saw.
    pub artifact_digest: ContentHash,
    /// Chosen action.
    pub decision: VerificationDecision,
    /// Optional reviewer comment.
    pub comment: Option<String>,
    /// Host-supplied timestamp. WVQ does not invent a clock here.
    pub decided_at: String,
}

/// Unvalidated fields for [`HumanDecision::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDecision {
    /// Decision identity.
    pub id: HumanDecisionId,
    /// Reviewer identity.
    pub reviewer: String,
    /// Reviewer role.
    pub role: HumanRole,
    /// The one thing being reviewed.
    pub subject: String,
    /// Digest of the artifact the reviewer saw.
    pub artifact_digest: ContentHash,
    /// Chosen action.
    pub decision: VerificationDecision,
    /// Optional comment.
    pub comment: Option<String>,
    /// Host-supplied timestamp.
    pub decided_at: String,
}

impl HumanDecision {
    /// Build a decision, refusing bulk subjects and anonymous reviewers.
    ///
    /// # Errors
    ///
    /// Returns [`DecisionError::BulkSubject`] when the subject is empty, a
    /// wildcard, or a list, and [`DecisionError::MissingReviewer`] when the
    /// reviewer is blank.
    pub fn new(input: NewDecision) -> Result<Self, DecisionError> {
        if input.reviewer.trim().is_empty() {
            return Err(DecisionError::MissingReviewer);
        }
        if is_bulk_subject(&input.subject) {
            return Err(DecisionError::BulkSubject);
        }
        Ok(Self {
            id: input.id,
            reviewer: input.reviewer,
            role: input.role,
            subject: input.subject,
            artifact_digest: input.artifact_digest,
            decision: input.decision,
            comment: input.comment,
            decided_at: input.decided_at,
        })
    }
}

/// Whether a subject would approve more than one thing at once.
fn is_bulk_subject(subject: &str) -> bool {
    let trimmed = subject.trim();
    trimmed.is_empty()
        || trimmed.contains(',')
        || trimmed.contains(char::is_whitespace)
        || matches!(trimmed, "*" | "all" | "ALL" | "any")
}

//! Spec §72–§73 protection snapshots.
//!
//! Before head is judged, WVQ records what actually protected the affected base
//! surface: which tests ran it, which code they executed, which obligations they
//! proved, and when the last passing proof was. Every entry is revision-bound —
//! evidence that cannot name its revision is refused rather than assumed good.

use serde::Serialize;
use thiserror::Error;
use wvq_domain::RevisionId;

/// Why protection evidence was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtectionError {
    /// Evidence belongs to a different revision than the snapshot.
    #[error("evidence for revision `{found}` cannot enter a `{expected}` snapshot")]
    RevisionMismatch {
        /// Revision the snapshot is for.
        expected: String,
        /// Revision the evidence carries.
        found: String,
    },
    /// Evidence carries no revision at all.
    #[error("protection evidence for flow `{flow}` is not revision-bound")]
    NotRevisionBound {
        /// Flow the evidence claimed to cover.
        flow: String,
    },
    /// Stored proof is too old to be trusted by policy.
    #[error("stored proof is {age} revisions old, policy allows {allowed}")]
    TooOld {
        /// How old the proof is.
        age: u32,
        /// What policy permits.
        allowed: u32,
    },
    /// The environment the proof ran in no longer matches.
    #[error("stored proof ran under environment `{found}`, current is `{expected}`")]
    EnvironmentDrift {
        /// Current environment fingerprint.
        expected: String,
        /// Fingerprint the proof carries.
        found: String,
    },
}

/// What protected one flow at one revision. Spec §72.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlowProtection {
    /// Flow identity.
    pub flow: String,
    /// Revision this measurement belongs to.
    pub revision: String,
    /// Tests that measurably executed the flow.
    pub tests: Vec<String>,
    /// Recorded manual sessions that reached it.
    pub sessions: Vec<String>,
    /// Graph nodes actually executed.
    pub covered_nodes: Vec<String>,
    /// Branches actually executed. Critical branches live here.
    pub covered_branches: Vec<String>,
    /// Obligations proved through this flow.
    pub proven_obligations: Vec<String>,
    /// Last successful proofs.
    pub proofs: Vec<String>,
}

impl FlowProtection {
    /// Whether anything at all protected this flow.
    #[must_use]
    pub fn is_protected(&self) -> bool {
        !self.tests.is_empty() || !self.sessions.is_empty()
    }

    /// Whether a specific branch was measurably executed.
    #[must_use]
    pub fn covers_branch(&self, branch: &str) -> bool {
        self.covered_branches.iter().any(|item| item == branch)
    }
}

/// Everything that protected the affected surface at one revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtectionSnapshot {
    /// Revision this snapshot describes.
    pub revision: String,
    /// Per-flow protection, sorted by flow.
    pub flows: Vec<FlowProtection>,
}

impl ProtectionSnapshot {
    /// Protection recorded for one flow.
    #[must_use]
    pub fn flow(&self, flow: &str) -> Option<&FlowProtection> {
        self.flows.iter().find(|item| item.flow == flow)
    }

    /// Flows with no protection at all.
    #[must_use]
    pub fn unprotected(&self) -> Vec<&str> {
        self.flows
            .iter()
            .filter(|item| !item.is_protected())
            .map(|item| item.flow.as_str())
            .collect()
    }
}

/// Build a snapshot, refusing evidence that is not bound to `revision`.
///
/// # Errors
///
/// [`ProtectionError::NotRevisionBound`] for evidence with no revision, and
/// [`ProtectionError::RevisionMismatch`] for evidence from another revision.
/// Missing evidence is never silently treated as "protected".
pub fn snapshot(
    revision: &RevisionId,
    flows: Vec<FlowProtection>,
) -> Result<ProtectionSnapshot, ProtectionError> {
    let mut checked = Vec::with_capacity(flows.len());
    for flow in flows {
        if flow.revision.is_empty() {
            return Err(ProtectionError::NotRevisionBound { flow: flow.flow });
        }
        if flow.revision != revision.as_str() {
            return Err(ProtectionError::RevisionMismatch {
                expected: revision.as_str().to_owned(),
                found: flow.revision,
            });
        }
        checked.push(flow);
    }
    checked.sort_by(|left, right| left.flow.cmp(&right.flow));
    Ok(ProtectionSnapshot {
        revision: revision.as_str().to_owned(),
        flows: checked,
    })
}

/// A proof recorded at some earlier revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalProof {
    /// Proof identity.
    pub id: String,
    /// How many revisions ago it was produced.
    pub age_revisions: u32,
    /// Environment and dependency fingerprint it ran under.
    pub environment: String,
    /// Program or test identity it belongs to.
    pub program: String,
}

/// When stored base evidence may stand in for a fresh run. Spec §81.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReusePolicy {
    /// Oldest proof that may still be trusted.
    pub max_age_revisions: u32,
    /// Environment fingerprint the current run uses.
    pub environment: String,
}

/// Whether a stored base proof can be reused instead of re-running the test.
///
/// This is what keeps CI affordable without weakening the comparison: reuse is
/// allowed only while the proof is recent enough and ran under a compatible
/// environment.
///
/// # Errors
///
/// [`ProtectionError::TooOld`] past the policy window, and
/// [`ProtectionError::EnvironmentDrift`] when the fingerprint no longer matches.
pub fn may_reuse(proof: &HistoricalProof, policy: &ReusePolicy) -> Result<(), ProtectionError> {
    if proof.age_revisions > policy.max_age_revisions {
        return Err(ProtectionError::TooOld {
            age: proof.age_revisions,
            allowed: policy.max_age_revisions,
        });
    }
    if proof.environment != policy.environment {
        return Err(ProtectionError::EnvironmentDrift {
            expected: policy.environment.clone(),
            found: proof.environment.clone(),
        });
    }
    Ok(())
}

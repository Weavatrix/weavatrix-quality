//! Five-way proof verdict. Missing evidence is never a contradiction.

use wvq_spec::EvidenceKind;

/// Spec §27 verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofVerdict {
    /// Required execution passed with all required evidence present.
    Proven,
    /// Sealed expectation was violated by measured behavior.
    Contradicted,
    /// Some required evidence/execution exists, but not enough for Proven.
    Partial,
    /// Required runtime evidence was not collected. Not a failure.
    Unproven,
    /// Spec/oracle is ambiguous; a human must decide.
    HumanRequired,
}

impl ProofVerdict {
    /// Wire / SQL token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proven => "PROVEN",
            Self::Contradicted => "CONTRADICTED",
            Self::Partial => "PARTIAL",
            Self::Unproven => "UNPROVEN",
            Self::HumanRequired => "HUMAN_REQUIRED",
        }
    }
}

/// Inputs that determine a verdict. Quality debt is intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictInput {
    /// Required evidence kinds from the sealed obligation.
    pub required_evidence: Vec<EvidenceKind>,
    /// Evidence kinds actually collected on this run.
    pub present_evidence: Vec<EvidenceKind>,
    /// Execution reached a passing assertion.
    pub execution_passed: bool,
    /// Measured behavior contradicts the `OracleSeal`.
    pub seal_contradicted: bool,
    /// Spec text is ambiguous (`HUMAN_REQUIRED`).
    pub spec_ambiguous: bool,
}

/// Decide a verdict. Contradiction wins over ambiguity; missing evidence is Unproven.
#[must_use]
pub fn decide_verdict(input: &VerdictInput) -> ProofVerdict {
    if input.seal_contradicted {
        return ProofVerdict::Contradicted;
    }
    if input.spec_ambiguous {
        return ProofVerdict::HumanRequired;
    }
    let missing = missing_required(input);
    if missing {
        if input.execution_passed && !input.present_evidence.is_empty() {
            return ProofVerdict::Partial;
        }
        return ProofVerdict::Unproven;
    }
    if input.execution_passed {
        ProofVerdict::Proven
    } else if input.present_evidence.is_empty() {
        ProofVerdict::Unproven
    } else {
        ProofVerdict::Partial
    }
}

fn missing_required(input: &VerdictInput) -> bool {
    input
        .required_evidence
        .iter()
        .any(|need| !input.present_evidence.contains(need))
}

//! Spec §65.2 evidence authority hierarchy.
//!
//! The safety rule of the whole subsystem is that implementation evidence can
//! *propose* intent but can never *establish* it. Only declared product intent
//! (Tier A) is normative, so the four confidence axes stay separate and are
//! never flattened into a single percentage.

use serde::Serialize;

/// Authority tier. `A` outranks `B`, which outranks `C`, which outranks `D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTier {
    /// Tier D — weak naming hints. Never normative on their own.
    WeakHint,
    /// Tier C — observed implementation and runtime behaviour.
    Implementation,
    /// Tier B — reviewed collaboration evidence.
    ReviewedCollaboration,
    /// Tier A — declared product intent.
    DeclaredIntent,
}

impl EvidenceTier {
    /// Stable token for transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeclaredIntent => "A",
            Self::ReviewedCollaboration => "B",
            Self::Implementation => "C",
            Self::WeakHint => "D",
        }
    }

    /// Whether evidence at this tier can establish intent by itself.
    ///
    /// Only Tier A can. Everything below proposes a candidate that still needs
    /// human verification.
    #[must_use]
    pub fn establishes_intent(self) -> bool {
        self == Self::DeclaredIntent
    }
}

/// Where one piece of evidence came from. The tier follows from the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// Existing `OpenSpec` requirement.
    ExistingOpenSpec,
    /// Approved linked ticket.
    ApprovedTicket,
    /// Explicit acceptance criterion.
    AcceptanceCriterion,
    /// Approved product or design decision.
    ProductDecision,
    /// Pull-request description.
    PullRequestBody,
    /// Approved review discussion.
    ReviewDiscussion,
    /// Commit body.
    CommitBody,
    /// Release note.
    ReleaseNote,
    /// Approved QA plan.
    QaPlan,
    /// Changed code.
    CodeDelta,
    /// Added, removed, or changed endpoint.
    ChangedEndpoint,
    /// Changed UI component.
    ChangedComponent,
    /// Changed permission path.
    ChangedPermission,
    /// Changed configuration.
    ChangedConfig,
    /// Changed data model.
    ChangedDataModel,
    /// Changed or added test.
    ChangedTest,
    /// Observed runtime behaviour difference.
    BehaviorDelta,
    /// Commit title.
    CommitTitle,
    /// Branch name.
    BranchName,
    /// File name.
    FileName,
    /// Symbol name.
    SymbolName,
    /// Source comment.
    Comment,
}

impl EvidenceSource {
    /// Authority tier of this source.
    #[must_use]
    pub fn tier(self) -> EvidenceTier {
        match self {
            Self::ExistingOpenSpec
            | Self::ApprovedTicket
            | Self::AcceptanceCriterion
            | Self::ProductDecision => EvidenceTier::DeclaredIntent,
            Self::PullRequestBody
            | Self::ReviewDiscussion
            | Self::CommitBody
            | Self::ReleaseNote
            | Self::QaPlan => EvidenceTier::ReviewedCollaboration,
            Self::CodeDelta
            | Self::ChangedEndpoint
            | Self::ChangedComponent
            | Self::ChangedPermission
            | Self::ChangedConfig
            | Self::ChangedDataModel
            | Self::ChangedTest
            | Self::BehaviorDelta => EvidenceTier::Implementation,
            Self::CommitTitle
            | Self::BranchName
            | Self::FileName
            | Self::SymbolName
            | Self::Comment => EvidenceTier::WeakHint,
        }
    }

    /// Whether this source observes runtime behaviour rather than source shape.
    #[must_use]
    pub fn is_behavioral(self) -> bool {
        self == Self::BehaviorDelta
    }
}

/// One recorded piece of evidence with its provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntentEvidence {
    /// Where it came from.
    pub source: EvidenceSource,
    /// Verbatim snippet. Never rewritten into a normative sentence here.
    pub text: String,
    /// Exact origin (`openspec/...`, `commit abc123`, `src/sankey.ts:40`).
    pub provenance: String,
}

impl IntentEvidence {
    /// Record one piece of evidence.
    #[must_use]
    pub fn new(
        source: EvidenceSource,
        text: impl Into<String>,
        provenance: impl Into<String>,
    ) -> Self {
        Self {
            source,
            text: text.into(),
            provenance: provenance.into(),
        }
    }

    /// Authority tier.
    #[must_use]
    pub fn tier(&self) -> EvidenceTier {
        self.source.tier()
    }

    /// Whether this single item can establish intent.
    #[must_use]
    pub fn establishes_intent(&self) -> bool {
        self.tier().establishes_intent()
    }
}

/// How much support one axis has. Deliberately coarse and non-numeric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    /// Nothing supports this axis.
    None,
    /// Only weak or indirect support.
    Weak,
    /// Reviewed but not declared support.
    Medium,
    /// Declared or directly measured support.
    Strong,
}

/// Spec §65.2 confidence. Four independent axes, never one percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Confidence {
    /// How well intent itself is declared.
    pub intent_evidence: ConfidenceLevel,
    /// How strongly the implementation was observed.
    pub implementation_evidence: ConfidenceLevel,
    /// How strongly runtime behaviour was observed.
    pub behavioral_observation: ConfidenceLevel,
    /// How independent the expectation is from the implementation under test.
    pub oracle_independence: ConfidenceLevel,
}

impl Confidence {
    /// Whether the expectation is only supported by the code it would verify.
    ///
    /// Spec §66.4 requires this to be displayed prominently.
    #[must_use]
    pub fn oracle_independence_is_weak(&self) -> bool {
        self.oracle_independence <= ConfidenceLevel::Weak
    }
}

/// Strongest tier present, if any.
#[must_use]
pub fn strongest_tier(evidence: &[IntentEvidence]) -> Option<EvidenceTier> {
    evidence.iter().map(IntentEvidence::tier).max()
}

/// Whether any item can establish intent on its own.
///
/// A commit title, a branch name, or an implementation diff never can, no
/// matter how many of them are present.
#[must_use]
pub fn establishes_intent(evidence: &[IntentEvidence]) -> bool {
    evidence.iter().any(IntentEvidence::establishes_intent)
}

/// Assess the four confidence axes for one candidate.
#[must_use]
pub fn assess(evidence: &[IntentEvidence]) -> Confidence {
    let has = |tier: EvidenceTier| evidence.iter().any(|item| item.tier() == tier);
    let declared = has(EvidenceTier::DeclaredIntent);
    let reviewed = has(EvidenceTier::ReviewedCollaboration);
    let implementation = has(EvidenceTier::Implementation);
    let hints = has(EvidenceTier::WeakHint);
    let behavioral = evidence.iter().any(|item| item.source.is_behavioral());

    let intent_evidence = if declared {
        ConfidenceLevel::Strong
    } else if reviewed {
        ConfidenceLevel::Medium
    } else if implementation || hints {
        ConfidenceLevel::Weak
    } else {
        ConfidenceLevel::None
    };

    let implementation_evidence = if implementation {
        ConfidenceLevel::Strong
    } else if hints {
        ConfidenceLevel::Weak
    } else {
        ConfidenceLevel::None
    };

    let behavioral_observation = if behavioral {
        ConfidenceLevel::Strong
    } else {
        ConfidenceLevel::None
    };

    // The expectation is independent only when something outside the changed
    // implementation asserts it. Implementation plus its own changed test is the
    // classic coding-before-testing bias and stays weak.
    let oracle_independence = if declared {
        ConfidenceLevel::Strong
    } else if reviewed {
        ConfidenceLevel::Medium
    } else if implementation || hints {
        ConfidenceLevel::Weak
    } else {
        ConfidenceLevel::None
    };

    Confidence {
        intent_evidence,
        implementation_evidence,
        behavioral_observation,
        oracle_independence,
    }
}

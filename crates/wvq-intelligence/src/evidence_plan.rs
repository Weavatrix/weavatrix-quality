//! Gap classification and cheapest-evidence planner.
//!
//! Input is the Surface Evidence Matrix. This is Coverage Autopilot input, not
//! a verdict axis, and it does not generate evidence. Unmeasured cells are not
//! gaps. Intent cannot be established by a test producer.

use serde::{Deserialize, Serialize};

use crate::surface_evidence::{EvidenceCell, SurfaceEvidenceMatrix};
use crate::surface_graph::ApplicationSurfaceKind;

/// One matrix column the planner can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceColumn {
    /// `OpenSpec` / obligation intent.
    Intent,
    /// Runtime behavior observations.
    Runtime,
    /// A normalized test.
    Test,
    /// Assembled Proof.
    Proof,
    /// Normalized coverage mapped onto Weavatrix nodes.
    Coverage,
    /// Protection continuity — not coverage hits.
    Protection,
    /// UI integrity.
    Ui,
    /// Accessibility.
    A11y,
    /// Source mutation.
    Mutation,
}

/// A producer that can fill a measured-absent cell. Costs are fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProducer {
    /// Adapt an existing test or binding. Cost 1.
    ExistingTestAdaptation,
    /// Promote a recorded session. Cost 2.
    RecordedSession,
    /// A Storybook/Vitest flow. Cost 3.
    StorybookFlow,
    /// Changed-line source mutation. Cost 4.
    SourceMutation,
    /// Deterministic browser explore. Cost 5. Not available until closed-loop exists.
    BrowserExplore,
    /// Brownfield spec recovery for missing intent. Cost 5.
    SpecRecovery,
    /// Product-owner review of recovered or missing intent. Cost 8.
    ProductReview,
    /// `TestProgram` draft through the AI Cost Firewall. Cost 10. Never first.
    AiTestProgram,
}

impl EvidenceProducer {
    /// Fixed relative cost. Lower is cheaper.
    #[must_use]
    pub const fn cost(self) -> u64 {
        match self {
            Self::ExistingTestAdaptation => 1,
            Self::RecordedSession => 2,
            Self::StorybookFlow => 3,
            Self::SourceMutation => 4,
            Self::BrowserExplore | Self::SpecRecovery => 5,
            Self::ProductReview => 8,
            Self::AiTestProgram => 10,
        }
    }

    /// Whether this producer has a live runtime capability.
    ///
    /// `BrowserExplore` is planned but has no browser-feedback loop yet.
    #[must_use]
    pub const fn available(self) -> bool {
        !matches!(self, Self::BrowserExplore)
    }
}

/// Which live producers actually exist for this repository.
///
/// The planner must not recommend a Storybook flow when there is no story,
/// or an existing-test adaptation when there is no matching test. Each flag is
/// an independent capability, not a state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
#[serde(deny_unknown_fields)]
pub struct ProducerInventory {
    /// A matching test or binding exists for the surface.
    pub matching_tests: bool,
    /// A recorded session exists that could be promoted.
    pub recorded_sessions: bool,
    /// A Storybook story exists for the surface.
    pub stories: bool,
    /// Source mutation is authorized for the owned ecosystem.
    pub mutation_available: bool,
    /// Spec recovery / QA review can run.
    pub spec_recovery_available: bool,
    /// AI Cost Firewall still has budget for a planning call.
    pub ai_budget: bool,
}

impl Default for ProducerInventory {
    fn default() -> Self {
        Self {
            matching_tests: true,
            recorded_sessions: true,
            stories: true,
            mutation_available: true,
            spec_recovery_available: true,
            ai_budget: true,
        }
    }
}

/// One ranked producer for a gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerOffer {
    /// Producer identity.
    pub producer: EvidenceProducer,
    /// Fixed cost from [`EvidenceProducer::cost`].
    pub cost: u64,
}

/// Why the planner named this cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceNeed {
    /// The producer ran and the cell is absent. Add evidence.
    #[default]
    MeasuredAbsent,
    /// The producer did not run. Measure first.
    Unmeasured,
}

/// A cell that needs measurement or evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceGap {
    /// Surface id from the Application Surface Graph.
    pub surface: String,
    /// Surface kind.
    pub kind: ApplicationSurfaceKind,
    /// Missing evidence column.
    pub column: EvidenceColumn,
    /// Unmeasured vs measured-absent.
    #[serde(default)]
    pub need: EvidenceNeed,
}

/// Ranked producers for one measured gap. Empty `producers` means none apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePlan {
    /// Surface id.
    pub surface: String,
    /// Surface kind.
    pub kind: ApplicationSurfaceKind,
    /// Missing evidence column.
    pub column: EvidenceColumn,
    /// Unmeasured vs measured-absent.
    #[serde(default)]
    pub need: EvidenceNeed,
    /// Cheapest applicable producer, if any.
    pub cheapest: Option<EvidenceProducer>,
    /// All applicable producers, cheapest first.
    pub producers: Vec<ProducerOffer>,
}

/// Planner reading over every measured-absent cell.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheapestEvidencePlan {
    /// One plan per measured gap, sorted by surface then column.
    pub gaps: Vec<EvidencePlan>,
    /// True when the surface graph or matrix was truncated.
    pub truncated: bool,
}

/// Measured-absent and unmeasured cells. Present cells are not planned.
#[must_use]
pub fn classify_evidence_gaps(matrix: &SurfaceEvidenceMatrix) -> Vec<EvidenceGap> {
    let mut gaps = Vec::new();
    for row in &matrix.surfaces {
        for (column, cell) in row.cells() {
            let need = match cell {
                EvidenceCell::Absent => EvidenceNeed::MeasuredAbsent,
                EvidenceCell::Unmeasured => EvidenceNeed::Unmeasured,
                EvidenceCell::Present => continue,
            };
            gaps.push(EvidenceGap {
                surface: row.surface.clone(),
                kind: row.kind,
                column,
                need,
            });
        }
    }
    gaps.sort_by(|left, right| {
        left.surface
            .cmp(&right.surface)
            .then(left.need.cmp(&right.need))
            .then(left.column.cmp(&right.column))
    });
    gaps
}

/// Rank the cheapest producer that can fill each measured-absent cell.
///
/// This does not generate evidence, preview a program, or call a model.
/// Uses a default inventory where every implemented producer is available.
#[must_use]
pub fn plan_cheapest_evidence(matrix: &SurfaceEvidenceMatrix) -> CheapestEvidencePlan {
    plan_cheapest_evidence_with(matrix, &ProducerInventory::default())
}

/// Rank producers against the live capability inventory.
#[must_use]
pub fn plan_cheapest_evidence_with(
    matrix: &SurfaceEvidenceMatrix,
    inventory: &ProducerInventory,
) -> CheapestEvidencePlan {
    let gaps = classify_evidence_gaps(matrix)
        .into_iter()
        .map(|gap| {
            let intent_present = matrix
                .surfaces
                .iter()
                .find(|row| row.surface == gap.surface)
                .is_some_and(|row| row.intent == EvidenceCell::Present);
            let producers = offers(gap.column, gap.kind, intent_present, inventory);
            EvidencePlan {
                surface: gap.surface,
                kind: gap.kind,
                column: gap.column,
                need: gap.need,
                cheapest: producers.first().map(|offer| offer.producer),
                producers,
            }
        })
        .collect();
    CheapestEvidencePlan {
        gaps,
        truncated: matrix.truncated,
    }
}

fn offers(
    column: EvidenceColumn,
    kind: ApplicationSurfaceKind,
    intent_present: bool,
    inventory: &ProducerInventory,
) -> Vec<ProducerOffer> {
    let ui = matches!(
        kind,
        ApplicationSurfaceKind::Route | ApplicationSurfaceKind::Component
    );
    let mut producers = Vec::new();
    match column {
        EvidenceColumn::Intent => {
            if inventory.spec_recovery_available {
                producers.push(EvidenceProducer::SpecRecovery);
                producers.push(EvidenceProducer::ProductReview);
            }
        }
        EvidenceColumn::Mutation => {
            if inventory.mutation_available {
                producers.push(EvidenceProducer::SourceMutation);
            }
        }
        EvidenceColumn::Runtime => {
            if inventory.recorded_sessions {
                producers.push(EvidenceProducer::RecordedSession);
            }
            if ui {
                producers.push(EvidenceProducer::BrowserExplore);
            }
            if inventory.ai_budget {
                producers.push(EvidenceProducer::AiTestProgram);
            }
        }
        EvidenceColumn::Test => {
            if inventory.matching_tests {
                producers.push(EvidenceProducer::ExistingTestAdaptation);
            }
            if inventory.recorded_sessions {
                producers.push(EvidenceProducer::RecordedSession);
            }
            if ui && inventory.stories {
                producers.push(EvidenceProducer::StorybookFlow);
            }
            if ui {
                producers.push(EvidenceProducer::BrowserExplore);
            }
            if inventory.ai_budget {
                producers.push(EvidenceProducer::AiTestProgram);
            }
        }
        EvidenceColumn::Proof => {
            if intent_present {
                if inventory.matching_tests {
                    producers.push(EvidenceProducer::ExistingTestAdaptation);
                }
                if inventory.recorded_sessions {
                    producers.push(EvidenceProducer::RecordedSession);
                }
                if ui && inventory.stories {
                    producers.push(EvidenceProducer::StorybookFlow);
                }
                if inventory.ai_budget {
                    producers.push(EvidenceProducer::AiTestProgram);
                }
            }
        }
        EvidenceColumn::Coverage | EvidenceColumn::Protection => {
            if inventory.matching_tests {
                producers.push(EvidenceProducer::ExistingTestAdaptation);
            }
            if ui && inventory.stories {
                producers.push(EvidenceProducer::StorybookFlow);
            }
            if inventory.ai_budget {
                producers.push(EvidenceProducer::AiTestProgram);
            }
        }
        EvidenceColumn::Ui | EvidenceColumn::A11y => {
            if inventory.recorded_sessions {
                producers.push(EvidenceProducer::RecordedSession);
            }
            if ui && inventory.stories {
                producers.push(EvidenceProducer::StorybookFlow);
            }
            if ui {
                producers.push(EvidenceProducer::BrowserExplore);
            }
            if inventory.ai_budget {
                producers.push(EvidenceProducer::AiTestProgram);
            }
        }
    }
    producers.retain(|producer| producer.available());
    producers.sort_by_key(|producer| (producer.cost(), *producer));
    producers
        .into_iter()
        .map(|producer| ProducerOffer {
            cost: producer.cost(),
            producer,
        })
        .collect()
}

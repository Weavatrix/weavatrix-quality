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
    /// Protection / coverage.
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
    /// Deterministic browser explore. Cost 5.
    BrowserExplore,
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
            Self::BrowserExplore => 5,
            Self::AiTestProgram => 10,
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

/// A measured-absent cell. Unmeasured is not a gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceGap {
    /// Surface id from the Application Surface Graph.
    pub surface: String,
    /// Surface kind.
    pub kind: ApplicationSurfaceKind,
    /// Missing evidence column.
    pub column: EvidenceColumn,
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

/// Measured-absent cells only. Unmeasured is not classified as a gap.
#[must_use]
pub fn classify_evidence_gaps(matrix: &SurfaceEvidenceMatrix) -> Vec<EvidenceGap> {
    let mut gaps = Vec::new();
    for row in &matrix.surfaces {
        for (column, cell) in row.cells() {
            if cell == EvidenceCell::Absent {
                gaps.push(EvidenceGap {
                    surface: row.surface.clone(),
                    kind: row.kind,
                    column,
                });
            }
        }
    }
    gaps.sort_by(|left, right| {
        left.surface
            .cmp(&right.surface)
            .then(left.column.cmp(&right.column))
    });
    gaps
}

/// Rank the cheapest producer that can fill each measured-absent cell.
///
/// This does not generate evidence, preview a program, or call a model.
#[must_use]
pub fn plan_cheapest_evidence(matrix: &SurfaceEvidenceMatrix) -> CheapestEvidencePlan {
    let gaps = classify_evidence_gaps(matrix)
        .into_iter()
        .map(|gap| {
            let intent_present = matrix
                .surfaces
                .iter()
                .find(|row| row.surface == gap.surface)
                .is_some_and(|row| row.intent == EvidenceCell::Present);
            let producers = offers(gap.column, gap.kind, intent_present);
            EvidencePlan {
                surface: gap.surface,
                kind: gap.kind,
                column: gap.column,
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
) -> Vec<ProducerOffer> {
    let ui = matches!(
        kind,
        ApplicationSurfaceKind::Route | ApplicationSurfaceKind::Component
    );
    let mut producers = Vec::new();
    match column {
        EvidenceColumn::Intent | EvidenceColumn::Mutation => {}
        EvidenceColumn::Runtime => {
            producers.push(EvidenceProducer::RecordedSession);
            if ui {
                producers.push(EvidenceProducer::BrowserExplore);
            }
            producers.push(EvidenceProducer::AiTestProgram);
        }
        EvidenceColumn::Test => {
            producers.push(EvidenceProducer::ExistingTestAdaptation);
            producers.push(EvidenceProducer::RecordedSession);
            if ui {
                producers.push(EvidenceProducer::StorybookFlow);
                producers.push(EvidenceProducer::BrowserExplore);
            }
            producers.push(EvidenceProducer::AiTestProgram);
        }
        EvidenceColumn::Proof => {
            if intent_present {
                producers.push(EvidenceProducer::ExistingTestAdaptation);
                producers.push(EvidenceProducer::RecordedSession);
                if ui {
                    producers.push(EvidenceProducer::StorybookFlow);
                }
                producers.push(EvidenceProducer::AiTestProgram);
            }
        }
        EvidenceColumn::Protection => {
            producers.push(EvidenceProducer::ExistingTestAdaptation);
            if ui {
                producers.push(EvidenceProducer::StorybookFlow);
            }
            producers.push(EvidenceProducer::AiTestProgram);
        }
        EvidenceColumn::Ui | EvidenceColumn::A11y => {
            producers.push(EvidenceProducer::RecordedSession);
            if ui {
                producers.push(EvidenceProducer::StorybookFlow);
                producers.push(EvidenceProducer::BrowserExplore);
            }
            producers.push(EvidenceProducer::AiTestProgram);
        }
    }
    producers.sort_by_key(|producer| (producer.cost(), *producer));
    producers
        .into_iter()
        .map(|producer| ProducerOffer {
            cost: producer.cost(),
            producer,
        })
        .collect()
}

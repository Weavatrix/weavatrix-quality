//! Evidence-based risk. Never an opaque percentage.

use wvq_domain::{QualityFinding, Severity};

/// Spec §13 risk kinds. Each finding contributes named evidence, not a score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskEvidenceKind {
    /// `OpenSpec` requirement is high-stakes.
    RequirementCriticality,
    /// Large reverse-dependency radius.
    CodeBlastRadius,
    /// Architecture boundary involved.
    ArchitectureBoundary,
    /// Externally visible API changed.
    PublicApiChange,
    /// Region with prior regressions.
    HistoricalRegression,
    /// High churn plus connectivity.
    ChurnHotspot,
    /// Missing measured coverage.
    LowCoverage,
    /// New runtime behavior state.
    NewBehaviorState,
    /// Authorization / permission path.
    PermissionChange,
    /// Data model / migration.
    DataMigration,
    /// Dependent repository/contract.
    CrossRepoImpact,
    /// Mutation survived selected tests.
    MutationSurvivor,
}

/// Spec §13 levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    /// Narrow, well-proven change.
    Low,
    /// Default.
    Medium,
    /// Broader execution / human visibility.
    High,
    /// Cannot omit mandatory proof.
    Critical,
}

/// One named risk axis. `detail` is prose plus counts, never `risk=87`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskEvidence {
    /// Which axis.
    pub kind: RiskEvidenceKind,
    /// Qualitative level.
    pub level: RiskLevel,
    /// Subject id (endpoint, path, repo).
    pub subject: String,
    /// Human-readable evidence.
    pub detail: String,
}

/// Map quality findings into [`RiskEvidence`]. No numeric score is produced.
#[must_use]
pub fn risk_evidence(findings: &[QualityFinding]) -> Vec<RiskEvidence> {
    let mut out = Vec::new();
    for finding in findings {
        if let Some(item) = from_finding(finding) {
            out.push(item);
        }
    }
    out.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.subject.cmp(&right.subject))
    });
    out
}

fn from_finding(finding: &QualityFinding) -> Option<RiskEvidence> {
    let check = finding.check.as_str();
    let (kind, level) = match check {
        "WVQ-API-001" | "WVQ-API-006" => {
            (RiskEvidenceKind::PublicApiChange, level_from(finding.severity))
        }
        "WVQ-API-002" | "WVQ-API-005" => (RiskEvidenceKind::PublicApiChange, RiskLevel::Medium),
        "WVQ-API-004" | "WVQ-HIST-005" => (RiskEvidenceKind::CrossRepoImpact, RiskLevel::Medium),
        "WVQ-HIST-002" | "WVQ-HIST-004" => {
            (RiskEvidenceKind::HistoricalRegression, RiskLevel::High)
        }
        "WVQ-HIST-003" => (RiskEvidenceKind::ChurnHotspot, RiskLevel::High),
        "WVQ-GRAPH-004" => (RiskEvidenceKind::CodeBlastRadius, RiskLevel::High),
        "WVQ-ARCH-001" | "WVQ-ARCH-003" => (RiskEvidenceKind::ArchitectureBoundary, RiskLevel::High),
        _ if check.starts_with("WVQ-API-") => (RiskEvidenceKind::PublicApiChange, RiskLevel::Low),
        _ if check.starts_with("WVQ-HIST-") => {
            (RiskEvidenceKind::HistoricalRegression, RiskLevel::Medium)
        }
        _ => return None,
    };
    Some(RiskEvidence {
        kind,
        level,
        subject: finding.weavatrix_fingerprint.clone().unwrap_or_else(|| {
            match &finding.subject {
                wvq_domain::SubjectRef::Endpoint(id) | wvq_domain::SubjectRef::File(id) => {
                    id.clone()
                }
                wvq_domain::SubjectRef::GraphNode(id) => id.clone(),
                other => other_subject(other),
            }
        }),
        detail: finding.summary.clone(),
    })
}

fn other_subject(subject: &wvq_domain::SubjectRef) -> String {
    match subject {
        wvq_domain::SubjectRef::Symbol(name)
        | wvq_domain::SubjectRef::Test(name) => name.clone(),
        wvq_domain::SubjectRef::Obligation(id) => id.to_string(),
        wvq_domain::SubjectRef::Requirement(id) => id.to_string(),
        wvq_domain::SubjectRef::Change(id) => id.to_string(),
        _ => "unknown".into(),
    }
}

fn level_from(severity: Severity) -> RiskLevel {
    match severity {
        Severity::Error => RiskLevel::High,
        Severity::Warn => RiskLevel::Medium,
        Severity::Info => RiskLevel::Low,
    }
}

impl RiskEvidence {
    /// True when this evidence is high enough to escalate an unproven contract.
    #[must_use]
    pub fn escalates_unproven_contract(&self) -> bool {
        matches!(self.level, RiskLevel::High | RiskLevel::Critical)
            && matches!(
                self.kind,
                RiskEvidenceKind::PublicApiChange
                    | RiskEvidenceKind::HistoricalRegression
                    | RiskEvidenceKind::CrossRepoImpact
            )
    }
}

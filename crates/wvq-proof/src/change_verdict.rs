//! Change-level verdict composed from every measured axis.
//!
//! `ProofVerdict` answers one question: did this obligation's sealed
//! expectation hold? That is not the same question a reviewer asks before
//! merging. A change can prove every obligation and still delete the only test
//! that guarded a critical branch, introduce a blocking architecture violation,
//! or hide a new interactive occlusion behind a passing behavioural test.
//!
//! This module keeps the axes apart. There is no single number: each axis
//! carries its own facts and provenance, the composed state comes from a fixed
//! priority order, and every reason that fired stays listed even when a
//! higher-priority reason decided the outcome.
//!
//! Three rules constrain the policy and are asserted by the tests:
//!
//! * a `PROVEN` behavioural proof cannot suppress a lost protection delta;
//! * a global coverage gain cannot suppress a local protection loss;
//! * missing evidence never becomes a pass.

use serde::Serialize;
use wvq_domain::Severity;

use crate::protection_checks::ProtectionFinding;
use crate::protection_delta::ProtectionSummary;
use crate::verdict::ProofVerdict;

/// Overall state of one change, most severe first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeVerdictState {
    /// A measured regression must not land.
    Blocked,
    /// Deterministic evidence cannot settle it; a human decides.
    NeedsHuman,
    /// Something required was never measured. Not a pass, not a failure.
    NotEnoughEvidence,
    /// Everything required held; non-blocking drift was recorded.
    PassWithWarnings,
    /// Everything required held with nothing to report.
    Pass,
}

impl ChangeVerdictState {
    /// Stable transport token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "BLOCKED",
            Self::NeedsHuman => "NEEDS_HUMAN",
            Self::NotEnoughEvidence => "NOT_ENOUGH_EVIDENCE",
            Self::PassWithWarnings => "PASS_WITH_WARNINGS",
            Self::Pass => "PASS",
        }
    }

    /// Whether CI must fail. Only a measured regression blocks; unmeasured
    /// evidence and human review are visible without being a failure.
    #[must_use]
    pub fn blocks(self) -> bool {
        matches!(self, Self::Blocked)
    }

    /// Process exit code: `0` clean, `2` blocking, `1` otherwise.
    #[must_use]
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Blocked => 2,
            Self::NeedsHuman | Self::NotEnoughEvidence => 1,
            Self::PassWithWarnings | Self::Pass => 0,
        }
    }
}

/// State of one axis. `NotApplicable` and `Unmeasured` are deliberately
/// distinct: the first says the axis was never in scope, the second says it was
/// in scope and no evidence arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisState {
    /// This change has no surface the axis can measure.
    NotApplicable,
    /// Measured with nothing to report.
    Clean,
    /// Measured; non-blocking drift only.
    Warnings,
    /// Measured; a regression that must not land.
    Blocking,
    /// In scope but not measured. Never treated as clean.
    Unmeasured,
}

impl AxisState {
    /// Stable transport token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::Clean => "clean",
            Self::Warnings => "warnings",
            Self::Blocking => "blocking",
            Self::Unmeasured => "unmeasured",
        }
    }
}

/// One fired policy rule, with the rank that decided its precedence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockingReason {
    /// Position in the deterministic priority order. Lower wins.
    pub rank: u8,
    /// Stable rule identity (`WVQ-VERDICT-002`).
    pub code: String,
    /// Axis the reason came from.
    pub axis: String,
    /// Flow, obligation, test, or finding the reason is about.
    pub subject: String,
    /// What was measured, in words a reviewer can act on.
    pub detail: String,
}

/// One axis that could not be measured, and why it matters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Limitation {
    /// Axis the gap belongs to.
    pub axis: String,
    /// What was not measured.
    pub detail: String,
}

/// One obligation's proof outcome as the composer needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofOutcome {
    /// Obligation identity.
    pub obligation: String,
    /// Requirement the obligation belongs to.
    pub requirement: String,
    /// Assembled proof verdict.
    pub verdict: ProofVerdict,
    /// True for high or critical risk. Mandatory obligations may not stay
    /// unproven without the change reporting missing evidence.
    pub mandatory: bool,
}

/// Sealed-expectation axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProofAxis {
    /// Axis state.
    pub state: AxisState,
    /// Proven obligations.
    pub proven: u64,
    /// Partially evidenced obligations.
    pub partial: u64,
    /// Obligations with no runtime evidence.
    pub unproven: u64,
    /// Obligations whose sealed expectation was contradicted.
    pub contradicted: u64,
    /// Obligations a human must settle.
    pub human_required: u64,
    /// Contradicted obligation identities.
    pub contradicted_obligations: Vec<String>,
    /// Mandatory obligations with no proof.
    pub unproven_mandatory: Vec<String>,
    /// Ambiguous obligation identities.
    pub ambiguous_obligations: Vec<String>,
}

/// Protection-continuity axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProtectionAxis {
    /// Axis state.
    pub state: AxisState,
    /// Whether both revisions were measured.
    pub measured: bool,
    /// Counts by delta state.
    pub summary: ProtectionSummary,
    /// Flows whose safety net disappeared.
    pub lost_flows: Vec<String>,
    /// Critical branches that stopped being executed. A global coverage gain
    /// never empties this list.
    pub lost_critical_branches: Vec<String>,
    /// Error-severity protection findings.
    pub blocking_findings: Vec<ProtectionFinding>,
    /// Warn-severity protection findings.
    pub warning_findings: Vec<ProtectionFinding>,
}

/// One debt finding as the composer needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebtItem {
    /// Finding identity.
    pub id: String,
    /// Rule that produced it.
    pub rule: String,
    /// Whether the rule family blocks by default.
    pub blocking: bool,
}

/// Quality-debt-ratchet axis. Existing debt never blocks adoption.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DebtAxis {
    /// Axis state.
    pub state: AxisState,
    /// Whether immutable base/head comparison was available.
    pub comparison_present: bool,
    /// Findings present on both revisions.
    pub existing: u64,
    /// Findings gone on head.
    pub fixed: u64,
    /// Findings covered by an explicit exception.
    pub excepted: u64,
    /// Findings introduced by this change.
    pub new: Vec<DebtItem>,
    /// Previously fixed findings that came back.
    pub returned: Vec<DebtItem>,
}

/// Test-stability axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StabilityAxis {
    /// Axis state.
    pub state: AxisState,
    /// Whether test history was recorded for this run.
    pub measured: bool,
    /// Identities with both pass and fail history.
    pub flaky: u64,
    /// Failures deterministic triage could not classify.
    pub unknown_failures: u64,
    /// Mandatory tests whose new flake is still unresolved.
    pub unresolved_mandatory_flakes: Vec<String>,
}

/// AI-budget axis. The ordinary green path spends nothing here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AiAxis {
    /// Axis state.
    pub state: AxisState,
    /// Runtime tokens spent verifying this change. Zero on the green path.
    pub runtime_tokens: u64,
    /// Whether a ceiling was reached.
    pub budget_exhausted: bool,
    /// Decisions the exhausted budget left unresolved.
    pub unresolved_decisions: Vec<String>,
}

/// One UI-integrity finding projected for the verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UiFindingRef {
    /// Detector identity (`WVQ-UI-DUP-001`).
    pub check: String,
    /// Gate severity.
    pub severity: Severity,
    /// Semantic identity of the target.
    pub subject: String,
    /// Route the finding was measured on.
    pub route: String,
    /// Viewport the finding was measured at (`1280x720`).
    pub viewport: String,
    /// Quantified evidence, never a guess.
    pub detail: String,
}

/// Deterministic UI-integrity axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UiIntegrityAxis {
    /// Axis state.
    pub state: AxisState,
    /// Findings first seen on head.
    pub new: Vec<UiFindingRef>,
    /// Findings that were fixed and came back.
    pub returned: Vec<UiFindingRef>,
    /// Findings present on both revisions. Old debt does not block adoption.
    pub existing: u64,
    /// Findings gone on head.
    pub fixed: u64,
    /// Findings covered by an explicit allowance.
    pub excepted: u64,
    /// Route/state/viewport combinations that were required but not collected.
    pub unmeasured_states: Vec<String>,
    /// Whether any layout snapshot hit a bound. Truncation is never clean.
    pub truncated: bool,
}

/// One explicit Delta Triangle finding projected into the change verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeltaFindingRef {
    /// Stable detector identity (`WVQ-BEHAV-001`).
    pub check: String,
    /// Gate severity from the triangle classifier.
    pub severity: Severity,
    /// Exact `TestProgram` replayed on both revisions.
    pub program: String,
    /// First changed structured axis and the table reading.
    pub detail: String,
}

/// Live Spec x Code x Behavior evidence from same-program base/head replay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DeltaTriangleAxis {
    /// Axis state.
    pub state: AxisState,
    /// Whether the change modified its own `OpenSpec` authority.
    pub spec_changed: bool,
    /// Whether Weavatrix measured a changed code node or edge.
    pub code_changed: bool,
    /// Whether any paired program produced a structured behavior delta.
    pub behavior_changed: bool,
    /// Programs successfully replayed on both revisions.
    pub measured_programs: u64,
    /// Programs whose measured behavior changed.
    pub changed_programs: Vec<String>,
    /// Stable triangle readings, one per measured program.
    pub readings: Vec<String>,
    /// Explicit unexpected-delta findings.
    pub findings: Vec<DeltaFindingRef>,
    /// Programs that could not be replayed on both revisions.
    pub unmeasured_programs: Vec<String>,
}

impl Default for AxisState {
    fn default() -> Self {
        Self::NotApplicable
    }
}

/// Everything the composer reads. Each field is produced by its own measured
/// path; none of them is inferred from another.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerdictInputs {
    /// Per-obligation proof outcomes.
    pub proofs: Vec<ProofOutcome>,
    /// Protection axis, already measured or explicitly unmeasured.
    pub protection: ProtectionAxis,
    /// Debt axis.
    pub debt: DebtAxis,
    /// Stability axis.
    pub stability: StabilityAxis,
    /// AI budget axis.
    pub ai: AiAxis,
    /// UI-integrity axis.
    pub ui_integrity: UiIntegrityAxis,
    /// Same-program base/head Delta Triangle axis.
    pub delta_triangle: DeltaTriangleAxis,
    /// Extra limitations discovered outside the axes.
    pub limitations: Vec<Limitation>,
}

/// The composed change-level verdict. Never a single quality percentage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeQualityVerdict {
    /// Composed state.
    pub state: ChangeVerdictState,
    /// Sealed-expectation axis.
    pub proof: ProofAxis,
    /// Protection-continuity axis.
    pub protection: ProtectionAxis,
    /// Debt-ratchet axis.
    pub debt: DebtAxis,
    /// Test-stability axis.
    pub stability: StabilityAxis,
    /// AI budget axis.
    pub ai: AiAxis,
    /// UI-integrity axis.
    pub ui_integrity: UiIntegrityAxis,
    /// Same-program base/head Delta Triangle axis.
    pub delta_triangle: DeltaTriangleAxis,
    /// Every rule that fired, most important first.
    pub blocking_reasons: Vec<BlockingReason>,
    /// Everything that was in scope and not measured.
    pub limitations: Vec<Limitation>,
}

impl ChangeQualityVerdict {
    /// Whether CI must fail.
    #[must_use]
    pub fn blocking(&self) -> bool {
        self.state.blocks()
    }

    /// Process exit code.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        self.state.exit_code()
    }
}

/// Whether a debt rule family blocks a change by default.
///
/// Architecture, API-surface, and security rules gate. Size, clone, dead-code,
/// graph-topology, history, hypothesis, and coverage rules are recorded and
/// ratcheted without failing the build.
#[must_use]
pub fn debt_rule_blocks(rule: &str) -> bool {
    let upper = rule.to_ascii_uppercase();
    if upper.contains("SECURITY") {
        return true;
    }
    upper
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| matches!(part, "ARCH" | "ARCHITECTURE" | "API" | "SEC"))
}

/// Compose the change-level verdict from independently measured axes.
///
/// The priority order below is the policy. The first rule that fires decides
/// the state; every other fired rule is still reported so nothing a reviewer
/// needs disappears behind a higher-priority reason.
///
/// | Rank | Rule | State |
/// | --- | --- | --- |
/// | 1 | active sealed-oracle contradiction | `Blocked` |
/// | 2 | lost critical protection | `Blocked` |
/// | 3 | new blocking debt or UI regression | `Blocked` |
/// | 4 | mandatory obligation unproven | `NotEnoughEvidence` |
/// | 5 | returned blocking debt or UI regression | `Blocked` |
/// | 6 | mandatory test has an unresolved new flake | `NeedsHuman` |
/// | 7 | a required axis is unmeasured | `NotEnoughEvidence` |
/// | 8 | AI budget exhausted for a mandatory unresolved decision | `NeedsHuman` |
/// | 9 | warning-only drift | `PassWithWarnings` |
/// | 10 | everything required proven or preserved | `Pass` |
#[must_use]
pub fn compose(inputs: &VerdictInputs) -> ChangeQualityVerdict {
    let proof = proof_axis(&inputs.proofs);
    let mut reasons = Vec::new();
    let mut limitations = inputs.limitations.clone();

    rank_1_contradiction(&proof, &mut reasons);
    rank_2_lost_protection(&inputs.protection, &mut reasons);
    rank_3_new_blocking(
        &inputs.debt,
        &inputs.ui_integrity,
        &inputs.delta_triangle,
        &mut reasons,
    );
    rank_4_unproven_mandatory(&proof, &mut reasons);
    rank_5_returned_blocking(&inputs.debt, &inputs.ui_integrity, &mut reasons);
    rank_6_needs_human(&proof, &inputs.stability, &mut reasons);
    rank_7_unmeasured(&proof, inputs, &mut reasons, &mut limitations);
    rank_8_ai_budget(&inputs.ai, &mut reasons);
    rank_9_warnings(inputs, &mut reasons);

    reasons.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.subject.cmp(&right.subject))
    });
    limitations.sort_by(|left, right| {
        left.axis
            .cmp(&right.axis)
            .then_with(|| left.detail.cmp(&right.detail))
    });
    limitations.dedup();

    let state = reasons
        .first()
        .map_or(ChangeVerdictState::Pass, |reason| state_for(reason.rank));

    ChangeQualityVerdict {
        state,
        proof,
        protection: inputs.protection.clone(),
        debt: inputs.debt.clone(),
        stability: inputs.stability.clone(),
        ai: inputs.ai.clone(),
        ui_integrity: inputs.ui_integrity.clone(),
        delta_triangle: inputs.delta_triangle.clone(),
        blocking_reasons: reasons,
        limitations,
    }
}

fn state_for(rank: u8) -> ChangeVerdictState {
    match rank {
        1 | 2 | 3 | 5 => ChangeVerdictState::Blocked,
        6 | 8 => ChangeVerdictState::NeedsHuman,
        4 | 7 => ChangeVerdictState::NotEnoughEvidence,
        _ => ChangeVerdictState::PassWithWarnings,
    }
}

fn reason(rank: u8, code: &str, axis: &str, subject: &str, detail: String) -> BlockingReason {
    BlockingReason {
        rank,
        code: code.to_owned(),
        axis: axis.to_owned(),
        subject: subject.to_owned(),
        detail,
    }
}

/// Fold per-obligation proof outcomes into one axis.
fn proof_axis(outcomes: &[ProofOutcome]) -> ProofAxis {
    let mut axis = ProofAxis::default();
    for outcome in outcomes {
        match outcome.verdict {
            ProofVerdict::Proven => axis.proven += 1,
            ProofVerdict::Partial => axis.partial += 1,
            ProofVerdict::Unproven => axis.unproven += 1,
            ProofVerdict::Contradicted => {
                axis.contradicted += 1;
                axis.contradicted_obligations
                    .push(outcome.obligation.clone());
            }
            ProofVerdict::HumanRequired => {
                axis.human_required += 1;
                axis.ambiguous_obligations.push(outcome.obligation.clone());
            }
        }
        if outcome.mandatory
            && matches!(
                outcome.verdict,
                ProofVerdict::Unproven | ProofVerdict::Partial
            )
        {
            axis.unproven_mandatory.push(outcome.obligation.clone());
        }
    }
    axis.contradicted_obligations.sort();
    axis.unproven_mandatory.sort();
    axis.ambiguous_obligations.sort();
    axis.state = if outcomes.is_empty() {
        AxisState::NotApplicable
    } else if axis.contradicted > 0 {
        AxisState::Blocking
    } else if axis.unproven > 0 || axis.partial > 0 {
        AxisState::Unmeasured
    } else if axis.human_required > 0 {
        // Evidence exists; the sealed specification is what cannot decide.
        AxisState::Warnings
    } else {
        AxisState::Clean
    };
    axis
}

fn rank_1_contradiction(proof: &ProofAxis, out: &mut Vec<BlockingReason>) {
    for obligation in &proof.contradicted_obligations {
        out.push(reason(
            1,
            "WVQ-VERDICT-001",
            "proof",
            obligation,
            "measured behaviour contradicts the sealed expectation".into(),
        ));
    }
}

/// A lost critical branch is rank 2 whatever the proof axis says: a passing
/// behavioural test proves the behaviour still works, not that it is still
/// guarded.
fn rank_2_lost_protection(protection: &ProtectionAxis, out: &mut Vec<BlockingReason>) {
    for branch in &protection.lost_critical_branches {
        out.push(reason(
            2,
            "WVQ-VERDICT-002",
            "protection",
            branch,
            "critical branch lost all dynamic execution; a global coverage gain does not offset it"
                .into(),
        ));
    }
    for finding in &protection.blocking_findings {
        // 006 already reports the critical-branch loss listed above.
        if finding.check.as_str() == "WVQ-PROTECT-006" {
            continue;
        }
        out.push(reason(
            2,
            "WVQ-VERDICT-002",
            "protection",
            &finding.subject,
            format!("{}: {}", finding.check, finding.detail),
        ));
    }
}

fn rank_3_new_blocking(
    debt: &DebtAxis,
    ui: &UiIntegrityAxis,
    delta: &DeltaTriangleAxis,
    out: &mut Vec<BlockingReason>,
) {
    for item in debt.new.iter().filter(|item| item.blocking) {
        out.push(reason(
            3,
            "WVQ-VERDICT-003",
            "debt",
            &item.id,
            format!(
                "new blocking debt introduced by this change ({})",
                item.rule
            ),
        ));
    }
    for finding in ui
        .new
        .iter()
        .filter(|item| item.severity == Severity::Error)
    {
        out.push(reason(
            3,
            "WVQ-VERDICT-003",
            "ui_integrity",
            &finding.subject,
            format!(
                "{} on {} at {}: {}",
                finding.check, finding.route, finding.viewport, finding.detail
            ),
        ));
    }
    for finding in delta
        .findings
        .iter()
        .filter(|item| item.severity == Severity::Error)
    {
        out.push(reason(
            3,
            "WVQ-VERDICT-011",
            "delta_triangle",
            &finding.program,
            format!("{}: {}", finding.check, finding.detail),
        ));
    }
}

fn rank_4_unproven_mandatory(proof: &ProofAxis, out: &mut Vec<BlockingReason>) {
    for obligation in &proof.unproven_mandatory {
        out.push(reason(
            4,
            "WVQ-VERDICT-004",
            "proof",
            obligation,
            "high or critical obligation has no proof; missing evidence is not absence of a defect"
                .into(),
        ));
    }
}

fn rank_5_returned_blocking(debt: &DebtAxis, ui: &UiIntegrityAxis, out: &mut Vec<BlockingReason>) {
    for item in debt.returned.iter().filter(|item| item.blocking) {
        out.push(reason(
            5,
            "WVQ-VERDICT-005",
            "debt",
            &item.id,
            format!("previously fixed blocking debt came back ({})", item.rule),
        ));
    }
    for finding in ui
        .returned
        .iter()
        .filter(|item| item.severity == Severity::Error)
    {
        out.push(reason(
            5,
            "WVQ-VERDICT-005",
            "ui_integrity",
            &finding.subject,
            format!(
                "previously fixed {} came back on {} at {}",
                finding.check, finding.route, finding.viewport
            ),
        ));
    }
}

/// Rank 6 collects everything deterministic evidence cannot settle: a new
/// unresolved flake on a mandatory test, and an obligation whose sealed
/// specification is ambiguous. Both need a human rather than a gate.
fn rank_6_needs_human(proof: &ProofAxis, stability: &StabilityAxis, out: &mut Vec<BlockingReason>) {
    for test in &stability.unresolved_mandatory_flakes {
        out.push(reason(
            6,
            "WVQ-VERDICT-006",
            "stability",
            test,
            "a mandatory test has a new unresolved flake; deterministic triage cannot settle it"
                .into(),
        ));
    }
    for obligation in &proof.ambiguous_obligations {
        out.push(reason(
            6,
            "WVQ-VERDICT-010",
            "proof",
            obligation,
            "the sealed specification is ambiguous for this obligation".into(),
        ));
    }
}

/// A required axis with no evidence is neither a pass nor a failure. Every
/// unmeasured axis is also recorded as a limitation so the gap is visible even
/// when a higher-priority rule decided the state.
fn rank_7_unmeasured(
    proof: &ProofAxis,
    inputs: &VerdictInputs,
    out: &mut Vec<BlockingReason>,
    limitations: &mut Vec<Limitation>,
) {
    let unmeasured: [(&str, AxisState, String); 6] = [
        (
            "proof",
            proof.state,
            format!(
                "{} obligation(s) have no runtime evidence and {} are partial",
                proof.unproven, proof.partial
            ),
        ),
        (
            "protection",
            inputs.protection.state,
            "base and head protection were not both measured".into(),
        ),
        (
            "debt",
            inputs.debt.state,
            "immutable base/head debt comparison was unavailable".into(),
        ),
        (
            "stability",
            inputs.stability.state,
            "no test history was recorded for this run".into(),
        ),
        (
            "ui_integrity",
            inputs.ui_integrity.state,
            ui_unmeasured_detail(&inputs.ui_integrity),
        ),
        (
            "delta_triangle",
            inputs.delta_triangle.state,
            if inputs.delta_triangle.unmeasured_programs.is_empty() {
                "same-program base/head browser replay was incomplete".into()
            } else {
                format!(
                    "same-program base/head replay was incomplete for {}",
                    inputs.delta_triangle.unmeasured_programs.join(", ")
                )
            },
        ),
    ];
    for (axis, state, detail) in unmeasured {
        if state != AxisState::Unmeasured {
            continue;
        }
        out.push(reason(7, "WVQ-VERDICT-007", axis, axis, detail.clone()));
        limitations.push(Limitation {
            axis: axis.to_owned(),
            detail,
        });
    }
}

fn ui_unmeasured_detail(ui: &UiIntegrityAxis) -> String {
    if ui.truncated {
        return "a layout snapshot hit its node bound; a truncated snapshot is never clean".into();
    }
    if ui.unmeasured_states.is_empty() {
        "required UI evidence was not collected".into()
    } else {
        format!("no layout snapshot for {}", ui.unmeasured_states.join(", "))
    }
}

fn rank_8_ai_budget(ai: &AiAxis, out: &mut Vec<BlockingReason>) {
    if !ai.budget_exhausted {
        return;
    }
    for decision in &ai.unresolved_decisions {
        out.push(reason(
            8,
            "WVQ-VERDICT-008",
            "ai",
            decision,
            "the AI budget was exhausted before this mandatory decision was resolved".into(),
        ));
    }
}

fn rank_9_warnings(inputs: &VerdictInputs, out: &mut Vec<BlockingReason>) {
    let mut warn = |axis: &str, subject: &str, detail: String| {
        out.push(reason(9, "WVQ-VERDICT-009", axis, subject, detail));
    };
    for finding in &inputs.protection.warning_findings {
        warn(
            "protection",
            &finding.subject,
            format!("{}: {}", finding.check, finding.detail),
        );
    }
    for item in inputs.debt.new.iter().filter(|item| !item.blocking) {
        warn(
            "debt",
            &item.id,
            format!("new non-blocking debt ({})", item.rule),
        );
    }
    for item in inputs.debt.returned.iter().filter(|item| !item.blocking) {
        warn(
            "debt",
            &item.id,
            format!("returned non-blocking debt ({})", item.rule),
        );
    }
    for finding in inputs
        .ui_integrity
        .new
        .iter()
        .chain(&inputs.ui_integrity.returned)
        .filter(|item| item.severity != Severity::Error)
    {
        warn(
            "ui_integrity",
            &finding.subject,
            format!(
                "{} on {} at {}: {}",
                finding.check, finding.route, finding.viewport, finding.detail
            ),
        );
    }
    for finding in inputs
        .delta_triangle
        .findings
        .iter()
        .filter(|item| item.severity != Severity::Error)
    {
        warn(
            "delta_triangle",
            &finding.program,
            format!("{}: {}", finding.check, finding.detail),
        );
    }
    if inputs.stability.flaky > 0 {
        warn(
            "stability",
            "test-history",
            format!(
                "{} test identity/identities have mixed pass and fail history",
                inputs.stability.flaky
            ),
        );
    }
}

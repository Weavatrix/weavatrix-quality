//! Spec §78–§82: `WVQ-PROTECT-001` … `WVQ-PROTECT-012`.
//!
//! These are the gates that turn a protection delta into a verdict. The rule
//! they exist to enforce is that a rise in overall coverage never silences a
//! local loss, and that protection may only disappear when the behaviour it
//! guarded was deliberately removed.

use serde::Serialize;
use wvq_domain::{CheckId, Severity};

use crate::protection_delta::{ProtectionDelta, ProtectionDeltaState};

/// One protection finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtectionFinding {
    /// Stable check identity (`WVQ-PROTECT-001`).
    pub check: CheckId,
    /// How hard this blocks.
    pub severity: Severity,
    /// Flow or test the finding is about.
    pub subject: String,
    /// What happened, in words a reviewer can act on.
    pub detail: String,
}

/// How a test changed between revisions, as the checks need it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestChange {
    /// Test identity.
    pub test: String,
    /// Flow it was believed to protect.
    pub flow: String,
    /// Whether the test still exists on head.
    pub survives: bool,
    /// Flows it used to reach and no longer does.
    pub lost_flows: Vec<String>,
    /// Obligations it used to prove and no longer does.
    pub lost_obligations: Vec<String>,
    /// Another test that now proves the same obligation and flow.
    pub replaced_by: Option<String>,
    /// A sealed assertion was removed or relaxed.
    pub assertions_weakened: bool,
    /// The test changed in the same change as the implementation.
    pub changed_with_implementation: bool,
    /// A new `OracleSeal` authorises the changed expectation.
    pub new_oracle_seal: bool,
    /// A declared `SpecDelta` covers the change.
    pub declared_spec_delta: bool,
}

/// Protection strength for one flow over recent revisions, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectionTrend {
    /// Flow.
    pub flow: String,
    /// Number of valid protectors at each revision.
    pub protectors: Vec<u32>,
}

/// Policy inputs the checks need.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtectionPolicy {
    /// Flows whose obligations are high or critical risk.
    pub high_risk_flows: Vec<String>,
    /// Ratio of newly covered low-risk lines that counts as suspicious.
    pub substitution_ratio: u32,
}

impl ProtectionPolicy {
    fn severity_for(&self, flow: &str) -> Severity {
        if self.high_risk_flows.iter().any(|item| item == flow) {
            Severity::Error
        } else {
            Severity::Warn
        }
    }
}

/// Everything the twelve checks read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtectionCheckInput {
    /// Per-flow protection deltas.
    pub deltas: Vec<ProtectionDelta>,
    /// Per-test changes.
    pub tests: Vec<TestChange>,
    /// Protection strength over time.
    pub trends: Vec<ProtectionTrend>,
    /// Policy.
    pub policy: ProtectionPolicy,
}

fn finding(id: &str, severity: Severity, subject: &str, detail: String) -> ProtectionFinding {
    ProtectionFinding {
        check: CheckId::new(id).unwrap_or_else(|_| unreachable!("check ids are literals")),
        severity,
        subject: subject.to_owned(),
        detail,
    }
}

/// Run every protection-continuity check.
///
/// Output is sorted by check then subject so CI diffs stay stable.
#[must_use]
pub fn gate_protection(input: &ProtectionCheckInput) -> Vec<ProtectionFinding> {
    let mut out = Vec::new();
    for delta in &input.deltas {
        check_flow(delta, &input.policy, &mut out);
    }
    for test in &input.tests {
        check_test(test, &input.policy, &mut out);
    }
    for trend in &input.trends {
        check_trend(trend, &mut out);
    }
    out.sort_by(|left, right| {
        left.check
            .as_str()
            .cmp(right.check.as_str())
            .then_with(|| left.subject.cmp(&right.subject))
    });
    out
}

/// Whether any finding must fail the build.
#[must_use]
pub fn blocks(findings: &[ProtectionFinding]) -> bool {
    findings.iter().any(|item| item.severity == Severity::Error)
}

fn check_flow(
    delta: &ProtectionDelta,
    policy: &ProtectionPolicy,
    out: &mut Vec<ProtectionFinding>,
) {
    let flow = delta.flow.as_str();
    let severity = policy.severity_for(flow);

    match delta.state {
        ProtectionDeltaState::Lost => {
            // 001 — a previously protected flow has no valid proof path.
            out.push(finding(
                "WVQ-PROTECT-001",
                severity,
                flow,
                format!(
                    "base had measured protection, head has none: {}",
                    delta.reasons.join("; ")
                ),
            ));
            // 006 — coverage moved away from a critical branch.
            if delta.lost_critical_protection() {
                out.push(finding(
                    "WVQ-PROTECT-006",
                    Severity::Error,
                    flow,
                    format!(
                        "critical branch(es) {} lost dynamic execution; \
                         a global coverage gain does not offset this",
                        delta.lost_critical_branches.join(", ")
                    ),
                ));
                // 010 — many new low-risk lines while a small high-risk path went.
                if delta.head_tests.len() > delta.base_tests.len() {
                    out.push(finding(
                        "WVQ-PROTECT-010",
                        Severity::Error,
                        flow,
                        format!(
                            "head added {} test(s) but dropped critical branch(es) {}",
                            delta.head_tests.len() - delta.base_tests.len(),
                            delta.lost_critical_branches.join(", ")
                        ),
                    ));
                }
            }
            // 007 — a previously proven requirement is no longer proven.
            if !delta.lost_obligations.is_empty() {
                out.push(finding(
                    "WVQ-PROTECT-007",
                    severity,
                    flow,
                    format!(
                        "obligation(s) {} were proven on base and are not now",
                        delta.lost_obligations.join(", ")
                    ),
                ));
            }
        }
        ProtectionDeltaState::Degraded => {
            // 007 — proven became partial.
            if delta.lost_obligations.is_empty() {
                out.push(finding(
                    "WVQ-PROTECT-006",
                    severity,
                    flow,
                    "fewer branches are exercised than on base".into(),
                ));
            } else {
                out.push(finding(
                    "WVQ-PROTECT-007",
                    severity,
                    flow,
                    format!(
                        "obligation(s) {} dropped from proven to partial",
                        delta.lost_obligations.join(", ")
                    ),
                ));
            }
        }
        ProtectionDeltaState::NewUnprotected => {
            // 009 — new or rewired flow with no relevant dynamic proof.
            out.push(finding(
                "WVQ-PROTECT-009",
                severity,
                flow,
                "new or rewired flow has no dynamic proof".into(),
            ));
        }
        ProtectionDeltaState::Replaced => {
            // 004 — healthy replacement. Recorded, never warned about.
            out.push(finding(
                "WVQ-PROTECT-004",
                Severity::Info,
                flow,
                format!(
                    "protection replaced: {} now proves what {} did",
                    delta.head_tests.join(", "),
                    delta.base_tests.join(", ")
                ),
            ));
        }
        ProtectionDeltaState::ObsoleteRemoved => {
            // 008 — flow and its tests went together with an approved removal.
            out.push(finding(
                "WVQ-PROTECT-008",
                Severity::Info,
                flow,
                "protection removed alongside an approved OpenSpec removal".into(),
            ));
        }
        ProtectionDeltaState::Preserved
        | ProtectionDeltaState::Improved
        | ProtectionDeltaState::Relocated
        | ProtectionDeltaState::Unknown => {}
    }
}

fn check_test(test: &TestChange, policy: &ProtectionPolicy, out: &mut Vec<ProtectionFinding>) {
    let severity = policy.severity_for(&test.flow);

    // 002 — the test survived by name but stopped reaching the flow.
    if test.survives && !test.lost_flows.is_empty() {
        out.push(finding(
            "WVQ-PROTECT-002",
            severity,
            &test.test,
            format!(
                "test still exists but no longer executes {}",
                test.lost_flows.join(", ")
            ),
        ));
    }

    // 003 — the only proof path was deleted with nothing taking over.
    if !test.survives && test.replaced_by.is_none() && !test.lost_obligations.is_empty() {
        out.push(finding(
            "WVQ-PROTECT-003",
            Severity::Error,
            &test.test,
            format!(
                "removed test was the only proof path for {}",
                test.lost_obligations.join(", ")
            ),
        ));
    }

    // 005 — an assertion was weakened without a new seal.
    if test.assertions_weakened && !test.new_oracle_seal {
        out.push(finding(
            "WVQ-PROTECT-005",
            Severity::Error,
            &test.test,
            "a sealed assertion was weakened without a new OracleSeal".into(),
        ));
    }

    // 011 — implementation and test changed together with nothing declaring it.
    if test.changed_with_implementation && !test.new_oracle_seal && !test.declared_spec_delta {
        out.push(finding(
            "WVQ-PROTECT-011",
            severity,
            &test.test,
            "POSSIBLE_TEST_ADAPTATION_TO_IMPLEMENTATION: test changed with the code it verifies, \
             with no SpecDelta, no new seal and no QA verification"
                .into(),
        ));
    }
}

fn check_trend(trend: &ProtectionTrend, out: &mut Vec<ProtectionFinding>) {
    // 012 — protection erodes across revisions without any single PR crossing a
    // threshold. Needs at least three points to be a trend rather than a blip.
    if trend.protectors.len() < 3 {
        return;
    }
    let declining = trend.protectors.windows(2).all(|pair| pair[1] <= pair[0]);
    let first = trend.protectors.first().copied().unwrap_or_default();
    let last = trend.protectors.last().copied().unwrap_or_default();
    if declining && last < first {
        out.push(finding(
            "WVQ-PROTECT-012",
            Severity::Warn,
            &trend.flow,
            format!("protection eroded from {first} protector(s) to {last} across revisions"),
        ));
    }
}

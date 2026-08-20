//! Spec §74–§77 protection delta.
//!
//! The rule that governs everything here is spec §77: a global coverage
//! improvement must never suppress a local protection loss. A change that raises
//! overall coverage while a critical base branch stops being executed is a
//! regression, and this module says so.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::protection::{FlowProtection, ProtectionSnapshot};

/// What happened to a flow's safety net. Spec §74.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionDeltaState {
    /// Equivalent or stronger runtime evidence for the same obligation.
    Preserved,
    /// Old protection kept and meaningful coverage added.
    Improved,
    /// Still protected, but fewer branches, nodes or scenarios run.
    Degraded,
    /// Base had measured, proven protection; head has no valid proof path.
    Lost,
    /// The old test went, another proves the same obligation and flow.
    Replaced,
    /// A refactor moved the implementation and protection followed it.
    Relocated,
    /// New or rewired behaviour with no protection at all.
    NewUnprotected,
    /// Protection went because the behaviour was intentionally removed.
    ObsoleteRemoved,
    /// Continuity could not be established. WVQ must not guess.
    Unknown,
}

impl ProtectionDeltaState {
    /// Stable token for transport.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preserved => "preserved",
            Self::Improved => "improved",
            Self::Degraded => "degraded",
            Self::Lost => "lost",
            Self::Replaced => "replaced",
            Self::Relocated => "relocated",
            Self::NewUnprotected => "new_unprotected",
            Self::ObsoleteRemoved => "obsolete_removed",
            Self::Unknown => "unknown",
        }
    }

    /// Whether this state must be shown to a human before the change lands.
    #[must_use]
    pub fn is_regression(self) -> bool {
        matches!(self, Self::Lost | Self::Degraded | Self::NewUnprotected)
    }
}

/// One flow's before-and-after safety net.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtectionDelta {
    /// Flow the delta is about.
    pub flow: String,
    /// Outcome.
    pub state: ProtectionDeltaState,
    /// Tests that protected it on base.
    pub base_tests: Vec<String>,
    /// Tests that protect it on head.
    pub head_tests: Vec<String>,
    /// Critical branches that stopped being executed.
    pub lost_critical_branches: Vec<String>,
    /// Obligations no longer proved.
    pub lost_obligations: Vec<String>,
    /// Why this verdict was reached, in order.
    pub reasons: Vec<String>,
}

impl ProtectionDelta {
    /// Whether a critical branch lost all dynamic execution.
    #[must_use]
    pub fn lost_critical_protection(&self) -> bool {
        !self.lost_critical_branches.is_empty()
    }
}

/// What the delta needs to know beyond the two snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeltaContext {
    /// Branches whose loss is never acceptable, whatever coverage does overall.
    pub critical_branches: Vec<String>,
    /// Flows an approved `OpenSpec` removal deliberately deleted.
    pub intentionally_removed: Vec<String>,
    /// Base flow → head flow, when a refactor renamed it.
    pub relocations: Vec<(String, String)>,
}

/// Summary counts for the `quality_verify` protection block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ProtectionSummary {
    /// Preserved flows.
    pub preserved: usize,
    /// Improved flows.
    pub improved: usize,
    /// Degraded flows.
    pub degraded: usize,
    /// Lost flows.
    pub lost: usize,
    /// Replaced flows.
    pub replaced: usize,
    /// Relocated flows.
    pub relocated: usize,
    /// New flows with no protection.
    pub new_unprotected: usize,
}

/// Count the deltas by state.
#[must_use]
pub fn summarise(deltas: &[ProtectionDelta]) -> ProtectionSummary {
    let mut out = ProtectionSummary::default();
    for delta in deltas {
        match delta.state {
            ProtectionDeltaState::Preserved => out.preserved += 1,
            ProtectionDeltaState::Improved => out.improved += 1,
            ProtectionDeltaState::Degraded => out.degraded += 1,
            ProtectionDeltaState::Lost => out.lost += 1,
            ProtectionDeltaState::Replaced => out.replaced += 1,
            ProtectionDeltaState::Relocated => out.relocated += 1,
            ProtectionDeltaState::NewUnprotected => out.new_unprotected += 1,
            ProtectionDeltaState::ObsoleteRemoved | ProtectionDeltaState::Unknown => {}
        }
    }
    out
}

/// Compare the base and head safety nets, flow by flow.
#[must_use]
pub fn protection_delta(
    base: &ProtectionSnapshot,
    head: &ProtectionSnapshot,
    context: &DeltaContext,
) -> Vec<ProtectionDelta> {
    let mut flows: BTreeSet<&str> = BTreeSet::new();
    flows.extend(base.flows.iter().map(|item| item.flow.as_str()));
    flows.extend(head.flows.iter().map(|item| item.flow.as_str()));

    let relocated_targets: BTreeSet<&str> = context
        .relocations
        .iter()
        .map(|(_, target)| target.as_str())
        .collect();

    let mut out = Vec::new();
    for flow in flows {
        // A relocated base flow is compared against its head name instead.
        if relocated_targets.contains(flow)
            && context
                .relocations
                .iter()
                .any(|(source, target)| target == flow && base.flow(source).is_some())
        {
            continue;
        }
        let head_name = context
            .relocations
            .iter()
            .find(|(source, _)| source == flow)
            .map(|(_, target)| target.as_str());
        let relocated = head_name.is_some();
        let base_flow = base.flow(flow);
        let head_flow = head.flow(head_name.unwrap_or(flow));
        out.push(compare(flow, base_flow, head_flow, relocated, context));
    }
    out
}

fn compare(
    flow: &str,
    base: Option<&FlowProtection>,
    head: Option<&FlowProtection>,
    relocated: bool,
    context: &DeltaContext,
) -> ProtectionDelta {
    let mut reasons = Vec::new();
    let base_tests = base.map(|item| item.tests.clone()).unwrap_or_default();
    let head_tests = head.map(|item| item.tests.clone()).unwrap_or_default();

    let (state, lost_critical, lost_obligations) = match (base, head) {
        (Some(base), None) => {
            if context
                .intentionally_removed
                .iter()
                .any(|item| item == flow)
            {
                reasons.push("an approved OpenSpec removal deleted this behaviour".into());
                (
                    ProtectionDeltaState::ObsoleteRemoved,
                    Vec::new(),
                    Vec::new(),
                )
            } else {
                reasons.push("base had measured protection, head has no proof path".into());
                (
                    ProtectionDeltaState::Lost,
                    critical_losses(base, &[], context),
                    base.proven_obligations.clone(),
                )
            }
        }
        (None, Some(head)) => {
            if head.is_protected() {
                reasons.push("new flow arrived with protection".into());
                (ProtectionDeltaState::Improved, Vec::new(), Vec::new())
            } else {
                reasons.push("new or rewired flow has no relevant dynamic proof".into());
                (ProtectionDeltaState::NewUnprotected, Vec::new(), Vec::new())
            }
        }
        (Some(base), Some(head)) => compare_present(base, head, relocated, context, &mut reasons),
        (None, None) => {
            reasons.push("no evidence on either revision".into());
            (ProtectionDeltaState::Unknown, Vec::new(), Vec::new())
        }
    };

    ProtectionDelta {
        flow: flow.to_owned(),
        state,
        base_tests,
        head_tests,
        lost_critical_branches: lost_critical,
        lost_obligations,
        reasons,
    }
}

/// Both revisions measured this flow. Decide what changed.
///
/// The order of the branches is the policy: the critical-branch check runs
/// first so no later gain can outvote it.
fn compare_present(
    base: &FlowProtection,
    head: &FlowProtection,
    relocated: bool,
    context: &DeltaContext,
    reasons: &mut Vec<String>,
) -> (ProtectionDeltaState, Vec<String>, Vec<String>) {
    let lost_critical = critical_losses(base, &head.covered_branches, context);
    let lost_obligations = difference(&base.proven_obligations, &head.proven_obligations);

    let state = if !lost_critical.is_empty() {
        reasons.push(format!(
            "critical branch(es) {} lost all dynamic execution",
            lost_critical.join(", ")
        ));
        reasons.push("a global coverage gain does not offset this".into());
        ProtectionDeltaState::Lost
    } else if !head.is_protected() {
        reasons.push("no test or session reaches this flow any more".into());
        ProtectionDeltaState::Lost
    } else if !lost_obligations.is_empty() {
        reasons.push(format!(
            "obligation(s) {} are no longer proved",
            lost_obligations.join(", ")
        ));
        ProtectionDeltaState::Degraded
    } else if relocated {
        reasons.push("implementation moved and protection followed it".into());
        ProtectionDeltaState::Relocated
    } else if base.tests != head.tests {
        reasons.push(
            "different tests prove the same obligations and flow with equivalent evidence".into(),
        );
        ProtectionDeltaState::Replaced
    } else if strictly_more(&head.covered_branches, &base.covered_branches) {
        reasons.push("same protection plus additional measured branches".into());
        ProtectionDeltaState::Improved
    } else if difference(&base.covered_branches, &head.covered_branches).is_empty() {
        reasons.push("equivalent runtime evidence for the same obligations".into());
        ProtectionDeltaState::Preserved
    } else {
        reasons.push("fewer branches are exercised than before".into());
        ProtectionDeltaState::Degraded
    };
    (state, lost_critical, lost_obligations)
}

fn critical_losses(
    base: &FlowProtection,
    head_branches: &[String],
    context: &DeltaContext,
) -> Vec<String> {
    difference(&base.covered_branches, head_branches)
        .into_iter()
        .filter(|branch| context.critical_branches.contains(branch))
        .collect()
}

fn strictly_more(left: &[String], right: &[String]) -> bool {
    difference(right, left).is_empty() && !difference(left, right).is_empty()
}

fn difference(left: &[String], right: &[String]) -> Vec<String> {
    let right_set: BTreeSet<&String> = right.iter().collect();
    let mut out: Vec<String> = left
        .iter()
        .filter(|item| !right_set.contains(item))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

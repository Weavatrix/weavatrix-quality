//! Requirement/scenario-scoped intent changes between two `OpenSpec` revisions.

use std::collections::{BTreeMap, BTreeSet};

use crate::openspec::slug_title;
use crate::{ClauseKind, OpenSpecChange, RequirementOp, SpecError};

/// Exact `OpenSpec` targets whose normative intent changed between revisions.
///
/// A requirement-level entry authorizes every scenario under that requirement.
/// A scenario-level entry authorizes only that scenario. Source locations are
/// deliberately excluded: moving unchanged prose is not an intent change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecChangeScope {
    changed_requirements: Vec<String>,
    changed_scenarios: Vec<(String, String)>,
}

impl SpecChangeScope {
    /// Construct a deterministic scope from already-normalized identities.
    #[must_use]
    pub fn from_parts(
        changed_requirements: Vec<String>,
        changed_scenarios: Vec<(String, String)>,
    ) -> Self {
        let mut changed_requirements = changed_requirements;
        changed_requirements.sort();
        changed_requirements.dedup();
        let requirement_set = changed_requirements.iter().collect::<BTreeSet<_>>();
        let mut changed_scenarios = changed_scenarios
            .into_iter()
            .filter(|(requirement, _)| !requirement_set.contains(requirement))
            .collect::<Vec<_>>();
        changed_scenarios.sort();
        changed_scenarios.dedup();
        Self {
            changed_requirements,
            changed_scenarios,
        }
    }

    /// Requirement-level changes, sorted and unique.
    #[must_use]
    pub fn changed_requirements(&self) -> &[String] {
        &self.changed_requirements
    }

    /// Scenario-only changes, sorted and unique.
    #[must_use]
    pub fn changed_scenarios(&self) -> &[(String, String)] {
        &self.changed_scenarios
    }

    /// Whether this exact requirement/scenario is authorized to change.
    #[must_use]
    pub fn authorizes(&self, requirement: &str, scenario: &str) -> bool {
        self.changed_requirements
            .binary_search_by(|item| item.as_str().cmp(requirement))
            .is_ok()
            || self
                .changed_scenarios
                .binary_search_by(|(candidate_requirement, candidate_scenario)| {
                    (candidate_requirement.as_str(), candidate_scenario.as_str())
                        .cmp(&(requirement, scenario))
                })
                .is_ok()
    }

    /// Whether no normative `OpenSpec` target changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changed_requirements.is_empty() && self.changed_scenarios.is_empty()
    }
}

/// Compare the same `OpenSpec` change folder at base and head.
///
/// `None` means the change folder did not exist at base, so every head
/// requirement is new. Ambiguous duplicate operations fail closed.
///
/// # Errors
///
/// Returns an error when the revisions name different changes or either
/// revision contains ambiguous duplicate requirement operations.
pub fn diff_spec_scope(
    base: Option<&OpenSpecChange>,
    head: &OpenSpecChange,
) -> Result<SpecChangeScope, SpecError> {
    if let Some(base) = base
        && base.id != head.id
    {
        return Err(SpecError::InvalidSyntax {
            file: "openspec/changes".into(),
            line: 1,
            message: format!(
                "cannot compare OpenSpec changes `{}` and `{}`",
                base.id, head.id
            ),
        });
    }
    let head = requirement_snapshots(head)?;
    let Some(base) = base else {
        return Ok(SpecChangeScope::from_parts(
            head.keys().cloned().collect(),
            Vec::new(),
        ));
    };
    let base = requirement_snapshots(base)?;
    let identities = base
        .keys()
        .chain(head.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut requirements = Vec::new();
    let mut scenarios = Vec::new();
    for identity in identities {
        let (Some(before), Some(after)) = (base.get(&identity), head.get(&identity)) else {
            requirements.push(identity);
            continue;
        };
        if before.kind != after.kind
            || before.name != after.name
            || before.text != after.text
            || before.rename != after.rename
        {
            requirements.push(identity);
            continue;
        }
        let scenario_ids = before
            .scenarios
            .keys()
            .chain(after.scenarios.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        for scenario in scenario_ids {
            if before.scenarios.get(&scenario) != after.scenarios.get(&scenario) {
                scenarios.push((identity.clone(), scenario));
            }
        }
    }
    Ok(SpecChangeScope::from_parts(requirements, scenarios))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationKind {
    Added,
    Modified,
    Removed,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequirementSnapshot {
    kind: OperationKind,
    name: String,
    text: String,
    rename: Option<(String, String)>,
    scenarios: BTreeMap<String, ScenarioSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScenarioSnapshot {
    name: String,
    clauses: Vec<(ClauseKind, String)>,
}

fn requirement_snapshots(
    change: &OpenSpecChange,
) -> Result<BTreeMap<String, RequirementSnapshot>, SpecError> {
    let mut snapshots = BTreeMap::new();
    for capability in &change.capabilities {
        for operation in &capability.operations {
            let (identity, snapshot) = match operation {
                RequirementOp::Added(delta) => (
                    delta.id.to_string(),
                    requirement_snapshot(OperationKind::Added, delta),
                ),
                RequirementOp::Modified(delta) => (
                    delta.id.to_string(),
                    requirement_snapshot(OperationKind::Modified, delta),
                ),
                RequirementOp::Removed(delta) => (
                    delta.id.to_string(),
                    requirement_snapshot(OperationKind::Removed, delta),
                ),
                RequirementOp::Renamed { from, to, .. } => {
                    let identity = format!(
                        "{}.{}",
                        capability.capability.replace('/', "."),
                        slug_title(to)
                    );
                    (
                        identity,
                        RequirementSnapshot {
                            kind: OperationKind::Renamed,
                            name: to.clone(),
                            text: String::new(),
                            rename: Some((from.clone(), to.clone())),
                            scenarios: BTreeMap::new(),
                        },
                    )
                }
            };
            if snapshots.insert(identity.clone(), snapshot).is_some() {
                return Err(SpecError::InvalidSyntax {
                    file: capability.source.display().to_string(),
                    line: 1,
                    message: format!("duplicate requirement operation `{identity}`"),
                });
            }
        }
    }
    Ok(snapshots)
}

fn requirement_snapshot(
    kind: OperationKind,
    delta: &crate::RequirementDelta,
) -> RequirementSnapshot {
    RequirementSnapshot {
        kind,
        name: delta.name.clone(),
        text: delta.text.clone(),
        rename: None,
        scenarios: delta
            .scenarios
            .iter()
            .map(|scenario| {
                (
                    scenario.id.to_string(),
                    ScenarioSnapshot {
                        name: scenario.name.clone(),
                        clauses: scenario
                            .clauses
                            .iter()
                            .map(|clause| (clause.kind, clause.text.clone()))
                            .collect(),
                    },
                )
            })
            .collect(),
    }
}

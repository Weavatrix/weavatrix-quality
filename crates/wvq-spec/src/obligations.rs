//! Compile `quality.yaml` rows into sealed [`TestObligation`] values.

use std::collections::{BTreeSet, HashSet};

use wvq_domain::{ObligationId, RequirementId, ScenarioId};

use crate::openspec::{OpenSpecChange, RequirementOp, SpecError};
use crate::quality_yaml::{EvidenceKind, ObligationKind, Predicate, QualityContract, RiskLevel};

/// Compiled obligation. Predicates stay with later IR work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestObligation {
    /// Obligation identity.
    pub id: ObligationId,
    /// Parent requirement.
    pub requirement: RequirementId,
    /// Parent scenario.
    pub scenario: ScenarioId,
    /// Kind from the contract.
    pub kind: ObligationKind,
    /// Optional sealed precondition.
    pub condition: Option<Predicate>,
    /// Optional sealed expected behavior.
    pub expected: Option<Predicate>,
    /// Evidence that must be collected for a `PROVEN` result.
    pub required_evidence: Vec<EvidenceKind>,
    /// Risk inherited from the contract default.
    pub risk: RiskLevel,
}

/// Bind contract rows to `OpenSpec` scenarios.
///
/// # Errors
///
/// Fails on duplicate obligation IDs, unknown requirement/scenario references,
/// or an empty obligation list.
pub fn compile_obligations(
    contract: &QualityContract,
    spec: &OpenSpecChange,
) -> Result<Vec<TestObligation>, SpecError> {
    if contract.change != spec.id {
        return Err(SpecError::InvalidSyntax {
            file: "quality.yaml".to_owned(),
            line: 1,
            message: format!(
                "contract change `{}` does not match OpenSpec change `{}`",
                contract.change, spec.id
            ),
        });
    }

    let mut known_requirements = BTreeSet::new();
    let mut known_scenarios = BTreeSet::new();
    collect_spec_refs(spec, &mut known_requirements, &mut known_scenarios);

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for req in &contract.requirements {
        let requirement_id = RequirementId::new(format!("{}.{}", req.capability, req.requirement))
            .map_err(|err| SpecError::InvalidSyntax {
                file: "quality.yaml".to_owned(),
                line: 1,
                message: err.to_string(),
            })?;
        if !known_requirements.contains(requirement_id.as_str()) {
            return Err(SpecError::InvalidSyntax {
                file: "quality.yaml".to_owned(),
                line: 1,
                message: format!("unknown requirement `{requirement_id}`"),
            });
        }
        for scenario in &req.scenarios {
            let scenario_id =
                ScenarioId::new(&scenario.scenario).map_err(|err| SpecError::InvalidSyntax {
                    file: "quality.yaml".to_owned(),
                    line: 1,
                    message: err.to_string(),
                })?;
            if !known_scenarios.contains(scenario_id.as_str()) {
                return Err(SpecError::InvalidSyntax {
                    file: "quality.yaml".to_owned(),
                    line: 1,
                    message: format!("unknown scenario `{scenario_id}`"),
                });
            }
            let required = scenario
                .evidence
                .as_ref()
                .map(|policy| policy.required.clone())
                .unwrap_or_default();
            for raw in &scenario.obligations {
                if !seen.insert(raw.id.clone()) {
                    return Err(SpecError::InvalidSyntax {
                        file: "quality.yaml".to_owned(),
                        line: 1,
                        message: format!("duplicate obligation id `{}`", raw.id),
                    });
                }
                out.push(TestObligation {
                    id: raw.id.clone(),
                    requirement: requirement_id.clone(),
                    scenario: scenario_id.clone(),
                    kind: raw.kind,
                    condition: raw.condition.clone(),
                    expected: raw.expected.clone(),
                    required_evidence: required.clone(),
                    risk: contract.risk.default,
                });
            }
        }
    }
    if out.is_empty() {
        return Err(SpecError::InvalidSyntax {
            file: "quality.yaml".to_owned(),
            line: 1,
            message: "quality contract has no obligations".to_owned(),
        });
    }
    Ok(out)
}

fn collect_spec_refs(
    spec: &OpenSpecChange,
    requirements: &mut BTreeSet<String>,
    scenarios: &mut BTreeSet<String>,
) {
    for cap in &spec.capabilities {
        for op in &cap.operations {
            match op {
                RequirementOp::Added(delta)
                | RequirementOp::Modified(delta)
                | RequirementOp::Removed(delta) => {
                    requirements.insert(delta.id.as_str().to_owned());
                    for scenario in &delta.scenarios {
                        scenarios.insert(scenario.id.as_str().to_owned());
                    }
                }
                RequirementOp::Renamed { to, .. } => {
                    requirements.insert(format!(
                        "{}.{}",
                        cap.capability.replace('/', "."),
                        slug(to)
                    ));
                }
            }
        }
    }
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut pending_hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() {
            pending_hyphen = true;
        }
    }
    out
}

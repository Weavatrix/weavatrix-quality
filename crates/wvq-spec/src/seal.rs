//! Canonical `OracleSeal` over intent — never over implementation metadata.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wvq_domain::{ChangeId, ContentHash, OracleSealId};

use crate::obligations::TestObligation;
use crate::openspec::{OpenSpecChange, RequirementOp, SpecError};
use crate::quality_yaml::QualityContract;

/// Sealed expected behavior. Independent of implementation repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleSeal {
    /// Seal schema. Currently `1`.
    pub schema_v: u32,
    /// Deterministic identity derived from [`Self::digest`].
    pub id: OracleSealId,
    /// Change this seal belongs to.
    pub change: ChangeId,
    /// Hashes of referenced requirement text, sorted.
    pub requirement_hashes: Vec<ContentHash>,
    /// Hashes of referenced scenario clauses, sorted.
    pub scenario_hashes: Vec<ContentHash>,
    /// Hashes of compiled obligations, sorted.
    pub obligation_hashes: Vec<ContentHash>,
    /// Hash of the quality policy subset (not AI tokens, mutation, or `on_failure`).
    pub quality_policy_hash: ContentHash,
    /// Hash of the canonical seal document.
    pub digest: ContentHash,
}

/// Build an [`OracleSeal`] from a compiled contract and its `OpenSpec` source.
///
/// AI token hints, mutation operators, and `on_failure` evidence are excluded.
///
/// # Errors
///
/// Returns [`SpecError`] when a hash cannot be formed.
pub fn seal(
    contract: &QualityContract,
    obligations: &[TestObligation],
    spec: &OpenSpecChange,
) -> Result<OracleSeal, SpecError> {
    let requirement_hashes = hash_requirements(obligations, spec)?;
    let scenario_hashes = hash_scenarios(obligations, spec)?;
    let obligation_hashes = hash_obligations(obligations)?;
    let quality_policy_hash = sha256_json(&json!({
        "risk": { "default": contract.risk.default },
    }))?;

    let document = json!({
        "schema_v": 1,
        "change": contract.change.as_str(),
        "requirement_hashes": hashes_as_strings(&requirement_hashes),
        "scenario_hashes": hashes_as_strings(&scenario_hashes),
        "obligation_hashes": hashes_as_strings(&obligation_hashes),
        "quality_policy_hash": quality_policy_hash.as_str(),
    });
    let digest = sha256_json(&document)?;
    let id = OracleSealId::new(format!("oseal-{}", &digest.as_str()[..16])).map_err(|err| {
        SpecError::InvalidSyntax {
            file: "oracle-seal".to_owned(),
            line: 1,
            message: err.to_string(),
        }
    })?;

    Ok(OracleSeal {
        schema_v: 1,
        id,
        change: contract.change.clone(),
        requirement_hashes,
        scenario_hashes,
        obligation_hashes,
        quality_policy_hash,
        digest,
    })
}

fn hash_requirements(
    obligations: &[TestObligation],
    spec: &OpenSpecChange,
) -> Result<Vec<ContentHash>, SpecError> {
    let mut hashes = Vec::new();
    let mut seen = BTreeSeen::default();
    for obligation in obligations {
        if !seen.insert(obligation.requirement.as_str()) {
            continue;
        }
        let Some(delta) = find_requirement(spec, obligation.requirement.as_str()) else {
            return Err(SpecError::InvalidSyntax {
                file: "oracle-seal".to_owned(),
                line: 1,
                message: format!("missing requirement `{}` while sealing", obligation.requirement),
            });
        };
        hashes.push(sha256_json(&json!({
            "id": delta.id.as_str(),
            "name": delta.name,
            "text": delta.text,
        }))?);
    }
    hashes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(hashes)
}

fn hash_scenarios(
    obligations: &[TestObligation],
    spec: &OpenSpecChange,
) -> Result<Vec<ContentHash>, SpecError> {
    let mut hashes = Vec::new();
    let mut seen = BTreeSeen::default();
    for obligation in obligations {
        let key = format!(
            "{}::{}",
            obligation.requirement.as_str(),
            obligation.scenario.as_str()
        );
        if !seen.insert(&key) {
            continue;
        }
        let Some(scenario) = find_scenario(
            spec,
            obligation.requirement.as_str(),
            obligation.scenario.as_str(),
        ) else {
            return Err(SpecError::InvalidSyntax {
                file: "oracle-seal".to_owned(),
                line: 1,
                message: format!("missing scenario `{}` while sealing", obligation.scenario),
            });
        };
        let clauses: Vec<Value> = scenario
            .clauses
            .iter()
            .map(|clause| {
                json!({
                    "kind": format!("{:?}", clause.kind).to_ascii_lowercase(),
                    "text": clause.text,
                })
            })
            .collect();
        hashes.push(sha256_json(&json!({
            "id": scenario.id.as_str(),
            "name": scenario.name,
            "clauses": clauses,
        }))?);
    }
    hashes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(hashes)
}

fn hash_obligations(obligations: &[TestObligation]) -> Result<Vec<ContentHash>, SpecError> {
    let mut hashes = Vec::with_capacity(obligations.len());
    for obligation in obligations {
        let mut evidence: Vec<&str> = obligation
            .required_evidence
            .iter()
            .copied()
            .map(evidence_name)
            .collect();
        evidence.sort_unstable();
        hashes.push(sha256_json(&json!({
            "id": obligation.id.as_str(),
            "requirement": obligation.requirement.as_str(),
            "scenario": obligation.scenario.as_str(),
            "kind": obligation_kind_name(obligation.kind),
            "required_evidence": evidence,
        }))?);
    }
    hashes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(hashes)
}

fn find_requirement<'a>(
    spec: &'a OpenSpecChange,
    id: &str,
) -> Option<&'a crate::openspec::RequirementDelta> {
    spec.capabilities.iter().find_map(|cap| {
        cap.operations.iter().find_map(|op| match op {
            RequirementOp::Added(delta)
            | RequirementOp::Modified(delta)
            | RequirementOp::Removed(delta)
                if delta.id.as_str() == id =>
            {
                Some(delta)
            }
            _ => None,
        })
    })
}

fn find_scenario<'a>(
    spec: &'a OpenSpecChange,
    requirement: &str,
    scenario: &str,
) -> Option<&'a crate::openspec::ScenarioDelta> {
    find_requirement(spec, requirement)?
        .scenarios
        .iter()
        .find(|item| item.id.as_str() == scenario)
}

fn hashes_as_strings(hashes: &[ContentHash]) -> Vec<&str> {
    hashes.iter().map(ContentHash::as_str).collect()
}

fn sha256_json(value: &Value) -> Result<ContentHash, SpecError> {
    let canonical = serde_json::to_vec(value).map_err(|err| SpecError::InvalidSyntax {
        file: "oracle-seal".to_owned(),
        line: 1,
        message: err.to_string(),
    })?;
    let digest = Sha256::digest(canonical);
    let hex = digest.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
        out
    });
    ContentHash::new(hex).map_err(|err| SpecError::InvalidSyntax {
        file: "oracle-seal".to_owned(),
        line: 1,
        message: err.to_string(),
    })
}

fn obligation_kind_name(kind: crate::quality_yaml::ObligationKind) -> &'static str {
    match kind {
        crate::quality_yaml::ObligationKind::Behavioral => "behavioral",
        crate::quality_yaml::ObligationKind::Invariant => "invariant",
        crate::quality_yaml::ObligationKind::Api => "api",
        crate::quality_yaml::ObligationKind::Contract => "contract",
        crate::quality_yaml::ObligationKind::Permission => "permission",
        crate::quality_yaml::ObligationKind::Accessibility => "accessibility",
        crate::quality_yaml::ObligationKind::Visual => "visual",
        crate::quality_yaml::ObligationKind::Performance => "performance",
        crate::quality_yaml::ObligationKind::Architecture => "architecture",
        crate::quality_yaml::ObligationKind::CodeHealth => "code_health",
        crate::quality_yaml::ObligationKind::Coverage => "coverage",
        crate::quality_yaml::ObligationKind::Mutation => "mutation",
        crate::quality_yaml::ObligationKind::Metamorphic => "metamorphic",
        crate::quality_yaml::ObligationKind::SecurityPolicy => "security_policy",
    }
}

fn evidence_name(kind: crate::quality_yaml::EvidenceKind) -> &'static str {
    match kind {
        crate::quality_yaml::EvidenceKind::Dom => "dom",
        crate::quality_yaml::EvidenceKind::Network => "network",
        crate::quality_yaml::EvidenceKind::Screenshot => "screenshot",
        crate::quality_yaml::EvidenceKind::Trace => "trace",
        crate::quality_yaml::EvidenceKind::Har => "har",
        crate::quality_yaml::EvidenceKind::Console => "console",
        crate::quality_yaml::EvidenceKind::Storage => "storage",
        crate::quality_yaml::EvidenceKind::Coverage => "coverage",
    }
}

#[derive(Default)]
struct BTreeSeen(std::collections::BTreeSet<String>);

impl BTreeSeen {
    fn insert(&mut self, key: &str) -> bool {
        self.0.insert(key.to_owned())
    }
}

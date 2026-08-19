//! Strict `quality.yaml` loader.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use wvq_domain::{ChangeId, ObligationId};

use crate::openspec::SpecError;

/// Parsed quality contract for one change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityContract {
    /// Schema version. Only `1` is accepted.
    pub quality_contract_v: u32,
    /// Change this contract seals.
    pub change: ChangeId,
    /// Default risk for compiled obligations.
    #[serde(default)]
    pub risk: RiskConfig,
    /// Requirement → scenario → obligation bindings.
    pub requirements: Vec<QualityRequirement>,
    /// AI budget hints. Never part of [`crate::OracleSeal`].
    #[serde(default)]
    pub ai: Option<AiHints>,
}

/// Risk defaults from the contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskConfig {
    /// Default risk level for obligations in this contract.
    pub default: RiskLevel,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            default: RiskLevel::Medium,
        }
    }
}

/// Spec §13 risk levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Low blast radius / well-covered.
    Low,
    /// Default.
    Medium,
    /// Requires stronger proof / broader selection.
    High,
    /// Always human-visible; cannot omit mandatory proof.
    Critical,
}

/// Planning/runtime token hints. Implementation metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiHints {
    /// Tokens allowed while compiling / planning.
    #[serde(default)]
    pub planning_tokens: u64,
    /// Tokens allowed at runtime. Green path must stay 0.
    #[serde(default)]
    pub runtime_tokens: u64,
}

/// One capability/requirement binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityRequirement {
    /// Capability path (`sankey`).
    pub capability: String,
    /// Requirement slug (`visual-limit-others`).
    pub requirement: String,
    /// Scenario bindings.
    pub scenarios: Vec<QualityScenario>,
}

/// Scenario-level obligations and generation hints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityScenario {
    /// Scenario slug (`overflow-grouped`).
    pub scenario: String,
    /// Actor matrix. Generation metadata, not sealed.
    #[serde(default)]
    pub actors: Option<ActorFilter>,
    /// Boundary dimensions. Generation metadata, not sealed.
    #[serde(default)]
    pub dimensions: BTreeMap<String, Dimension>,
    /// Obligations that must be proven.
    pub obligations: Vec<RawObligation>,
    /// Evidence policy for this scenario.
    #[serde(default)]
    pub evidence: Option<EvidencePolicy>,
    /// Mutation hints. Not sealed.
    #[serde(default)]
    pub mutation: Option<MutationHints>,
}

/// Included/excluded actors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorFilter {
    /// Actors that must be exercised.
    #[serde(default)]
    pub include: Vec<String>,
    /// Actors explicitly excluded.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Named test dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dimension {
    /// Discrete values.
    #[serde(default)]
    pub values: Vec<serde_yaml::Value>,
    /// Named classes (`below_limit`).
    #[serde(default)]
    pub classes: Vec<String>,
}

/// Obligation row from YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawObligation {
    /// Obligation identity.
    pub id: ObligationId,
    /// Obligation kind.
    pub kind: ObligationKind,
}

/// Spec §7 obligation kinds. Unknown values fail closed via Serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationKind {
    /// Observable UI/runtime behavior.
    Behavioral,
    /// Boundary or conservation invariant.
    Invariant,
    /// HTTP/RPC surface.
    Api,
    /// Schema/contract compatibility.
    Contract,
    /// Authorization.
    Permission,
    /// Accessibility.
    Accessibility,
    /// Visual / screenshot oracle.
    Visual,
    /// Performance budget.
    Performance,
    /// Architecture firewall.
    Architecture,
    /// Code-health ratchet.
    CodeHealth,
    /// Coverage / protection continuity.
    Coverage,
    /// Mutation sensitivity.
    Mutation,
    /// Metamorphic relation.
    Metamorphic,
    /// Security policy.
    SecurityPolicy,
}

/// Required vs failure-only evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePolicy {
    /// Must be collected on every run.
    #[serde(default)]
    pub required: Vec<EvidenceKind>,
    /// Collected only on failure. Not sealed.
    #[serde(default)]
    pub on_failure: Vec<EvidenceKind>,
}

/// Known evidence kinds. Unknown YAML values fail closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Accessibility / DOM digest.
    Dom,
    /// Network operations.
    Network,
    /// Screenshot (usually failure-only).
    Screenshot,
    /// Playwright / runner trace.
    Trace,
    /// HAR archive.
    Har,
    /// Console errors/warnings.
    Console,
    /// Storage mutation.
    Storage,
    /// Measured coverage.
    Coverage,
}

/// Mutation-operator hints. Not sealed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationHints {
    /// Operator names (`boundary_flip`).
    #[serde(default)]
    pub operators: Vec<String>,
}

/// Load `openspec/changes/<change>/quality.yaml`.
///
/// # Errors
///
/// Returns [`SpecError`] when the file is missing, the schema version is not
/// `1`, YAML is malformed, or unknown fields/enums are present.
pub fn load_quality_contract(root: &Path, change: &str) -> Result<QualityContract, SpecError> {
    let path = root
        .join("openspec")
        .join("changes")
        .join(change)
        .join("quality.yaml");
    let raw = fs::read_to_string(&path).map_err(|err| SpecError::Io {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let contract: QualityContract = serde_yaml::from_str(&raw).map_err(|err| {
        SpecError::InvalidSyntax {
            file: path.display().to_string(),
            line: 1,
            message: err.to_string(),
        }
    })?;
    if contract.quality_contract_v != 1 {
        return Err(SpecError::InvalidSyntax {
            file: path.display().to_string(),
            line: 1,
            message: format!(
                "unknown quality_contract_v {}",
                contract.quality_contract_v
            ),
        });
    }
    if contract.change.as_str() != change {
        return Err(SpecError::InvalidSyntax {
            file: path.display().to_string(),
            line: 1,
            message: format!(
                "quality.yaml change `{}` does not match folder `{change}`",
                contract.change
            ),
        });
    }
    Ok(contract)
}

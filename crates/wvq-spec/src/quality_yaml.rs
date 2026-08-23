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
    /// Optional predicate that must hold before the expectation is evaluated.
    #[serde(default)]
    pub condition: Option<Predicate>,
    /// Sealed expected behavior. Required when a `TestProgram` asserts this obligation.
    #[serde(default)]
    pub expected: Option<Predicate>,
}

/// Semantic target used by sealed browser predicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PredicateTarget {
    /// ARIA role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Accessible name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessible_name: Option<String>,
    /// Associated label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Project-stable test id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    /// Last-resort CSS selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_css: Option<String>,
    /// Optional semantic scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Box<PredicateTarget>>,
}

/// Deterministic predicate sealed independently of implementation repair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Predicate {
    /// Target is visible.
    Visible { target: PredicateTarget },
    /// Target is hidden or absent.
    Hidden { target: PredicateTarget },
    /// Target accepts interaction.
    Enabled { target: PredicateTarget },
    /// Target rejects interaction.
    Disabled { target: PredicateTarget },
    /// Target text equals the expected string after trimming.
    TextEquals {
        target: PredicateTarget,
        value: String,
    },
    /// Target text contains the expected string.
    TextContains {
        target: PredicateTarget,
        value: String,
    },
    /// Input value equals the expected string.
    ValueEquals {
        target: PredicateTarget,
        value: String,
    },
    /// Current route equals the expected path or URL.
    RouteEquals { value: String },
    /// Current route contains the expected fragment.
    RouteContains { value: String },
    /// A matching response was observed.
    NetworkResponse {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        method: Option<String>,
        url_contains: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<u16>,
    },
    /// No console messages at the error level were observed.
    NoConsoleErrors,
    /// Web-storage value equals the expectation.
    StorageEquals {
        area: StorageArea,
        key: String,
        value: String,
    },
    /// Web-storage key is absent.
    StorageAbsent { area: StorageArea, key: String },
    /// Named API operation returned the expected status.
    ApiStatus { operation: String, status: u16 },
    /// JSON Pointer in a named API response equals a sealed JSON value.
    ApiJsonEquals {
        operation: String,
        pointer: String,
        value: serde_json::Value,
    },
    /// Exactly one rendered node matches the target.
    ///
    /// A sealed expectation, not a code-health finding: "there is one Save
    /// button in this dialog" is product intent, and the automatic duplicate
    /// detectors say nothing about whether it was intended.
    Unique { target: PredicateTarget },
    /// At most `max` rendered nodes match the target.
    MaxMultiplicity { target: PredicateTarget, max: u32 },
    /// The target actually receives pointer events on at least
    /// `min_ratio_permille` of its probed points.
    ReceivesEvents {
        target: PredicateTarget,
        min_ratio_permille: u16,
    },
    /// The target lies inside the viewport, with `margin_px` of slack.
    InsideViewport {
        target: PredicateTarget,
        margin_px: u32,
    },
    /// The target's text is not clipped by its own box.
    TextNotClipped { target: PredicateTarget },
    /// Two targets overlap by no more than `max_ratio_permille` of the first.
    NoOverlap {
        target: PredicateTarget,
        with: PredicateTarget,
        max_ratio_permille: u16,
    },
    /// Every nested predicate must hold.
    All { predicates: Vec<Predicate> },
    /// At least one nested predicate must hold.
    Any { predicates: Vec<Predicate> },
    /// Negate one predicate.
    Not { predicate: Box<Predicate> },
}

/// Browser storage namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageArea {
    /// `localStorage`.
    Local,
    /// `sessionStorage`.
    Session,
}

impl PredicateTarget {
    fn validate(&self) -> Result<(), String> {
        let identities = [
            self.role.as_deref(),
            self.accessible_name.as_deref(),
            self.label.as_deref(),
            self.test_id.as_deref(),
            self.fallback_css.as_deref(),
        ];
        if identities
            .iter()
            .flatten()
            .all(|value| value.trim().is_empty())
        {
            return Err("predicate target needs a semantic identity".into());
        }
        if identities
            .iter()
            .flatten()
            .any(|value| value.to_ascii_lowercase().contains("xpath"))
        {
            return Err("XPath is not a predicate target identity".into());
        }
        if let Some(scope) = &self.scope {
            scope.validate()?;
        }
        Ok(())
    }
}

impl Predicate {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Visible { target }
            | Self::Hidden { target }
            | Self::Enabled { target }
            | Self::Disabled { target }
            | Self::TextEquals { target, .. }
            | Self::TextContains { target, .. }
            | Self::ValueEquals { target, .. }
            | Self::Unique { target }
            | Self::TextNotClipped { target } => target.validate(),
            Self::MaxMultiplicity { target, max } => {
                target.validate()?;
                // `max: 0` says "this must not exist", which is what `hidden`
                // is for. Accepting both would give one expectation two spellings.
                if *max == 0 {
                    return Err("max_multiplicity max must be at least 1; use `hidden`".into());
                }
                Ok(())
            }
            Self::ReceivesEvents {
                target,
                min_ratio_permille,
            } => {
                target.validate()?;
                require_permille("receives_events min_ratio_permille", *min_ratio_permille)
            }
            Self::InsideViewport { target, margin_px } => {
                target.validate()?;
                if *margin_px > 4_096 {
                    return Err("inside_viewport margin_px must be at most 4096".into());
                }
                Ok(())
            }
            Self::NoOverlap {
                target,
                with,
                max_ratio_permille,
            } => {
                target.validate()?;
                with.validate()?;
                if target == with {
                    return Err("no_overlap needs two different targets".into());
                }
                require_permille("no_overlap max_ratio_permille", *max_ratio_permille)
            }
            Self::RouteEquals { value } | Self::RouteContains { value } => {
                require_non_empty("route predicate value", value)
            }
            Self::NetworkResponse {
                method,
                url_contains,
                status,
            } => {
                if let Some(method) = method {
                    require_non_empty("network method", method)?;
                }
                require_non_empty("network URL fragment", url_contains)?;
                if status.is_some_and(|status| !(100..=599).contains(&status)) {
                    return Err("network status must be between 100 and 599".into());
                }
                Ok(())
            }
            Self::NoConsoleErrors => Ok(()),
            Self::StorageEquals { key, .. } | Self::StorageAbsent { key, .. } => {
                require_non_empty("storage key", key)
            }
            Self::ApiStatus { operation, status } => {
                require_non_empty("API operation", operation)?;
                if !(100..=599).contains(status) {
                    return Err("API status must be between 100 and 599".into());
                }
                Ok(())
            }
            Self::ApiJsonEquals {
                operation, pointer, ..
            } => {
                require_non_empty("API operation", operation)?;
                if !pointer.is_empty() && !pointer.starts_with('/') {
                    return Err("API JSON pointer must be empty or start with `/`".into());
                }
                Ok(())
            }
            Self::All { predicates } | Self::Any { predicates } => {
                if predicates.is_empty() {
                    return Err("predicate group must not be empty".into());
                }
                predicates.iter().try_for_each(Self::validate)
            }
            Self::Not { predicate } => predicate.validate(),
        }
    }
}

fn require_non_empty(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} must be non-empty"))
    } else {
        Ok(())
    }
}

/// Ratios are sealed as permille so an expectation compares and hashes exactly.
fn require_permille(label: &str, value: u16) -> Result<(), String> {
    if value > 1_000 {
        return Err(format!("{label} must be between 0 and 1000"));
    }
    Ok(())
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
    let contract: QualityContract =
        serde_yaml::from_str(&raw).map_err(|err| SpecError::InvalidSyntax {
            file: path.display().to_string(),
            line: 1,
            message: err.to_string(),
        })?;
    if contract.quality_contract_v != 1 {
        return Err(SpecError::InvalidSyntax {
            file: path.display().to_string(),
            line: 1,
            message: format!("unknown quality_contract_v {}", contract.quality_contract_v),
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
    for obligation in contract
        .requirements
        .iter()
        .flat_map(|requirement| &requirement.scenarios)
        .flat_map(|scenario| &scenario.obligations)
    {
        for predicate in obligation.condition.iter().chain(&obligation.expected) {
            predicate
                .validate()
                .map_err(|message| SpecError::InvalidSyntax {
                    file: path.display().to_string(),
                    line: 1,
                    message: format!("obligation {}: {message}", obligation.id),
                })?;
        }
    }
    Ok(contract)
}

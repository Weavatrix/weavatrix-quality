//! Typed `TestProgram` IR. Canonical tests are programs, not Playwright source.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wvq_domain::{ObligationId, ProgramId};

/// Why a program or target was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProgramError {
    /// `schema_v` is not `1`.
    #[error("unknown test_program schema_v {0}")]
    UnknownSchema(u32),
    /// Action/target/field is not in the IR.
    #[error("{0}")]
    Invalid(String),
    /// JSON could not be decoded, including unknown fields.
    #[error("malformed TestProgram: {0}")]
    Malformed(String),
}

/// When a binary/text artifact may be captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureWhen {
    /// Never capture.
    Never,
    /// Capture only after a failed assertion.
    OnFailure,
    /// Capture on every step.
    Always,
}

/// Evidence collection for one program. Screenshot follows this policy only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePolicy {
    /// Screenshot capture.
    #[serde(default = "never")]
    pub screenshot: CaptureWhen,
    /// Playwright trace.
    #[serde(default = "never")]
    pub trace: CaptureWhen,
    /// Network metadata (not bodies).
    #[serde(default = "always")]
    pub network: CaptureWhen,
    /// Console messages.
    #[serde(default = "always")]
    pub console: CaptureWhen,
    /// Web storage keys.
    #[serde(default = "on_failure")]
    pub storage: CaptureWhen,
}

fn never() -> CaptureWhen {
    CaptureWhen::Never
}
fn always() -> CaptureWhen {
    CaptureWhen::Always
}
fn on_failure() -> CaptureWhen {
    CaptureWhen::OnFailure
}

impl Default for EvidencePolicy {
    fn default() -> Self {
        Self {
            screenshot: CaptureWhen::Never,
            trace: CaptureWhen::Never,
            network: CaptureWhen::Always,
            console: CaptureWhen::Always,
            storage: CaptureWhen::OnFailure,
        }
    }
}

impl EvidencePolicy {
    /// Whether a screenshot handle may appear on this observation.
    #[must_use]
    pub fn allow_screenshot(&self, failed: bool) -> bool {
        match self.screenshot {
            CaptureWhen::Always => true,
            CaptureWhen::OnFailure => failed,
            CaptureWhen::Never => false,
        }
    }
}

/// How the program was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramSource {
    /// Hand-authored against sealed obligations.
    Authored,
    /// Promoted from a recorded session (later task).
    Recorded,
    /// Recovered candidate. Cannot seal by itself.
    Recovered,
}

/// Semantic UI target. `XPath` is not a field and unknown keys fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Target {
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
    /// Component name hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_hint: Option<String>,
    /// Last-resort CSS. Never `XPath`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_css: Option<String>,
}

impl Target {
    fn validate(&self) -> Result<(), ProgramError> {
        let empty = self.role.is_none()
            && self.accessible_name.is_none()
            && self.label.is_none()
            && self.test_id.is_none()
            && self.component_hint.is_none()
            && self.fallback_css.is_none();
        if empty {
            return Err(ProgramError::Invalid(
                "target needs a semantic identity (test_id, role, name, label, or CSS fallback)"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// Wait predicate. Timeouts are explicit; no implicit sleeps in the IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WaitCondition {
    /// Target is visible.
    Visible {
        /// Target.
        target: Target,
    },
    /// URL matches a prefix/path.
    Url {
        /// Route prefix or path.
        route: String,
    },
}

/// Typed action. Unknown `action` tags fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TestAction {
    /// Open a route.
    Navigate {
        /// Application route.
        route: String,
    },
    /// Click / press a control.
    Activate {
        /// Semantic target.
        target: Target,
    },
    /// Fill an input.
    Fill {
        /// Semantic target.
        target: Target,
        /// Fixture value (not a locator).
        value: String,
    },
    /// Choose an option.
    Select {
        /// Semantic target.
        target: Target,
        /// Option value.
        value: String,
    },
    /// Keyboard key.
    Press {
        /// Optional focused target.
        #[serde(default)]
        target: Option<Target>,
        /// Key token (`Enter`).
        key: String,
    },
    /// Deterministic wait.
    Wait {
        /// Condition.
        condition: WaitCondition,
    },
    /// Feature flag.
    SetFeatureFlag {
        /// Flag key.
        key: String,
        /// Flag value.
        value: String,
    },
    /// Inject a named fault.
    InjectFault {
        /// Fault identity.
        fault: String,
    },
    /// Direct API operation.
    ApiCall {
        /// Operation id.
        operation: String,
        /// Input fixture name.
        input: String,
    },
    /// Assert a sealed obligation.
    Assert {
        /// Obligation id.
        obligation: ObligationId,
    },
}

impl TestAction {
    fn validate(&self) -> Result<(), ProgramError> {
        match self {
            Self::Navigate { route } if route.is_empty() => Err(ProgramError::Invalid(
                "navigate route must be non-empty".into(),
            )),
            Self::Activate { target }
            | Self::Fill { target, .. }
            | Self::Select { target, .. }
            | Self::Wait {
                condition: WaitCondition::Visible { target },
            } => target.validate(),
            Self::Press { target, key } => {
                if key.is_empty() {
                    return Err(ProgramError::Invalid("press key must be non-empty".into()));
                }
                target.as_ref().map_or(Ok(()), Target::validate)
            }
            Self::Wait {
                condition: WaitCondition::Url { route },
            } if route.is_empty() => {
                Err(ProgramError::Invalid("wait url must be non-empty".into()))
            }
            Self::SetFeatureFlag { key, .. } if key.is_empty() => Err(ProgramError::Invalid(
                "feature flag key must be non-empty".into(),
            )),
            Self::InjectFault { fault } if fault.is_empty() => {
                Err(ProgramError::Invalid("fault id must be non-empty".into()))
            }
            Self::ApiCall { operation, .. } if operation.is_empty() => Err(ProgramError::Invalid(
                "api operation must be non-empty".into(),
            )),
            _ => Ok(()),
        }
    }
}

/// Canonical browser/API program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestProgram {
    /// Schema version. Only `1`.
    pub schema_v: u32,
    /// Program identity.
    pub id: ProgramId,
    /// Provenance.
    pub source: ProgramSource,
    /// Sealed obligations this program may prove.
    pub obligations: Vec<ObligationId>,
    /// Ordered steps.
    pub steps: Vec<TestAction>,
    /// Capture policy.
    #[serde(default)]
    pub evidence_policy: EvidencePolicy,
    /// Deterministic seed for fixtures/faults.
    #[serde(default)]
    pub deterministic_seed: Option<u64>,
}

impl TestProgram {
    /// Decode and validate a program document.
    ///
    /// # Errors
    ///
    /// Unknown schema, unknown fields/actions, empty/XPath-like targets, or
    /// missing obligations/steps.
    pub fn from_json(raw: &str) -> Result<Self, ProgramError> {
        if raw.contains("\"xpath\"") {
            return Err(ProgramError::Invalid(
                "XPath is not a TestProgram identity".into(),
            ));
        }
        let program: Self =
            serde_json::from_str(raw).map_err(|err| ProgramError::Malformed(err.to_string()))?;
        program.validate()?;
        Ok(program)
    }

    /// Structural validation.
    ///
    /// # Errors
    ///
    /// Returns [`ProgramError`] when the program cannot be executed.
    pub fn validate(&self) -> Result<(), ProgramError> {
        if self.schema_v != 1 {
            return Err(ProgramError::UnknownSchema(self.schema_v));
        }
        if self.obligations.is_empty() {
            return Err(ProgramError::Invalid(
                "TestProgram needs at least one obligation".into(),
            ));
        }
        if self.steps.is_empty() {
            return Err(ProgramError::Invalid(
                "TestProgram needs at least one step".into(),
            ));
        }
        for step in &self.steps {
            step.validate()?;
        }
        Ok(())
    }
}

/// Structured observation. Binary screenshots stay handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Observation {
    /// Current route.
    #[serde(default)]
    pub route: Option<String>,
    /// Accessibility / DOM digest.
    #[serde(default)]
    pub a11y_digest: Option<String>,
    /// Network metadata (method + URL), not bodies.
    #[serde(default)]
    pub network: Vec<String>,
    /// Console lines.
    #[serde(default)]
    pub console: Vec<String>,
    /// Storage keys.
    #[serde(default)]
    pub storage: BTreeMap<String, String>,
    /// Viewport `WxH`.
    #[serde(default)]
    pub viewport: Option<String>,
    /// CAS handle. Absent unless the evidence policy allows it.
    #[serde(default)]
    pub screenshot_handle: Option<String>,
}

/// Apply [`EvidencePolicy`] to a raw observation.
#[must_use]
pub fn filter_observation(
    mut observation: Observation,
    policy: &EvidencePolicy,
    failed: bool,
) -> Observation {
    if !policy.allow_screenshot(failed) {
        observation.screenshot_handle = None;
    }
    if matches!(policy.network, CaptureWhen::Never)
        || (matches!(policy.network, CaptureWhen::OnFailure) && !failed)
    {
        observation.network.clear();
    }
    if matches!(policy.console, CaptureWhen::Never)
        || (matches!(policy.console, CaptureWhen::OnFailure) && !failed)
    {
        observation.console.clear();
    }
    if matches!(policy.storage, CaptureWhen::Never)
        || (matches!(policy.storage, CaptureWhen::OnFailure) && !failed)
    {
        observation.storage.clear();
    }
    observation
}

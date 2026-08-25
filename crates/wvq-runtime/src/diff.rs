//! Base/head replay and structured-before-visual-digest `BehaviorDelta`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::behavior::{BehaviorState, ReplayHost, replay_program};
use crate::program::{Observation, ProgramError, TestProgram};

/// Comparison axes, in the spec-mandated order. Visual digest is last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffAxis {
    /// Current route.
    Route,
    /// Accessibility / DOM digest.
    A11y,
    /// Visible semantic text.
    SemanticText,
    /// Normalized component / modal / data class.
    Component,
    /// Network operation set (not bodies).
    Network,
    /// Console lines.
    Console,
    /// Web storage keys.
    Storage,
    /// Viewport / geometry.
    Geometry,
    /// SHA-256 of a named visual surface. Not a perceptual pixel kernel.
    ///
    /// Wire token is `visual_digest`. The old `pixel` token is accepted when
    /// reading stored artefacts so a renamed axis does not look like a new one.
    #[serde(alias = "pixel")]
    VisualDigest,
}

impl DiffAxis {
    /// The visual digest is not a structured axis.
    #[must_use]
    pub fn is_structured(&self) -> bool {
        *self != Self::VisualDigest
    }

    /// Wire token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Route => "route",
            Self::A11y => "a11y",
            Self::SemanticText => "semantic_text",
            Self::Component => "component",
            Self::Network => "network",
            Self::Console => "console",
            Self::Storage => "storage",
            Self::Geometry => "geometry",
            Self::VisualDigest => "visual_digest",
        }
    }
}

/// One axis that differed between base and head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxisDelta {
    /// Which axis.
    pub axis: DiffAxis,
    /// Base rendering.
    pub base: String,
    /// Head rendering.
    pub head: String,
}

/// Structured observation delta. Not a quality percentage.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BehaviorDelta {
    /// Changed axes, structured first. Visual digest appears only if no structured change.
    pub axes: Vec<AxisDelta>,
    /// First structured change, if any.
    pub first_structured: Option<DiffAxis>,
    /// True only when structured axes matched and both sides had a visual digest.
    #[serde(alias = "pixel_compared")]
    pub visual_compared: bool,
}

impl BehaviorDelta {
    /// Whether runtime behavior actually changed.
    #[must_use]
    pub fn changed(&self) -> bool {
        !self.axes.is_empty()
    }
}

/// Flattened view used for ordered comparison.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StructuredView {
    /// Route.
    pub route: Option<String>,
    /// A11y digest.
    pub a11y_digest: Option<String>,
    /// Visible semantic text.
    pub semantic_text: Option<String>,
    /// Component / modal / data class.
    pub component: Option<String>,
    /// Network metadata.
    pub network: Vec<String>,
    /// Console lines.
    pub console: Vec<String>,
    /// Storage map, rendered as `k=v` sorted.
    pub storage: Vec<String>,
    /// Viewport.
    pub viewport: Option<String>,
    /// SHA-256 of [`Self::visual_surface`]. Absent when no visual bytes were captured.
    pub visual_digest: Option<String>,
    /// Which bytes `visual_digest` covers (`screenshot_png` today).
    pub visual_surface: Option<String>,
}

impl StructuredView {
    /// Observation plus optional normalized state (component/route fill-in).
    #[must_use]
    pub fn from_replay(observation: &Observation, state: Option<&BehaviorState>) -> Self {
        let mut view = Self {
            route: observation.route.clone(),
            a11y_digest: observation.a11y_digest.clone(),
            semantic_text: None,
            component: None,
            // Base and head preview deployments normally use different
            // origins. Origin is infrastructure identity, not application
            // behaviour, so compare method/path/status instead.
            network: if observation.network_requests.is_empty() {
                observation
                    .network
                    .iter()
                    .map(|event| network_event_identity(event))
                    .collect()
            } else {
                observation
                    .network_requests
                    .iter()
                    .map(crate::NetworkRequestObservation::identity_key)
                    .collect()
            },
            console: observation.console.clone(),
            storage: observation
                .storage
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect(),
            viewport: observation.viewport.clone(),
            visual_digest: observation.visual_digest.clone(),
            visual_surface: observation.visual_surface.clone(),
        };
        view.storage.sort();
        if let Some(state) = state {
            if view.route.is_none() && !state.route.is_empty() {
                view.route = Some(state.route.clone());
            }
            if view.a11y_digest.is_none() {
                view.a11y_digest.clone_from(&state.a11y_digest);
            }
            let mut component = Vec::new();
            if let Some(name) = &state.component {
                component.push(name.clone());
            }
            if let Some(modal) = &state.modal {
                component.push(format!("modal:{modal}"));
            }
            if let Some(class) = &state.data_class {
                component.push(format!("data:{class}"));
            }
            if !component.is_empty() {
                view.component = Some(component.join("|"));
            }
        }
        view
    }
}

fn network_event_identity(event: &str) -> String {
    let mut fields = event.splitn(3, ' ');
    let method = fields.next().unwrap_or_default();
    let url = fields.next().unwrap_or_default();
    let status = fields.next().unwrap_or_default();
    let identity = url_identity(url);
    if status.is_empty() {
        format!("{method} {identity}").trim().to_owned()
    } else {
        format!("{method} {identity} {status}")
    }
}

fn url_identity(url: &str) -> &str {
    let Some((_, after_scheme)) = url.split_once("://") else {
        return url;
    };
    after_scheme
        .find('/')
        .map_or("/", |index| &after_scheme[index..])
}

/// Compare base vs head. Structured axes always run before the visual digest.
#[must_use]
pub fn behavior_delta(base: &StructuredView, head: &StructuredView) -> BehaviorDelta {
    let mut changed = Vec::new();
    push_opt(
        &mut changed,
        DiffAxis::Route,
        base.route.as_ref(),
        head.route.as_ref(),
    );
    push_opt(
        &mut changed,
        DiffAxis::A11y,
        base.a11y_digest.as_ref(),
        head.a11y_digest.as_ref(),
    );
    push_opt(
        &mut changed,
        DiffAxis::SemanticText,
        base.semantic_text.as_ref(),
        head.semantic_text.as_ref(),
    );
    push_opt(
        &mut changed,
        DiffAxis::Component,
        base.component.as_ref(),
        head.component.as_ref(),
    );
    push_set(
        &mut changed,
        DiffAxis::Network,
        &base.network,
        &head.network,
    );
    push_set(
        &mut changed,
        DiffAxis::Console,
        &base.console,
        &head.console,
    );
    push_set(
        &mut changed,
        DiffAxis::Storage,
        &base.storage,
        &head.storage,
    );
    push_opt(
        &mut changed,
        DiffAxis::Geometry,
        base.viewport.as_ref(),
        head.viewport.as_ref(),
    );
    let first_structured = changed
        .iter()
        .map(|item| item.axis)
        .find(DiffAxis::is_structured);
    let mut visual_compared = false;
    if first_structured.is_none()
        && let (Some(left), Some(right)) =
            (base.visual_digest.as_ref(), head.visual_digest.as_ref())
    {
        visual_compared = true;
        if left != right {
            changed.push(AxisDelta {
                axis: DiffAxis::VisualDigest,
                base: format!(
                    "{}:{}",
                    base.visual_surface.as_deref().unwrap_or("screenshot_png"),
                    left
                ),
                head: format!(
                    "{}:{}",
                    head.visual_surface.as_deref().unwrap_or("screenshot_png"),
                    right
                ),
            });
        }
    }
    BehaviorDelta {
        axes: changed,
        first_structured,
        visual_compared,
    }
}

fn push_opt(
    changed: &mut Vec<AxisDelta>,
    axis: DiffAxis,
    base: Option<&String>,
    head: Option<&String>,
) {
    let left = base.cloned().unwrap_or_default();
    let right = head.cloned().unwrap_or_default();
    if left != right {
        changed.push(AxisDelta {
            axis,
            base: left,
            head: right,
        });
    }
}

fn push_set(changed: &mut Vec<AxisDelta>, axis: DiffAxis, base: &[String], head: &[String]) {
    let left: BTreeSet<&str> = base.iter().map(String::as_str).collect();
    let right: BTreeSet<&str> = head.iter().map(String::as_str).collect();
    if left != right {
        changed.push(AxisDelta {
            axis,
            base: join_set(&left),
            head: join_set(&right),
        });
    }
}

fn join_set(set: &BTreeSet<&str>) -> String {
    set.iter().copied().collect::<Vec<_>>().join(",")
}

/// Dual-revision replay of one program with the same seed.
///
/// # Errors
///
/// Seed mismatch or host failure on either revision.
pub fn replay_base_head(
    program: &TestProgram,
    seed: Option<u64>,
    base: &mut dyn ReplayHost,
    head: &mut dyn ReplayHost,
) -> Result<(Vec<BehaviorState>, Vec<BehaviorState>), ProgramError> {
    let base_states = replay_program(program, seed, base)?;
    let head_states = replay_program(program, seed, head)?;
    Ok((base_states, head_states))
}

//! Normalized `BehaviorGraph`: hashed states, recorded traces, replay, promotion.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wvq_domain::{ContentHash, ObligationId, ProgramId};

use crate::program::{ProgramError, ProgramSource, Target, TestAction, TestProgram};

/// Semantically normalized runtime state. Screenshots are not part of identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BehaviorState {
    /// Application route.
    pub route: String,
    /// Actor / auth role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Visible component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Modal identity, or `closed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modal: Option<String>,
    /// Network phase (`idle`, `loading`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_phase: Option<String>,
    /// Data class (`above_visual_limit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_class: Option<String>,
    /// Feature flags, sorted.
    #[serde(default)]
    pub feature_flags: BTreeMap<String, String>,
    /// Accessibility / DOM digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a11y_digest: Option<String>,
    /// Viewport `WxH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
}

impl BehaviorState {
    /// SHA-256 of the canonical JSON. Flag insertion order does not matter.
    ///
    /// # Errors
    ///
    /// Returns [`ProgramError::Malformed`] if the digest cannot be formed.
    pub fn digest(&self) -> Result<ContentHash, ProgramError> {
        let value = canonical_value(self);
        let bytes =
            serde_json::to_vec(&value).map_err(|err| ProgramError::Malformed(err.to_string()))?;
        let hex = Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            });
        ContentHash::new(hex).map_err(|err| ProgramError::Malformed(err.to_string()))
    }
}

fn canonical_value(state: &BehaviorState) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    insert_opt(&mut map, "a11y_digest", state.a11y_digest.as_ref());
    insert_opt(&mut map, "actor", state.actor.as_ref());
    insert_opt(&mut map, "component", state.component.as_ref());
    insert_opt(&mut map, "data_class", state.data_class.as_ref());
    map.insert(
        "feature_flags".into(),
        serde_json::to_value(&state.feature_flags).unwrap_or(serde_json::Value::Null),
    );
    insert_opt(&mut map, "modal", state.modal.as_ref());
    insert_opt(&mut map, "network_phase", state.network_phase.as_ref());
    map.insert(
        "route".into(),
        serde_json::Value::String(state.route.clone()),
    );
    insert_opt(&mut map, "viewport", state.viewport.as_ref());
    serde_json::Value::Object(map)
}

fn insert_opt(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&String>,
) {
    if let Some(text) = value.filter(|item| !item.is_empty()) {
        map.insert(key.to_owned(), serde_json::Value::String(text.clone()));
    }
}

/// One recorded transition: `before --action--> after`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehaviorEdge {
    /// Source state digest.
    pub src: ContentHash,
    /// Semantic action.
    pub action: TestAction,
    /// Destination state digest.
    pub dst: ContentHash,
}

/// One recorded event in a manual session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedEvent {
    /// User/runtime action.
    pub action: TestAction,
    /// State after the action.
    pub after: BehaviorState,
}

/// Finished manual session. Valuable QA must not disappear after one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehaviorTrace {
    /// Session identity.
    pub session_id: String,
    /// Fixture name (`admin-above-limit`).
    pub fixture: Option<String>,
    /// Deterministic seed used while recording.
    pub seed: Option<u64>,
    /// Linked sealed obligations.
    pub obligations: Vec<ObligationId>,
    /// Linked API operations.
    pub api_operations: Vec<String>,
    /// Linked code-coverage node / file ids.
    pub coverage: Vec<String>,
    /// Initial state.
    pub initial: BehaviorState,
    /// Ordered events.
    pub events: Vec<RecordedEvent>,
}

impl BehaviorTrace {
    /// Unique state digests in visit order.
    ///
    /// # Errors
    ///
    /// Hash failure.
    pub fn state_digests(&self) -> Result<Vec<ContentHash>, ProgramError> {
        let mut out = vec![self.initial.digest()?];
        for event in &self.events {
            out.push(event.after.digest()?);
        }
        Ok(out)
    }

    /// Graph edges for persistence.
    ///
    /// # Errors
    ///
    /// Hash failure.
    pub fn edges(&self) -> Result<Vec<BehaviorEdge>, ProgramError> {
        let mut edges = Vec::new();
        let mut src = self.initial.digest()?;
        for event in &self.events {
            let dst = event.after.digest()?;
            edges.push(BehaviorEdge {
                src,
                action: event.action.clone(),
                dst: dst.clone(),
            });
            src = dst;
        }
        Ok(edges)
    }
}

/// Semantic manual recorder. Targets must be semantic; `XPath` fails closed.
#[derive(Debug, Clone)]
pub struct Recorder {
    session_id: String,
    fixture: Option<String>,
    seed: Option<u64>,
    initial: Option<BehaviorState>,
    current: Option<BehaviorState>,
    events: Vec<RecordedEvent>,
    obligations: BTreeSet<ObligationId>,
    api_operations: BTreeSet<String>,
    coverage: BTreeSet<String>,
}

impl Recorder {
    /// Start a session. Same seed/fixture must be reused on replay.
    #[must_use]
    pub fn new(session_id: impl Into<String>, fixture: Option<String>, seed: Option<u64>) -> Self {
        Self {
            session_id: session_id.into(),
            fixture,
            seed,
            initial: None,
            current: None,
            events: Vec::new(),
            obligations: BTreeSet::new(),
            api_operations: BTreeSet::new(),
            coverage: BTreeSet::new(),
        }
    }

    /// Observe the starting state before any action.
    pub fn start(&mut self, initial: BehaviorState) {
        self.initial = Some(initial.clone());
        self.current = Some(initial);
    }

    /// Record a semantic transition.
    ///
    /// # Errors
    ///
    /// Unknown/empty action or missing initial state.
    pub fn step(&mut self, action: TestAction, after: BehaviorState) -> Result<(), ProgramError> {
        action.validate()?;
        if self.initial.is_none() {
            return Err(ProgramError::Invalid(
                "recorder requires start() before step()".into(),
            ));
        }
        self.current = Some(after.clone());
        self.events.push(RecordedEvent { action, after });
        Ok(())
    }

    /// Link a sealed obligation covered by this session.
    pub fn link_obligation(&mut self, id: ObligationId) {
        self.obligations.insert(id);
    }

    /// Link an API operation observed during the session.
    pub fn link_api(&mut self, operation: impl Into<String>) {
        self.api_operations.insert(operation.into());
    }

    /// Link a measured coverage node/file.
    pub fn link_coverage(&mut self, node: impl Into<String>) {
        self.coverage.insert(node.into());
    }

    /// Finish the session.
    ///
    /// # Errors
    ///
    /// Missing initial state.
    pub fn finish(self) -> Result<BehaviorTrace, ProgramError> {
        let Some(initial) = self.initial else {
            return Err(ProgramError::Invalid(
                "recorder has no initial state".into(),
            ));
        };
        Ok(BehaviorTrace {
            session_id: self.session_id,
            fixture: self.fixture,
            seed: self.seed,
            obligations: self.obligations.into_iter().collect(),
            api_operations: self.api_operations.into_iter().collect(),
            coverage: self.coverage.into_iter().collect(),
            initial,
            events: self.events,
        })
    }
}

/// Known graph used to compute a session's contribution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphMemory {
    /// Digests already in the `BehaviorGraph`.
    pub known_states: BTreeSet<String>,
    /// Obligations already proven or linked.
    pub known_obligations: BTreeSet<String>,
    /// API operations already seen.
    pub known_apis: BTreeSet<String>,
    /// Coverage nodes already measured.
    pub known_coverage: BTreeSet<String>,
}

/// What a session added: existing vs new, plus redundant steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageContribution {
    /// Obligations already known.
    pub existing_obligations: Vec<String>,
    /// Obligations first seen here.
    pub new_obligations: Vec<String>,
    /// New hashed behavior states.
    pub new_behavior_states: u64,
    /// New API operations.
    pub new_api_operations: Vec<String>,
    /// New coverage nodes.
    pub new_code_coverage: Vec<String>,
    /// Steps that did not change the state digest.
    pub redundant_steps: u64,
}

/// Score a trace against graph memory.
///
/// # Errors
///
/// Hash failure.
pub fn coverage_contribution(
    trace: &BehaviorTrace,
    memory: &GraphMemory,
) -> Result<CoverageContribution, ProgramError> {
    let mut seen_states = BTreeSet::new();
    let mut new_behavior_states = 0_u64;
    for digest in trace.state_digests()? {
        if seen_states.insert(digest.as_str().to_owned())
            && !memory.known_states.contains(digest.as_str())
        {
            new_behavior_states = new_behavior_states.saturating_add(1);
        }
    }
    let (existing_obligations, new_obligations) =
        split_known(&trace.obligations, &memory.known_obligations);
    let (_, new_api_operations) = split_known_str(&trace.api_operations, &memory.known_apis);
    let (_, new_code_coverage) = split_known_str(&trace.coverage, &memory.known_coverage);
    Ok(CoverageContribution {
        existing_obligations,
        new_obligations,
        new_behavior_states,
        new_api_operations,
        new_code_coverage,
        redundant_steps: count_redundant(trace)?,
    })
}

fn split_known(ids: &[ObligationId], known: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let mut existing = Vec::new();
    let mut new = Vec::new();
    for id in ids {
        if known.contains(id.as_str()) {
            existing.push(id.to_string());
        } else {
            new.push(id.to_string());
        }
    }
    (existing, new)
}

fn split_known_str(ids: &[String], known: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let mut existing = Vec::new();
    let mut new = Vec::new();
    for id in ids {
        if known.contains(id) {
            existing.push(id.clone());
        } else {
            new.push(id.clone());
        }
    }
    (existing, new)
}

fn count_redundant(trace: &BehaviorTrace) -> Result<u64, ProgramError> {
    let mut prev = trace.initial.digest()?;
    let mut redundant = 0_u64;
    for event in &trace.events {
        let next = event.after.digest()?;
        if next == prev {
            redundant = redundant.saturating_add(1);
        }
        prev = next;
    }
    Ok(redundant)
}

/// Promote a useful path into a versioned `TestProgram`.
///
/// Redundant steps are dropped. The recording seed is copied through.
///
/// # Errors
///
/// No remaining steps, invalid identity, or empty obligations after promotion.
pub fn promote(trace: &BehaviorTrace, program_id: ProgramId) -> Result<TestProgram, ProgramError> {
    let mut steps = Vec::new();
    let mut prev = trace.initial.digest()?;
    for event in &trace.events {
        let next = event.after.digest()?;
        if next != prev {
            steps.push(event.action.clone());
        }
        prev = next;
    }
    if steps.is_empty() {
        return Err(ProgramError::Invalid(
            "promotion candidate has no non-redundant steps".into(),
        ));
    }
    if let Some(obligation) = trace.obligations.first()
        && !steps.iter().any(|step| {
            matches!(step, TestAction::Assert { obligation: linked } if linked == obligation)
        })
    {
        steps.push(TestAction::Assert {
            obligation: obligation.clone(),
        });
    }
    let program = TestProgram {
        schema_v: 1,
        id: program_id,
        source: ProgramSource::Recorded,
        obligations: trace.obligations.clone(),
        preconditions: Vec::new(),
        steps,
        data: BTreeMap::new(),
        faults: BTreeMap::new(),
        api_operations: BTreeMap::new(),
        evidence_policy: crate::program::EvidencePolicy::default(),
        deterministic_seed: trace.seed,
    };
    program.validate()?;
    Ok(program)
}

/// Host that applies a typed action and returns the resulting state.
pub trait ReplayHost {
    /// Apply one IR action.
    ///
    /// # Errors
    ///
    /// Host/runtime failure.
    fn apply(&mut self, action: &TestAction) -> Result<BehaviorState, ProgramError>;
}

/// Replay a promoted program with the same seed/fixture contract.
///
/// # Errors
///
/// Seed mismatch, empty program, or host failure.
pub fn replay_program(
    program: &TestProgram,
    seed: Option<u64>,
    host: &mut dyn ReplayHost,
) -> Result<Vec<BehaviorState>, ProgramError> {
    program.validate()?;
    check_seed(program.deterministic_seed, seed)?;
    let mut states = Vec::new();
    for step in &program.steps {
        states.push(host.apply(step)?);
    }
    Ok(states)
}

/// Replay a recorded session with the same fixture and seed.
///
/// # Errors
///
/// Fixture/seed mismatch or host failure.
pub fn replay_trace(
    trace: &BehaviorTrace,
    fixture: Option<&str>,
    seed: Option<u64>,
    host: &mut dyn ReplayHost,
) -> Result<Vec<BehaviorState>, ProgramError> {
    check_seed(trace.seed, seed)?;
    match (&trace.fixture, fixture) {
        (Some(recorded), Some(wanted)) if recorded != wanted => {
            return Err(ProgramError::Invalid(
                "replay fixture does not match the recorded session".into(),
            ));
        }
        _ => {}
    }
    let mut states = Vec::new();
    for event in &trace.events {
        let after = host.apply(&event.action)?;
        if after.digest()? != event.after.digest()? {
            return Err(ProgramError::Invalid(
                "replay diverged from the recorded BehaviorGraph".into(),
            ));
        }
        states.push(after);
    }
    Ok(states)
}

fn check_seed(recorded: Option<u64>, requested: Option<u64>) -> Result<(), ProgramError> {
    match (recorded, requested) {
        (Some(left), Some(right)) if left != right => Err(ProgramError::Invalid(
            "replay seed does not match the recorded session".into(),
        )),
        _ => Ok(()),
    }
}

/// Helper for tests: a tiny semantic activate target.
#[must_use]
pub fn semantic_target(role: &str, name: &str) -> Target {
    Target {
        role: Some(role.to_owned()),
        accessible_name: Some(name.to_owned()),
        ..Target::default()
    }
}

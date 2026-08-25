//! Bounded continuous observation journal. Ordinary dev/staging use, not a seal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::behavior::{BehaviorState, BehaviorTrace, Recorder};
use crate::program::{ProgramError, TestAction};

/// Only `1`. Unknown versions fail closed.
pub const CONTINUOUS_JOURNAL_SCHEMA_V: u32 = 1;
/// Event ceiling shared with the Playwright page recorder.
pub const MAX_CONTINUOUS_JOURNAL_EVENTS: usize = 1_000;
/// JSON document ceiling. Larger payloads never enter the ledger.
pub const MAX_CONTINUOUS_JOURNAL_BYTES: usize = 1_048_576;

/// How a journal was produced. Only continuous observation is in this document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuousJournalSource {
    /// Optional in-app / staging capture. Not a Playwright `wvq record` session.
    Continuous,
}

/// One semantic transition captured outside Playwright.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousJournalEvent {
    /// Typed IR action. Unknown tags fail closed.
    pub action: TestAction,
    /// State after the action. Route is required.
    pub after: BehaviorState,
}

/// Fail-closed observation document emitted by `@wvq/recorder`.
///
/// Admission may grow the `BehaviorGraph`. It cannot preview, promote, or seal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuousJournal {
    /// Schema version. Only [`CONTINUOUS_JOURNAL_SCHEMA_V`].
    pub schema_v: u32,
    /// Provenance tag.
    pub source: ContinuousJournalSource,
    /// Must be `true`. A `false` document is refused rather than upgraded.
    pub observed_only: bool,
    /// Session identity. Not a filesystem path.
    pub session_id: String,
    /// State before the first recorded action.
    pub initial: BehaviorState,
    /// Named safe fixtures referenced by fill/select.
    #[serde(default)]
    pub data: BTreeMap<String, serde_json::Value>,
    /// Ordered semantic events.
    pub events: Vec<ContinuousJournalEvent>,
}

impl ContinuousJournal {
    /// Decode and validate a journal document.
    ///
    /// # Errors
    ///
    /// Unknown schema/source/actions, missing `observed_only`, path-shaped
    /// session ids, oversize documents, or actions that cannot be observed
    /// without Playwright or a sealed oracle.
    pub fn from_json(raw: &str) -> Result<Self, ProgramError> {
        if raw.len() > MAX_CONTINUOUS_JOURNAL_BYTES {
            return Err(ProgramError::Invalid(
                "continuous journal exceeds 1MiB".into(),
            ));
        }
        if raw.contains("\"xpath\"") {
            return Err(ProgramError::Invalid(
                "XPath is not a continuous journal identity".into(),
            ));
        }
        let journal: Self =
            serde_json::from_str(raw).map_err(|err| ProgramError::Malformed(err.to_string()))?;
        journal.validate()?;
        Ok(journal)
    }

    /// Structural validation.
    ///
    /// # Errors
    ///
    /// Returns [`ProgramError`] when the document cannot be admitted.
    pub fn validate(&self) -> Result<(), ProgramError> {
        if self.schema_v != CONTINUOUS_JOURNAL_SCHEMA_V {
            return Err(ProgramError::UnknownSchema(self.schema_v));
        }
        if !self.observed_only {
            return Err(ProgramError::Invalid(
                "continuous journal must set observed_only true".into(),
            ));
        }
        validate_session_id(&self.session_id)?;
        validate_state(&self.initial)?;
        if self.events.len() > MAX_CONTINUOUS_JOURNAL_EVENTS {
            return Err(ProgramError::Invalid(
                "continuous journal exceeds 1000 events".into(),
            ));
        }
        for event in &self.events {
            validate_continuous_action(&event.action, &self.data)?;
            validate_state(&event.after)?;
        }
        Ok(())
    }

    /// Build a [`BehaviorTrace`]. Obligations and APIs are never claimed.
    ///
    /// # Errors
    ///
    /// Invalid actions or missing initial route.
    pub fn to_trace(&self) -> Result<BehaviorTrace, ProgramError> {
        self.validate()?;
        let mut recorder = Recorder::new(&self.session_id, None, None);
        recorder.start(self.initial.clone());
        for (name, value) in &self.data {
            recorder.link_fixture(name, value.clone());
        }
        for event in &self.events {
            recorder.step(event.action.clone(), event.after.clone())?;
        }
        recorder.finish()
    }
}

fn validate_session_id(id: &str) -> Result<(), ProgramError> {
    if id.trim().is_empty() {
        return Err(ProgramError::Invalid(
            "continuous journal session_id must be non-empty".into(),
        ));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(ProgramError::Invalid(
            "continuous journal session_id must not be a path".into(),
        ));
    }
    Ok(())
}

fn validate_state(state: &BehaviorState) -> Result<(), ProgramError> {
    if state.route.trim().is_empty() {
        return Err(ProgramError::Invalid(
            "continuous journal state needs a route".into(),
        ));
    }
    Ok(())
}

fn validate_continuous_action(
    action: &TestAction,
    data: &BTreeMap<String, serde_json::Value>,
) -> Result<(), ProgramError> {
    action.validate()?;
    match action {
        TestAction::Navigate { .. } | TestAction::Activate { .. } | TestAction::Press { .. } => {
            Ok(())
        }
        TestAction::Fill { value, .. } | TestAction::Select { value, .. } => {
            if !data.contains_key(value) {
                return Err(ProgramError::Invalid(format!(
                    "continuous journal fill/select names unknown data `{value}`"
                )));
            }
            Ok(())
        }
        other => Err(ProgramError::Invalid(format!(
            "continuous journal cannot include action `{}`",
            other.kind()
        ))),
    }
}

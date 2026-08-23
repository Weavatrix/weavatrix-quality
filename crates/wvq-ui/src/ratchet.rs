//! Base/head UI-integrity ratchet.
//!
//! Old UI debt must not block adoption and a new regression must not land.
//! Both need the same comparison: the same program, at the same step, on the
//! same route, at the same viewport, on two revisions.
//!
//! A state measured on only one revision is not evidence about the other. Its
//! findings are held back from `new` and the state is reported as unmeasured,
//! because "we never looked at base here" is a different statement from "this
//! is new".

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::finding::{UiIntegrityFinding, sort_findings};
use crate::policy::UiIntegrityPolicy;
use crate::snapshot::UiStateKey;

/// Everything one revision measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UiIntegritySnapshot {
    /// Exact revision.
    pub revision: String,
    /// States that were actually collected.
    pub measured_states: BTreeSet<UiStateKey>,
    /// Findings across those states.
    pub findings: Vec<UiIntegrityFinding>,
    /// True when any collection hit a bound.
    pub truncated: bool,
}

impl UiIntegritySnapshot {
    /// Findings keyed by fingerprint.
    #[must_use]
    pub fn by_fingerprint(&self) -> BTreeMap<String, &UiIntegrityFinding> {
        self.findings
            .iter()
            .map(|finding| (finding.fingerprint(), finding))
            .collect()
    }
}

/// What the ratchet decided about one finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiFindingState {
    /// Present on base and head. Old debt; never blocks adoption.
    Existing,
    /// First seen on head.
    New,
    /// Present on base and gone on head. Credited.
    Fixed,
    /// Fixed at some earlier revision and back on head.
    Returned,
    /// Covered by an explicit, provenance-bearing exception.
    Excepted,
}

impl UiFindingState {
    /// Stable transport token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Existing => "existing",
            Self::New => "new",
            Self::Fixed => "fixed",
            Self::Returned => "returned",
            Self::Excepted => "excepted",
        }
    }
}

/// The classified base/head comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct UiIntegrityDelta {
    /// Findings first seen on head, in a state base also measured.
    pub new: Vec<UiIntegrityFinding>,
    /// Findings that were fixed earlier and are back.
    pub returned: Vec<UiIntegrityFinding>,
    /// Findings present on both revisions.
    pub existing: Vec<UiIntegrityFinding>,
    /// Findings base had and head does not.
    pub fixed: Vec<UiIntegrityFinding>,
    /// Findings an explicit exception accepted.
    pub excepted: Vec<UiIntegrityFinding>,
    /// States one revision measured and the other did not, plus any state a
    /// required program never reached.
    pub unmeasured_states: Vec<String>,
    /// True when either revision hit a bound.
    pub truncated: bool,
    /// Expired allowances, kept visible instead of silently reapplied.
    pub expired_policy: Vec<String>,
}

impl UiIntegrityDelta {
    /// Fingerprints that were fixed by this change, for the persistent history
    /// a later `returned` classification reads.
    #[must_use]
    pub fn fixed_fingerprints(&self) -> Vec<String> {
        self.fixed
            .iter()
            .map(UiIntegrityFinding::fingerprint)
            .collect()
    }

    /// Whether anything must fail CI: a new or returned error-severity finding.
    #[must_use]
    pub fn blocks(&self) -> bool {
        self.new
            .iter()
            .chain(&self.returned)
            .any(|finding| finding.severity == wvq_domain::Severity::Error)
    }

    /// Whether anything at all was measured on both revisions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.new.is_empty()
            && self.returned.is_empty()
            && self.existing.is_empty()
            && self.fixed.is_empty()
            && self.excepted.is_empty()
    }
}

/// Classify head against base.
///
/// `previously_fixed` is the persistent set of fingerprints an earlier change
/// removed; a fingerprint in it that reappears is `returned` rather than `new`,
/// which is what makes re-introducing a fixed defect visible.
#[must_use]
pub fn ratchet(
    base: &UiIntegritySnapshot,
    head: &UiIntegritySnapshot,
    previously_fixed: &BTreeSet<String>,
    policy: &UiIntegrityPolicy,
) -> UiIntegrityDelta {
    let comparable: BTreeSet<&UiStateKey> = base
        .measured_states
        .intersection(&head.measured_states)
        .collect();
    let mut unmeasured: Vec<String> = base
        .measured_states
        .symmetric_difference(&head.measured_states)
        .map(ToString::to_string)
        .collect();

    let excepted = policy.excepted();
    let base_prints = base.by_fingerprint();
    let head_prints = head.by_fingerprint();

    let mut delta = UiIntegrityDelta {
        truncated: base.truncated || head.truncated,
        expired_policy: policy.expired.clone(),
        ..UiIntegrityDelta::default()
    };

    for (fingerprint, finding) in &head_prints {
        if excepted.contains(fingerprint.as_str()) {
            delta.excepted.push((*finding).clone());
            continue;
        }
        if !comparable.contains(&finding.state) {
            // Base never measured this exact point, so head's finding is real
            // but its novelty is unknown. Report the gap; do not claim `new`.
            unmeasured.push(finding.state.to_string());
            delta.existing.push((*finding).clone());
            continue;
        }
        if base_prints.contains_key(fingerprint) {
            delta.existing.push((*finding).clone());
        } else if previously_fixed.contains(fingerprint.as_str()) {
            delta.returned.push((*finding).clone());
        } else {
            delta.new.push((*finding).clone());
        }
    }

    for (fingerprint, finding) in &base_prints {
        if head_prints.contains_key(fingerprint) || excepted.contains(fingerprint.as_str()) {
            continue;
        }
        if comparable.contains(&finding.state) {
            delta.fixed.push((*finding).clone());
        }
    }

    for bucket in [
        &mut delta.new,
        &mut delta.returned,
        &mut delta.existing,
        &mut delta.fixed,
        &mut delta.excepted,
    ] {
        sort_findings(bucket);
    }
    unmeasured.sort();
    unmeasured.dedup();
    delta.unmeasured_states = unmeasured;
    delta.expired_policy.sort();
    delta.expired_policy.dedup();
    delta
}

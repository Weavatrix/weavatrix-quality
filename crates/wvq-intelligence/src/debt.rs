//! Generic Quality Debt Ratchet: classify base/head findings without a second graph.

use std::collections::{BTreeMap, BTreeSet};

use wvq_domain::{DebtFingerprint, FindingState, QualityFinding};

/// Historical ratchet memory. Does not store graphs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebtBaseline {
    /// Fingerprints that were `Fixed` on a previous change.
    pub previously_fixed: BTreeSet<DebtFingerprint>,
    /// Explicit exceptions. Visible, not newly blamed, never silent.
    pub excepted: BTreeMap<DebtFingerprint, DebtException>,
}

/// Provenance for an excepted fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtException {
    /// Why the exception exists.
    pub reason: String,
    /// Optional expiry (`YYYY-MM-DD`). Not interpreted here.
    pub expires: Option<String>,
}

/// Classified no-new-debt delta. Each bucket is sorted by fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtDelta {
    /// Present on base and head. Not newly blamed.
    pub existing: Vec<QualityFinding>,
    /// Present only on head, and not previously fixed.
    pub new: Vec<QualityFinding>,
    /// Present on base, gone on head.
    pub fixed: Vec<QualityFinding>,
    /// Previously fixed fingerprint reappears on head.
    pub returned: Vec<QualityFinding>,
    /// Head finding covered by an explicit exception.
    pub excepted: Vec<QualityFinding>,
}

impl DebtDelta {
    /// Every classified finding, buckets then fingerprint order.
    pub fn all(&self) -> impl Iterator<Item = &QualityFinding> {
        self.existing
            .iter()
            .chain(&self.new)
            .chain(&self.fixed)
            .chain(&self.returned)
            .chain(&self.excepted)
    }
}

/// Compare base vs head findings using canonical fingerprints.
///
/// Input order does not matter. Summary/severity are not part of identity.
#[must_use]
pub fn classify_debt(
    base: &[QualityFinding],
    head: &[QualityFinding],
    baseline: &DebtBaseline,
) -> DebtDelta {
    let base_map = index_findings(base);
    let head_map = index_findings(head);
    let mut existing = Vec::new();
    let mut new = Vec::new();
    let mut fixed = Vec::new();
    let mut returned = Vec::new();
    let mut excepted = Vec::new();

    let mut keys = BTreeSet::new();
    keys.extend(base_map.keys().cloned());
    keys.extend(head_map.keys().cloned());

    for fingerprint in keys {
        let in_base = base_map.get(&fingerprint);
        let in_head = head_map.get(&fingerprint);
        let was_fixed = baseline.previously_fixed.contains(&fingerprint);
        let is_excepted = baseline.excepted.contains_key(&fingerprint);

        match (in_base, in_head) {
            (_, Some(finding)) if is_excepted => {
                excepted.push(finding.clone().with_state(FindingState::Excepted));
            }
            (Some(_), Some(finding)) => {
                existing.push(finding.clone().with_state(FindingState::Existing));
            }
            (None, Some(finding)) if was_fixed => {
                returned.push(finding.clone().with_state(FindingState::Returned));
            }
            (None, Some(finding)) => {
                new.push(finding.clone().with_state(FindingState::New));
            }
            (Some(finding), None) => {
                fixed.push(finding.clone().with_state(FindingState::Fixed));
            }
            (None, None) => {}
        }
    }

    DebtDelta {
        existing,
        new,
        fixed,
        returned,
        excepted,
    }
}

fn index_findings(findings: &[QualityFinding]) -> BTreeMap<DebtFingerprint, QualityFinding> {
    let mut map = BTreeMap::new();
    for finding in findings {
        let key = finding.fingerprint();
        match map.get(&key) {
            None => {
                map.insert(key, finding.clone());
            }
            Some(current) if representative_rank(finding) < representative_rank(current) => {
                map.insert(key, finding.clone());
            }
            Some(_) => {}
        }
    }
    map
}

/// Deterministic pick when the same fingerprint appears twice in one side.
fn representative_rank(finding: &QualityFinding) -> (std::cmp::Reverse<SeverityRank>, &str) {
    (
        std::cmp::Reverse(SeverityRank(finding.severity)),
        finding.summary.as_str(),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SeverityRank(wvq_domain::Severity);

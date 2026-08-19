//! Deterministic flake triage. Only `unknown` emits a `DecisionPacket`.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};
use thiserror::Error;
use wvq_domain::ContentHash;

/// Timing bucket for a failure. Coarse on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimingBucket {
    /// Completed quickly.
    Fast,
    /// Slow but finished.
    Slow,
    /// Hit a deadline.
    Timeout,
}

/// Independent failure signals. Not a collapsed quality score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FailureSignal {
    /// Semantic target was missing.
    SelectorMissing,
    /// Different seed passes.
    SeedSensitive,
    /// Same state digest always fails.
    SameStateAlwaysFails,
    /// Executor/browser identity mismatch.
    EnvironmentMismatch,
}

/// Why a failure was classified. Order matches spec §23.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlakeClass {
    /// Previously seen fingerprint.
    Known,
    /// Same state always fails: product regression, not a flake.
    ProductRegression,
    /// Ordering dependence.
    Ordering,
    /// Timeout / timing distribution.
    Timing,
    /// Network instability.
    Network,
    /// Environment / executor mismatch.
    Environment,
    /// Selector drift (target missing).
    SelectorDrift,
    /// Data / seed dependence.
    Seed,
    /// Test-order dependence.
    TestOrder,
    /// Needs a compact human/agent packet. Not an LLM call by itself.
    Unknown,
}

/// Evidence collected for one failure occurrence. Revision is stored beside, not in the id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FailureEvidence {
    /// `TestProgram` id.
    pub program: String,
    /// Obligation, when known.
    pub obligation: Option<String>,
    /// Executor id.
    pub executor: String,
    /// Seed used.
    pub seed: Option<u64>,
    /// Behavior state digest.
    pub state_digest: Option<String>,
    /// Stack digest.
    pub stack_digest: Option<String>,
    /// Console digest.
    pub console_digest: Option<String>,
    /// Network digest.
    pub network_digest: Option<String>,
    /// Timing bucket.
    pub timing_bucket: Option<TimingBucket>,
    /// Passed when run in isolation.
    pub passed_when_isolated: Option<bool>,
    /// Passed after reordering siblings.
    pub passed_when_reordered: Option<bool>,
    /// Network retry count.
    pub network_retries: u32,
    /// Independent signals (selector, seed, regression, environment).
    pub signals: BTreeSet<FailureSignal>,
}

/// Compact packet for unknown flakes. No DOM/screenshot dump. 0 runtime tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionPacket {
    /// Why the packet exists.
    pub goal: String,
    /// Obligation under test.
    pub obligation: Option<String>,
    /// State digest.
    pub state_digest: Option<String>,
    /// Failed deterministic classifiers.
    pub failed_candidates: Vec<String>,
    /// Always 0: this packet is not an LLM call.
    pub runtime_tokens: u64,
}

/// Triage result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlakeTriage {
    /// Assigned class.
    pub class: FlakeClass,
    /// Present only for [`FlakeClass::Unknown`].
    pub packet: Option<DecisionPacket>,
}

/// Fingerprint / triage failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FlakeError {
    /// Digest could not be formed.
    #[error("flake fingerprint: {0}")]
    Invalid(String),
}

/// Stable identity for clustering repeats. Revision is excluded.
///
/// # Errors
///
/// Returns [`FlakeError::Invalid`] if the digest is not valid hex.
pub fn fingerprint_id(evidence: &FailureEvidence) -> Result<ContentHash, FlakeError> {
    let seed = evidence
        .seed
        .map_or(String::new(), |value| value.to_string());
    let timing = evidence
        .timing_bucket
        .map(|bucket| format!("{bucket:?}"))
        .unwrap_or_default();
    let canonical = format!(
        "{}|{}|{}|{seed}|{}|{}|{}|{}|{timing}",
        evidence.program,
        evidence.obligation.as_deref().unwrap_or(""),
        evidence.executor,
        evidence.state_digest.as_deref().unwrap_or(""),
        evidence.stack_digest.as_deref().unwrap_or(""),
        evidence.console_digest.as_deref().unwrap_or(""),
        evidence.network_digest.as_deref().unwrap_or(""),
    );
    let hex = Sha256::digest(canonical.as_bytes())
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        });
    ContentHash::new(hex).map_err(|err| FlakeError::Invalid(err.to_string()))
}

/// Classify a failure. Known fingerprints short-circuit. Unknown is last.
#[must_use]
pub fn triage(evidence: &FailureEvidence, known: bool) -> FlakeTriage {
    if known {
        return FlakeTriage {
            class: FlakeClass::Known,
            packet: None,
        };
    }
    let class = classify(evidence);
    let packet = if class == FlakeClass::Unknown {
        Some(DecisionPacket {
            goal: "classify unknown flake".into(),
            obligation: evidence.obligation.clone(),
            state_digest: evidence.state_digest.clone(),
            failed_candidates: vec![
                "product_regression".into(),
                "ordering".into(),
                "timing".into(),
                "network".into(),
                "environment".into(),
                "selector_drift".into(),
                "seed".into(),
                "test_order".into(),
            ],
            runtime_tokens: 0,
        })
    } else {
        None
    };
    FlakeTriage { class, packet }
}

fn classify(evidence: &FailureEvidence) -> FlakeClass {
    if evidence
        .signals
        .contains(&FailureSignal::SameStateAlwaysFails)
    {
        return FlakeClass::ProductRegression;
    }
    if evidence.passed_when_reordered == Some(true) {
        return FlakeClass::Ordering;
    }
    if evidence.timing_bucket == Some(TimingBucket::Timeout)
        || evidence.timing_bucket == Some(TimingBucket::Slow)
    {
        return FlakeClass::Timing;
    }
    if evidence.network_retries > 0 {
        return FlakeClass::Network;
    }
    if evidence
        .signals
        .contains(&FailureSignal::EnvironmentMismatch)
    {
        return FlakeClass::Environment;
    }
    if evidence.signals.contains(&FailureSignal::SelectorMissing) {
        return FlakeClass::SelectorDrift;
    }
    if evidence.signals.contains(&FailureSignal::SeedSensitive) {
        return FlakeClass::Seed;
    }
    if evidence.passed_when_isolated == Some(true) {
        return FlakeClass::TestOrder;
    }
    FlakeClass::Unknown
}

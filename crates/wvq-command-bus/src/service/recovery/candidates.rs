//! Candidate requirements recovered from public-surface and symbol deltas.

use wvq_spec_recovery::{
    CandidateRequirement, CandidateShape, CodeDeltaSummary, EvidenceSource, IntentEvidence,
    PublicSurfaceDelta,
};

pub(in crate::service) fn recovery_candidates(
    surfaces: &PublicSurfaceDelta,
    code: &CodeDeltaSummary,
    evidence: &[IntentEvidence],
    recover_changed_symbols: bool,
) -> Vec<CandidateRequirement> {
    let mut subjects = surfaces
        .added
        .iter()
        .map(|surface| (surface.as_str(), true, "surface is available", false))
        .chain(
            surfaces
                .removed
                .iter()
                .map(|surface| (surface.as_str(), false, "surface is unavailable", false)),
        )
        .chain(
            code.components
                .iter()
                .map(|component| (component.as_str(), true, "component is visible", false)),
        )
        .chain(
            recover_changed_symbols
                .then_some(code.public_symbols.as_slice())
                .into_iter()
                .flatten()
                .map(|symbol| (symbol.as_str(), true, "", true)),
        )
        .collect::<Vec<_>>();
    subjects.sort_by_key(|(subject, expected, _, _)| ((*subject).to_owned(), *expected));
    subjects.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    subjects
        .into_iter()
        .take(100)
        .enumerate()
        .map(|(index, (subject, expected_to_hold, outcome, changed_symbol))| {
            let lower = subject.to_ascii_lowercase();
            CandidateRequirement {
                id: format!("recovered-{}-{}", index + 1, recovery_slug(subject)),
                subject: subject.to_owned(),
                text: if changed_symbol {
                    "When a user exercises the affected public capability, the externally observable outcome SHALL match the behavior demonstrated by the changed test.".into()
                } else {
                    format!(
                        "When a user exercises `{subject}`, the externally observable {outcome}."
                    )
                },
                expected_to_hold,
                actor: Some("user".into()),
                precondition: Some("the changed capability is deployed".into()),
                trigger: Some(if changed_symbol {
                    "the user exercises the affected public capability".into()
                } else {
                    format!("the user exercises `{subject}`")
                }),
                endpoint: (surfaces.added.contains(&subject.to_owned())
                    || surfaces.removed.contains(&subject.to_owned()))
                .then(|| subject.to_owned()),
                evidence: evidence
                    .iter()
                    .filter(|item| {
                        item.text.contains(subject)
                            || (changed_symbol && item.source == EvidenceSource::ChangedTest)
                    })
                    .take(20)
                    .cloned()
                    .collect(),
                shape: if changed_symbol {
                    CandidateShape::default()
                } else {
                    CandidateShape {
                        numeric_limit: subject.chars().any(|character| character.is_ascii_digit()),
                        permission_sensitive: ["permission", "auth", "role", "admin", "viewer"]
                            .iter()
                            .any(|token| lower.contains(token)),
                        async_ui: ["async", "loading", "refresh", "request"]
                            .iter()
                            .any(|token| lower.contains(token)),
                    }
                },
                covered_cases: Vec::new(),
            }
        })
        .collect()
}

fn recovery_slug(value: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !out.is_empty() {
                out.push('-');
            }
            out.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    if out.is_empty() { "change".into() } else { out }
}

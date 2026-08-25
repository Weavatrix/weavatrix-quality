//! Spec-recovery evidence from Git and Weavatrix. Opt-in, not a default gate.

use std::path::Path;

use serde_json::Value;
use wvq_spec::read_change;
use wvq_spec_recovery::{
    CandidateRequirement, CandidateShape, CodeDeltaSummary, CommitFacts, EvidenceSource,
    IntentEvidence, PublicSurfaceDelta,
};

use super::{
    BusError, ChangedFiles, RevisionRange, git_output, graph_node_id,
    graph_node_is_public_function, recovery_public_symbol_id, requirement_texts, surface_labels,
    values_at,
};

pub(super) fn recovery_code_delta(diff: &Value) -> (CodeDeltaSummary, PublicSurfaceDelta) {
    let added = values_at(diff, "/nodes/added");
    let removed = values_at(diff, "/nodes/removed");
    let changed = values_at(diff, "/nodes/changed");
    let mut changed_nodes = Vec::new();
    changed_nodes.extend(added.iter());
    changed_nodes.extend(removed.iter());
    for item in changed {
        changed_nodes.extend(item.get("before"));
        changed_nodes.extend(item.get("after"));
    }
    let mut changed_symbols = changed_nodes
        .iter()
        .filter_map(|node| graph_node_id(node))
        .collect::<Vec<_>>();
    changed_symbols.sort();
    changed_symbols.dedup();
    let mut public_symbols = changed_nodes
        .iter()
        .filter(|node| graph_node_is_public_function(node))
        .filter_map(|node| recovery_public_symbol_id(node))
        .collect::<Vec<_>>();
    public_symbols.sort();
    public_symbols.dedup();
    let mut components = changed_nodes
        .iter()
        .filter(|node| {
            node.get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.to_ascii_lowercase().contains("component"))
        })
        .filter_map(|node| graph_node_id(node))
        .collect::<Vec<_>>();
    components.sort();
    components.dedup();
    let surfaces = PublicSurfaceDelta {
        added: surface_labels(added),
        removed: surface_labels(removed),
    };
    (
        CodeDeltaSummary {
            components,
            endpoints_added: surfaces.added.clone(),
            endpoints_removed: surfaces.removed.clone(),
            changed_symbols,
            public_symbols,
        },
        surfaces,
    )
}

pub(super) fn recovery_existing_requirements(repo: &Path, change: &str) -> Result<Vec<String>, BusError> {
    let path = repo.join("openspec").join("changes").join(change);
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let spec = read_change(repo, change)?;
    Ok(requirement_texts(&spec))
}

pub(super) fn recovery_evidence(
    repo: &Path,
    range: &RevisionRange,
    code: &CodeDeltaSummary,
    files: &ChangedFiles,
    existing_requirements: &[String],
) -> Result<Vec<IntentEvidence>, BusError> {
    let mut out = existing_requirements
        .iter()
        .map(|text| IntentEvidence::new(EvidenceSource::ExistingOpenSpec, text, "OpenSpec"))
        .collect::<Vec<_>>();
    for symbol in code.changed_symbols.iter().take(500) {
        out.push(IntentEvidence::new(
            EvidenceSource::CodeDelta,
            symbol,
            format!(
                "Weavatrix graph_diff {}..{}",
                range.merge_base, range.head_ref
            ),
        ));
    }
    for endpoint in code
        .endpoints_added
        .iter()
        .chain(code.endpoints_removed.iter())
    {
        out.push(IntentEvidence::new(
            EvidenceSource::ChangedEndpoint,
            endpoint,
            "Weavatrix public-surface delta",
        ));
    }
    for test in files.changed_tests() {
        out.push(IntentEvidence::new(
            EvidenceSource::ChangedTest,
            format!("test changed: {test}"),
            format!("Git diff {test}"),
        ));
    }
    let log = recovery_log(repo, range)?;
    for record in log {
        if !record.title.is_empty() {
            out.push(IntentEvidence::new(
                EvidenceSource::CommitTitle,
                record.title,
                format!("commit {}", record.id),
            ));
        }
        if !record.body.is_empty() {
            out.push(IntentEvidence::new(
                EvidenceSource::CommitBody,
                record.body,
                format!("commit {} body", record.id),
            ));
        }
    }
    Ok(out)
}

struct RecoveryLogRecord {
    id: String,
    title: String,
    body: String,
}

fn recovery_log(repo: &Path, range: &RevisionRange) -> Result<Vec<RecoveryLogRecord>, BusError> {
    let revset = format!("{}..{}", range.merge_base, range.head_commit);
    let raw = git_output(
        repo,
        &[
            "log".into(),
            "--reverse".into(),
            "--format=%H%x1f%s%x1f%b%x1e".into(),
            revset,
            "--".into(),
        ],
    )?;
    let raw = String::from_utf8(raw)
        .map_err(|err| BusError::Intelligence(format!("Git log is not UTF-8: {err}")))?;
    let mut records = Vec::new();
    for record in raw.split('\u{1e}') {
        let record = record.trim_matches(['\r', '\n']);
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, '\u{1f}');
        let id = fields.next().unwrap_or_default().trim().to_owned();
        let title = fields.next().unwrap_or_default().trim().to_owned();
        let body = fields.next().unwrap_or_default().trim().to_owned();
        if id.len() != 40 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BusError::Intelligence(format!(
                "Git log returned an invalid commit id `{id}`"
            )));
        }
        records.push(RecoveryLogRecord { id, title, body });
    }
    Ok(records)
}

pub(super) fn recovery_commits(
    repo: &Path,
    range: &RevisionRange,
    head_revision: &str,
    components: &[String],
    has_file_delta: bool,
) -> Result<Vec<CommitFacts>, BusError> {
    let mut facts = recovery_log(repo, range)?
        .into_iter()
        .enumerate()
        .map(|(index, record)| CommitFacts {
            id: record.id,
            title: record.title,
            index: u32::try_from(index).unwrap_or(u32::MAX),
            issue: linked_issue(&record.body),
            ..CommitFacts::default()
        })
        .collect::<Vec<_>>();
    if range.head_ref == "WORKTREE" && has_file_delta {
        facts.push(CommitFacts {
            id: head_revision.to_owned(),
            title: "working tree change".into(),
            index: u32::try_from(facts.len()).unwrap_or(u32::MAX),
            components: components.to_vec(),
            ..CommitFacts::default()
        });
    }
    Ok(facts)
}

fn linked_issue(text: &str) -> Option<String> {
    text.split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .find(|token| {
            let Some((prefix, number)) = token.rsplit_once('-') else {
                return false;
            };
            !prefix.is_empty()
                && prefix
                    .chars()
                    .all(|character| character.is_ascii_uppercase())
                && !number.is_empty()
                && number.chars().all(|character| character.is_ascii_digit())
        })
        .map(ToOwned::to_owned)
}

pub(super) fn recovery_candidates(
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

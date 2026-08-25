//! Spec-recovery evidence from Git and Weavatrix. Opt-in, not a default gate.

mod candidates;
mod log;

use std::path::Path;

use serde_json::Value;
use wvq_spec::read_change;
use wvq_spec_recovery::{CodeDeltaSummary, EvidenceSource, IntentEvidence, PublicSurfaceDelta};

use super::{
    BusError, ChangedFiles, RevisionRange, graph_node_id, graph_node_is_public_function,
    recovery_public_symbol_id, requirement_texts, surface_labels, values_at,
};

pub(super) use candidates::recovery_candidates;
pub(super) use log::recovery_commits;
use log::recovery_log;

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

pub(super) fn recovery_existing_requirements(
    repo: &Path,
    change: &str,
) -> Result<Vec<String>, BusError> {
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
    let records = recovery_log(repo, range)?;
    for record in records {
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

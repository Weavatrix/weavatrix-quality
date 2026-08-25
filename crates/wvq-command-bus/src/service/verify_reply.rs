use super::access::*;
use super::persist_evidence::parse_revision_range_evidence;
use super::selection_audit::read_single_run_json;

/// Explain one UI-integrity finding by fingerprint, detector id, or subject.
///
/// The reply is quantitative on purpose. A reviewer is told which control, on
/// which route, at which viewport, what covered or duplicated it, and the exact
/// hit-test or geometry numbers behind the call — never that something
/// "possibly overlaps".
pub(in crate::service) fn explain_ui_finding(store: &Store, id: &str) -> Result<Option<ExplainReply>, BusError> {
    let looks_like_ui = id.starts_with("ui:") || id.starts_with("WVQ-UI-");
    if !looks_like_ui {
        return Ok(None);
    }
    let Some(run) = store
        .latest_run_any()
        .map_err(|err| BusError::Store(err.to_string()))?
    else {
        return Ok(None);
    };
    let Ok(document) = read_single_run_json(store, &run.id, "ui-integrity-findings") else {
        return Ok(None);
    };
    let findings: Vec<UiIntegrityFinding> =
        serde_json::from_value(document.get("findings").cloned().unwrap_or(json!([])))
            .map_err(|err| BusError::Store(format!("malformed ui findings: {err}")))?;
    let Some(finding) = findings
        .iter()
        .find(|item| item.fingerprint() == id)
        .or_else(|| findings.iter().find(|item| item.check.id() == id))
    else {
        return Ok(None);
    };

    let mut provenance = vec![
        format!("check {}", finding.check.id()),
        format!("fingerprint {}", finding.fingerprint()),
        format!("head revision {}", run.revision),
        format!("state {}", finding.state),
        format!("route {} at {}", finding.route, finding.viewport),
        format!("target {}", finding.subject),
    ];
    if let Some(counterpart) = &finding.counterpart {
        provenance.push(format!("counterpart {counterpart}"));
    }
    if let Some(component) = &finding.component_hint {
        provenance.push(format!("component {component}"));
    }
    let evidence = finding.evidence;
    if evidence.sample_count > 0 {
        provenance.push(format!(
            "hit tests {}/{} points received events ({} permille lost)",
            evidence.received_event_samples, evidence.sample_count, evidence.failure_ratio_permille
        ));
    }
    if evidence.overlap_ratio_permille > 0 {
        provenance.push(format!(
            "overlap {} permille of the target box",
            evidence.overlap_ratio_permille
        ));
    }
    if evidence.overflow_px != 0 {
        provenance.push(format!("overflow {}px", evidence.overflow_px));
    }
    if evidence.scroll_width != 0 || evidence.client_width != 0 {
        provenance.push(format!(
            "text {}x{} in a {}x{} box",
            evidence.scroll_width,
            evidence.scroll_height,
            evidence.client_width,
            evidence.client_height
        ));
    }
    if evidence.duplicate_count > 0 {
        provenance.push(format!("{} matching nodes", evidence.duplicate_count));
    }
    if !finding.nodes.is_empty() {
        provenance.push(format!("collector nodes {}", finding.nodes.join(", ")));
    }
    // The full snapshot and hit-test map stay handles; only their identity is
    // inlined so a caller can fetch them through `quality_evidence`.
    for kind in [
        "ui-layout-snapshot",
        "ui-hit-test-map",
        UI_INTEGRITY_DELTA_KIND,
    ] {
        if let Some(handle) = artifact_handle_of_kind(store, &run.id, kind)? {
            provenance.push(format!("artifact {kind} {handle}"));
        }
    }
    Ok(Some(ExplainReply {
        id: id.to_owned(),
        kind: "ui_finding".into(),
        summary: finding.detail.clone(),
        provenance,
    }))
}

pub(in crate::service) fn artifact_handle_of_kind(
    store: &Store,
    run: &RunId,
    kind: &str,
) -> Result<Option<String>, BusError> {
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, _) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind == kind {
            return Ok(Some(artifact.to_string()));
        }
    }
    Ok(None)
}

pub(in crate::service) fn explain_stored_proof(
    store: &Store,
    id: &ProofId,
    requested_id: &str,
) -> Result<Option<ExplainReply>, BusError> {
    let Some(proof) = store
        .get_proof(id)
        .map_err(|err| BusError::Store(err.to_string()))?
    else {
        return Ok(None);
    };
    let artifacts = store
        .proof_artifacts(&proof.id)
        .map_err(|err| BusError::Store(err.to_string()))?;
    let mut provenance = vec![
        format!("revision {}", proof.revision),
        format!("oracle seal {}", proof.oracle_seal),
    ];
    let mut revision_range_seen = false;
    for artifact in &artifacts {
        let (record, bytes) = store
            .read_artifact(artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind == "revision-range" {
            if revision_range_seen {
                return Err(BusError::Store(
                    "proof has more than one revision-range artifact".into(),
                ));
            }
            revision_range_seen = true;
            let range = parse_revision_range_evidence(&bytes)?;
            provenance.push(format!("base {} ({})", range.base_commit, range.base_ref));
            provenance.push(format!("head {} ({})", range.head_commit, range.head_ref));
            provenance.push(format!("merge base {}", range.merge_base));
        }
        provenance.push(format!("evidence {artifact}"));
    }
    Ok(Some(ExplainReply {
        id: requested_id.to_owned(),
        kind: "proof".into(),
        summary: format!(
            "proof {} is {} for obligation {}",
            proof.id, proof.verdict, proof.obligation
        ),
        provenance,
    }))
}

/// Exact base/head range the run measured, when it recorded one.
pub(in crate::service) fn stored_range(store: &Store, run: &RunId) -> Option<RevisionRange> {
    for artifact in store.run_artifacts(run).ok()? {
        let (record, bytes) = store.read_artifact(&artifact).ok()?;
        if record.kind == "revision-range" {
            return parse_revision_range_evidence(&bytes).ok();
        }
    }
    None
}

/// The single protection snapshot of `kind` attached to `run`, if any.
pub(in crate::service) fn snapshot_artifact(
    store: &Store,
    run: &RunId,
    kind: &str,
) -> Result<Option<ProtectionSnapshot>, BusError> {
    let mut found = None;
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, bytes) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind != kind {
            continue;
        }
        if found.is_some() {
            return Err(BusError::Store(format!(
                "run {run} has more than one {kind}"
            )));
        }
        found = Some(
            serde_json::from_slice(&bytes)
                .map_err(|err| BusError::Store(format!("invalid {kind} on run {run}: {err}")))?,
        );
    }
    Ok(found)
}

pub(in crate::service) fn stored_oracle_replacement(
    store: &Store,
    run: &RunId,
) -> Result<Option<(OracleReplacementDocument, OracleReplacementReview)>, BusError> {
    let mut found = None;
    for artifact in store
        .run_artifacts(run)
        .map_err(|err| BusError::Store(err.to_string()))?
    {
        let (record, bytes) = store
            .read_artifact(&artifact)
            .map_err(|err| BusError::Store(err.to_string()))?;
        if record.kind != ORACLE_REPLACEMENT_KIND {
            continue;
        }
        if found.is_some() {
            return Err(BusError::Store(format!(
                "run {run} has more than one OracleSeal replacement proposal"
            )));
        }
        let document: OracleReplacementDocument =
            serde_json::from_slice(&bytes).map_err(|err| {
                BusError::Store(format!(
                    "invalid OracleSeal replacement proposal on run {run}: {err}"
                ))
            })?;
        if document.schema_v != 1 {
            return Err(BusError::Store(format!(
                "unknown OracleSeal replacement schema {} on run {run}",
                document.schema_v
            )));
        }
        let digest = record.content_hash.to_string();
        let subject = format!("oracle-replacement-{}", &digest[..16]);
        let approval_decision = store
            .human_decisions_for_subject(&subject)
            .map_err(|err| BusError::Store(err.to_string()))?
            .into_iter()
            .find(|decision| {
                matches!(decision.role.as_str(), "qa" | "product")
                    && decision.decision == "accept_as_intended"
                    && decision.artifact_digest == digest
            })
            .map(|decision| decision.id);
        let review = OracleReplacementReview {
            subject,
            artifact_digest: digest,
            change: document.change.clone(),
            base_revision: document.base_revision.clone(),
            head_revision: document.head_revision.clone(),
            head_content_revision: document.head_content_revision.clone(),
            merge_base: document.merge_base.clone(),
            base_seal: document.base_seal.clone(),
            base_seal_digest: document.base_seal_digest.clone(),
            head_seal: document.head_seal.clone(),
            head_seal_digest: document.head_seal_digest.clone(),
            changed_obligations: document.changed_obligations.clone(),
            obligation_replacements: document.obligation_replacements.clone(),
            approved: approval_decision.is_some(),
            approval_decision,
        };
        found = Some((document, review));
    }
    Ok(found)
}

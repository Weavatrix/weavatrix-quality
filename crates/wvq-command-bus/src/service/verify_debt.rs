//! Extracted command-bus helper.

use super::access::*;

/// Join the backward-compatible `ProofVerdict` token with the composite
/// change-level verdict. `blocking` and the exit code follow the composite
/// state, so a lost protection net or a new UI regression fails CI even when
/// every sealed obligation is `PROVEN`.
pub(in crate::service) fn combine_verify(
    change: &str,
    proofs: Vec<ProofSummary>,
    verdicts: &[ProofVerdict],
    quality: ChangeQualityVerdict,
) -> VerifyReply {
    let combined = combine_verdicts(verdicts);
    VerifyReply {
        change: change.to_owned(),
        verdict: combined.as_str().to_owned(),
        blocking: quality.blocking(),
        proofs,
        state: quality.state.as_str().to_owned(),
        quality,
        application_surface: ApplicationSurfaceView::absent(),
    }
}

pub(in crate::service) fn combine_verdicts(verdicts: &[ProofVerdict]) -> ProofVerdict {
    if verdicts.is_empty() {
        return ProofVerdict::Unproven;
    }
    if verdicts.contains(&ProofVerdict::Contradicted) {
        return ProofVerdict::Contradicted;
    }
    if verdicts.contains(&ProofVerdict::HumanRequired) {
        return ProofVerdict::HumanRequired;
    }
    if verdicts.iter().all(|item| *item == ProofVerdict::Proven) {
        return ProofVerdict::Proven;
    }
    if verdicts.iter().all(|item| *item == ProofVerdict::Unproven) {
        return ProofVerdict::Unproven;
    }
    ProofVerdict::Partial
}

pub(in crate::service) fn count_field(counts: &Value, name: &str) -> Result<u64, BusError> {
    counts
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| BusError::Intelligence(format!("run_audit omitted debt count {name}")))
}

pub(in crate::service) fn debt_bucket_ids(
    debt: &Value,
    bucket: &str,
    expected: u64,
) -> Result<BTreeSet<String>, BusError> {
    let items = debt
        .pointer(&format!("/findings/{bucket}"))
        .and_then(Value::as_array)
        .ok_or_else(|| BusError::Intelligence(format!("run_audit omitted debt bucket {bucket}")))?;
    if u64::try_from(items.len()).unwrap_or(u64::MAX) != expected {
        return Err(BusError::Intelligence(format!(
            "run_audit debt bucket {bucket} is incomplete: expected {expected}, received {}",
            items.len()
        )));
    }
    items
        .iter()
        .map(|item| {
            item.as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
                .filter(|id| !id.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    BusError::Intelligence(format!(
                        "run_audit debt bucket {bucket} contains a finding without id"
                    ))
                })
        })
        .collect()
}

pub(in crate::service) fn compact_debt_findings(
    debt: &Value,
    returned: &BTreeSet<String>,
    excepted: &BTreeSet<String>,
) -> Vec<String> {
    let Some(findings) = debt.get("findings") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (bucket, items) in ["new", "existing", "fixed"]
        .into_iter()
        .filter_map(|bucket| {
            findings
                .get(bucket)
                .and_then(Value::as_array)
                .map(|items| (bucket, items))
        })
    {
        for item in items {
            let id = item
                .as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
                .unwrap_or("unknown-finding");
            if (bucket == "new" && returned.contains(id)) || excepted.contains(id) {
                continue;
            }
            let rule = item.get("rule").and_then(Value::as_str).unwrap_or("");
            out.push(if rule.is_empty() {
                format!("{bucket}: {id}")
            } else {
                format!("{bucket}: {id} ({rule})")
            });
        }
    }
    out.extend(returned.iter().map(|id| format!("returned: {id}")));
    out.extend(excepted.iter().map(|id| format!("excepted: {id}")));
    out
}

pub(in crate::service) fn explain_debt_finding(report: &Value, id: &str, revision: &RevisionId) -> Option<ExplainReply> {
    let findings = report.pointer("/debt/findings")?;
    for bucket in ["new", "existing", "fixed"] {
        for finding in findings
            .get(bucket)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let found = finding
                .as_str()
                .or_else(|| finding.get("id").and_then(Value::as_str));
            if found != Some(id) {
                continue;
            }
            let mut provenance = vec![
                format!("revision {revision}"),
                format!("debt bucket {bucket}"),
            ];
            if let Some(rule) = finding.get("rule").and_then(Value::as_str) {
                provenance.push(format!("rule {rule}"));
            }
            if let Some(path) = finding
                .get("path")
                .or_else(|| finding.get("file"))
                .and_then(Value::as_str)
            {
                let line = finding.get("line").and_then(Value::as_u64);
                provenance.push(line.map_or_else(
                    || format!("source {path}"),
                    |line| format!("source {path}:{line}"),
                ));
            }
            let detail = finding
                .get("message")
                .and_then(Value::as_str)
                .map_or_else(String::new, |message| format!(": {message}"));
            return Some(ExplainReply {
                id: id.to_owned(),
                kind: "finding".into(),
                summary: format!("{bucket} debt finding {id}{detail}"),
                provenance,
            });
        }
    }
    None
}

pub(in crate::service) fn verify_from_token(change: &str, verdict: &str) -> VerifyReply {
    let proofs = vec![ProofSummary {
        id: "proof-fake".into(),
        requirement: "sankey.visual-limit-others".into(),
        obligation: "others-visible".into(),
        verdict: verdict.to_owned(),
    }];
    let outcomes = vec![ProofOutcome {
        obligation: "others-visible".into(),
        requirement: "sankey.visual-limit-others".into(),
        verdict: parse_proof_verdict(verdict),
        mandatory: false,
    }];
    combine_verify(
        change,
        proofs,
        &[parse_proof_verdict(verdict)],
        compose(&VerdictInputs {
            proofs: outcomes,
            ..VerdictInputs::default()
        }),
    )
}

pub(in crate::service) fn parse_proof_verdict(token: &str) -> ProofVerdict {
    match token {
        "PROVEN" => ProofVerdict::Proven,
        "CONTRADICTED" => ProofVerdict::Contradicted,
        "PARTIAL" => ProofVerdict::Partial,
        "HUMAN_REQUIRED" => ProofVerdict::HumanRequired,
        _ => ProofVerdict::Unproven,
    }
}

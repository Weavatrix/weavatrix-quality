//! Clone-family delta from Weavatrix `find_duplicates`. No auto-delete.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::weavatrix::IntelligenceError;
use wvq_domain::{CheckId, QualityFinding, Severity, SubjectRef};

/// Map one `find_duplicates` report, using prior family sizes for growth.
///
/// # Errors
///
/// Fails closed when a family has no id.
pub fn map_duplicates_report(
    report: &Value,
    prior_sizes: &BTreeMap<String, usize>,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let Some(families) = report.get("families").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let pairs = report
        .get("pairs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut findings = Vec::new();
    for family in families {
        findings.extend(map_family(family, &pairs, prior_sizes)?);
    }
    Ok(findings)
}

/// Family id → member count from a `find_duplicates` report.
#[must_use]
pub fn family_sizes(report: &Value) -> BTreeMap<String, usize> {
    let mut sizes = BTreeMap::new();
    let Some(families) = report.get("families").and_then(Value::as_array) else {
        return sizes;
    };
    for family in families {
        if let Some(id) = family.get("id").and_then(Value::as_str) {
            let count = family
                .get("members")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            sizes.insert(id.to_owned(), count);
        }
    }
    sizes
}

fn map_family(
    family: &Value,
    pairs: &[Value],
    prior_sizes: &BTreeMap<String, usize>,
) -> Result<Vec<QualityFinding>, IntelligenceError> {
    let id = family.get("id").and_then(Value::as_str).ok_or_else(|| {
        IntelligenceError::InvalidEvidence("clone family missing id".into())
    })?;
    let members = family
        .get("members")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let pair_ids = family
        .get("pairs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let family_pairs = pairs
        .iter()
        .filter(|pair| {
            pair.get("id")
                .and_then(Value::as_str)
                .is_some_and(|pid| pair_ids.contains(pid))
        })
        .collect::<Vec<_>>();
    let string_clone = is_string_or_contract_clone(&members, &family_pairs);
    let severity = family_severity(&family_pairs);
    let subject = SubjectRef::GraphNode(id.to_owned());
    let check = if string_clone {
        static_check("WVQ-CLONE-004")
    } else {
        static_check("WVQ-CLONE-001")
    };
    let mut family_finding = QualityFinding::new(
        check,
        if string_clone { Severity::Warn } else { severity },
        subject.clone(),
        format!("clone family {id} with {} members", members.len()),
    );
    family_finding.weavatrix_fingerprint = Some(id.to_owned());
    let mut findings = vec![family_finding];

    if let Some(&prior) = prior_sizes.get(id) {
        if members.len() > prior {
            let mut growth = QualityFinding::new(
                static_check("WVQ-CLONE-002"),
                severity,
                subject.clone(),
                format!("clone family {id} grew from {prior} to {} members", members.len()),
            );
            growth.weavatrix_fingerprint = Some(format!("{id}:growth"));
            findings.push(growth);
        }
        if sibling_risk(&members) {
            let mut sibling = QualityFinding::new(
                static_check("WVQ-CLONE-003"),
                Severity::Warn,
                subject,
                format!("clone family {id} changed in one sibling but not the other"),
            );
            sibling.weavatrix_fingerprint = Some(format!("{id}:sibling"));
            findings.push(sibling);
        }
    }
    Ok(findings)
}

fn family_severity(pairs: &[&Value]) -> Severity {
    if pairs.iter().any(|pair| pair_kind(pair) == "type3") {
        return Severity::Info;
    }
    Severity::Warn
}

fn pair_kind(pair: &Value) -> &str {
    pair.get("kind").and_then(Value::as_str).unwrap_or("type1")
}

fn sibling_risk(members: &[Value]) -> bool {
    let mut changed = false;
    let mut unchanged = false;
    for member in members {
        if member.get("changed") == Some(&Value::Bool(true)) {
            changed = true;
        } else {
            unchanged = true;
        }
    }
    changed && unchanged
}

fn is_string_or_contract_clone(members: &[Value], pairs: &[&Value]) -> bool {
    if pairs.iter().any(|pair| {
        pair.pointer("/evidence/source")
            .and_then(Value::as_str)
            == Some("strings")
    }) {
        return true;
    }
    members.iter().any(|member| {
        let path = member.get("path").and_then(Value::as_str).unwrap_or("");
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        ext.eq_ignore_ascii_case("sql")
            || ext.eq_ignore_ascii_case("html")
            || ext.eq_ignore_ascii_case("hbs")
    })
}

fn static_check(id: &str) -> CheckId {
    CheckId::new(id).expect("static WVQ check ids are non-empty")
}

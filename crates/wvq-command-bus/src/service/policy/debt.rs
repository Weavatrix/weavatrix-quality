//! Debt-ratchet exceptions from quality policy.

use std::collections::BTreeSet;
use std::path::Path;

use super::super::BusError;
use super::yaml::{utc_date, valid_iso_date, yaml_get, yaml_string};

#[derive(Default)]
pub(in crate::service) struct DebtExceptions {
    pub(in crate::service) active: BTreeSet<String>,
    pub(in crate::service) notes: Vec<String>,
}

pub(in crate::service) fn load_debt_exceptions(repo: &Path) -> Result<DebtExceptions, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DebtExceptions::default());
        }
        Err(err) => {
            return Err(BusError::Runtime(format!(
                "cannot read quality policy {}: {err}",
                path.display()
            )));
        }
    };
    let value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        BusError::Runtime(format!("invalid quality policy {}: {err}", path.display()))
    })?;
    let root = value.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} must be a mapping",
            path.display()
        ))
    })?;
    let version = yaml_get(root, "quality_policy_v")
        .and_then(serde_yaml::Value::as_u64)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} is missing quality_policy_v",
                path.display()
            ))
        })?;
    if version != 1 {
        return Err(BusError::Runtime(format!(
            "unknown quality_policy_v {version} in {}",
            path.display()
        )));
    }
    let Some(ratchet) = yaml_get(root, "ratchet") else {
        return Ok(DebtExceptions::default());
    };
    let ratchet = ratchet.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} ratchet must be a mapping",
            path.display()
        ))
    })?;
    let Some(exceptions) = yaml_get(ratchet, "exceptions") else {
        return Ok(DebtExceptions::default());
    };
    let exceptions = exceptions.as_sequence().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} ratchet.exceptions must be a list",
            path.display()
        ))
    })?;
    let today = utc_date();
    let mut out = DebtExceptions::default();
    for (index, item) in exceptions.iter().enumerate() {
        let item = item.as_mapping().ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} exception {} must be a mapping",
                path.display(),
                index + 1
            ))
        })?;
        let fingerprint = yaml_string(item, "fingerprint", &path, index)?;
        let _reason = yaml_string(item, "reason", &path, index)?;
        if let Some(expires) = yaml_get(item, "expires") {
            let expires = expires
                .as_str()
                .filter(|date| valid_iso_date(date))
                .ok_or_else(|| {
                    BusError::Runtime(format!(
                        "quality policy {} exception {} has invalid expires date",
                        path.display(),
                        index + 1
                    ))
                })?;
            if expires < today.as_str() {
                out.notes.push(format!(
                    "expired debt exception {fingerprint} (expired {expires})"
                ));
                continue;
            }
        }
        out.active.insert(fingerprint);
    }
    Ok(out)
}


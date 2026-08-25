//! YAML accessors and calendar helpers. Unknown fields fail closed.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::{BusError, normalize_path};

pub(in crate::service::policy) fn yaml_required_runtime_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<String, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser.{key} must be non-empty",
                path.display()
            ))
        })
}

pub(in crate::service::policy) fn checked_repo_path(
    repo: &Path,
    raw: &str,
    label: &str,
) -> Result<(String, PathBuf), BusError> {
    let normalized = normalize_path(raw);
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(BusError::Runtime(format!(
            "{label} must stay repository-relative"
        )));
    }
    Ok((normalized.clone(), repo.join(normalized)))
}

pub(in crate::service::policy) fn yaml_get<'a>(mapping: &'a serde_yaml::Mapping, key: &str) -> Option<&'a serde_yaml::Value> {
    mapping.get(serde_yaml::Value::String(key.to_owned()))
}

pub(in crate::service::policy) fn yaml_required_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<String, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} requires non-empty {key}",
                path.display()
            ))
        })
}

pub(in crate::service::policy) fn yaml_required_u64(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<u64, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_u64)
        .ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} requires integer {key}",
                path.display()
            ))
        })
}

pub(in crate::service::policy) fn yaml_required_positive_u64(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<u64, BusError> {
    yaml_required_u64(mapping, key, path).and_then(|value| {
        if value == 0 {
            Err(BusError::Model(format!(
                "model policy {} requires positive {key}",
                path.display()
            )))
        } else {
            Ok(value)
        }
    })
}

pub(in crate::service::policy) fn yaml_optional_u64(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
) -> Result<u64, BusError> {
    yaml_get(mapping, key).map_or(Ok(0), |value| {
        value.as_u64().ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} {key} must be an integer",
                path.display()
            ))
        })
    })
}

pub(in crate::service::policy) fn yaml_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
    index: usize,
) -> Result<String, BusError> {
    yaml_get(mapping, key)
        .and_then(serde_yaml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} exception {} requires non-empty {key}",
                path.display(),
                index + 1
            ))
        })
}

pub(in crate::service::policy) fn yaml_optional_binding_string(
    mapping: &serde_yaml::Mapping,
    key: &str,
    path: &Path,
    index: usize,
) -> Result<Option<String>, BusError> {
    yaml_get(mapping, key).map_or(Ok(None), |value| {
        value
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} requires non-empty {key}",
                    path.display(),
                    index + 1
                ))
            })
    })
}

pub(in crate::service) fn valid_iso_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        && date[5..7]
            .parse::<u8>()
            .is_ok_and(|month| (1..=12).contains(&month))
        && date[8..10]
            .parse::<u8>()
            .is_ok_and(|day| (1..=31).contains(&day))
}

pub(in crate::service) fn utc_date() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() / 86_400);
    let z = i64::try_from(days)
        .unwrap_or(i64::MAX)
        .saturating_add(719_468);
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

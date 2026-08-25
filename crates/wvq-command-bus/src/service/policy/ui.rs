//! UI-integrity policy and collection config.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;
use wvq_runtime::{ProgramOracle, Target, UiCollectionConfig};
use wvq_ui::{UiIntegrityPolicy, parse_policy as parse_ui_policy};

use super::super::BusError;
use super::yaml::{utc_date, yaml_get};

pub(in crate::service) fn load_ui_integrity_policy(repo: &Path) -> Result<UiIntegrityPolicy, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UiIntegrityPolicy::default());
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
    let Some(section) = yaml_get(root, "ui_integrity") else {
        return Ok(UiIntegrityPolicy::default());
    };
    parse_ui_policy(section, &utc_date())
        .map_err(|err| BusError::Runtime(format!("{}: {err}", path.display())))
}

/// Turn the analysis policy into browser collection bounds.
///
/// Every semantic target a sealed predicate names is passed through as a
/// required test id, so the collector can never drop the exact node an
/// obligation depends on to stay under its node ceiling.
pub(in crate::service) fn ui_collection_config(
    policy: &UiIntegrityPolicy,
    oracles: &[ProgramOracle],
) -> Option<UiCollectionConfig> {
    if !policy.enabled {
        return None;
    }
    let mut required = BTreeSet::new();
    let mut required_targets = BTreeMap::new();
    for oracle in oracles {
        collect_predicate_test_ids(&oracle.expected, &mut required);
        collect_predicate_targets(&oracle.expected, &mut required_targets);
        if let Some(condition) = &oracle.condition {
            collect_predicate_test_ids(condition, &mut required);
            collect_predicate_targets(condition, &mut required_targets);
        }
    }
    Some(UiCollectionConfig {
        enabled: true,
        max_nodes: policy.max_nodes,
        geometry_tolerance_px: policy.geometry_tolerance_px,
        settle_timeout_ms: 2_000,
        test_id_attribute: "data-testid".into(),
        required_test_ids: required.into_iter().collect(),
        required_targets: required_targets.into_values().collect(),
        responsive_breakpoints: policy.responsive.enabled,
    })
}

/// Every semantic `target` object nested in a sealed predicate. The canonical
/// JSON is the deterministic deduplication key; invalid target-shaped values
/// are ignored here because predicate compilation validates executable shapes.
fn collect_predicate_targets(predicate: &Value, out: &mut BTreeMap<String, Target>) {
    match predicate {
        Value::Object(map) => {
            if let Some(value) = map.get("target")
                && let Ok(target) = serde_json::from_value::<Target>(value.clone())
                && let Ok(key) = serde_json::to_string(&target)
            {
                out.insert(key, target);
            }
            for value in map.values() {
                collect_predicate_targets(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_predicate_targets(item, out);
            }
        }
        _ => {}
    }
}

/// Every `test_id` any nested predicate target names.
fn collect_predicate_test_ids(predicate: &Value, out: &mut BTreeSet<String>) {
    match predicate {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "test_id"
                    && let Some(id) = value.as_str().filter(|id| !id.is_empty())
                {
                    out.insert(id.to_owned());
                }
                collect_predicate_test_ids(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_predicate_test_ids(item, out);
            }
        }
        _ => {}
    }
}

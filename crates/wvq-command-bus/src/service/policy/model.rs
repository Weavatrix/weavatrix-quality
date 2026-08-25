//! Loopback model policy and AI cost ceiling.

use std::path::Path;

use wvq_proof::{AiBudget, LocalModelConfig};

use super::super::{BusError, ModelPolicy};
use super::yaml::{
    yaml_get, yaml_optional_u64, yaml_required_positive_u64, yaml_required_string, yaml_required_u64,
};

pub(in crate::service) fn load_model_policy(repo: &Path) -> Result<ModelPolicy, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = std::fs::read_to_string(&path).map_err(|err| {
        BusError::Model(format!(
            "cannot read model policy {}: {err}",
            path.display()
        ))
    })?;
    let value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        BusError::Model(format!("invalid model policy {}: {err}", path.display()))
    })?;
    let root = value.as_mapping().ok_or_else(|| {
        BusError::Model(format!("model policy {} must be a mapping", path.display()))
    })?;
    let version = yaml_get(root, "quality_policy_v")
        .and_then(serde_yaml::Value::as_u64)
        .ok_or_else(|| {
            BusError::Model(format!(
                "model policy {} is missing quality_policy_v",
                path.display()
            ))
        })?;
    if version != 1 {
        return Err(BusError::Model(format!(
            "unknown quality_policy_v {version} in {}",
            path.display()
        )));
    }
    let ai = yaml_get(root, "ai")
        .and_then(serde_yaml::Value::as_mapping)
        .ok_or_else(|| BusError::Model(format!("model policy {} requires ai", path.display())))?;
    let endpoint = yaml_required_string(ai, "endpoint", &path)?;
    let model = yaml_required_string(ai, "model", &path)?;
    let max_output_tokens = yaml_required_positive_u64(ai, "max_output_tokens", &path)?;
    let planning_tokens = yaml_required_u64(ai, "max_tokens_per_change", &path)?;
    let runtime_tokens = yaml_required_u64(ai, "max_runtime_tokens", &path)?;
    let browser_escape_calls =
        u32::try_from(yaml_required_u64(ai, "max_browser_escape_calls", &path)?).map_err(|_| {
            BusError::Model(format!(
                "model policy {} max_browser_escape_calls exceeds u32",
                path.display()
            ))
        })?;
    let vision_calls =
        u32::try_from(yaml_required_u64(ai, "max_vision_calls", &path)?).map_err(|_| {
            BusError::Model(format!(
                "model policy {} max_vision_calls exceeds u32",
                path.display()
            ))
        })?;
    let max_cost_micros = yaml_get(ai, "max_cost_micros")
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                BusError::Model(format!(
                    "model policy {} max_cost_micros must be an integer",
                    path.display()
                ))
            })
        })
        .transpose()?;
    let input_micros_per_million = yaml_optional_u64(ai, "input_micros_per_million", &path)?;
    let output_micros_per_million = yaml_optional_u64(ai, "output_micros_per_million", &path)?;
    Ok(ModelPolicy {
        model: LocalModelConfig {
            endpoint,
            model,
            max_output_tokens,
            input_micros_per_million,
            output_micros_per_million,
        },
        budget: AiBudget {
            planning_tokens,
            runtime_tokens,
            browser_escape_calls,
            vision_calls,
            max_cost_micros,
        },
    })
}

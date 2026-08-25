//! Network record/replay policy from quality.yaml.

use std::collections::BTreeSet;
use std::path::Path;

use wvq_runtime::{NetworkMode, NetworkReplayProfile, NetworkRunPolicy};

use super::super::BusError;
use super::yaml::{checked_repo_path, yaml_get};

pub(in crate::service::policy) fn parse_network_run_policy(
    repo: &Path,
    path: &Path,
    browser: &serde_yaml::Mapping,
) -> Result<NetworkRunPolicy, BusError> {
    let Some(value) = yaml_get(browser, "network") else {
        return Ok(NetworkRunPolicy::default());
    };
    let network = value.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} browser.network must be a mapping",
            path.display()
        ))
    })?;
    let allowed = [
        "mode",
        "profile",
        "redact_json_keys",
        "max_entries",
        "max_body_bytes",
        "max_total_bytes",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if let Some(unknown) = network
        .keys()
        .filter_map(serde_yaml::Value::as_str)
        .find(|key| !allowed.contains(key))
    {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser.network has unknown field {unknown}",
            path.display()
        )));
    }
    let mode = match yaml_get(network, "mode")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("live")
    {
        "live" => NetworkMode::Live,
        "record" => NetworkMode::Record,
        "replay" => NetworkMode::Replay,
        "hybrid" => NetworkMode::Hybrid,
        other => {
            return Err(BusError::Runtime(format!(
                "quality policy {} has unknown browser.network.mode {other}",
                path.display()
            )));
        }
    };
    let profile_path = yaml_get(network, "profile")
        .and_then(serde_yaml::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    if matches!(mode, NetworkMode::Replay | NetworkMode::Hybrid) && profile_path.is_none() {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser.network.mode requires a profile",
            path.display()
        )));
    }
    if matches!(mode, NetworkMode::Live | NetworkMode::Record) && profile_path.is_some() {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser.network.profile is only valid for replay or hybrid mode",
            path.display()
        )));
    }
    let profile = parse_network_profile(repo, profile_path)?;
    let redact_json_keys = parse_network_redact_keys(path, network)?;
    Ok(NetworkRunPolicy {
        mode,
        profile,
        redact_json_keys,
        max_entries: parse_network_bound(path, network, "max_entries", 256, 2_048)?,
        max_body_bytes: parse_network_bound(
            path,
            network,
            "max_body_bytes",
            64 * 1024,
            1024 * 1024,
        )?,
        max_total_bytes: parse_network_bound(
            path,
            network,
            "max_total_bytes",
            4 * 1024 * 1024,
            8 * 1024 * 1024,
        )?,
    })
}

fn parse_network_profile(
    repo: &Path,
    profile_path: Option<&str>,
) -> Result<Option<NetworkReplayProfile>, BusError> {
    profile_path
        .map(|raw| {
            let (_, absolute) = checked_repo_path(repo, raw, "browser.network.profile")?;
            let body = std::fs::read(&absolute).map_err(|err| {
                BusError::Runtime(format!(
                    "cannot read network replay profile {}: {err}",
                    absolute.display()
                ))
            })?;
            serde_json::from_slice::<NetworkReplayProfile>(&body).map_err(|err| {
                BusError::Runtime(format!(
                    "invalid network replay profile {}: {err}",
                    absolute.display()
                ))
            })
        })
        .transpose()
}

fn parse_network_redact_keys(
    path: &Path,
    network: &serde_yaml::Mapping,
) -> Result<Vec<String>, BusError> {
    let Some(keys) = yaml_get(network, "redact_json_keys") else {
        return Ok(Vec::new());
    };
    keys.as_sequence()
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser.network.redact_json_keys must be a list",
                path.display()
            ))
        })?
        .iter()
        .map(|key| {
            key.as_str()
                .filter(|key| !key.trim().is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    BusError::Runtime(format!(
                        "quality policy {} browser.network.redact_json_keys must contain strings",
                        path.display()
                    ))
                })
        })
        .collect()
}

fn parse_network_bound(
    path: &Path,
    network: &serde_yaml::Mapping,
    field: &str,
    default: u32,
    max: u32,
) -> Result<u32, BusError> {
    let value = yaml_get(network, field).map_or(u64::from(default), |value| {
        value.as_u64().unwrap_or(u64::MAX)
    });
    u32::try_from(value)
        .ok()
        .filter(|value| (1..=max).contains(value))
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser.network.{field} must be between 1 and {max}",
                path.display()
            ))
        })
}


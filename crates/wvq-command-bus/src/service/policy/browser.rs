//! Browser engine policy loaders.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use wvq_spec::TestObligation;
use wvq_store::Store;

use super::super::authoring::validate_author_candidate;
use super::super::{
    BrowserPolicy, BusError, Compiled, ConfiguredBrowserProgram, TestBinding,
};
use super::parse::{parse_browser_programs, parse_browser_runtime};
use super::yaml::yaml_get;

pub(in crate::service) fn load_browser_policy(
    repo: &Path,
    obligations: &[TestObligation],
) -> Result<Option<BrowserPolicy>, BusError> {
    load_browser_policy_with(repo, obligations, None)
}

/// Load a browser policy, optionally supplying the Playwright installation.
///
/// A base-revision worktree is a fresh checkout, so it has no `node_modules`:
/// the browser engine is toolchain, not source, and is deliberately not
/// versioned. Replaying base therefore reuses the working repository's engine.
/// That is also the only correct comparison — measuring base with a different
/// browser build would confound the very geometry the ratchet compares.
pub(in crate::service) fn load_browser_policy_with(
    repo: &Path,
    obligations: &[TestObligation],
    module_root: Option<&Path>,
) -> Result<Option<BrowserPolicy>, BusError> {
    let Some(mut policy) = load_browser_runtime_with(repo, module_root)? else {
        return Ok(None);
    };
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = std::fs::read_to_string(&path).map_err(|err| {
        BusError::Runtime(format!(
            "cannot read quality policy {}: {err}",
            path.display()
        ))
    })?;
    let value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        BusError::Runtime(format!("invalid quality policy {}: {err}", path.display()))
    })?;
    let root = value.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} must be a mapping",
            path.display()
        ))
    })?;
    let browser = yaml_get(root, "browser")
        .and_then(serde_yaml::Value::as_mapping)
        .ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser must be a mapping",
                path.display()
            ))
        })?;
    policy.programs = parse_browser_programs(repo, &path, browser, obligations)?;
    Ok(Some(policy))
}

/// Load only the versioned browser runtime coordinates. Differential replay
/// intentionally supplies the exact head `TestProgram` to both sides, so a
/// stale or absent base program file must not replace it.
pub(in crate::service) fn load_browser_runtime_with(
    repo: &Path,
    module_root: Option<&Path>,
) -> Result<Option<BrowserPolicy>, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
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
    let Some(browser) = yaml_get(root, "browser") else {
        return Ok(None);
    };
    let browser = browser.as_mapping().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} browser must be a mapping",
            path.display()
        ))
    })?;
    parse_browser_runtime(repo, &path, browser, module_root).map(Some)
}

pub(in crate::service) fn load_live_browser_policy(
    repo: &Path,
    compiled: &Compiled,
    store: &Store,
) -> Result<Option<BrowserPolicy>, BusError> {
    let Some(mut policy) = load_browser_policy(repo, &compiled.obligations)? else {
        return Ok(None);
    };
    let stored = store
        .latest_program_revisions_for_change(&compiled.change)
        .map_err(|err| BusError::Store(err.to_string()))?;
    if stored.len() > 500 {
        return Err(BusError::Store(
            "more than 500 promoted browser programs require explicit repository curation".into(),
        ));
    }
    let mut ids = policy
        .programs
        .iter()
        .map(|configured| configured.program.id.to_string())
        .collect::<BTreeSet<_>>();
    for (record, body) in stored {
        let candidate: Value = serde_json::from_slice(&body).map_err(|err| {
            BusError::Store(format!(
                "stored TestProgram {} revision {} is malformed: {err}",
                record.program, record.revision
            ))
        })?;
        let validated = validate_author_candidate(repo, compiled, &candidate)?;
        if validated.program.id.as_str() != record.program {
            return Err(BusError::Store(format!(
                "stored TestProgram {} revision {} has a different body id {}",
                record.program, record.revision, validated.program.id
            )));
        }
        if validated.seal_id != record.seal {
            continue;
        }
        if !ids.insert(record.program.clone()) {
            return Err(BusError::Store(format!(
                "browser TestProgram {} is configured both as a repository file and a promoted revision",
                record.program
            )));
        }
        policy.programs.push(ConfiguredBrowserProgram {
            path: format!("wvq-program:{}@{}", record.program, record.revision),
            program: validated.program,
            oracles: validated.oracles,
        });
    }
    Ok(Some(policy))
}

pub(in crate::service) fn browser_test_bindings(policy: &BrowserPolicy) -> Vec<TestBinding> {
    policy
        .programs
        .iter()
        .map(|configured| TestBinding {
            path: configured.path.clone(),
            runner: Some("playwright-browser".into()),
            suite: Some(configured.path.clone()),
            case: Some(configured.program.id.to_string()),
            obligations: configured
                .program
                .obligations
                .iter()
                .map(ToString::to_string)
                .collect(),
            cost: 500,
            flake_penalty: 0,
        })
        .collect()
}

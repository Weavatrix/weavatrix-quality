//! Parse browser runtime mapping from quality policy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use wvq_runtime::{ProgramOracle, TestProgram};
use wvq_spec::TestObligation;

use super::super::{BrowserPolicy, BusError, ConfiguredBrowserProgram};
use super::network::parse_network_run_policy;
use super::yaml::{checked_repo_path, yaml_get, yaml_required_runtime_string};

pub(in crate::service::policy) fn parse_browser_runtime(
    repo: &Path,
    path: &Path,
    browser: &serde_yaml::Mapping,
    module_root_override: Option<&Path>,
) -> Result<BrowserPolicy, BusError> {
    let allowed = [
        "base_url",
        "engine",
        "headless",
        "timeout_ms",
        "module_root",
        "network",
        "programs",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if let Some(unknown) = browser
        .keys()
        .filter_map(serde_yaml::Value::as_str)
        .find(|key| !allowed.contains(key))
    {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser has unknown field {unknown}",
            path.display()
        )));
    }
    let base_url = yaml_required_runtime_string(browser, "base_url", path)?;
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser.base_url must use http or https",
            path.display()
        )));
    }
    let engine = yaml_get(browser, "engine")
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or("chromium")
        .to_owned();
    if !matches!(engine.as_str(), "chromium" | "firefox" | "webkit") {
        return Err(BusError::Runtime(format!(
            "quality policy {} has unknown browser engine {engine}",
            path.display()
        )));
    }
    let headless = yaml_get(browser, "headless").map_or(Ok(true), |value| {
        value.as_bool().ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} browser.headless must be boolean",
                path.display()
            ))
        })
    })?;
    let timeout_ms = yaml_get(browser, "timeout_ms").map_or(Ok(30_000), |value| {
        value
            .as_u64()
            .filter(|timeout| (1..=120_000).contains(timeout))
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} browser.timeout_ms must be between 1 and 120000",
                    path.display()
                ))
            })
    })?;
    let module_root = if let Some(override_root) = module_root_override {
        override_root.to_path_buf()
    } else {
        let module_root_raw = yaml_get(browser, "module_root")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or(".");
        checked_repo_path(repo, module_root_raw, "browser.module_root")?.1
    };
    if !module_root.join("package.json").is_file() {
        return Err(BusError::Runtime(format!(
            "quality policy {} browser.module_root has no package.json: {}",
            path.display(),
            module_root.display()
        )));
    }
    let network = parse_network_run_policy(repo, path, browser)?;
    Ok(BrowserPolicy {
        base_url,
        browser: engine,
        headless,
        timeout: Duration::from_millis(timeout_ms),
        module_root,
        network,
        programs: Vec::new(),
    })
}


pub(in crate::service::policy) fn parse_browser_programs(
    repo: &Path,
    path: &Path,
    browser: &serde_yaml::Mapping,
    obligations: &[TestObligation],
) -> Result<Vec<ConfiguredBrowserProgram>, BusError> {
    let Some(programs_value) = yaml_get(browser, "programs") else {
        return Ok(Vec::new());
    };
    let programs = programs_value.as_sequence().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} browser.programs must be a list",
            path.display()
        ))
    })?;
    let known = obligations
        .iter()
        .map(|obligation| (obligation.id.as_str(), obligation))
        .collect::<BTreeMap<_, _>>();
    let mut seen_paths = BTreeSet::new();
    let mut seen_ids = BTreeSet::new();
    let mut configured = Vec::new();
    for (index, item) in programs.iter().enumerate() {
        let raw_path = item
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} browser program {} must be a path string",
                    path.display(),
                    index + 1
                ))
            })?;
        let (program_path, absolute) = checked_repo_path(repo, raw_path, "browser program path")?;
        if !seen_paths.insert(program_path.clone()) {
            return Err(BusError::Runtime(format!(
                "quality policy {} repeats browser program {program_path}",
                path.display()
            )));
        }
        let raw = std::fs::read_to_string(&absolute).map_err(|err| {
            BusError::Runtime(format!(
                "cannot read browser TestProgram {}: {err}",
                absolute.display()
            ))
        })?;
        let program = TestProgram::from_json(&raw)
            .map_err(|err| BusError::Runtime(format!("{}: {err}", absolute.display())))?;
        if !seen_ids.insert(program.id.to_string()) {
            return Err(BusError::Runtime(format!(
                "duplicate browser TestProgram id {}",
                program.id
            )));
        }
        let mut oracles = Vec::new();
        for obligation in &program.obligations {
            let sealed = known.get(obligation.as_str()).ok_or_else(|| {
                BusError::Runtime(format!(
                    "browser TestProgram {} names unknown obligation {obligation}",
                    program.id
                ))
            })?;
            let expected = sealed.expected.as_ref().ok_or_else(|| {
                BusError::Runtime(format!(
                    "browser TestProgram {} cannot assert {obligation}: quality.yaml has no sealed expected predicate",
                    program.id
                ))
            })?;
            oracles.push(ProgramOracle {
                obligation: obligation.clone(),
                condition: sealed
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                expected: serde_json::to_value(expected)
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
            });
        }
        configured.push(ConfiguredBrowserProgram {
            path: program_path,
            program,
            oracles,
        });
    }
    Ok(configured)
}

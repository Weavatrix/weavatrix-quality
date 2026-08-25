//! Repository quality-policy loading. Unknown fields fail closed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use wvq_proof::{AiBudget, LocalModelConfig};
use wvq_runtime::{
    NetworkMode, NetworkReplayProfile, NetworkRunPolicy, ProgramOracle, Target, TestProgram,
    UiCollectionConfig,
};
use wvq_spec::TestObligation;
use wvq_store::Store;
use wvq_ui::{UiIntegrityPolicy, parse_policy as parse_ui_policy};

use super::authoring::validate_author_candidate;
use super::{
    BrowserPolicy, BusError, Compiled, ConfiguredBrowserProgram, ModelPolicy, TestBinding,
    normalize_path,
};

#[derive(Default)]
pub(super) struct DebtExceptions {
    pub(super) active: BTreeSet<String>,
    pub(super) notes: Vec<String>,
}

pub(super) fn load_debt_exceptions(repo: &Path) -> Result<DebtExceptions, BusError> {
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

#[allow(clippy::too_many_lines)]
pub(super) fn load_test_bindings(repo: &Path) -> Result<Vec<TestBinding>, BusError> {
    let path = repo.join(".weavatrix-quality").join("config.yaml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
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
    let Some(bindings) = yaml_get(root, "test_bindings") else {
        return Ok(Vec::new());
    };
    let bindings = bindings.as_sequence().ok_or_else(|| {
        BusError::Runtime(format!(
            "quality policy {} test_bindings must be a list",
            path.display()
        ))
    })?;
    let mut out = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        let binding = binding.as_mapping().ok_or_else(|| {
            BusError::Runtime(format!(
                "quality policy {} test binding {} must be a mapping",
                path.display(),
                index + 1
            ))
        })?;
        let test_path = normalize_path(&yaml_string(binding, "path", &path, index)?);
        let parsed_path = Path::new(&test_path);
        if parsed_path.is_absolute()
            || parsed_path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} path must stay repository-relative",
                path.display(),
                index + 1
            )));
        }
        let runner = yaml_optional_binding_string(binding, "runner", &path, index)?;
        if let Some(runner) = runner.as_deref()
            && !matches!(
                runner,
                "cargo-test"
                    | "vitest"
                    | "storybook-vitest"
                    | "storybook-vitest-v8"
                    | "jest"
                    | "bun-test"
                    | "go-test"
                    | "playwright"
                    | "npm-test"
            )
        {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} has unknown runner {runner}",
                path.display(),
                index + 1
            )));
        }
        let suite = yaml_optional_binding_string(binding, "suite", &path, index)?
            .map(|suite| normalize_path(&suite));
        let case = yaml_optional_binding_string(binding, "case", &path, index)?;
        if suite.is_some() && case.is_none() {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} cannot name suite without case",
                path.display(),
                index + 1
            )));
        }
        if case.is_some() && runner.is_none() {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} requires runner with case",
                path.display(),
                index + 1
            )));
        }
        let obligations = yaml_get(binding, "obligations")
            .and_then(serde_yaml::Value::as_sequence)
            .ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} requires obligations",
                    path.display(),
                    index + 1
                ))
            })?
            .iter()
            .map(|obligation| {
                obligation
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        BusError::Runtime(format!(
                            "quality policy {} test binding {} has invalid obligation",
                            path.display(),
                            index + 1
                        ))
                    })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if obligations.is_empty() {
            return Err(BusError::Runtime(format!(
                "quality policy {} test binding {} has no obligations",
                path.display(),
                index + 1
            )));
        }
        let cost = yaml_get(binding, "cost").map_or(Ok(100), |value| {
            value.as_u64().filter(|cost| *cost > 0).ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} cost must be positive",
                    path.display(),
                    index + 1
                ))
            })
        })?;
        let flake_penalty = yaml_get(binding, "flake_penalty").map_or(Ok(0), |value| {
            value.as_u64().ok_or_else(|| {
                BusError::Runtime(format!(
                    "quality policy {} test binding {} flake_penalty must be an integer",
                    path.display(),
                    index + 1
                ))
            })
        })?;
        out.push(TestBinding {
            path: test_path,
            runner,
            suite,
            case,
            obligations,
            cost,
            flake_penalty,
        });
    }
    Ok(out)
}

/// Load and validate `ui_integrity` from `.weavatrix-quality/config.yaml`.
///
/// A repository with no section gets the disabled default, which makes the axis
/// `not_applicable`. A section that is present but invalid fails the run: a
/// typo in an allowance must never quietly widen what is accepted.
pub(super) fn load_ui_integrity_policy(repo: &Path) -> Result<UiIntegrityPolicy, BusError> {
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
pub(super) fn ui_collection_config(
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

pub(super) fn load_browser_policy(
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
pub(super) fn load_browser_policy_with(
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
pub(super) fn load_browser_runtime_with(
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

pub(super) fn load_live_browser_policy(
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

fn parse_browser_runtime(
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

fn parse_network_run_policy(
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

fn parse_browser_programs(
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

pub(super) fn browser_test_bindings(policy: &BrowserPolicy) -> Vec<TestBinding> {
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

fn checked_repo_path(repo: &Path, raw: &str, label: &str) -> Result<(String, PathBuf), BusError> {
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

fn yaml_required_runtime_string(
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

pub(super) fn load_model_policy(repo: &Path) -> Result<ModelPolicy, BusError> {
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

fn yaml_get<'a>(mapping: &'a serde_yaml::Mapping, key: &str) -> Option<&'a serde_yaml::Value> {
    mapping.get(serde_yaml::Value::String(key.to_owned()))
}

fn yaml_required_string(
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

fn yaml_required_u64(
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

fn yaml_required_positive_u64(
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

fn yaml_optional_u64(
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

fn yaml_string(
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

fn yaml_optional_binding_string(
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

pub(super) fn valid_iso_date(date: &str) -> bool {
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

pub(super) fn utc_date() -> String {
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

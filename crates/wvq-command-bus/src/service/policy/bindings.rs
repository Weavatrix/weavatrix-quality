//! Test-binding table from quality policy.

use std::collections::BTreeSet;
use std::path::Path;

use super::super::{BusError, TestBinding, normalize_path};
use super::yaml::{yaml_get, yaml_optional_binding_string, yaml_string};

#[allow(clippy::too_many_lines)]
pub(in crate::service) fn load_test_bindings(repo: &Path) -> Result<Vec<TestBinding>, BusError> {
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

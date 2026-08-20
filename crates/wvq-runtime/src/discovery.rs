//! Repository runner discovery. Manifests select only frozen registry ids.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{ExecutorId, RuntimeError};

/// One registered executor discovered from repository manifests.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExecutorTarget {
    /// Frozen registry id, never an executable supplied by repository data.
    pub executor: ExecutorId,
    /// Directory in which the registered executor runs.
    pub cwd: PathBuf,
}

/// Discover supported existing runners without accepting repository commands.
///
/// Root Cargo workspaces are executed once. JavaScript package manifests are
/// inspected only to decide between frozen `npm-test`, `bun-test`, `vitest`,
/// `jest`, and `playwright` registry entries; script bodies are never executed
/// directly or copied into argv.
///
/// # Errors
///
/// Returns [`RuntimeError::InvalidArg`] for an unreadable root or malformed
/// `package.json` that claims a supported runner surface.
pub fn discover_executor_targets(repo: &Path) -> Result<Vec<ExecutorTarget>, RuntimeError> {
    if !repo.is_dir() {
        return Err(RuntimeError::InvalidArg(format!(
            "repository root is not a directory: {}",
            repo.display()
        )));
    }

    let mut targets = BTreeSet::new();
    let root_cargo = repo.join("Cargo.toml").is_file();
    if root_cargo {
        insert(&mut targets, "cargo-test", repo)?;
    }
    discover_dir(repo, repo, root_cargo, &mut targets, 0)?;
    Ok(targets.into_iter().collect())
}

fn discover_dir(
    repo: &Path,
    dir: &Path,
    root_cargo: bool,
    targets: &mut BTreeSet<ExecutorTarget>,
    depth: usize,
) -> Result<(), RuntimeError> {
    const MAX_DEPTH: usize = 12;
    if depth > MAX_DEPTH {
        return Ok(());
    }
    if dir != repo && ignored_dir(dir) {
        return Ok(());
    }

    if dir != repo && !root_cargo && dir.join("Cargo.toml").is_file() {
        insert(targets, "cargo-test", dir)?;
    }
    if dir.join("go.mod").is_file() {
        insert(targets, "go-test", dir)?;
    }
    let package = dir.join("package.json");
    if package.is_file() {
        discover_package(&package, targets)?;
    }

    let entries = fs::read_dir(dir)
        .map_err(|err| RuntimeError::InvalidArg(format!("cannot read {}: {err}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            RuntimeError::InvalidArg(format!("cannot read {}: {err}", dir.display()))
        })?;
        if entry
            .file_type()
            .map_err(|err| RuntimeError::InvalidArg(err.to_string()))?
            .is_dir()
        {
            discover_dir(repo, &entry.path(), root_cargo, targets, depth + 1)?;
        }
    }
    Ok(())
}

fn discover_package(
    package: &Path,
    targets: &mut BTreeSet<ExecutorTarget>,
) -> Result<(), RuntimeError> {
    let raw = fs::read_to_string(package).map_err(|err| {
        RuntimeError::InvalidArg(format!("cannot read {}: {err}", package.display()))
    })?;
    let json: Value = serde_json::from_str(&raw).map_err(|err| RuntimeError::Malformed {
        kind: "package.json".into(),
        message: format!("{}: {err}", package.display()),
    })?;
    let cwd = package.parent().ok_or_else(|| {
        RuntimeError::InvalidArg(format!(
            "package manifest has no parent: {}",
            package.display()
        ))
    })?;
    let scripts = json.get("scripts").and_then(Value::as_object);
    if scripts
        .and_then(|value| value.get("test"))
        .and_then(Value::as_str)
        .is_some_and(|script| !is_placeholder_test_script(script))
    {
        if cwd.join("bun.lock").is_file() || cwd.join("bun.lockb").is_file() {
            insert(targets, "bun-test", cwd)?;
        } else {
            insert(targets, "npm-test", cwd)?;
        }
        return Ok(());
    }

    for (dependency, executor) in [
        ("vitest", "vitest"),
        ("jest", "jest"),
        ("@playwright/test", "playwright"),
    ] {
        if has_dependency(&json, dependency) {
            insert(targets, executor, cwd)?;
        }
    }
    Ok(())
}

fn has_dependency(json: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies"]
        .iter()
        .filter_map(|key| json.get(*key).and_then(Value::as_object))
        .any(|items| items.contains_key(name))
}

fn is_placeholder_test_script(script: &str) -> bool {
    let normalized = script.to_ascii_lowercase();
    normalized.contains("error: no test specified") || normalized.trim().is_empty()
}

fn insert(
    targets: &mut BTreeSet<ExecutorTarget>,
    executor: &str,
    cwd: &Path,
) -> Result<(), RuntimeError> {
    targets.insert(ExecutorTarget {
        executor: ExecutorId::new(executor)?,
        cwd: cwd.to_path_buf(),
    });
    Ok(())
}

fn ignored_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".git"
                    | ".weavatrix-quality"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "vendor"
                    | "fixtures"
                    | "benchmark"
                    | "benchmarks"
            )
        })
}

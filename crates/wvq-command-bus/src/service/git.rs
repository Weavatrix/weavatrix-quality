//! Git subprocess helpers. Fail closed on malformed refs and output.

use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use super::BusError;
use super::paths::normalize_path;
use super::types::{ChangedFiles, RevisionRange};

pub(in crate::service) fn canonical_repo_path(repo: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    strip_windows_verbatim_prefix(&canonical)
}

#[cfg(not(windows))]
pub(in crate::service) fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(windows)]
pub(in crate::service) fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path.to_path_buf();
    };
    let mut normalized = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut root = PathBuf::from(r"\\");
            root.push(server);
            root.push(share);
            root
        }
        _ => return path.to_path_buf(),
    };
    for component in components {
        if !matches!(component, Component::RootDir | Component::CurDir) {
            normalized.push(component.as_os_str());
        }
    }
    normalized
}
pub(in crate::service) fn git_output(repo: &Path, args: &[String]) -> Result<Vec<u8>, BusError> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|err| BusError::Intelligence(format!("cannot run Git: {err}")))?;
    if !output.status.success() {
        return Err(BusError::Intelligence(format!(
            "Git {} failed: {}",
            args.first().map_or("operation", String::as_str),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

pub(in crate::service) fn changed_files(repo: &Path, range: &RevisionRange) -> Result<ChangedFiles, BusError> {
    let mut args = vec![
        "diff".into(),
        "--name-status".into(),
        "-M".into(),
        range.merge_base.clone(),
    ];
    if range.head_ref != "WORKTREE" {
        args.push(range.head_commit.clone());
    }
    args.push("--".into());
    let raw = String::from_utf8(git_output(repo, &args)?)
        .map_err(|err| BusError::Intelligence(format!("Git diff paths are not UTF-8: {err}")))?;
    let mut out = ChangedFiles::default();
    for line in raw.lines().filter(|line| !line.is_empty()) {
        let fields = line.split('\t').collect::<Vec<_>>();
        let status = fields.first().copied().unwrap_or_default();
        match status.chars().next() {
            Some('A') if fields.len() >= 2 => out.added.push(normalize_path(fields[1])),
            Some('D') if fields.len() >= 2 => out.removed.push(normalize_path(fields[1])),
            Some('R' | 'C') if fields.len() >= 3 => {
                out.removed.push(normalize_path(fields[1]));
                out.added.push(normalize_path(fields[2]));
            }
            Some(_) if fields.len() >= 2 => out.changed.push(normalize_path(fields[1])),
            _ => {
                return Err(BusError::Intelligence(format!(
                    "cannot decode Git name-status row `{line}`"
                )));
            }
        }
    }
    if range.head_ref == "WORKTREE" {
        let untracked = String::from_utf8(git_output(
            repo,
            &[
                "ls-files".into(),
                "--others".into(),
                "--exclude-standard".into(),
            ],
        )?)
        .map_err(|err| BusError::Intelligence(format!("Git paths are not UTF-8: {err}")))?;
        out.added.extend(
            untracked
                .lines()
                .filter(|line| !line.is_empty())
                .map(normalize_path),
        );
    }
    for list in [&mut out.added, &mut out.changed, &mut out.removed] {
        list.sort();
        list.dedup();
    }
    Ok(out)
}
pub(in crate::service) fn list_changes(repo: &Path) -> Result<Vec<String>, BusError> {
    let dir = repo.join("openspec").join("changes");
    let entries = std::fs::read_dir(&dir)
        .map_err(|err| BusError::NotFound(format!("openspec/changes: {err}")))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| BusError::NotFound(err.to_string()))?;
        if entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false) {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

pub(in crate::service) fn resolve_change(repo: &Path, change: &str) -> Result<String, BusError> {
    if change != "current" {
        return Ok(change.to_owned());
    }
    let names = list_changes(repo)?;
    match names.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(BusError::NotFound("no OpenSpec changes".into())),
        _ => Err(BusError::Ambiguous(
            "change=current is ambiguous; pass a change id".into(),
        )),
    }
}

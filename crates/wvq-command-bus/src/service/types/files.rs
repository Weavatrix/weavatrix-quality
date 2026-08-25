//! Git revision range, changed-file set, and temporary worktrees.

use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use wvq_spec_recovery::TestsDelta;

use super::super::BusError;
use super::super::git::git_output;
use super::super::paths::is_test_path;

pub(in crate::service) struct RevisionRange {
    pub(in crate::service) base_ref: String,
    pub(in crate::service) base_commit: String,
    pub(in crate::service) head_ref: String,
    pub(in crate::service) head_commit: String,
    pub(in crate::service) head_content_revision: String,
    pub(in crate::service) merge_base: String,
}

#[derive(Default)]
pub(in crate::service) struct ChangedFiles {
    pub(in crate::service) added: Vec<String>,
    pub(in crate::service) changed: Vec<String>,
    pub(in crate::service) removed: Vec<String>,
}

impl ChangedFiles {
    pub(in crate::service) fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }

    pub(in crate::service) fn tests_delta(&self) -> TestsDelta {
        TestsDelta {
            added: self
                .added
                .iter()
                .filter(|path| is_test_path(path))
                .cloned()
                .collect(),
            changed: self
                .changed
                .iter()
                .filter(|path| is_test_path(path))
                .cloned()
                .collect(),
            removed: self
                .removed
                .iter()
                .filter(|path| is_test_path(path))
                .cloned()
                .collect(),
        }
    }

    pub(in crate::service) fn changed_tests(&self) -> Vec<String> {
        let mut tests = self.tests_delta();
        tests.added.append(&mut tests.changed);
        tests.added.append(&mut tests.removed);
        tests.added.sort();
        tests.added.dedup();
        tests.added
    }

    pub(in crate::service) fn all(&self) -> Vec<String> {
        let mut files = self.added.clone();
        files.extend(self.changed.iter().cloned());
        files.extend(self.removed.iter().cloned());
        files.sort();
        files.dedup();
        files
    }

    pub(in crate::service) fn changes_openspec_change(&self, change: &str) -> bool {
        let prefix = format!("openspec/changes/{change}/");
        self.all().iter().any(|path| path.starts_with(&prefix))
    }
}

pub(in crate::service) struct TemporaryWorktree {
    pub(in crate::service) repo: PathBuf,
    pub(in crate::service) path: PathBuf,
}

impl TemporaryWorktree {
    pub(in crate::service) fn create(repo: &Path, commit: &str) -> Result<Self, BusError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| BusError::Identity(err.to_string()))?
            .as_nanos();
        let short = commit.get(..12).unwrap_or(commit);
        let path =
            std::env::temp_dir().join(format!("wvq-base-{}-{}-{nanos}", std::process::id(), short));
        if path.exists() {
            return Err(BusError::Runtime(format!(
                "temporary base worktree path already exists: {}",
                path.display()
            )));
        }
        git_output(
            repo,
            &[
                "worktree".into(),
                "add".into(),
                "--detach".into(),
                path.display().to_string(),
                commit.to_owned(),
            ],
        )?;
        Ok(Self {
            repo: repo.to_path_buf(),
            path,
        })
    }
}

impl Drop for TemporaryWorktree {
    fn drop(&mut self) {
        let _ = ProcessCommand::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .current_dir(&self.repo)
            .output();
        let _ = ProcessCommand::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo)
            .output();
    }
}

//! Git revision identity and Weavatrix operation helpers.

use std::process::Command as ProcessCommand;

use super::super::access::*;
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn require_git_root(&self) -> Result<(), BusError> {
        if self.repo.join(".git").exists() {
            Ok(())
        } else {
            Err(BusError::Intelligence(format!(
                "base/head analysis requires the repository Git root, got {}",
                self.repo.display()
            )))
        }
    }

    pub(in crate::service) fn revision_range(
        &self,
        base: &str,
        head: &str,
    ) -> Result<RevisionRange, BusError> {
        self.require_git_root()?;
        validate_revision_ref("base", base)?;
        validate_revision_ref("head", head)?;

        let checked_out_head = self.resolve_commit("HEAD")?;
        let head_commit = if head == "WORKTREE" {
            checked_out_head.clone()
        } else {
            let requested_head = self.resolve_commit(head)?;
            if requested_head != checked_out_head {
                return Err(BusError::Ambiguous(format!(
                    "explicit head `{head}` resolves to `{requested_head}`, but the checked-out HEAD is `{checked_out_head}`"
                )));
            }
            if self.worktree_is_dirty()? {
                return Err(BusError::Ambiguous(format!(
                    "explicit committed head `{head}` requires a clean repository; dirty worktree content must use head `WORKTREE`"
                )));
            }
            requested_head
        };
        let base_commit = self.resolve_commit(base)?;
        let merge_base = self.resolve_merge_base(&base_commit, &head_commit)?;
        let head_content_revision = self.revision()?.to_string();
        Ok(RevisionRange {
            base_ref: base.to_owned(),
            base_commit,
            head_ref: head.to_owned(),
            head_commit,
            head_content_revision,
            merge_base,
        })
    }

    pub(in crate::service) fn resolve_merge_base(
        &self,
        base: &str,
        head: &str,
    ) -> Result<String, BusError> {
        let output = ProcessCommand::new("git")
            .args(["merge-base", "--", base, head])
            .current_dir(&self.repo)
            .output()
            .map_err(|err| {
                BusError::Intelligence(format!("cannot resolve Git merge-base: {err}"))
            })?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(BusError::Intelligence(format!(
                "cannot resolve a common ancestor for `{base}` and `{head}`{}",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            )));
        }
        let commit = String::from_utf8(output.stdout).map_err(|err| {
            BusError::Intelligence(format!("Git returned a non-UTF-8 merge-base: {err}"))
        })?;
        let commit = commit.trim();
        if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BusError::Intelligence(format!(
                "Git returned an invalid merge-base `{commit}`"
            )));
        }
        Ok(commit.to_owned())
    }

    pub(in crate::service) fn resolve_commit(&self, reference: &str) -> Result<String, BusError> {
        let output = ProcessCommand::new("git")
            .args(["rev-parse", "--verify", "--end-of-options"])
            .arg(format!("{reference}^{{commit}}"))
            .current_dir(&self.repo)
            .output()
            .map_err(|err| {
                BusError::Intelligence(format!("cannot resolve Git ref `{reference}`: {err}"))
            })?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(BusError::Intelligence(format!(
                "cannot resolve Git ref `{reference}` to a commit{}",
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            )));
        }
        let commit = String::from_utf8(output.stdout).map_err(|err| {
            BusError::Intelligence(format!("Git returned a non-UTF-8 commit id: {err}"))
        })?;
        let commit = commit.trim();
        if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BusError::Intelligence(format!(
                "Git returned an invalid commit id for `{reference}`: `{commit}`"
            )));
        }
        Ok(commit.to_owned())
    }

    pub(in crate::service) fn worktree_is_dirty(&self) -> Result<bool, BusError> {
        let output = ProcessCommand::new("git")
            .args([
                "status",
                "--porcelain=v1",
                "--untracked-files=normal",
                "--ignore-submodules=none",
            ])
            .current_dir(&self.repo)
            .output()
            .map_err(|err| BusError::Intelligence(format!("cannot inspect Git worktree: {err}")))?;
        if !output.status.success() {
            return Err(BusError::Intelligence(format!(
                "cannot inspect Git worktree: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(!output.stdout.is_empty())
    }

    pub(in crate::service) fn weavatrix_operation(
        &self,
        revision: &RevisionId,
        name: &str,
        args: &Value,
    ) -> Result<Value, BusError> {
        let report = WeavatrixProvider
            .operation(&self.repo, name, args)
            .map_err(|err| BusError::Intelligence(err.to_string()))?;
        let found = report
            .get("revision")
            .and_then(Value::as_str)
            .ok_or_else(|| BusError::Intelligence(format!("{name} omitted revision identity")))?;
        if found != revision.as_str() {
            return Err(BusError::Ambiguous(format!(
                "{name} evidence belongs to revision `{found}`, expected `{revision}`"
            )));
        }
        Ok(report)
    }
}

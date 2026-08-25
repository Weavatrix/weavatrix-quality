//! Extracted command-bus helper.

use super::access::*;

pub(in crate::service) const ARTIFACT_CLOCK_TOLERANCE: Duration = Duration::from_secs(2);

pub(in crate::service) fn normalize_coverage_paths(repo: &Path, cwd: &Path, coverage: &mut CoverageArtifact) {
    let repo_path = normalize_path(&repo.to_string_lossy());
    let cwd_path = normalize_path(&cwd.to_string_lossy());
    let cwd_prefix = cwd
        .strip_prefix(repo)
        .ok()
        .map(|path| normalize_path(&path.to_string_lossy()))
        .filter(|path| !path.is_empty());
    let go_module = read_go_module(cwd);
    for file in &mut coverage.files {
        let mut path = normalize_path(&file.path);
        if let Some(relative) = path.strip_prefix(&format!("{repo_path}/")) {
            path = String::from(relative);
        } else if let Some(relative) = path.strip_prefix(&format!("{cwd_path}/")) {
            path = cwd_prefix.as_ref().map_or_else(
                || relative.to_owned(),
                |prefix| format!("{prefix}/{relative}"),
            );
        } else {
            if let Some(module) = &go_module
                && let Some(relative) = path.strip_prefix(&format!("{module}/"))
            {
                path = String::from(relative);
            }
            path = String::from(path.trim_start_matches("./"));
            if let Some(prefix) = &cwd_prefix
                && !path.starts_with(&format!("{prefix}/"))
            {
                path = format!("{prefix}/{path}");
            }
        }
        file.path = path;
    }
}

pub(in crate::service) fn read_go_module(cwd: &Path) -> Option<String> {
    std::fs::read_to_string(cwd.join("go.mod"))
        .ok()?
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("module ").map(str::trim))
        .filter(|module| !module.is_empty())
        .map(ToOwned::to_owned)
}

pub(in crate::service) fn runner_artifact_candidates(cwd: &Path) -> Vec<(PathBuf, &'static str)> {
    let candidates = [
        (".weavatrix-quality/go-cover.out", "go-coverprofile"),
        (".weavatrix-quality/junit.xml", "junit"),
        ("junit.xml", "junit"),
        ("test-results/junit.xml", "junit"),
        ("reports/junit.xml", "junit"),
        ("coverage/junit.xml", "junit"),
        ("lcov.info", "lcov"),
        ("coverage.lcov", "lcov"),
        ("coverage/lcov.info", "lcov"),
        ("coverage/lcov-report/lcov.info", "lcov"),
    ];
    candidates
        .into_iter()
        .map(|(path, kind)| (cwd.join(path), kind))
        .collect()
}

pub(in crate::service) fn artifact_is_fresh(metadata: &std::fs::Metadata, started: SystemTime) -> bool {
    let threshold = started
        .checked_sub(ARTIFACT_CLOCK_TOLERANCE)
        .unwrap_or(UNIX_EPOCH);
    metadata
        .modified()
        .is_ok_and(|modified| modified >= threshold)
}

pub(in crate::service) fn set_record_error(record: &mut ExecutorRecord, message: impl Into<String>) {
    let message = message.into();
    record.passed = false;
    record.error = Some(match record.error.take() {
        Some(existing) => format!("{existing}; {message}"),
        None => message,
    });
}

pub(in crate::service) fn stdout_kind(executor: &str) -> &'static str {
    if executor == "go-test" {
        "go-json"
    } else {
        "stdout"
    }
}

//! Stable `cargo test` / libtest text normalization.

use crate::normalize::{
    ArtifactDescriptor, NormalizedTestRun, RuntimeError, TestCaseResult, TestStatus,
};

/// Normalize captured `cargo test` streams into exact test-target and case identities.
///
/// Cargo writes test-target identities to stderr and libtest case results to stdout.
/// WVQ pairs those ordered blocks without treating aggregate exit zero as case execution.
///
/// # Errors
///
/// Returns [`RuntimeError::Malformed`] when a reported case cannot be associated with
/// a concrete test target or has an unknown terminal status.
pub fn parse_cargo_test(stdout: &str, stderr: &str) -> Result<NormalizedTestRun, RuntimeError> {
    let suites = cargo_suites(stderr);
    let mut suite_index = 0_usize;
    let mut current_suite = None;
    let mut cases = Vec::new();

    for raw in stdout.lines() {
        let clean = strip_ansi(raw);
        let line = clean.trim();
        if is_run_header(line) {
            current_suite = suites.get(suite_index).cloned();
            suite_index = suite_index.saturating_add(1);
            continue;
        }
        let Some(case) = line.strip_prefix("test ") else {
            continue;
        };
        let Some((name, raw_status)) = case.rsplit_once(" ... ") else {
            continue;
        };
        let suite = current_suite.clone().ok_or_else(|| RuntimeError::Malformed {
            kind: "cargo-test".into(),
            message: format!("case `{name}` has no cargo test target"),
        })?;
        let (status, message) = cargo_status(raw_status)?;
        cases.push(TestCaseResult {
            name: name.to_owned(),
            suite,
            status,
            duration_ms: None,
            message,
        });
    }

    Ok(NormalizedTestRun {
        cases,
        coverage: None,
        raw_artifacts: vec![ArtifactDescriptor {
            kind: "cargo-test".into(),
            path: None,
        }],
    })
}

fn is_run_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("running ") else {
        return false;
    };
    let mut parts = rest.split_whitespace();
    parts.next().is_some_and(|count| count.parse::<u64>().is_ok())
        && parts.next().is_some_and(|unit| unit.starts_with("test"))
}

fn cargo_suites(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| {
            let clean = strip_ansi(line);
            let line = clean.trim();
            if let Some(target) = line.strip_prefix("Running unittests ") {
                return Some(cargo_target_path(target));
            }
            if let Some(target) = line.strip_prefix("Running tests/") {
                return Some(cargo_target_path(&format!("tests/{target}")));
            }
            if let Some(target) = line.strip_prefix("Running tests\\") {
                return Some(cargo_target_path(&format!("tests\\{target}")));
            }
            if let Some(target) = line.strip_prefix("Running benches/") {
                return Some(cargo_target_path(&format!("benches/{target}")));
            }
            line.strip_prefix("Doc-tests ")
                .map(|package| format!("doc-tests:{package}"))
        })
        .collect()
}

fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn cargo_target_path(raw: &str) -> String {
    raw.split_once(" (")
        .map_or(raw, |(path, _)| path)
        .replace('\\', "/")
}

fn cargo_status(raw: &str) -> Result<(TestStatus, Option<String>), RuntimeError> {
    if raw == "ok" {
        return Ok((TestStatus::Pass, None));
    }
    if raw.starts_with("FAILED") {
        return Ok((TestStatus::Fail, None));
    }
    if raw.starts_with("ignored") {
        return Ok((TestStatus::Skip, Some(raw.to_owned())));
    }
    Err(RuntimeError::Malformed {
        kind: "cargo-test".into(),
        message: format!("unknown libtest case status `{raw}`"),
    })
}

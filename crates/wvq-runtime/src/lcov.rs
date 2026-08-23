//! LCOV to inclusive source line ranges.

use crate::normalize::{CoverageArtifact, FileCoverage, LineRange, RuntimeError};

/// Parse LCOV into per-file covered/uncovered ranges.
///
/// # Errors
///
/// Fails closed on `DA` before `SF`, non-integer lines, or a record without
/// `end_of_record`.
pub fn parse_lcov(text: &str) -> Result<CoverageArtifact, RuntimeError> {
    let mut files = Vec::new();
    let mut current_path: Option<String> = None;
    let mut hits: Vec<(u32, u64)> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line == "TN:" || line.starts_with("TN:") {
            continue;
        }
        if let Some(path) = line.strip_prefix("SF:") {
            if current_path.is_some() {
                return truncated();
            }
            current_path = Some(path.replace('\\', "/"));
            hits.clear();
        } else if let Some(da) = line.strip_prefix("DA:") {
            if current_path.is_none() {
                return malformed("DA line before SF");
            }
            let (lineno, hit) = da
                .split_once(',')
                .ok_or_else(|| malformed_msg("DA missing comma"))?;
            let lineno: u32 = lineno
                .parse()
                .map_err(|_| malformed_msg("DA line is not an integer"))?;
            let hit: u64 = hit
                .parse()
                .map_err(|_| malformed_msg("DA hit count is not an integer"))?;
            hits.push((lineno, hit));
        } else if line == "end_of_record" {
            let path = current_path.take().ok_or_else(truncated_err)?;
            files.push(file_coverage(path, &hits));
            hits.clear();
        }
    }
    if current_path.is_some() {
        return truncated();
    }
    Ok(CoverageArtifact { files })
}

fn file_coverage(path: String, hits: &[(u32, u64)]) -> FileCoverage {
    let mut covered = Vec::new();
    let mut uncovered = Vec::new();
    for &(line, count) in hits {
        if count == 0 {
            push_range(&mut uncovered, line);
        } else {
            push_range(&mut covered, line);
        }
    }
    FileCoverage {
        path,
        covered,
        uncovered,
    }
}

fn push_range(ranges: &mut Vec<LineRange>, line: u32) {
    if let Some(last) = ranges.last_mut()
        && last.end.saturating_add(1) == line
    {
        last.end = line;
        return;
    }
    ranges.push(LineRange {
        start: line,
        end: line,
    });
}

fn malformed(message: &str) -> Result<CoverageArtifact, RuntimeError> {
    Err(malformed_msg(message))
}

fn malformed_msg(message: &str) -> RuntimeError {
    RuntimeError::Malformed {
        kind: "lcov".into(),
        message: message.into(),
    }
}

fn truncated() -> Result<CoverageArtifact, RuntimeError> {
    Err(truncated_err())
}

fn truncated_err() -> RuntimeError {
    RuntimeError::Truncated {
        kind: "lcov".into(),
    }
}

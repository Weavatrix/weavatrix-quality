//! Go `-coverprofile` normalization.

use std::collections::BTreeMap;

use crate::{CoverageArtifact, FileCoverage, LineRange, RuntimeError};

/// Parse a Go coverage profile into repository-relative covered/uncovered ranges.
///
/// # Errors
///
/// Rejects missing mode headers and malformed/truncated coverage records.
pub fn parse_go_coverprofile(text: &str) -> Result<CoverageArtifact, RuntimeError> {
    let mut lines = text.lines();
    let Some(mode) = lines.next() else {
        return Err(RuntimeError::Truncated {
            kind: "go-coverprofile".into(),
        });
    };
    if !mode.starts_with("mode: ") {
        return Err(malformed("missing mode header"));
    }

    let mut files = BTreeMap::<String, (Vec<LineRange>, Vec<LineRange>)>::new();
    let mut records = 0_usize;
    for (index, line) in lines.enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (location, counts) = line
            .split_once(' ')
            .ok_or_else(|| malformed_at(index + 2, "missing counts"))?;
        let mut counts = counts.split_whitespace();
        let _statements: u64 = counts
            .next()
            .ok_or_else(|| malformed_at(index + 2, "missing statement count"))?
            .parse()
            .map_err(|_| malformed_at(index + 2, "invalid statement count"))?;
        let hits: u64 = counts
            .next()
            .ok_or_else(|| malformed_at(index + 2, "missing hit count"))?
            .parse()
            .map_err(|_| malformed_at(index + 2, "invalid hit count"))?;
        if counts.next().is_some() {
            return Err(malformed_at(index + 2, "too many count fields"));
        }
        let (path, span) = location
            .rsplit_once(':')
            .ok_or_else(|| malformed_at(index + 2, "missing path/span separator"))?;
        let (start, end) = span
            .split_once(',')
            .ok_or_else(|| malformed_at(index + 2, "missing range separator"))?;
        let start = parse_line(start, index + 2)?;
        let end = parse_line(end, index + 2)?;
        if start == 0 || end < start {
            return Err(malformed_at(index + 2, "invalid line range"));
        }
        let ranges = files
            .entry(path.replace('\\', "/"))
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let target = if hits > 0 {
            &mut ranges.0
        } else {
            &mut ranges.1
        };
        target.push(LineRange { start, end });
        records = records.saturating_add(1);
    }
    if records == 0 {
        return Err(RuntimeError::Truncated {
            kind: "go-coverprofile".into(),
        });
    }

    Ok(CoverageArtifact {
        files: files
            .into_iter()
            .map(|(path, (covered, uncovered))| FileCoverage {
                path,
                covered: merge(covered),
                uncovered: merge(uncovered),
            })
            .collect(),
    })
}

fn parse_line(raw: &str, record: usize) -> Result<u32, RuntimeError> {
    raw.split_once('.')
        .map_or(raw, |(line, _)| line)
        .parse()
        .map_err(|_| malformed_at(record, "invalid line number"))
}

fn merge(mut ranges: Vec<LineRange>) -> Vec<LineRange> {
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut out: Vec<LineRange> = Vec::new();
    for range in ranges {
        if let Some(last) = out.last_mut()
            && range.start <= last.end.saturating_add(1)
        {
            last.end = last.end.max(range.end);
            continue;
        }
        out.push(range);
    }
    out
}

fn malformed(message: &str) -> RuntimeError {
    RuntimeError::Malformed {
        kind: "go-coverprofile".into(),
        message: message.into(),
    }
}

fn malformed_at(record: usize, message: &str) -> RuntimeError {
    malformed(&format!("record {record}: {message}"))
}

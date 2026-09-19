//! Write LCOV so Weavatrix `coverage_map` can ingest a Quality run.

use std::collections::BTreeMap;

use crate::normalize::CoverageArtifact;

/// Encode normalized file ranges as LCOV. Covered wins on a shared line.
#[must_use]
pub fn encode_lcov(coverage: &CoverageArtifact) -> String {
    let mut out = String::from("TN:\n");
    for file in &coverage.files {
        out.push_str("SF:");
        out.push_str(&file.path.replace('\\', "/"));
        out.push('\n');
        let mut lines = BTreeMap::<u32, u64>::new();
        for range in &file.uncovered {
            for line in range.start..=range.end {
                lines.insert(line, 0);
            }
        }
        for range in &file.covered {
            for line in range.start..=range.end {
                lines.insert(line, 1);
            }
        }
        for (line, hits) in lines {
            out.push_str("DA:");
            out.push_str(&line.to_string());
            out.push(',');
            out.push_str(&hits.to_string());
            out.push('\n');
        }
        out.push_str("end_of_record\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode_lcov;
    use crate::normalize::{CoverageArtifact, FileCoverage, LineRange};
    use crate::parse_lcov;

    #[test]
    fn covered_and_uncovered_ranges_round_trip() {
        let artifact = CoverageArtifact {
            files: vec![FileCoverage {
                path: "src/add.rs".into(),
                covered: vec![LineRange { start: 1, end: 2 }],
                uncovered: vec![LineRange { start: 3, end: 3 }],
            }],
        };
        let parsed = parse_lcov(&encode_lcov(&artifact)).unwrap();
        assert_eq!(parsed, artifact);
    }
}

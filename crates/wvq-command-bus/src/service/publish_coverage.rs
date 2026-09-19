//! Persist a Weavatrix-readable measured report after a runner produces one.

use super::access::*;
use wvq_runtime::encode_lcov;

/// Write `.weavatrix/coverage/lcov.info` from normalized coverage artifacts.
///
/// Weavatrix `coverage_map` looks here. Missing coverage stays unpublished.
pub(in crate::service) fn publish_measured_coverage(
    repo: &Path,
    record: &ExecutorRecord,
) -> Result<(), BusError> {
    let Some(artifact) = record
        .artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.kind == "coverage")
    else {
        return Ok(());
    };
    let coverage: CoverageArtifact = serde_json::from_slice(&artifact.bytes).map_err(|err| {
        BusError::Runtime(format!(
            "cannot decode normalized coverage for Weavatrix publish: {err}"
        ))
    })?;
    if coverage.files.is_empty() {
        return Ok(());
    }
    let dir = repo.join(".weavatrix").join("coverage");
    std::fs::create_dir_all(&dir).map_err(|err| {
        BusError::Runtime(format!(
            "cannot prepare Weavatrix coverage directory {}: {err}",
            dir.display()
        ))
    })?;
    let path = dir.join("lcov.info");
    std::fs::write(&path, encode_lcov(&coverage)).map_err(|err| {
        BusError::Runtime(format!(
            "cannot write measured coverage {}: {err}",
            path.display()
        ))
    })?;
    Ok(())
}

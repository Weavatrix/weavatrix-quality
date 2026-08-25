//! Persist a diagnostic failure reel. Never a verdict source.

use std::path::Path;

use wvq_domain::RunId;
use wvq_runtime::{BrowserProgramRun, FailureReelCapture, assemble_failure_reel};
use wvq_store::Store;

use super::BusError;
use super::persist_evidence::remove_browser_evidence_file;
use super::persist_run::{put_json_run_artifact, put_run_artifact};

pub(in crate::service) const FAILURE_REEL_KIND: &str = "failure-reel";
pub(in crate::service) const FAILURE_REEL_FRAME_KIND: &str = "failure-reel-frame";

pub(in crate::service) fn persist_failure_reel(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    result: &BrowserProgramRun,
    keep: bool,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let Some(capture) = result.failure_reel.as_ref() else {
        return Ok(());
    };
    if result.passed || !keep {
        remove_reel_files(capture)?;
        return Ok(());
    }
    let Some(mut reel) = assemble_failure_reel(false, Some(capture)) else {
        remove_reel_files(capture)?;
        return Ok(());
    };
    reel.frames.before = if reel.frames.before.is_some() {
        import_reel_frame(
            store,
            run_id,
            program_index,
            "before",
            capture.before_path.as_deref(),
            handles,
        )?
    } else {
        None
    };
    reel.frames.highlight = if reel.frames.highlight.is_some() {
        import_reel_frame(
            store,
            run_id,
            program_index,
            "highlight",
            capture.highlight_path.as_deref(),
            handles,
        )?
    } else {
        None
    };
    reel.frames.after = if reel.frames.after.is_some() {
        import_reel_frame(
            store,
            run_id,
            program_index,
            "after",
            capture.after_path.as_deref(),
            handles,
        )?
    } else {
        None
    };
    put_json_run_artifact(
        store,
        run_id,
        &format!(
            "artifact-{}-browser-{program_index}-failure-reel",
            run_id.as_str()
        ),
        FAILURE_REEL_KIND,
        &reel,
        handles,
    )?;
    remove_reel_files(capture)
}

fn import_reel_frame(
    store: &Store,
    run_id: &RunId,
    program_index: usize,
    slot: &str,
    path: Option<&Path>,
    handles: &mut Vec<String>,
) -> Result<Option<String>, BusError> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|err| {
        BusError::Runtime(format!(
            "cannot import failure-reel {slot} {}: {err}",
            path.display()
        ))
    })?;
    let id = format!(
        "artifact-{}-browser-{program_index}-failure-reel-{slot}",
        run_id.as_str()
    );
    put_run_artifact(
        store,
        run_id,
        &id,
        FAILURE_REEL_FRAME_KIND,
        &bytes,
        handles,
    )?;
    Ok(Some(id))
}

fn remove_reel_files(capture: &FailureReelCapture) -> Result<(), BusError> {
    for path in capture.frame_paths() {
        if path.exists() {
            remove_browser_evidence_file(path)?;
        }
    }
    Ok(())
}

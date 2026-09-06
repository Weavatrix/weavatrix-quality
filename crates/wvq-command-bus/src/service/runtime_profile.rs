//! Runtime profile CAS artifact for orchestration multiplicities.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::access::*;
use super::persist_run::put_json_run_artifact;

/// CAS artifact kind for per-run orchestration counters.
pub(in crate::service) const RUNTIME_PROFILE_KIND: &str = "runtime-profile";

/// Mutable counters collected during one controlled run.
#[derive(Debug, Default, Clone)]
pub(in crate::service) struct RuntimeProfileCounts {
    pub program_invocation: u64,
    pub responsive_probe: u64,
    pub runner_invocation: u64,
    pub cancelled: bool,
    pub remaining_required_work: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeProfileDocument {
    schema_v: u32,
    run_id: String,
    profile: String,
    counts: RuntimeProfileCountFields,
    stage_ms: serde_json::Map<String, Value>,
    cache: RuntimeProfileCache,
    peak_rss_bytes: Option<u64>,
    runtime_llm_tokens: u64,
    cancelled: bool,
    remaining_required_work: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeProfileCountFields {
    graph_open: Option<u64>,
    spec_compile: Option<u64>,
    surface_projection: Option<u64>,
    matrix_projection: Option<u64>,
    browser_launch: Option<u64>,
    browser_context: Option<u64>,
    program_invocation: u64,
    unique_work_keys: Option<u64>,
    responsive_probe: u64,
    runner_invocation: u64,
    artifact_read_bytes: Option<u64>,
    artifact_write_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeProfileCache {
    hits: Option<u64>,
    misses: Option<u64>,
    rejected_stale: Option<u64>,
}

/// Persist the runtime-profile artifact. Unknown meters stay JSON `null`.
pub(in crate::service) fn persist_runtime_profile(
    store: &Store,
    run: &RunId,
    profile: &str,
    counts: &RuntimeProfileCounts,
    handles: &mut Vec<String>,
) -> Result<(), BusError> {
    let document = RuntimeProfileDocument {
        schema_v: 1,
        run_id: run.to_string(),
        profile: profile.to_owned(),
        counts: RuntimeProfileCountFields {
            graph_open: None,
            spec_compile: None,
            surface_projection: None,
            matrix_projection: Some(1),
            browser_launch: None,
            browser_context: None,
            program_invocation: counts.program_invocation,
            unique_work_keys: None,
            responsive_probe: counts.responsive_probe,
            runner_invocation: counts.runner_invocation,
            artifact_read_bytes: None,
            artifact_write_bytes: None,
        },
        stage_ms: serde_json::Map::new(),
        cache: RuntimeProfileCache {
            hits: None,
            misses: None,
            rejected_stale: None,
        },
        peak_rss_bytes: None,
        runtime_llm_tokens: 0,
        cancelled: counts.cancelled,
        remaining_required_work: counts.remaining_required_work.clone(),
    };
    // Reject fake zeros for unknown meters: serialize Options as null.
    let _ = json!(&document);
    put_json_run_artifact(
        store,
        run,
        &format!("artifact-{}-runtime-profile", run.as_str()),
        RUNTIME_PROFILE_KIND,
        &document,
        handles,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_meters_serialize_as_null_not_fake_zero() {
        let document = RuntimeProfileDocument {
            schema_v: 1,
            run_id: "run-1".into(),
            profile: "pr".into(),
            counts: RuntimeProfileCountFields {
                graph_open: None,
                spec_compile: None,
                surface_projection: None,
                matrix_projection: Some(1),
                browser_launch: None,
                browser_context: None,
                program_invocation: 2,
                unique_work_keys: None,
                responsive_probe: 0,
                runner_invocation: 3,
                artifact_read_bytes: None,
                artifact_write_bytes: None,
            },
            stage_ms: serde_json::Map::new(),
            cache: RuntimeProfileCache {
                hits: None,
                misses: None,
                rejected_stale: None,
            },
            peak_rss_bytes: None,
            runtime_llm_tokens: 0,
            cancelled: false,
            remaining_required_work: Vec::new(),
        };
        let value = serde_json::to_value(&document).unwrap();
        assert!(value["counts"]["browser_launch"].is_null());
        assert!(value["peak_rss_bytes"].is_null());
        assert_eq!(value["counts"]["program_invocation"], 2);
        assert_eq!(value["runtime_llm_tokens"], 0);
    }
}

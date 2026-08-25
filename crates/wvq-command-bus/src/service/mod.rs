//! Domain facade. CLI and MCP call this; they do not reimplement policy.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use wvq_runtime::{CaptureWhen, CoverageArtifact, ExecutorTarget, TestStatus};

/// CAS artifact kind holding the base/head UI-integrity ratchet for one run.
pub(in crate::service) const UI_INTEGRITY_DELTA_KIND: &str = "ui-integrity-delta";
pub(in crate::service) const MUTATION_RESULTS_KIND: &str = "mutation-results";

mod authoring;
mod recovery;
mod policy;
mod delta;
mod types;
mod access;
mod error;
mod api;
mod fake;
mod live;
mod validate;
mod git;
mod graph;
mod paths;
mod verify_reply;
mod verify_axes;
mod verify_json;
mod verify_debt;
mod selection_build;
mod selection_audit;
mod execute;
mod persist_run;
mod persist_browser;
mod persist_failure_reel;
mod persist_ui;
mod persist_ui_analyse;
mod persist_behavior;
mod persist_evidence;
mod impact;
mod protection_snapshot;
mod protection_coverage;
mod protection_view;
mod protection_lineage;
mod protection_graph_extra;
mod analytics;
mod runner;
mod runner_coverage;

use authoring::{
    author_preview_token, authoring_authority_tokens, authoring_context, authoring_model_prompt,
    authoring_obligations, deterministic_checks, empty_debt,
    map_authoring_store_error, obligation_kind_token, obligation_texts, pack_context,
    persist_author_preview, requirement_texts, risk_token, unique_requirements,
    validate_author_candidate, validate_authoring_budget, working_tree_selection,
};
use policy::{
    browser_test_bindings, load_browser_policy, load_browser_policy_with,
    load_browser_runtime_with, load_debt_exceptions, load_live_browser_policy, load_model_policy,
    load_test_bindings, load_ui_integrity_policy, ui_collection_config,
};
use recovery::{
    recovery_candidates, recovery_code_delta, recovery_commits, recovery_evidence,
    recovery_existing_requirements,
};

use delta::{declared_code_flows, persist_delta_triangle};
use types::*;
pub use error::BusError;
pub use api::{QualityService, dispatch};
pub use fake::FakeService;
pub use live::LiveService;
use validate::*;
use git::*;
use graph::*;
use paths::*;
use verify_reply::{
    explain_ui_finding,
    artifact_handle_of_kind,
    explain_stored_proof,
    stored_range,
    snapshot_artifact,
    stored_oracle_replacement,
};
use verify_axes::{
    protection_axis_from,
    debt_axis_from,
    stability_axis,
    ui_integrity_axis,
    delta_triangle_axis,
};
use verify_json::{
    parse_axis_state,
    parse_ui_findings,
    json_string,
    json_u64,
    mandatory_test_paths,
};
use verify_debt::{
    verify_from_token,
    parse_proof_verdict,
    combine_verify,
    combine_verdicts,
    count_field,
    debt_bucket_ids,
    compact_debt_findings,
    explain_debt_finding,
};
use selection_build::{
    static_and_base_tests,
    historical_selection_candidates,
    merge_historical_selection,
    merge_impacted_stories,
    build_live_selection,
    merged_test_bindings,
    selection_candidates,
};
use selection_audit::{
    SelectionAuditArtifactInput,
    audit_live_selection,
    load_shadow_runs,
    persist_selection_audit_artifact,
    stored_selection_audit_reply,
    validate_shadow_scopes,
    missed_failure_identities,
    impact_nodes_from_artifact,
    resolve_observed_test_path,
    read_single_run_json,
    live_selection_report,
};
use execute::{
    build_execution_requests,
    batch_filter_groups,
    supports_path_filters,
    target_accepts_filter,
    available_test_paths,
    full_execution_requests,
    execute_full_targets,
};
use persist_run::{
    make_run_id,
    make_ai_usage_id,
    put_run_artifact,
    put_json_run_artifact,
    obligation_execution_map,
    normalized_suite_matches,
    normalized_status,
    severity_token,
};
use persist_browser::{
    persist_browser_runs,
    persist_browser_run,
    persist_browser_observations,
    persist_browser_files,
    stored_browser_assertions,
    BEHAVIOR_SAMPLE_LIMIT,
    BEHAVIOR_PROGRAM_SAMPLE_LIMIT,
    MAX_UI_ARTIFACT_BYTES,
    MAX_UI_REPLY_FINDINGS,
};
use persist_ui::{
    ui_delta_document,
    responsive_probe_incomplete,
    ui_finding_refs_with_intervals,
    ui_finding_refs,
    persist_ui_integrity,
};
use persist_ui_analyse::{
    CollectedUi,
    analyse_ui_snapshots,
    duplicate_mutation_finding,
    hit_test_summary,
    put_bounded_ui_artifact,
};
use persist_behavior::{
    persist_browser_behavior,
    persist_program_behavior,
    normalized_behavior_state,
    persist_behavior_edge,
    program_behavior_artifact,
    bounded_set,
    bounded_network_operation,
    recorded_api_operation,
};
use persist_evidence::{
    remove_browser_evidence_file,
    capture_active,
    browser_capture_active,
    browser_evidence_kinds,
    cap_browser_evidence,
    parse_obligation_execution_map,
    parse_revision_range_evidence,
    valid_commit_id,
};
use impact::{
    merge_browser_proof_evidence,
    live_impacted_surface,
};
use protection_snapshot::{
    ensure_complete_diff,
    live_protection_snapshot,
    executed_test_inventory,
    persist_dynamic_coverage_history,
};
use protection_coverage::{
    measured_protection_flows,
    CoverageProtector,
    coverage_protectors,
    coverage_graph_mismatch,
};
use protection_view::{
    expectation_change,
    build_protection_view,
};
use protection_lineage::{
    protection_test_changes,
    approved_replaced_flows,
    replacement_test_for_flow,
    test_identity_has_path,
    graph_relocations,
    snapshot_relocations,
    stable_symbol_signature,
    protection_lineage,
};
use protection_graph_extra::{
    graph_singleton_path,
    PersistedTestAnalytics,
    TestAnalyticsDocument,
    TestOutcomeCounts,
    ObservedTestCase,
};
use analytics::{
    collect_observed_test_cases,
    persist_failure,
    persist_test_analytics,
    test_status_token,
    failure_timing_bucket,
    flake_class_token,
};
use runner::{
    execution_summary,
    MAX_RUNNER_ARTIFACT_BYTES,
    clear_generated_runner_artifacts,
    attach_normalized_artifacts,
};
use runner_coverage::{
    ARTIFACT_CLOCK_TOLERANCE,
    normalize_coverage_paths,
    read_go_module,
    runner_artifact_candidates,
    artifact_is_fresh,
    set_record_error,
    stdout_kind,
};

/// CAS artifact kind holding live same-program Spec x Code x Behavior evidence.
pub(in crate::service) const DELTA_TRIANGLE_KIND: &str = "delta-triangle";

/// CAS artifact kind for the exact expectation replacement a QA reviewed.
pub(in crate::service) const ORACLE_REPLACEMENT_KIND: &str = "oracle-replacement-proposal";

#[cfg(test)]
mod tests;

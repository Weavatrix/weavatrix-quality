//! Intermediate state for the split live run_controlled pipeline.

use super::super::access::*;
use wvq_intelligence::ImpactedSurface;

pub(in crate::service) struct PreparedControlledRun {
    pub compiled: Compiled,
    pub mutation_policy: Option<MutationPolicy>,
    pub range: RevisionRange,
    pub changed: ChangedFiles,
    pub store: Store,
    pub browser: Option<BrowserPolicy>,
    pub before: RevisionId,
    pub protection_graph: Value,
    pub graph_diff: Value,
    pub static_selection: Value,
    pub impact: ImpactedSurface,
    pub historical_selection: Vec<HistoricalTestCandidate>,
    pub live_selection: LiveSelection,
    pub available_test_count: usize,
    pub execution_requests: Vec<ExecutionRequest>,
    pub effective_scope: String,
    pub scope_reason: String,
    pub executed_tests: Option<BTreeSet<String>>,
}

pub(in crate::service) struct ExecutedControlledRun<'a> {
    pub records: Vec<ExecutorRecord>,
    pub mutation_document: Option<MutationRunDocument>,
    pub ui_policy: UiIntegrityPolicy,
    pub browser_runs: Vec<(&'a ConfiguredBrowserProgram, BrowserProgramRun)>,
    pub base_browser_replay: Option<Result<BaseBrowserReplay, BusError>>,
    pub outcome: &'static str,
    pub run_id: RunId,
}

pub(in crate::service) struct PersistedControlledRun {
    pub handles: Vec<String>,
    pub head_ui: Option<UiIntegritySnapshot>,
}

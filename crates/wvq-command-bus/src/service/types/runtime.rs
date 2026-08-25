//! Live-run records, browser policy, and selection types.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use wvq_proof::{AiBudget, LocalModelConfig};
use wvq_runtime::{
    BrowserProgramRun, ExecutorTarget, NetworkRunPolicy, ProgramOracle, TestProgram,
};
use wvq_spec::{EvidenceKind, OpenSpecChange};
use wvq_domain::RevisionId;

#[derive(Debug, Clone)]
pub(in crate::service) struct RunState {
    pub(in crate::service) id: String,
    pub(in crate::service) status: String,
    pub(in crate::service) outcome: String,
    pub(in crate::service) handles: Vec<String>,
}

#[derive(Debug)]
pub(in crate::service) struct ExecutorRecord {
    pub(in crate::service) executor: String,
    pub(in crate::service) cwd: String,
    pub(in crate::service) selection: Vec<String>,
    pub(in crate::service) status_code: Option<i32>,
    pub(in crate::service) passed: bool,
    pub(in crate::service) error: Option<String>,
    pub(in crate::service) stdout: Vec<u8>,
    pub(in crate::service) stderr: Vec<u8>,
    pub(in crate::service) artifacts: Vec<ProducedArtifact>,
}

#[derive(Debug)]
pub(in crate::service) struct ProducedArtifact {
    pub(in crate::service) kind: String,
    pub(in crate::service) path: String,
    pub(in crate::service) bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(in crate::service) struct TestBinding {
    pub(in crate::service) path: String,
    pub(in crate::service) runner: Option<String>,
    pub(in crate::service) suite: Option<String>,
    pub(in crate::service) case: Option<String>,
    pub(in crate::service) obligations: BTreeSet<String>,
    pub(in crate::service) cost: u64,
    pub(in crate::service) flake_penalty: u64,
}

pub(in crate::service) struct BrowserPolicy {
    pub(in crate::service) base_url: String,
    pub(in crate::service) browser: String,
    pub(in crate::service) headless: bool,
    pub(in crate::service) timeout: Duration,
    pub(in crate::service) module_root: PathBuf,
    pub(in crate::service) network: NetworkRunPolicy,
    pub(in crate::service) programs: Vec<ConfiguredBrowserProgram>,
}

pub(in crate::service) struct ConfiguredBrowserProgram {
    pub(in crate::service) path: String,
    pub(in crate::service) program: TestProgram,
    pub(in crate::service) oracles: Vec<ProgramOracle>,
}

pub(in crate::service) struct BaseBrowserReplay {
    pub(in crate::service) revision: RevisionId,
    pub(in crate::service) spec: Option<OpenSpecChange>,
    pub(in crate::service) runs: Vec<BrowserProgramRun>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(in crate::service) struct StoredBrowserProgramEvidence {
    pub(in crate::service) schema_v: u32,
    pub(in crate::service) program: String,
    pub(in crate::service) asserted: Vec<String>,
    pub(in crate::service) contradicted: Vec<String>,
    pub(in crate::service) assertions: Vec<StoredBrowserAssertionEvidence>,
    pub(in crate::service) present: Vec<EvidenceKind>,
    pub(in crate::service) observations: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service) struct StoredBrowserAssertionEvidence {
    pub(in crate::service) obligation: String,
    pub(in crate::service) step: usize,
    pub(in crate::service) status: String,
    pub(in crate::service) observation: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service) struct StoredObligationExecutionMap {
    pub(in crate::service) schema_v: u32,
    pub(in crate::service) obligations: BTreeMap<String, Vec<StoredObligationExecution>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service) struct StoredObligationExecution {
    pub(in crate::service) executor: String,
    pub(in crate::service) path: String,
    pub(in crate::service) suite: String,
    pub(in crate::service) case: String,
    pub(in crate::service) status: String,
    pub(in crate::service) invocation_passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::service) assertion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in crate::service) observation: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service) struct StoredRevisionRangeEvidence {
    pub(in crate::service) schema_v: u32,
    pub(in crate::service) base: StoredRevisionEndpoint,
    pub(in crate::service) head: StoredRevisionEndpoint,
    pub(in crate::service) merge_base: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::service) struct StoredRevisionEndpoint {
    #[serde(rename = "ref")]
    pub(in crate::service) reference: String,
    pub(in crate::service) commit: String,
    #[serde(default)]
    pub(in crate::service) content_revision: Option<String>,
}

#[derive(Default)]
pub(in crate::service) struct BrowserProofEvidence {
    pub(in crate::service) programs: BTreeSet<String>,
    pub(in crate::service) present: Vec<EvidenceKind>,
    pub(in crate::service) observations: Vec<String>,
    pub(in crate::service) passed: bool,
    pub(in crate::service) failed: bool,
    pub(in crate::service) contradicted: bool,
}

#[derive(Default)]
pub(in crate::service) struct BehaviorContributionSummary {
    pub(in crate::service) states: u64,
    pub(in crate::service) new_states: u64,
    pub(in crate::service) edges: u64,
    pub(in crate::service) new_edges: u64,
}

pub(in crate::service) struct ProgramBehaviorContribution {
    pub(in crate::service) states: BTreeSet<String>,
    pub(in crate::service) new_states: BTreeSet<String>,
    pub(in crate::service) edges: BTreeSet<String>,
    pub(in crate::service) new_edges: BTreeSet<String>,
    pub(in crate::service) api_operations: BTreeSet<String>,
    pub(in crate::service) artifact: serde_json::Value,
}

pub(in crate::service) struct ModelPolicy {
    pub(in crate::service) model: LocalModelConfig,
    pub(in crate::service) budget: AiBudget,
}

pub(in crate::service) struct LiveSelection {
    pub(in crate::service) selected: Vec<String>,
    pub(in crate::service) explanations: Vec<Vec<String>>,
    pub(in crate::service) uncovered_mandatory: Vec<String>,
    pub(in crate::service) uncovered_all: Vec<String>,
    pub(in crate::service) bindings: Vec<TestBinding>,
}

pub(in crate::service) struct ExecutionRequest {
    pub(in crate::service) target: ExecutorTarget,
    pub(in crate::service) filters: Vec<String>,
    pub(in crate::service) selected_tests: Vec<String>,
}

pub(in crate::service) type FilterGroups =
    BTreeMap<(String, String), (ExecutorTarget, Vec<(String, String)>)>;

impl LiveSelection {
    pub(in crate::service) fn complete(&self) -> bool {
        self.uncovered_all.is_empty()
    }
}

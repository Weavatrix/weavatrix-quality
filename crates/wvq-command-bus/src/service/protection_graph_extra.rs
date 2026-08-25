//! Extracted command-bus helper.

use super::access::*;

pub(in crate::service) fn graph_singleton_path(graph: &Value, flow: &str) -> Vec<String> {
    if graph
        .get("nodes")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes
                .iter()
                .any(|node| graph_node_id(node).as_deref() == Some(flow))
        })
    {
        vec![flow.to_owned()]
    } else {
        Vec::new()
    }
}


#[derive(Debug)]
pub(in crate::service) struct PersistedTestAnalytics {
    pub(in crate::service) recorded_test_count: u64,
    pub(in crate::service) failed_test_count: u64,
    pub(in crate::service) flaky_test_count: u64,
    pub(in crate::service) unknown_failure_count: u64,
    pub(in crate::service) bytes: Vec<u8>,
}

#[derive(serde::Serialize)]
pub(in crate::service) struct TestAnalyticsDocument {
    pub(in crate::service) schema_v: u32,
    pub(in crate::service) run_id: String,
    pub(in crate::service) revision: String,
    pub(in crate::service) recorded_cases: u64,
    pub(in crate::service) outcomes: TestOutcomeCounts,
    pub(in crate::service) failure_occurrences: Vec<Value>,
    pub(in crate::service) flaky_tests: Vec<Value>,
    pub(in crate::service) slowest_tests: Vec<Value>,
    pub(in crate::service) runtime_llm_tokens: u64,
}

#[derive(serde::Serialize)]
pub(in crate::service) struct TestOutcomeCounts {
    pub(in crate::service) passed: u64,
    pub(in crate::service) failed: u64,
    pub(in crate::service) errors: u64,
    pub(in crate::service) skipped: u64,
}

#[derive(Debug)]
pub(in crate::service) struct ObservedTestCase {
    pub(in crate::service) executor: String,
    pub(in crate::service) suite: String,
    pub(in crate::service) name: String,
    pub(in crate::service) status: TestStatus,
    pub(in crate::service) duration_ms: Option<u64>,
    pub(in crate::service) message: Option<String>,
}

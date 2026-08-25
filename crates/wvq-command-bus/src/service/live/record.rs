//! Inherent LiveService record_controlled orchestrator.

use super::super::access::*;
use super::LiveService;
use wvq_runtime::{BehaviorTrace, BrowserRecording};

pub(in crate::service) struct CapturedRecording {
    pub compiled: Compiled,
    pub before: RevisionId,
    pub session_id: String,
    pub recording: BrowserRecording,
    pub trace: BehaviorTrace,
    pub store: Store,
    pub new_behavior_states: u64,
    pub new_behavior_edges: u64,
    pub linked_obligations: Vec<String>,
    pub new_obligations: Vec<String>,
    pub api_operations: Vec<String>,
    pub new_api_operations: Vec<String>,
    pub useful: bool,
}

impl LiveService {
    pub(in crate::service) fn record_controlled(
        &self,
        cmd: &RecordCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<RecordReply, BusError> {
        let captured = self.capture_controlled_record(cmd, Arc::clone(&cancel))?;
        self.persist_controlled_record(cmd, cancel, captured)
    }
}

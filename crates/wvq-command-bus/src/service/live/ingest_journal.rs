//! Admit a continuous observation journal as `OBSERVED_ONLY` `BehaviorGraph` evidence.

use super::super::access::*;
use super::LiveService;
use wvq_runtime::{BehaviorEdge, BehaviorTrace};
use wvq_store::Store;

impl LiveService {
    pub(in crate::service) fn ingest_journal(
        &self,
        cmd: &IngestJournalCommand,
    ) -> Result<IngestJournalReply, BusError> {
        let journal = ContinuousJournal::from_json(&cmd.journal)
            .map_err(|err| BusError::InvalidInput(err.to_string()))?;
        let compiled = self.compiled(&cmd.change)?;
        let _range = self.revision_range(&cmd.base, &cmd.head)?;
        let revision = self.revision()?.to_string();
        let trace = journal
            .to_trace()
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let store = self.store()?;
        let (new_behavior_states, new_behavior_edges, edges) = journal_novelty(&store, &trace)?;
        let captured_events = u64::try_from(trace.events.len()).unwrap_or(u64::MAX);
        if new_behavior_states == 0 && new_behavior_edges == 0 {
            return Ok(journal_reply(JournalOutcome {
                session_id: journal.session_id,
                change: compiled.change,
                revision,
                captured_events,
                new_behavior_states: 0,
                new_behavior_edges: 0,
                trace_handle: None,
                journal_handle: None,
            }));
        }
        persist_journal_behavior(&store, &trace, &edges)?;
        let session_id = journal.session_id.clone();
        let (trace_handle, journal_handle) =
            persist_journal_artifacts(&store, &session_id, &trace, cmd.journal.as_bytes())?;
        Ok(journal_reply(JournalOutcome {
            session_id,
            change: compiled.change,
            revision,
            captured_events,
            new_behavior_states,
            new_behavior_edges,
            trace_handle: Some(trace_handle),
            journal_handle: Some(journal_handle),
        }))
    }
}

fn journal_novelty(
    store: &Store,
    trace: &BehaviorTrace,
) -> Result<(u64, u64, Vec<BehaviorEdge>), BusError> {
    let mut new_behavior_states = 0_u64;
    for digest in trace
        .state_digests()
        .map_err(|err| BusError::Runtime(err.to_string()))?
    {
        if !store
            .has_behavior_state(&digest)
            .map_err(|err| BusError::Store(err.to_string()))?
        {
            new_behavior_states = new_behavior_states.saturating_add(1);
        }
    }
    let mut new_behavior_edges = 0_u64;
    let edges = trace
        .edges()
        .map_err(|err| BusError::Runtime(err.to_string()))?;
    for edge in edges.iter().filter(|edge| edge.src != edge.dst) {
        let action = serde_json::to_string(&edge.action)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        if !store
            .has_behavior_edge(&edge.src, &edge.dst, &action)
            .map_err(|err| BusError::Store(err.to_string()))?
        {
            new_behavior_edges = new_behavior_edges.saturating_add(1);
        }
    }
    Ok((new_behavior_states, new_behavior_edges, edges))
}

fn persist_journal_behavior(
    store: &Store,
    trace: &BehaviorTrace,
    edges: &[BehaviorEdge],
) -> Result<(), BusError> {
    for state in std::iter::once(&trace.initial).chain(trace.events.iter().map(|event| &event.after))
    {
        let body = state
            .canonical_json()
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let digest = state
            .digest()
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        store
            .put_behavior_state(&digest, &body)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    for edge in edges {
        let action =
            serde_json::to_string(&edge.action).map_err(|err| BusError::Runtime(err.to_string()))?;
        store
            .put_behavior_edge(&edge.src, &edge.dst, &action)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    Ok(())
}

fn persist_journal_artifacts(
    store: &Store,
    session_id: &str,
    trace: &BehaviorTrace,
    journal: &[u8],
) -> Result<(String, String), BusError> {
    let trace_handle = format!("artifact-session-{session_id}-trace");
    let journal_handle = format!("artifact-session-{session_id}-journal");
    let trace_artifact =
        ArtifactId::new(&trace_handle).map_err(|err| BusError::Identity(err.to_string()))?;
    let journal_artifact =
        ArtifactId::new(&journal_handle).map_err(|err| BusError::Identity(err.to_string()))?;
    let trace_body = serde_json::to_vec(trace).map_err(|err| BusError::Runtime(err.to_string()))?;
    store
        .put_artifact(&trace_artifact, "behavior-trace", &trace_body)
        .map_err(|err| BusError::Store(err.to_string()))?;
    store
        .put_artifact(
            &journal_artifact,
            CONTINUOUS_OBSERVATION_JOURNAL_KIND,
            journal,
        )
        .map_err(|err| BusError::Store(err.to_string()))?;
    Ok((trace_handle, journal_handle))
}

struct JournalOutcome {
    session_id: String,
    change: String,
    revision: String,
    captured_events: u64,
    new_behavior_states: u64,
    new_behavior_edges: u64,
    trace_handle: Option<String>,
    journal_handle: Option<String>,
}

fn journal_reply(outcome: JournalOutcome) -> IngestJournalReply {
    let useful = outcome.new_behavior_states != 0 || outcome.new_behavior_edges != 0;
    IngestJournalReply {
        session_id: outcome.session_id,
        change: outcome.change,
        revision: outcome.revision,
        captured_events: outcome.captured_events,
        useful,
        discarded: !useful,
        discard_reason: (!useful).then(|| "no_new_behavior".into()),
        new_behavior_states: outcome.new_behavior_states,
        new_behavior_edges: outcome.new_behavior_edges,
        observed_only: true,
        seal_eligible: false,
        trace_handle: outcome.trace_handle,
        journal_handle: outcome.journal_handle,
        runtime_llm_tokens: 0,
    }
}

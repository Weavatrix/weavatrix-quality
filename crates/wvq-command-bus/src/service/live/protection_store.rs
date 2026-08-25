//! Persist and measure protection snapshots.

use super::super::access::*;
use super::super::execute::execute_full_targets;
use super::super::persist_run::put_json_run_artifact;
use super::super::persist_run::put_run_artifact;
use super::super::protection_snapshot::live_protection_snapshot;
use super::super::verify_reply::{snapshot_artifact, stored_oracle_replacement};
use super::LiveService;

impl LiveService {
    /// Attach the measured base snapshot to the head run, idempotently.
    pub(in crate::service) fn persist_base_protection(
        &self,
        run: &str,
        base: &ProtectionSnapshot,
    ) -> Result<(), BusError> {
        let run = RunId::new(run).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        if snapshot_artifact(&store, &run, "base-protection-snapshot")?.is_some() {
            return Ok(());
        }
        let mut handles = Vec::new();
        put_json_run_artifact(
            &store,
            &run,
            &format!("{run}-base-protection-snapshot"),
            "base-protection-snapshot",
            base,
            &mut handles,
        )
    }

    /// Persist one immutable, revision-bound expectation replacement proposal.
    ///
    /// A human decision is stored separately and must match both the derived
    /// subject and the CAS digest of these exact bytes.
    pub(in crate::service) fn persist_oracle_replacement(
        &self,
        run: &str,
        document: &OracleReplacementDocument,
    ) -> Result<OracleReplacementReview, BusError> {
        let run = RunId::new(run).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        if let Some((stored, review)) = stored_oracle_replacement(&store, &run)? {
            if &stored != document {
                return Err(BusError::Ambiguous(format!(
                    "run {run} already carries a different OracleSeal replacement proposal"
                )));
            }
            return Ok(review);
        }
        let bytes = serde_json::to_vec_pretty(document).map_err(|err| {
            BusError::Runtime(format!("cannot encode OracleSeal replacement: {err}"))
        })?;
        let mut handles = Vec::new();
        put_run_artifact(
            &store,
            &run,
            &format!("artifact-{run}-oracle-replacement"),
            ORACLE_REPLACEMENT_KIND,
            &bytes,
            &mut handles,
        )?;
        stored_oracle_replacement(&store, &run)?
            .map(|(_, review)| review)
            .ok_or_else(|| {
                BusError::Store(format!(
                    "run {run} did not retain its OracleSeal replacement proposal"
                ))
            })
    }

    pub(in crate::service) fn stored_protection_snapshot(
        &self,
        run: &str,
    ) -> Result<ProtectionSnapshot, BusError> {
        let run = RunId::new(run).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        let mut found = None;
        for artifact in store
            .run_artifacts(&run)
            .map_err(|err| BusError::Store(err.to_string()))?
        {
            let (record, bytes) = store
                .read_artifact(&artifact)
                .map_err(|err| BusError::Store(err.to_string()))?;
            if record.kind != "protection-snapshot" {
                continue;
            }
            if found.is_some() {
                return Err(BusError::Store(format!(
                    "run {run} has more than one protection snapshot"
                )));
            }
            found = Some(serde_json::from_slice(&bytes).map_err(|err| {
                BusError::Store(format!("invalid protection snapshot on run {run}: {err}"))
            })?);
        }
        found.ok_or_else(|| {
            BusError::Runtime(format!(
                "run {run} produced no measured protection snapshot; coverage is required"
            ))
        })
    }

    pub(in crate::service) fn measure_base_protection(
        &self,
        range: &RevisionRange,
        files: &[String],
        change: &str,
    ) -> Result<(ProtectionSnapshot, Value, Compiled, OracleIdentity), BusError> {
        if let Some(err) = &self.executor_init_error {
            return Err(BusError::Runtime(format!(
                "registered executor initialization failed: {err}"
            )));
        }
        let worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let compiled = compile_repository(&worktree.path, change)?;
        let oracle = oracle_identity(&worktree.path, &compiled)?;
        let evidence = WeavatrixProvider
            .analyze(&worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?;
        let graph = protection_graph_for_files(&worktree.path, &evidence.revision, files)?;
        let targets = discover_executor_targets(&worktree.path)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        if targets.is_empty() {
            return Err(BusError::Runtime(
                "base revision has no supported registered executor".into(),
            ));
        }
        let records = execute_full_targets(
            &self.executors,
            &worktree.path,
            &targets,
            &Arc::new(AtomicBool::new(false)),
        )?;
        if records
            .iter()
            .any(|record| !record.passed || record.error.is_some())
        {
            return Err(BusError::Runtime(
                "base protection replay did not pass every registered runner".into(),
            ));
        }
        let bindings = load_test_bindings(&worktree.path)?;
        let protection = live_protection_snapshot(
            &worktree.path,
            &evidence.revision,
            &graph,
            &records,
            &bindings,
        )?
        .ok_or_else(|| {
            BusError::Runtime(
                "base protection replay produced no coverage for the impacted graph".into(),
            )
        })?;
        Ok((protection, graph, compiled, oracle))
    }
}

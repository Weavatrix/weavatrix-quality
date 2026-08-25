//! UI-integrity view and stored snapshot persistence.

use super::super::access::*;
use super::super::persist_ui::ui_delta_document;
use super::super::persist_ui_analyse::put_bounded_ui_artifact;
use super::super::selection_audit::read_single_run_json;
use super::LiveService;

impl LiveService {
    /// Replay the browser programs on both revisions and ratchet the UI.
    ///
    /// Head evidence comes from a normal run, which already collects it. Base
    /// evidence needs the same programs against the merge-base, so this creates
    /// a temporary worktree and replays them there — the same shape as
    /// `protection_view`, and for the same reason: a regression is only
    /// meaningful against what the code used to do.
    ///
    /// The resulting delta is persisted against the head run, so a later
    /// `quality_verify` composes the UI axis from stored evidence without
    /// executing anything.
    ///
    /// # Errors
    ///
    /// A disabled policy, a head run that did not pass, missing base programs,
    /// or a malformed snapshot. Uses zero runtime model tokens.
    pub fn ui_integrity_view(
        &self,
        change: &str,
        base: &str,
        head: &str,
    ) -> Result<UiIntegrityDelta, BusError> {
        let compiled = self.compiled(change)?;
        let policy = load_ui_integrity_policy(&self.repo)?;
        if !policy.enabled {
            return Err(BusError::Runtime(
                "ui_integrity is not enabled in .weavatrix-quality/config.yaml".into(),
            ));
        }
        let range = self.revision_range(base, head)?;
        let head_run = self.run(&RunCommand {
            change: compiled.change.clone(),
            base: range.base_ref.clone(),
            head: range.head_ref.clone(),
            scope: "all".into(),
            evidence_policy: "standard".into(),
        })?;
        let store = self.store()?;
        let run_id =
            RunId::new(&head_run.run_id).map_err(|err| BusError::Identity(err.to_string()))?;
        let head_snapshot = Self::stored_ui_snapshot(&store, &run_id)?.ok_or_else(|| {
            BusError::Runtime(format!(
                "run {} collected no UI evidence; check that a browser program is selected",
                head_run.run_id
            ))
        })?;
        let base_snapshot = self.measure_base_ui(&range, &compiled, &policy)?;
        let previously_fixed = store
            .previously_fixed_debt()
            .map_err(|err| BusError::Store(err.to_string()))?
            .into_iter()
            .filter(|item| item.starts_with("ui:"))
            .collect::<BTreeSet<_>>();
        let mut delta = ratchet_ui(&base_snapshot, &head_snapshot, &previously_fixed, &policy);
        if policy.responsive.enabled {
            let (intervals, truncated) = self.measure_responsive_ui(
                &range,
                &compiled,
                &policy,
                &base_snapshot,
                &head_snapshot,
                &previously_fixed,
            )?;
            delta.responsive_intervals = intervals;
            delta.responsive_truncated = truncated;
        }
        // Remember what this change fixed so a later reintroduction is
        // `returned` rather than `new`.
        let fixed = delta.fixed_fingerprints();
        if !fixed.is_empty() {
            let revision = RevisionId::new(&head_snapshot.revision)
                .map_err(|err| BusError::Identity(err.to_string()))?;
            store
                .remember_fixed_debt(&fixed, &revision)
                .map_err(|err| BusError::Store(err.to_string()))?;
        }
        Self::persist_ui_delta(&store, &run_id, &base_snapshot, &delta)?;
        Ok(delta)
    }

    /// Read back the `ui-integrity-findings` artifact a run persisted.
    pub(in crate::service) fn stored_ui_snapshot(
        store: &Store,
        run: &RunId,
    ) -> Result<Option<UiIntegritySnapshot>, BusError> {
        let Ok(document) = read_single_run_json(store, run, "ui-integrity-findings") else {
            return Ok(None);
        };
        if document.get("schema_v").and_then(Value::as_u64) != Some(1) {
            return Err(BusError::Store(
                "unknown ui-integrity-findings schema version".into(),
            ));
        }
        let findings: Vec<UiIntegrityFinding> =
            serde_json::from_value(document.get("findings").cloned().unwrap_or(json!([])))
                .map_err(|err| {
                    BusError::Store(format!("malformed stored ui-integrity findings: {err}"))
                })?;
        let measured_states = serde_json::from_value(
            document
                .get("measured_states")
                .cloned()
                .unwrap_or(json!([])),
        )
        .map_err(|err| BusError::Store(format!("malformed stored ui measured states: {err}")))?;
        Ok(Some(UiIntegritySnapshot {
            revision: document
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            measured_states,
            findings,
            responsive_breakpoints: serde_json::from_value(
                document
                    .get("responsive_breakpoints")
                    .cloned()
                    .unwrap_or(json!([])),
            )
            .map_err(|err| {
                BusError::Store(format!("malformed stored responsive breakpoints: {err}"))
            })?,
            responsive_breakpoints_incomplete: document
                .get("responsive_breakpoints_incomplete")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            truncated: document
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }))
    }

    /// Store the base snapshot and the classified delta on the head run.
    pub(in crate::service) fn persist_ui_delta(
        store: &Store,
        run: &RunId,
        base: &UiIntegritySnapshot,
        delta: &UiIntegrityDelta,
    ) -> Result<(), BusError> {
        let mut handles = Vec::new();
        Self::persist_ui_delta_with_handles(store, run, base, delta, &mut handles)
    }

    pub(in crate::service) fn persist_ui_delta_with_handles(
        store: &Store,
        run: &RunId,
        base: &UiIntegritySnapshot,
        delta: &UiIntegrityDelta,
        handles: &mut Vec<String>,
    ) -> Result<(), BusError> {
        if read_single_run_json(store, run, "base-ui-integrity-findings").is_err() {
            put_bounded_ui_artifact(
                store,
                run,
                &format!("artifact-{}-base-ui-integrity", run.as_str()),
                "base-ui-integrity-findings",
                &json!({
                    "schema_v": 1,
                    "revision": base.revision,
                    "measured_states": base.measured_states,
                    "findings": base.findings,
                    "responsive_breakpoints": base.responsive_breakpoints,
                    "responsive_breakpoints_incomplete": base.responsive_breakpoints_incomplete,
                    "truncated": base.truncated,
                }),
                handles,
            )?;
        }
        if read_single_run_json(store, run, UI_INTEGRITY_DELTA_KIND).is_ok() {
            return Ok(());
        }
        put_bounded_ui_artifact(
            store,
            run,
            &format!("artifact-{}-ui-integrity-delta", run.as_str()),
            UI_INTEGRITY_DELTA_KIND,
            &ui_delta_document(delta),
            handles,
        )
    }
}

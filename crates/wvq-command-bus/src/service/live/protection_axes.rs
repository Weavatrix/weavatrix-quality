//! Compose protection, debt, and AI axes from stored evidence.

use super::super::access::*;
use super::super::protection_lineage::{
    approved_replaced_flows, protection_test_changes, snapshot_relocations,
};
use super::super::verify_axes::{
    debt_axis_from, delta_triangle_axis, protection_axis_from, stability_axis, ui_integrity_axis,
};
use super::super::verify_reply::{snapshot_artifact, stored_oracle_replacement, stored_range};
use super::LiveService;

impl LiveService {
    /// Gather every axis `quality_verify` composes, from stored evidence only.
    ///
    /// `verify` never executes. Each axis therefore reports one of three honest
    /// states: measured, `not_applicable` when this change has no surface the
    /// axis can see, or `unmeasured` when it does have that surface and the
    /// evidence is absent. No axis is silently reported as clean.
    pub(in crate::service) fn verdict_inputs(
        &self,
        compiled: &Compiled,
        run: Option<&StoredRun>,
        proofs: Vec<ProofOutcome>,
    ) -> Result<VerdictInputs, BusError> {
        let Some(run) = run else {
            // Nothing ran at this revision. The proof axis still reports the
            // gap; the execution-backed axes have no surface to measure.
            return Ok(VerdictInputs {
                proofs,
                ..VerdictInputs::default()
            });
        };
        let store = self.store()?;
        let range = stored_range(&store, &run.id);
        let (protection, protection_limits) = self.protection_axis(&store, run, compiled)?;
        let (debt, debt_limits) = self.debt_axis(compiled, range.as_ref());
        let (stability, stability_limits) = stability_axis(&self.repo, &store, run, compiled);
        let ai = self.ai_axis(&store, compiled)?;
        let (ui_integrity, ui_limits) = ui_integrity_axis(&store, run, compiled)?;
        let (delta_triangle, delta_limits) = delta_triangle_axis(&store, run)?;
        let mut limitations = protection_limits;
        limitations.extend(debt_limits);
        limitations.extend(stability_limits);
        limitations.extend(ui_limits);
        limitations.extend(delta_limits);
        Ok(VerdictInputs {
            proofs,
            protection,
            debt,
            stability,
            ai,
            ui_integrity,
            delta_triangle,
            limitations,
        })
    }

    /// Protection continuity from the two stored snapshots.
    ///
    /// The head snapshot is written by every run that produced measured
    /// coverage. The base snapshot is written by `protection_view`, which is the
    /// only path allowed to replay the base suite; `verify` reuses it instead of
    /// re-running anything. With head coverage but no base snapshot the axis is
    /// `unmeasured`, never `clean` — a change cannot be shown to have preserved
    /// protection it never compared against.
    pub(in crate::service) fn protection_axis(
        &self,
        store: &Store,
        run: &StoredRun,
        compiled: &Compiled,
    ) -> Result<(ProtectionAxis, Vec<Limitation>), BusError> {
        let head = snapshot_artifact(store, &run.id, "protection-snapshot")?;
        let base = snapshot_artifact(store, &run.id, "base-protection-snapshot")?;
        match (base, head) {
            (Some(base), Some(head)) => {
                let oracle_replacement = stored_oracle_replacement(store, &run.id)?;
                let review = oracle_replacement.as_ref().map(|(_, review)| review);
                if let Some((document, _)) = &oracle_replacement {
                    let range = stored_range(store, &run.id).ok_or_else(|| {
                        BusError::Store(format!(
                            "run {} has an OracleSeal replacement without revision-range evidence",
                            run.id
                        ))
                    })?;
                    let current_oracle = oracle_identity(&self.repo, compiled)?;
                    if document.change != compiled.change
                        || document.base_revision != range.merge_base
                        || document.head_revision != range.head_commit
                        || document.head_content_revision != range.head_content_revision
                        || document.merge_base != range.merge_base
                        || document.head_content_revision != run.revision.as_str()
                        || document.head_seal != current_oracle.id
                        || document.head_seal_digest != current_oracle.digest
                    {
                        return Err(BusError::Ambiguous(format!(
                            "run {} carries an OracleSeal replacement for a different change, revision range, or head seal",
                            run.id
                        )));
                    }
                }
                let context = DeltaContext {
                    relocations: snapshot_relocations(&base, &head),
                    changed_obligations: review
                        .map(|review| review.changed_obligations.clone())
                        .unwrap_or_default(),
                    obligation_replacements: review
                        .map(|review| review.obligation_replacements.clone())
                        .unwrap_or_default(),
                    oracle_replacement_approved: review.is_some_and(|review| review.approved),
                    approved_replaced_flows: approved_replaced_flows(&base, &head, review),
                    ..DeltaContext::default()
                };
                let deltas = protection_delta(&base, &head, &context);
                let any_high_risk = compiled
                    .obligations
                    .iter()
                    .any(|item| matches!(item.risk, RiskLevel::High | RiskLevel::Critical));
                let high_risk_flows = if any_high_risk {
                    deltas.iter().map(|item| item.flow.clone()).collect()
                } else {
                    Vec::new()
                };
                let findings = gate_protection(&ProtectionCheckInput {
                    deltas: deltas.clone(),
                    tests: protection_test_changes(
                        &base,
                        &head,
                        &deltas,
                        &BTreeSet::new(),
                        &context,
                    ),
                    trends: Vec::new(),
                    policy: ProtectionPolicy {
                        high_risk_flows,
                        substitution_ratio: 10,
                    },
                });
                Ok((protection_axis_from(&deltas, &findings), Vec::new()))
            }
            (_, Some(_)) => Ok((
                ProtectionAxis {
                    state: AxisState::Unmeasured,
                    ..ProtectionAxis::default()
                },
                vec![Limitation {
                    axis: "protection".into(),
                    detail: "head protection was measured but no base snapshot exists; \
                             run the protection profile to replay the base suite"
                        .into(),
                }],
            )),
            // No coverage reached the impacted graph at all: this change has no
            // protection surface to compare.
            (_, None) => Ok((ProtectionAxis::default(), Vec::new())),
        }
    }

    /// Debt ratchet over the exact range the run measured.
    ///
    /// Weavatrix `run_audit` is a read-only graph query, so it is safe on the
    /// verify path. Any failure degrades to `unmeasured` rather than turning a
    /// missing comparison into a clean axis or aborting the whole verdict.
    pub(in crate::service) fn debt_axis(
        &self,
        compiled: &Compiled,
        range: Option<&RevisionRange>,
    ) -> (DebtAxis, Vec<Limitation>) {
        let Some(range) = range else {
            return (DebtAxis::default(), Vec::new());
        };
        match self.debt(&DebtCommand {
            change: compiled.change.clone(),
            base: range.base_ref.clone(),
            head: range.head_ref.clone(),
        }) {
            Ok(reply) => (debt_axis_from(&reply), Vec::new()),
            Err(err) => (
                DebtAxis {
                    state: AxisState::Unmeasured,
                    ..DebtAxis::default()
                },
                vec![Limitation {
                    axis: "debt".into(),
                    detail: format!("base/head debt comparison is unavailable: {err}"),
                }],
            ),
        }
    }

    /// Measured AI spend for this change. The ordinary green path spends none.
    pub(in crate::service) fn ai_axis(
        &self,
        store: &Store,
        compiled: &Compiled,
    ) -> Result<AiAxis, BusError> {
        let persisted = store
            .ai_usage_for_change(&compiled.change)
            .map_err(|err| BusError::Store(err.to_string()))?
            .unwrap_or_default();
        if persisted.planning_tokens == 0
            && persisted.runtime_tokens == 0
            && persisted.browser_escape_calls == 0
            && persisted.vision_calls == 0
        {
            // Nothing was ever charged to this change: the axis has no surface.
            return Ok(AiAxis::default());
        }
        let usage = AiUsage {
            planning_tokens: persisted.planning_tokens,
            runtime_tokens: persisted.runtime_tokens,
            browser_escape_calls: u32::try_from(persisted.browser_escape_calls).unwrap_or(u32::MAX),
            vision_calls: u32::try_from(persisted.vision_calls).unwrap_or(u32::MAX),
            cost_micros: persisted.cost_micros,
        };
        let budget_exhausted = load_model_policy(&self.repo)
            .is_ok_and(|policy| AiCostFirewall::with_usage(policy.budget, usage).is_exhausted());
        Ok(AiAxis {
            state: if budget_exhausted {
                AxisState::Warnings
            } else {
                AxisState::Clean
            },
            runtime_tokens: persisted.runtime_tokens,
            budget_exhausted,
            // A decision only becomes an unresolved blocker once a caller
            // records one. WVQ never invents a pending decision.
            unresolved_decisions: Vec::new(),
        })
    }
}

//! Inherent LiveService debt and select commands.

use super::super::access::*;
use super::super::impact::live_impacted_surface;
use super::super::selection_build::{build_live_selection, historical_selection_candidates};
use super::super::verify_debt::{compact_debt_findings, count_field, debt_bucket_ids};
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn debt(&self, cmd: &DebtCommand) -> Result<DebtReply, BusError> {
        let _ = self.compiled(&cmd.change)?;
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let revision = self.revision()?;
        let report = self.weavatrix_operation(
            &revision,
            "run_audit",
            &json!({"base_ref": range.merge_base, "debt": "all", "max_findings": 5000}),
        )?;
        let debt = report
            .get("debt")
            .ok_or_else(|| BusError::Intelligence("run_audit omitted debt evidence".into()))?;
        let comparison_present = debt
            .pointer("/comparison/present")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !comparison_present {
            return Err(BusError::Intelligence(
                debt.pointer("/comparison/reason")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("immutable debt comparison is unavailable")
                    .into(),
            ));
        }
        let counts = debt
            .get("counts")
            .ok_or_else(|| BusError::Intelligence("run_audit omitted debt counts".into()))?;
        let expected_new = count_field(counts, "new")?;
        let expected_existing = count_field(counts, "existing")?;
        let expected_fixed = count_field(counts, "fixed")?;
        let new_ids = debt_bucket_ids(debt, "new", expected_new)?;
        let existing_ids = debt_bucket_ids(debt, "existing", expected_existing)?;
        let fixed_ids = debt_bucket_ids(debt, "fixed", expected_fixed)?;
        let store = self.store()?;
        let previously_fixed = store
            .previously_fixed_debt()
            .map_err(|err| BusError::Store(err.to_string()))?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let exceptions = load_debt_exceptions(&self.repo)?;
        let head_ids = new_ids
            .iter()
            .chain(existing_ids.iter())
            .cloned()
            .collect::<BTreeSet<_>>();
        let excepted = head_ids
            .intersection(&exceptions.active)
            .cloned()
            .collect::<BTreeSet<_>>();
        let returned = new_ids
            .intersection(&previously_fixed)
            .filter(|id| !excepted.contains(*id))
            .cloned()
            .collect::<BTreeSet<_>>();
        store
            .remember_fixed_debt(&fixed_ids.iter().cloned().collect::<Vec<_>>(), &revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let mut limitations = debt
            .get("uncomparable_categories")
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flat_map(|items| items.iter())
            .map(|(kind, reason)| {
                format!("{kind}: {}", reason.as_str().unwrap_or("not comparable"))
            })
            .collect::<Vec<_>>();
        limitations.extend(exceptions.notes);
        Ok(DebtReply {
            base: range.base_ref,
            head: range.head_ref,
            revision: Some(revision.to_string()),
            comparison_present,
            existing: u64::try_from(existing_ids.difference(&excepted).count()).unwrap_or(u64::MAX),
            new: u64::try_from(
                new_ids
                    .difference(&excepted)
                    .filter(|id| !returned.contains(*id))
                    .count(),
            )
            .unwrap_or(u64::MAX),
            fixed: expected_fixed,
            returned: u64::try_from(returned.len()).unwrap_or(u64::MAX),
            excepted: u64::try_from(excepted.len()).unwrap_or(u64::MAX),
            findings: compact_debt_findings(debt, &returned, &excepted),
            limitations,
        })
    }

    pub(in crate::service) fn select(&self, cmd: &SelectCommand) -> Result<SelectReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let revision = self.revision()?;
        let static_report = self.weavatrix_operation(
            &revision,
            "select_tests",
            &json!({
                "base_ref": range.merge_base,
                "max_tests": 500,
                "depth": 6,
                "max_nodes": 2000
            }),
        )?;
        let diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 2000,
                "token_budget": 20000
            }),
        )?;
        let change_impact = self.weavatrix_operation(
            &revision,
            "change_impact",
            &json!({
                "base_ref": range.merge_base,
                "depth": 6,
                "max_nodes": 100_000,
                "precision": "graph"
            }),
        )?;
        let obligations: Vec<ObligationNeed> = compiled
            .obligations
            .iter()
            .map(|item| ObligationNeed {
                id: item.id.to_string(),
                high_risk: matches!(item.risk, RiskLevel::High | RiskLevel::Critical),
            })
            .collect();
        let store = self.store()?;
        let browser_bindings = load_live_browser_policy(&self.repo, &compiled, &store)?
            .as_ref()
            .map_or_else(Vec::new, browser_test_bindings);
        let impact = live_impacted_surface(&diff, &change_impact)?;
        let historical_selection = historical_selection_candidates(&store, &impact)?;
        let selection = build_live_selection(
            &self.repo,
            &static_report,
            &diff,
            &impact,
            &obligations,
            &browser_bindings,
            &historical_selection,
        )?;
        let selection_complete = selection.complete();
        Ok(SelectReply {
            base: range.base_ref,
            head: range.head_ref,
            revision: Some(revision.to_string()),
            algorithm: "weavatrix-base-head-history-union+greedy-weighted-set-cover".into(),
            selected: selection.selected,
            uncovered_mandatory: selection.uncovered_mandatory,
            explanations: selection.explanations,
            executed: false,
            selection_complete,
        })
    }
}

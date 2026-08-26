//! Snapshot existing quality debt as `OBSERVED_ONLY` baseline evidence.

use super::super::access::*;
use super::super::verify_debt::{count_field, debt_bucket_ids};
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn baseline(
        &self,
        cmd: &BaselineCommand,
    ) -> Result<BaselineReply, BusError> {
        if cmd.decision != "observed_only" {
            return Err(BusError::Unknown {
                field: "decision",
                value: cmd.decision.clone(),
            });
        }
        let compiled = self.compiled(&cmd.change)?;
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
        if !debt
            .pointer("/comparison/present")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
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
        let expected_existing = count_field(counts, "existing")?;
        let new_unbaselined = count_field(counts, "new")?;
        let mut fingerprints: Vec<String> = debt_bucket_ids(debt, "existing", expected_existing)?
            .into_iter()
            .collect();
        fingerprints.sort();
        let recorded = u64::try_from(fingerprints.len()).unwrap_or(u64::MAX);
        self.store()?
            .remember_observed_baseline(&fingerprints, &revision, &compiled.change)
            .map_err(|err| BusError::Store(err.to_string()))?;
        Ok(BaselineReply {
            change: compiled.change,
            revision: revision.to_string(),
            fingerprints,
            recorded,
            new_unbaselined,
            observed_only: true,
            seal_eligible: false,
            runtime_llm_tokens: 0,
        })
    }
}

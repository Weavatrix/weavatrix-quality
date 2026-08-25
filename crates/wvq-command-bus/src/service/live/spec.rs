//! Inherent LiveService spec, status, recovery, and evidence commands.

use super::super::access::*;
use super::super::selection_audit::audit_live_selection;
use super::LiveService;
use crate::replies::INLINE_LIMIT;

impl LiveService {
    pub(in crate::service) fn context(&self, cmd: &ContextCommand) -> Result<ContextReply, BusError> {
        validate_purpose(&cmd.purpose)?;
        let compiled = self.compiled(&cmd.change)?;
        let mut items = requirement_texts(&compiled.spec);
        items.extend(obligation_texts(&compiled.obligations));
        items.push("heuristic: 0 runtime LLM tokens on the green path".into());
        items.push("coverage: unmeasured is not uncovered".into());
        Ok(pack_context(
            &compiled.change,
            &cmd.purpose,
            cmd.token_budget,
            items,
        ))
    }

    pub(in crate::service) fn plan(&self, cmd: &PlanCommand) -> Result<PlanReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let requirements = unique_requirements(&compiled.obligations);
        let obligations = obligation_texts(&compiled.obligations);
        let risk = compiled
            .obligations
            .iter()
            .map(|item| format!("obligation {} risk {}", item.id, risk_token(item.risk)))
            .collect();
        let revision = self.revision()?;
        let store = self.store()?;
        let mut existing_proofs = Vec::new();
        let mut gaps = Vec::new();
        for obligation in &compiled.obligations {
            let proof = store
                .proof_for_obligation(&revision, &obligation.id)
                .map_err(|err| BusError::Store(err.to_string()))?;
            match proof {
                Some(proof) if proof.verdict == "PROVEN" => {
                    existing_proofs.push(proof.id.to_string());
                }
                Some(proof) => {
                    existing_proofs.push(proof.id.to_string());
                    gaps.push(format!(
                        "{}: proof verdict {}",
                        obligation.id, proof.verdict
                    ));
                }
                None => gaps.push(format!("{}: no same-revision proof", obligation.id)),
            }
        }
        Ok(PlanReply {
            change: compiled.change,
            requirements,
            obligations,
            risk,
            existing_proofs,
            gaps,
            checks: deterministic_checks(),
            executed: false,
        })
    }

    pub(in crate::service) fn audit_selection(
        &self,
        impacted_run: &str,
        full_run: &str,
    ) -> Result<SelectionAuditReply, BusError> {
        audit_live_selection(&self.repo, &self.store()?, impacted_run, full_run)
    }

    pub(in crate::service) fn status(&self, cmd: &StatusCommand) -> Result<StatusReply, BusError> {
        if let Some(run) = self.lock().clone()
            && cmd.run_id.as_ref().is_none_or(|want| want == &run.id)
        {
            return Ok(StatusReply {
                run_id: Some(run.id.clone()),
                status: run.status.clone(),
                outcome: Some(run.outcome.clone()),
                handles: run.handles.clone(),
            });
        }

        let store = self.store()?;
        let stored = match &cmd.run_id {
            Some(want) => {
                let id = RunId::new(want).map_err(|err| BusError::Identity(err.to_string()))?;
                store
                    .get_run(&id)
                    .map_err(|err| BusError::Store(err.to_string()))?
            }
            None => store
                .latest_run_any()
                .map_err(|err| BusError::Store(err.to_string()))?,
        };
        match stored {
            Some(run) => {
                let handles = store
                    .run_artifacts(&run.id)
                    .map_err(|err| BusError::Store(err.to_string()))?
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect();
                Ok(StatusReply {
                    run_id: Some(run.id.to_string()),
                    status: run.status,
                    outcome: Some(run.outcome),
                    handles,
                })
            }
            None if cmd.run_id.is_some() => Err(BusError::NotFound(format!(
                "run {}",
                cmd.run_id.as_deref().unwrap_or_default()
            ))),
            None => Ok(StatusReply {
                run_id: None,
                status: "idle".into(),
                outcome: None,
                handles: Vec::new(),
            }),
        }
    }

    pub(in crate::service) fn spec_validate(
        &self,
        cmd: &SpecCommand,
    ) -> Result<SpecValidateReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        Ok(SpecValidateReply {
            change: compiled.change,
            requirements: unique_requirements(&compiled.obligations).len() as u64,
            obligations: compiled.obligations.len() as u64,
            ok: true,
        })
    }

    pub(in crate::service) fn spec_seal(&self, cmd: &SpecCommand) -> Result<SpecSealReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let contract = load_quality_contract(&self.repo, &compiled.change)?;
        let oracle = seal(&contract, &compiled.obligations, &compiled.spec)?;
        Ok(SpecSealReply {
            change: compiled.change,
            seal_id: oracle.id.to_string(),
            digest: oracle.digest.to_string(),
            obligations: compiled.obligations.len() as u64,
        })
    }

    pub(in crate::service) fn changes(&self, cmd: &ChangesCommand) -> Result<ChangesReply, BusError> {
        let _ = cmd;
        Ok(ChangesReply {
            changes: list_changes(&self.repo)?,
        })
    }

    pub(in crate::service) fn recovery(
        &self,
        cmd: &RecoveryCommand,
    ) -> Result<RecoveryReply, BusError> {
        let desk = self.recovery_desk(&cmd.change, &cmd.base, &cmd.head)?;
        let packet = desk.packet().cloned().ok_or_else(|| {
            BusError::Intelligence("recovery producer omitted its evidence packet".into())
        })?;
        Ok(RecoveryReply {
            packet,
            review: desk.review(),
            questions: desk.questions(),
            proposed_patch: desk.preview_patch(),
            runtime_llm_tokens: 0,
        })
    }

    pub(in crate::service) fn evidence(
        &self,
        cmd: &EvidenceCommand,
    ) -> Result<EvidenceReply, BusError> {
        let id = ArtifactId::new(&cmd.handle).map_err(|err| BusError::Identity(err.to_string()))?;
        let store = self.store()?;
        let (record, bytes) = match store.read_artifact(&id) {
            Ok(value) => value,
            Err(wvq_store::StoreError::MissingBlob(_)) => {
                return Err(BusError::NotFound(format!("handle {}", cmd.handle)));
            }
            Err(err) => return Err(BusError::Store(err.to_string())),
        };
        let inline_text = if bytes.len() <= INLINE_LIMIT {
            std::str::from_utf8(&bytes).ok().map(ToOwned::to_owned)
        } else {
            None
        };
        Ok(EvidenceReply {
            handle: cmd.handle.clone(),
            kind: record.kind,
            byte_len: record.byte_len,
            content_hash: Some(record.content_hash.to_string()),
            inline_text,
        })
    }
}

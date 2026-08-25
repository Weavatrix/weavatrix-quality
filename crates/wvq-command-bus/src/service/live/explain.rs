//! Inherent LiveService explain command.

use super::super::access::*;
use super::super::verify_debt::explain_debt_finding;
use super::super::verify_reply::{explain_stored_proof, explain_ui_finding};
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn explain(&self, cmd: &ExplainCommand) -> Result<ExplainReply, BusError> {
        let store = self.store()?;
        if let Some(reply) = explain_ui_finding(&store, &cmd.id)? {
            return Ok(reply);
        }
        if let Ok(id) = ProofId::new(&cmd.id)
            && let Some(reply) = explain_stored_proof(&store, &id, &cmd.id)?
        {
            return Ok(reply);
        }
        if let Ok(id) = RunId::new(&cmd.id)
            && let Some(run) = store
                .get_run(&id)
                .map_err(|err| BusError::Store(err.to_string()))?
        {
            let handles = store
                .run_artifacts(&run.id)
                .map_err(|err| BusError::Store(err.to_string()))?;
            return Ok(ExplainReply {
                id: cmd.id.clone(),
                kind: "run".into(),
                summary: format!(
                    "run {} completed with outcome {} for change {}",
                    run.id, run.outcome, run.change_id
                ),
                provenance: std::iter::once(format!("revision {}", run.revision))
                    .chain(
                        handles
                            .into_iter()
                            .map(|handle| format!("evidence {handle}")),
                    )
                    .collect(),
            });
        }
        for change in list_changes(&self.repo)? {
            let Ok(compiled) = self.compiled(&change) else {
                continue;
            };
            if let Some(obligation) = compiled
                .obligations
                .iter()
                .find(|item| item.id.as_str() == cmd.id)
            {
                return Ok(ExplainReply {
                    id: cmd.id.clone(),
                    kind: "obligation".into(),
                    summary: format!(
                        "obligation {} ({}) for {}",
                        obligation.id,
                        obligation_kind_token(obligation.kind),
                        obligation.requirement
                    ),
                    provenance: vec![format!("openspec/changes/{}/quality.yaml", compiled.change)],
                });
            }
        }
        if self.repo.join(".git").exists() {
            for change in list_changes(&self.repo)? {
                let selection = self.select(&working_tree_selection(change));
                let Ok(selection) = selection else {
                    continue;
                };
                if let Some(index) = selection.selected.iter().position(|item| item == &cmd.id) {
                    let mut provenance = selection
                        .explanations
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(revision) = selection.revision {
                        provenance.insert(0, format!("revision {revision}"));
                    }
                    return Ok(ExplainReply {
                        id: cmd.id.clone(),
                        kind: "selection".into(),
                        summary: format!("test {} selected by {}", cmd.id, selection.algorithm),
                        provenance,
                    });
                }
            }
            let revision = self.revision()?;
            let report = self.weavatrix_operation(
                &revision,
                "run_audit",
                &json!({"base_ref": "HEAD", "debt": "all", "max_findings": 5000}),
            )?;
            if let Some(reply) = explain_debt_finding(&report, &cmd.id, &revision) {
                return Ok(reply);
            }
        }
        Err(BusError::NotFound(format!("id {}", cmd.id)))
    }
}

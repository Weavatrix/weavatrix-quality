//! Inherent LiveService verify command.

use super::super::access::*;
use super::super::impact::merge_browser_proof_evidence;
use super::super::persist_evidence::parse_obligation_execution_map;
use super::super::verify_debt::combine_verify;
use super::LiveService;

impl LiveService {
    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn verify(&self, cmd: &VerifyCommand) -> Result<VerifyReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let contract = load_quality_contract(&self.repo, &compiled.change)?;
        let mutation_policy =
            MutationPolicy::from_contract(&contract).map_err(BusError::Runtime)?;
        let oracle = seal(&contract, &compiled.obligations, &compiled.spec)?;
        let revision = self.revision()?;
        let store = self.store()?;
        let run = store
            .latest_run(&compiled.change, &revision)
            .map_err(|err| BusError::Store(err.to_string()))?;
        let artifact_ids = match &run {
            Some(run) => store
                .run_artifacts(&run.id)
                .map_err(|err| BusError::Store(err.to_string()))?,
            None => Vec::new(),
        };
        let mut present = Vec::new();
        let mut obligation_execution = BTreeMap::<String, Vec<StoredObligationExecution>>::new();
        let mut browser_evidence = BTreeMap::<String, BrowserProofEvidence>::new();
        let mut mutation_evidence = None::<MutationRunDocument>;
        for artifact in &artifact_ids {
            let (record, bytes) = store
                .read_artifact(artifact)
                .map_err(|err| BusError::Store(err.to_string()))?;
            if matches!(record.kind.as_str(), "coverage" | "lcov")
                && !present.contains(&EvidenceKind::Coverage)
            {
                present.push(EvidenceKind::Coverage);
            }
            if record.kind == "obligation-execution-map" {
                if !obligation_execution.is_empty() {
                    return Err(BusError::Store(
                        "run has more than one obligation execution map".into(),
                    ));
                }
                obligation_execution = parse_obligation_execution_map(&bytes)?;
            }
            if record.kind == "browser-program-evidence" {
                merge_browser_proof_evidence(&mut browser_evidence, &bytes)?;
            }
            if record.kind == MUTATION_RESULTS_KIND {
                if mutation_evidence.is_some() {
                    return Err(BusError::Store(
                        "run has more than one mutation-results artifact".into(),
                    ));
                }
                let document: MutationRunDocument =
                    serde_json::from_slice(&bytes).map_err(|err| {
                        BusError::Store(format!("run has malformed mutation-results: {err}"))
                    })?;
                if document.schema_v != 1 {
                    return Err(BusError::Store(format!(
                        "unknown mutation-results schema_v {}",
                        document.schema_v
                    )));
                }
                let policy = mutation_policy.as_ref().ok_or_else(|| {
                    BusError::Store(
                        "run contains mutation evidence not requested by quality.yaml".into(),
                    )
                })?;
                document.validate(policy).map_err(|error| {
                    BusError::Store(format!("run has invalid mutation-results: {error}"))
                })?;
                mutation_evidence = Some(document);
            }
        }
        let mut proofs = Vec::new();
        let mut verdicts = Vec::new();
        let mut outcomes = Vec::new();
        for obligation in &compiled.obligations {
            let proof_suffix = run.as_ref().map_or_else(
                || sha256_hex(revision.as_str().as_bytes())[..16].to_owned(),
                |run| run.id.to_string(),
            );
            let id = ProofId::new(format!("proof-{}-{proof_suffix}", obligation.id))
                .map_err(|err| BusError::Identity(err.to_string()))?;
            let browser = browser_evidence.get(obligation.id.as_str());
            let mut obligation_present = present.clone();
            if let Some(browser) = browser {
                for kind in &browser.present {
                    if !obligation_present.contains(kind) {
                        obligation_present.push(*kind);
                    }
                }
            }
            let exact = obligation_execution
                .get(obligation.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let contradicted = exact.iter().any(|entry| entry.status == "contradicted")
                || browser.is_some_and(|evidence| evidence.contradicted);
            let failed = exact.iter().any(|entry| {
                !entry.invocation_passed || matches!(entry.status.as_str(), "failed" | "error")
            }) || browser.is_some_and(|evidence| evidence.failed);
            let passed = exact
                .iter()
                .any(|entry| entry.invocation_passed && entry.status == "passed")
                || browser.is_some_and(|evidence| evidence.passed);
            let execution = if contradicted {
                ExecutionEvidence::Failed {
                    seal_contradicted: true,
                    present: obligation_present,
                }
            } else if failed {
                ExecutionEvidence::Failed {
                    seal_contradicted: false,
                    present: obligation_present,
                }
            } else if passed {
                ExecutionEvidence::Passed {
                    present: obligation_present,
                }
            } else {
                ExecutionEvidence::Absent
            };
            let assembled = assemble(AssemblyInput {
                id: id.clone(),
                requirement: obligation.requirement.clone(),
                scenario: obligation.scenario.clone(),
                obligation: obligation.id.clone(),
                oracle_seal: oracle.id.clone(),
                revision: revision.clone(),
                program: browser
                    .filter(|evidence| evidence.programs.len() == 1)
                    .and_then(|evidence| evidence.programs.first())
                    .map(ProgramId::new)
                    .transpose()
                    .map_err(|err| BusError::Identity(err.to_string()))?,
                run: run.as_ref().map(|item| item.id.clone()),
                observations: browser
                    .map(|evidence| evidence.observations.clone())
                    .unwrap_or_default(),
                artifacts: artifact_ids.clone(),
                required_evidence: obligation.required_evidence.clone(),
                execution,
                spec_ambiguous: false,
                quality_debt: Vec::new(),
                mutation: mutation_policy.as_ref().and_then(|policy| {
                    policy.summary_for(mutation_evidence.as_ref(), obligation.id.as_str())
                }),
            });
            if run.is_some()
                && store
                    .get_proof(&id)
                    .map_err(|err| BusError::Store(err.to_string()))?
                    .is_none()
            {
                store
                    .put_proof_with_artifacts(
                        &StoredProof {
                            id,
                            revision: revision.clone(),
                            obligation: obligation.id.clone(),
                            oracle_seal: oracle.id.clone(),
                            verdict: assembled.proof.verdict.as_str().into(),
                        },
                        &assembled.proof.artifacts,
                    )
                    .map_err(|err| BusError::Store(err.to_string()))?;
            }
            verdicts.push(assembled.proof.verdict);
            outcomes.push(ProofOutcome {
                obligation: obligation.id.to_string(),
                requirement: obligation.requirement.to_string(),
                verdict: assembled.proof.verdict,
                mandatory: matches!(obligation.risk, RiskLevel::High | RiskLevel::Critical),
            });
            proofs.push(ProofSummary {
                id: assembled.proof.id.to_string(),
                requirement: obligation.requirement.to_string(),
                obligation: obligation.id.to_string(),
                verdict: assembled.proof.verdict.as_str().to_owned(),
            });
        }
        let quality = compose(&self.verdict_inputs(&compiled, run.as_ref(), outcomes)?);
        Ok(combine_verify(&compiled.change, proofs, &verdicts, quality))
    }
}

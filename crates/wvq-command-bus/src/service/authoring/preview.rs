//! Candidate validation and preview evidence persistence.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use wvq_domain::ArtifactId;
use wvq_runtime::{BrowserProgramRun, ProgramOracle, TestProgram};
use wvq_spec::{load_quality_contract, seal};
use wvq_store::Store;

use super::super::{
    BusError, Compiled, remove_browser_evidence_file, safe_file_token,
};

pub(in crate::service) struct ValidatedAuthorProgram {
    pub(in crate::service) program: TestProgram,
    pub(in crate::service) oracles: Vec<ProgramOracle>,
    pub(in crate::service) seal_id: String,
}

pub(in crate::service) struct PersistedAuthorPreview {
    pub(in crate::service) observation_handles: Vec<String>,
    pub(in crate::service) screenshot_handles: Vec<String>,
    pub(in crate::service) trace_handle: Option<String>,
}

pub(in crate::service) fn validate_author_candidate(
    repo: &Path,
    compiled: &Compiled,
    candidate: &Value,
) -> Result<ValidatedAuthorProgram, BusError> {
    if !candidate.is_object() {
        return Err(BusError::InvalidInput(
            "authoring candidate must be one TestProgram JSON object".into(),
        ));
    }
    let raw = serde_json::to_string(candidate).map_err(|err| BusError::Runtime(err.to_string()))?;
    let program = TestProgram::from_json(&raw)
        .map_err(|err| BusError::InvalidInput(format!("invalid authoring candidate: {err}")))?;
    let mut unique = BTreeSet::new();
    if program
        .obligations
        .iter()
        .any(|obligation| !unique.insert(obligation.as_str()))
    {
        return Err(BusError::InvalidInput(format!(
            "authoring candidate {} repeats an obligation",
            program.id
        )));
    }
    let known = compiled
        .obligations
        .iter()
        .map(|obligation| (obligation.id.as_str(), obligation))
        .collect::<BTreeMap<_, _>>();
    let mut oracles = Vec::new();
    for obligation in &program.obligations {
        let sealed = known.get(obligation.as_str()).ok_or_else(|| {
            BusError::InvalidInput(format!(
                "authoring candidate {} names unknown obligation {obligation}",
                program.id
            ))
        })?;
        let expected = sealed.expected.as_ref().ok_or_else(|| {
            BusError::InvalidInput(format!(
                "authoring candidate {} cannot assert {obligation}: the existing seal has no executable expected predicate",
                program.id
            ))
        })?;
        oracles.push(ProgramOracle {
            obligation: obligation.clone(),
            condition: sealed
                .condition
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|err| BusError::Runtime(err.to_string()))?,
            expected: serde_json::to_value(expected)
                .map_err(|err| BusError::Runtime(err.to_string()))?,
        });
    }
    let contract = load_quality_contract(repo, &compiled.change)?;
    let oracle_seal = seal(&contract, &compiled.obligations, &compiled.spec)?;
    Ok(ValidatedAuthorProgram {
        program,
        oracles,
        seal_id: oracle_seal.id.to_string(),
    })
}

pub(in crate::service) fn author_preview_token(program: &str) -> Result<String, BusError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BusError::Runtime(format!("system clock is before Unix epoch: {err}")))?
        .as_nanos();
    Ok(format!("{}-{nanos}", safe_file_token(program)))
}

pub(in crate::service) fn persist_author_preview(
    store: &Store,
    token: &str,
    result: &BrowserProgramRun,
) -> Result<PersistedAuthorPreview, BusError> {
    let mut artifacts = Vec::<(String, String, Vec<u8>)>::new();
    let mut observation_handles = Vec::new();
    for (index, observation) in result.observations.iter().enumerate() {
        let id = format!("artifact-author-{token}-observation-{index}");
        let bytes =
            serde_json::to_vec(observation).map_err(|err| BusError::Store(err.to_string()))?;
        artifacts.push((id.clone(), "browser-observation".into(), bytes));
        observation_handles.push(id);
    }
    let mut screenshot_handles = Vec::new();
    for (index, path) in result.screenshot_paths.iter().enumerate() {
        let bytes = std::fs::read(path).map_err(|err| {
            BusError::Runtime(format!(
                "cannot import authoring screenshot {}: {err}",
                path.display()
            ))
        })?;
        remove_browser_evidence_file(path)?;
        let id = format!("artifact-author-{token}-screenshot-{index}");
        artifacts.push((id.clone(), "screenshot".into(), bytes));
        screenshot_handles.push(id);
    }
    let trace_handle = if let Some(path) = &result.trace_path {
        let bytes = std::fs::read(path).map_err(|err| {
            BusError::Runtime(format!(
                "cannot import authoring trace {}: {err}",
                path.display()
            ))
        })?;
        remove_browser_evidence_file(path)?;
        let id = format!("artifact-author-{token}-trace");
        artifacts.push((id.clone(), "playwright-trace".into(), bytes));
        Some(id)
    } else {
        None
    };
    for (raw_id, kind, bytes) in artifacts {
        let id = ArtifactId::new(&raw_id).map_err(|err| BusError::Identity(err.to_string()))?;
        store
            .put_artifact(&id, &kind, &bytes)
            .map_err(|err| BusError::Store(err.to_string()))?;
    }
    Ok(PersistedAuthorPreview {
        observation_handles,
        screenshot_handles,
        trace_handle,
    })
}

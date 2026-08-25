//! Compiled OpenSpec change and oracle identity.

use std::path::Path;

use serde::{Deserialize, Serialize};
use wvq_spec::{OpenSpecChange, SpecError, TestObligation, compile_obligations, load_quality_contract, read_change, seal};

use super::super::{BusError, resolve_change};

pub(in crate::service) struct Compiled {
    pub(in crate::service) change: String,
    pub(in crate::service) spec: OpenSpecChange,
    pub(in crate::service) obligations: Vec<TestObligation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::service) struct OracleIdentity {
    pub(in crate::service) id: String,
    pub(in crate::service) digest: String,
}

/// Immutable proposal bytes stored in CAS before a human decision exists.
/// Approval state is deliberately not part of this document or its digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::service) struct OracleReplacementDocument {
    pub(in crate::service) schema_v: u32,
    pub(in crate::service) change: String,
    pub(in crate::service) base_revision: String,
    pub(in crate::service) head_revision: String,
    pub(in crate::service) head_content_revision: String,
    pub(in crate::service) merge_base: String,
    pub(in crate::service) base_seal: String,
    pub(in crate::service) base_seal_digest: String,
    pub(in crate::service) head_seal: String,
    pub(in crate::service) head_seal_digest: String,
    pub(in crate::service) changed_obligations: Vec<String>,
    pub(in crate::service) obligation_replacements: Vec<(String, String)>,
}

pub(in crate::service) fn compile_repository(
    repo: &Path,
    change: &str,
) -> Result<Compiled, BusError> {
    let change = resolve_change(repo, change)?;
    let spec = read_change(repo, &change)?;
    let contract = load_quality_contract(repo, &change)?;
    let obligations = compile_obligations(&contract, &spec)?;
    Ok(Compiled {
        change,
        spec,
        obligations,
    })
}

pub(in crate::service) fn optional_change(
    repo: &Path,
    change: &str,
) -> Result<Option<OpenSpecChange>, BusError> {
    match read_change(repo, change) {
        Ok(change) => Ok(Some(change)),
        Err(SpecError::ChangeNotFound(_) | SpecError::NoDeltaSpecs(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(in crate::service) fn oracle_identity(
    repo: &Path,
    compiled: &Compiled,
) -> Result<OracleIdentity, BusError> {
    let contract = load_quality_contract(repo, &compiled.change)?;
    let oracle = seal(&contract, &compiled.obligations, &compiled.spec)?;
    Ok(OracleIdentity {
        id: oracle.id.to_string(),
        digest: oracle.digest.to_string(),
    })
}

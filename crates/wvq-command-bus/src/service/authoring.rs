//! Authoring helpers. Preview/heal stay behind an explicit command.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use wvq_domain::ArtifactId;
use wvq_runtime::{BrowserProgramRun, ProgramOracle, TestProgram};
use wvq_spec::{
    EvidenceKind, ObligationKind, OpenSpecChange, RequirementOp, RiskLevel, TestObligation,
    load_quality_contract, seal,
};
use wvq_store::{Store, StoreError};

use super::{
    BusError, Compiled, graph_node_id, remove_browser_evidence_file, safe_file_token, values_at,
};
use crate::commands::SelectCommand;
use crate::replies::{
    AuthorDraftReply, AuthoringObligation, ContextReply, DebtReply, bound_items, estimate_tokens,
};

pub(super) struct ValidatedAuthorProgram {
    pub(super) program: TestProgram,
    pub(super) oracles: Vec<ProgramOracle>,
    pub(super) seal_id: String,
}

pub(super) fn map_authoring_store_error(err: StoreError) -> BusError {
    match err {
        StoreError::Invalid(message) => BusError::InvalidInput(message),
        other => BusError::Store(other.to_string()),
    }
}

pub(super) fn validate_authoring_budget(budget: u64) -> Result<(), BusError> {
    if (256..=64_000).contains(&budget) {
        Ok(())
    } else {
        Err(BusError::Unknown {
            field: "token_budget",
            value: budget.to_string(),
        })
    }
}

pub(super) fn authoring_obligations(
    obligations: &[TestObligation],
) -> Result<Vec<AuthoringObligation>, BusError> {
    obligations
        .iter()
        .map(|item| {
            Ok(AuthoringObligation {
                id: item.id.to_string(),
                requirement: item.requirement.to_string(),
                scenario: item.scenario.to_string(),
                kind: obligation_kind_token(item.kind).into(),
                risk: risk_token(item.risk).into(),
                condition: item
                    .condition
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                expected: item
                    .expected
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
                required_evidence: item
                    .required_evidence
                    .iter()
                    .map(|kind| evidence_kind_token(*kind).to_owned())
                    .collect(),
            })
        })
        .collect()
}

pub(super) fn authoring_authority_tokens(
    changed_files: &[String],
    obligations: &[AuthoringObligation],
) -> Result<u64, BusError> {
    let authority = serde_json::to_string(&json!({
        "changed_files": changed_files,
        "obligations": obligations,
    }))
    .map_err(|err| BusError::Runtime(err.to_string()))?;
    Ok(estimate_tokens(&authority).max(1))
}

pub(super) fn authoring_context(
    spec: &OpenSpecChange,
    changed_files: &[String],
    diff: &Value,
    impact: &Value,
) -> Vec<String> {
    let mut out = detailed_requirement_texts(spec);
    out.extend(
        changed_files
            .iter()
            .map(|path| format!("changed file: {path}")),
    );
    for (label, pointer) in [
        ("graph added", "/nodes/added"),
        ("graph removed", "/nodes/removed"),
    ] {
        out.extend(
            values_at(diff, pointer)
                .iter()
                .filter_map(graph_node_id)
                .map(|id| format!("{label}: {id}")),
        );
    }
    for item in values_at(diff, "/nodes/changed") {
        if let Some(id) = item.get("before").and_then(graph_node_id) {
            out.push(format!("graph changed base: {id}"));
        }
        if let Some(id) = item.get("after").and_then(graph_node_id) {
            out.push(format!("graph changed head: {id}"));
        }
    }
    out.extend(
        values_at(impact, "/impacted_nodes")
            .iter()
            .filter_map(graph_node_id)
            .map(|id| format!("graph impacted: {id}")),
    );
    out.sort();
    out.dedup();
    out
}

fn detailed_requirement_texts(spec: &OpenSpecChange) -> Vec<String> {
    let mut out = Vec::new();
    for capability in &spec.capabilities {
        for operation in &capability.operations {
            let delta = match operation {
                RequirementOp::Added(delta)
                | RequirementOp::Modified(delta)
                | RequirementOp::Removed(delta) => delta,
                RequirementOp::Renamed { from, to, location } => {
                    out.push(format!(
                        "intent rename at {}:{}: {from} -> {to}",
                        location.file.display(),
                        location.line
                    ));
                    continue;
                }
            };
            out.push(format!(
                "intent requirement {}: {} — {}",
                delta.id, delta.name, delta.text
            ));
            for scenario in &delta.scenarios {
                let clauses = scenario
                    .clauses
                    .iter()
                    .map(|clause| format!("{:?} {}", clause.kind, clause.text))
                    .collect::<Vec<_>>()
                    .join("; ");
                out.push(format!(
                    "intent scenario {} ({}) for {}: {clauses}",
                    scenario.id, scenario.name, delta.id
                ));
            }
        }
    }
    out
}

pub(super) fn authoring_model_prompt(reply: &AuthorDraftReply) -> Result<String, BusError> {
    let input = serde_json::to_value(reply).map_err(|err| BusError::Runtime(err.to_string()))?;
    serde_json::to_string(&json!({
        "task": "Return exactly one JSON object containing a canonical schema_v=1 TestProgram. Do not use markdown.",
        "rules": [
            "source must be generated",
            "only assert obligation ids whose expected field is non-null",
            "every declared obligation must have an assert step",
            "prefer semantic targets: test_id, role plus accessible_name, or label",
            "routes and api operation paths must be same-origin root-relative",
            "never invent an oracle, expected predicate, shell command, XPath, JavaScript, or filesystem write",
            "use only navigate, activate, fill, select, press, wait, set_feature_flag, inject_fault, api_call, assert"
        ],
        "test_program_shape": {
            "schema_v": 1,
            "id": "generated-program-id",
            "source": "generated",
            "obligations": ["sealed-obligation-id"],
            "preconditions": [],
            "steps": [{"action": "navigate", "route": "/"}, {"action": "assert", "obligation": "sealed-obligation-id"}],
            "data": {},
            "faults": {},
            "api_operations": {},
            "evidence_policy": {"screenshot": "on_failure", "trace": "on_failure", "network": "always", "console": "always", "storage": "on_failure"},
            "deterministic_seed": 1
        },
        "authoring_packet": input
    }))
    .map_err(|err| BusError::Runtime(err.to_string()))
}

pub(super) fn validate_author_candidate(
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

pub(super) fn author_preview_token(program: &str) -> Result<String, BusError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| BusError::Runtime(format!("system clock is before Unix epoch: {err}")))?
        .as_nanos();
    Ok(format!("{}-{nanos}", safe_file_token(program)))
}

pub(super) struct PersistedAuthorPreview {
    pub(super) observation_handles: Vec<String>,
    pub(super) screenshot_handles: Vec<String>,
    pub(super) trace_handle: Option<String>,
}

pub(super) fn persist_author_preview(
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

pub(super) fn obligation_kind_token(kind: ObligationKind) -> &'static str {
    match kind {
        ObligationKind::Behavioral => "behavioral",
        ObligationKind::Invariant => "invariant",
        ObligationKind::Api => "api",
        ObligationKind::Contract => "contract",
        ObligationKind::Permission => "permission",
        ObligationKind::Accessibility => "accessibility",
        ObligationKind::Visual => "visual",
        ObligationKind::Performance => "performance",
        ObligationKind::Architecture => "architecture",
        ObligationKind::CodeHealth => "code_health",
        ObligationKind::Coverage => "coverage",
        ObligationKind::Mutation => "mutation",
        ObligationKind::Metamorphic => "metamorphic",
        ObligationKind::SecurityPolicy => "security_policy",
    }
}

pub(super) fn evidence_kind_token(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Dom => "dom",
        EvidenceKind::Network => "network",
        EvidenceKind::Screenshot => "screenshot",
        EvidenceKind::Trace => "trace",
        EvidenceKind::Har => "har",
        EvidenceKind::Console => "console",
        EvidenceKind::Storage => "storage",
        EvidenceKind::Coverage => "coverage",
    }
}

pub(super) fn pack_context(change: &str, purpose: &str, budget: u64, items: Vec<String>) -> ContextReply {
    let (kept, used, truncated) = bound_items(items, budget.max(1));
    let mut requirements = Vec::new();
    let mut obligations = Vec::new();
    let mut heuristics = Vec::new();
    let mut coverage = Vec::new();
    for item in kept {
        if item.starts_with("obligation") {
            obligations.push(item);
        } else if item.starts_with("heuristic") {
            heuristics.push(item);
        } else if item.starts_with("coverage") {
            coverage.push(item);
        } else {
            requirements.push(item);
        }
    }
    ContextReply {
        change: change.to_owned(),
        purpose: purpose.to_owned(),
        requirements,
        obligations,
        heuristics,
        coverage,
        truncated,
        tokens_used: used,
        token_budget: budget.max(1),
    }
}

pub(super) fn requirement_texts(spec: &OpenSpecChange) -> Vec<String> {
    let mut out = Vec::new();
    for capability in &spec.capabilities {
        for operation in &capability.operations {
            let delta = match operation {
                RequirementOp::Added(delta)
                | RequirementOp::Modified(delta)
                | RequirementOp::Removed(delta) => delta,
                RequirementOp::Renamed { from, to, location } => {
                    out.push(format!(
                        "requirement rename {from} → {to} ({}:{})",
                        location.file.display(),
                        location.line
                    ));
                    continue;
                }
            };
            out.push(format!(
                "requirement {} at {}:{}: {}",
                delta.id,
                delta.location.file.display(),
                delta.location.line,
                delta.name
            ));
        }
    }
    out
}

pub(super) fn obligation_texts(obligations: &[TestObligation]) -> Vec<String> {
    obligations
        .iter()
        .map(|item| {
            format!(
                "obligation {} {} risk {}",
                item.id,
                obligation_kind_token(item.kind),
                risk_token(item.risk)
            )
        })
        .collect()
}

pub(super) fn unique_requirements(obligations: &[TestObligation]) -> Vec<String> {
    let mut out = Vec::new();
    for item in obligations {
        let id = item.requirement.to_string();
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

pub(super) fn risk_token(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

pub(super) fn deterministic_checks() -> Vec<String> {
    vec![
        "architecture".into(),
        "size".into(),
        "dead_code".into(),
        "clones".into(),
        "topology".into(),
        "api".into(),
        "history".into(),
        "coverage".into(),
    ]
}

pub(super) fn empty_debt(base: &str, head: &str) -> DebtReply {
    DebtReply {
        base: base.to_owned(),
        head: head.to_owned(),
        revision: None,
        comparison_present: false,
        existing: 0,
        new: 0,
        fixed: 0,
        returned: 0,
        excepted: 0,
        findings: Vec::new(),
        limitations: Vec::new(),
    }
}

pub(super) fn working_tree_selection(change: String) -> SelectCommand {
    SelectCommand {
        change,
        base: "HEAD".into(),
        head: "WORKTREE".into(),
    }
}


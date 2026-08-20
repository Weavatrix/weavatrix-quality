//! Exception-first Quality Studio API. Spec §31 and §58.
//!
//! Every endpoint is a projection of the shared command bus. The dashboard shows
//! unresolved exceptions, never hundreds of green cases, and a human decision is
//! always one reviewer against one subject.

use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use wvq_command_bus::{
    AuthorDraftCommand, AuthorPreviewCommand, AuthorValidateCommand, BusError, ChangesCommand,
    DebtCommand, DebtReply, EvidenceCommand, ExplainCommand, ProofSummary, QualityService,
    StatusCommand, VerifyCommand,
};
use wvq_domain::{
    ContentHash, HumanDecision, HumanDecisionId, HumanRole, NewDecision, VerificationDecision,
};
use wvq_proof::ProtectionView;
use wvq_spec_recovery::RecoveryDesk;
use wvq_store::{Store, StoredAiUsage};

use crate::http::{HttpRequest, HttpResponse};

/// One Studio endpoint and the single method it accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route<'a> {
    Changes,
    Summary(&'a str),
    RequirementProofs(&'a str),
    Finding(&'a str),
    Run(&'a str),
    Artifact(&'a str),
    HumanDecisions,
    RecoveryReview,
    RecoveryQuestions,
    RecoveryPatch,
    RecoveryDecisions,
    Protection,
    ProtectionTest(&'a str),
    ProtectionFlow(&'a str),
    AuthorDraft,
    AuthorValidate,
    AuthorPreview,
}

impl Route<'_> {
    fn method(self) -> &'static str {
        match self {
            Self::HumanDecisions
            | Self::RecoveryDecisions
            | Self::AuthorDraft
            | Self::AuthorValidate
            | Self::AuthorPreview => "POST",
            _ => "GET",
        }
    }
}

/// AI budget consumption for one change. Absent when nothing was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AiBody {
    planning_tokens: u64,
    runtime_tokens: u64,
    browser_escape_calls: u64,
    vision_calls: u64,
    cost_micros: u64,
}

impl From<StoredAiUsage> for AiBody {
    fn from(usage: StoredAiUsage) -> Self {
        Self {
            planning_tokens: usage.planning_tokens,
            runtime_tokens: usage.runtime_tokens,
            browser_escape_calls: usage.browser_escape_calls,
            vision_calls: usage.vision_calls,
            cost_micros: usage.cost_micros,
        }
    }
}

/// Change dashboard. Passing proofs are counted, never listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SummaryBody {
    change: String,
    verdict: String,
    blocking: bool,
    requirements: usize,
    obligations: usize,
    proven: usize,
    /// Only unresolved proofs reach the dashboard.
    needs_attention: Vec<ProofSummary>,
    /// How many green proofs were deliberately hidden.
    suppressed_passing: usize,
    debt: DebtReply,
    /// `null` when no AI usage was recorded. Unmeasured is not zero.
    ai: Option<AiBody>,
}

/// Requirement drill-down. Detail screens may show green proofs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RequirementProofsBody {
    requirement: String,
    change: String,
    proofs: Vec<ProofSummary>,
}

/// Incoming human decision. Unknown fields are refused, so a body carrying
/// `accept_all` or a `subjects` list never reaches the domain.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionRequest {
    id: String,
    reviewer: String,
    role: HumanRole,
    subject: String,
    artifact_digest: String,
    decision: VerificationDecision,
    #[serde(default)]
    comment: Option<String>,
    decided_at: String,
}

/// Proposed `OpenSpec` patch text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RecoveryPatchBody {
    patch: String,
}

/// Where one candidate now stands on the mandatory verification path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RecoveryStateBody {
    candidate: String,
    state: &'static str,
}

/// Stored decision plus what it permits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DecisionBody {
    #[serde(flatten)]
    decision: HumanDecision,
    /// Whether this decision may carry the subject towards a seal.
    seal_eligible: bool,
    /// Whether it blocks sealing pending someone else's answer.
    escalates: bool,
}

/// Quality Studio over the shared command bus.
pub struct Studio {
    service: Arc<dyn QualityService>,
    store: Mutex<Store>,
    recovery: Option<Arc<Mutex<RecoveryDesk>>>,
    protection: Option<Arc<Mutex<ProtectionView>>>,
}

impl Studio {
    /// Serve `service`, recording decisions and reading AI usage from `store`.
    #[must_use]
    pub fn new(service: Arc<dyn QualityService>, store: Store) -> Self {
        Self {
            service,
            store: Mutex::new(store),
            recovery: None,
            protection: None,
        }
    }

    /// Attach a computed protection view, enabling the continuity screens.
    #[must_use]
    pub fn with_protection(mut self, view: Arc<Mutex<ProtectionView>>) -> Self {
        self.protection = Some(view);
        self
    }

    /// Enable the spec-recovery screens over a desk the host has populated.
    ///
    /// Recovery is opt-in: a repository with complete `OpenSpec` never needs it.
    #[must_use]
    pub fn with_recovery(mut self, desk: Arc<Mutex<RecoveryDesk>>) -> Self {
        self.recovery = Some(desk);
        self
    }

    /// Route and answer one request.
    #[must_use]
    pub fn handle(&self, request: &HttpRequest) -> HttpResponse {
        let path = request.path.trim_matches('/').to_owned();
        let Some(route) = parse_route(&path) else {
            return HttpResponse::error(404, "unknown route");
        };
        if request.method != route.method() {
            return HttpResponse::error(405, "method not allowed for this route");
        }
        match route {
            Route::Changes => self.changes(),
            Route::Summary(change) => self.summary(change),
            Route::RequirementProofs(requirement) => self.requirement_proofs(requirement),
            Route::Finding(id) => self.finding(id),
            Route::Run(id) => self.run(id),
            Route::Artifact(id) => self.artifact(id),
            Route::HumanDecisions => self.record_decision(&request.body),
            Route::RecoveryReview => self.with_desk(|desk| ok(&desk.review())),
            Route::RecoveryQuestions => self.with_desk(|desk| ok(&desk.questions())),
            Route::RecoveryPatch => self.with_desk(|desk| {
                ok(&RecoveryPatchBody {
                    patch: desk.preview_patch(),
                })
            }),
            Route::RecoveryDecisions => self.record_recovery_decision(&request.body),
            Route::Protection => self.read_protection(|view| ok(&view.report())),
            Route::ProtectionTest(test) => {
                self.read_protection(|view| match view.lineage_of(test) {
                    Some(record) => ok(record),
                    None => HttpResponse::error(404, "no lineage recorded for that test"),
                })
            }
            Route::ProtectionFlow(flow) => self.read_protection(|view| match view.flow(flow) {
                Some(record) => ok(record),
                None => HttpResponse::error(404, "that flow is not in the impacted surface"),
            }),
            Route::AuthorDraft => self.author_draft(&request.body),
            Route::AuthorValidate => self.author_validate(&request.body),
            Route::AuthorPreview => self.author_preview(&request.body),
        }
    }

    /// Run `body` against the protection view, or answer 404 when it is off.
    fn read_protection<F>(&self, body: F) -> HttpResponse
    where
        F: FnOnce(&ProtectionView) -> HttpResponse,
    {
        let Some(view) = &self.protection else {
            return HttpResponse::error(404, "protection continuity has not been computed");
        };
        let guard = view
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        body(&guard)
    }

    /// Run `body` against the recovery desk, or answer 404 when it is off.
    fn with_desk<F>(&self, body: F) -> HttpResponse
    where
        F: FnOnce(&RecoveryDesk) -> HttpResponse,
    {
        let Some(desk) = &self.recovery else {
            return HttpResponse::error(404, "spec recovery is not enabled for this repository");
        };
        let guard = desk
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        body(&guard)
    }

    fn record_recovery_decision(&self, body: &str) -> HttpResponse {
        let Some(desk) = &self.recovery else {
            return HttpResponse::error(404, "spec recovery is not enabled for this repository");
        };
        let decision = match parse_decision(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let mut guard = desk
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match guard.decide(&decision) {
            Ok(state) => ok(&RecoveryStateBody {
                candidate: decision.subject,
                state: state.as_str(),
            }),
            Err(err) => HttpResponse::error(422, &err.to_string()),
        }
    }

    fn changes(&self) -> HttpResponse {
        match self.service.changes(&ChangesCommand::default()) {
            Ok(reply) => ok(&reply),
            Err(err) => bus_error(&err),
        }
    }

    fn summary(&self, change: &str) -> HttpResponse {
        let verify = match self.service.verify(&VerifyCommand {
            change: change.to_owned(),
        }) {
            Ok(reply) => reply,
            Err(err) => return bus_error(&err),
        };
        let debt = match self.service.debt(&DebtCommand {
            change: change.to_owned(),
            base: "HEAD".into(),
            head: "WORKTREE".into(),
        }) {
            Ok(reply) => reply,
            Err(err) => return bus_error(&err),
        };
        let ai = match self.lock_store().ai_usage_for_change(&verify.change) {
            Ok(usage) => usage.map(AiBody::from),
            Err(err) => return HttpResponse::error(500, &err.to_string()),
        };
        let mut requirements: Vec<&str> = verify
            .proofs
            .iter()
            .map(|proof| proof.requirement.as_str())
            .collect();
        requirements.sort_unstable();
        requirements.dedup();
        let requirement_count = requirements.len();
        let obligations = verify.proofs.len();
        let (passing, needs_attention): (Vec<ProofSummary>, Vec<ProofSummary>) = verify
            .proofs
            .into_iter()
            .partition(ProofSummary::is_passing);
        ok(&SummaryBody {
            change: verify.change,
            verdict: verify.verdict,
            blocking: verify.blocking,
            requirements: requirement_count,
            obligations,
            proven: passing.len(),
            needs_attention,
            suppressed_passing: passing.len(),
            debt,
            ai,
        })
    }

    fn requirement_proofs(&self, requirement: &str) -> HttpResponse {
        let changes = match self.service.changes(&ChangesCommand::default()) {
            Ok(reply) => reply.changes,
            Err(err) => return bus_error(&err),
        };
        for change in changes {
            let Ok(verify) = self.service.verify(&VerifyCommand {
                change: change.clone(),
            }) else {
                continue;
            };
            let proofs: Vec<ProofSummary> = verify
                .proofs
                .into_iter()
                .filter(|proof| proof.requirement == requirement)
                .collect();
            if !proofs.is_empty() {
                return ok(&RequirementProofsBody {
                    requirement: requirement.to_owned(),
                    change: verify.change,
                    proofs,
                });
            }
        }
        HttpResponse::error(404, "no proofs for that requirement")
    }

    fn finding(&self, id: &str) -> HttpResponse {
        match self.service.explain(&ExplainCommand { id: id.to_owned() }) {
            Ok(reply) => ok(&reply),
            Err(err) => bus_error(&err),
        }
    }

    fn run(&self, id: &str) -> HttpResponse {
        match self.service.status(&StatusCommand {
            run_id: Some(id.to_owned()),
        }) {
            Ok(reply) => ok(&reply),
            Err(err) => bus_error(&err),
        }
    }

    fn artifact(&self, id: &str) -> HttpResponse {
        match self.service.evidence(&EvidenceCommand {
            handle: id.to_owned(),
        }) {
            Ok(reply) => ok(&reply),
            Err(err) => bus_error(&err),
        }
    }

    fn author_draft(&self, body: &str) -> HttpResponse {
        let command: AuthorDraftCommand = match parse_json_body(body) {
            Ok(command) => command,
            Err(response) => return response,
        };
        match self.service.author_draft(&command) {
            Ok(reply) => ok(&reply),
            Err(err) => bus_error(&err),
        }
    }

    fn author_validate(&self, body: &str) -> HttpResponse {
        let command: AuthorValidateCommand = match parse_json_body(body) {
            Ok(command) => command,
            Err(response) => return response,
        };
        match self.service.author_validate(&command) {
            Ok(reply) => ok(&reply),
            Err(err) => bus_error(&err),
        }
    }

    fn author_preview(&self, body: &str) -> HttpResponse {
        let command: AuthorPreviewCommand = match parse_json_body(body) {
            Ok(command) => command,
            Err(response) => return response,
        };
        match self.service.author_preview(&command) {
            Ok(reply) => ok(&reply),
            Err(err) => bus_error(&err),
        }
    }

    fn record_decision(&self, body: &str) -> HttpResponse {
        let decision = match parse_decision(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        if let Err(err) = self.lock_store().put_human_decision(&decision) {
            return HttpResponse::error(500, &err.to_string());
        }
        let body = DecisionBody {
            seal_eligible: decision.decision.seal_eligible(),
            escalates: decision.decision.escalates(),
            decision,
        };
        match serde_json::to_string(&body) {
            Ok(json) => HttpResponse::new(201, json),
            Err(err) => HttpResponse::error(500, &err.to_string()),
        }
    }

    fn lock_store(&self) -> MutexGuard<'_, Store> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Parse one decision body, refusing bulk fields and malformed identities.
fn parse_decision(body: &str) -> Result<HumanDecision, HttpResponse> {
    let request: DecisionRequest =
        serde_json::from_str(body).map_err(|err| HttpResponse::error(400, &err.to_string()))?;
    let id = HumanDecisionId::new(&request.id)
        .map_err(|_| HttpResponse::error(400, "decision id must be a non-empty token"))?;
    let digest = ContentHash::new(&request.artifact_digest)
        .map_err(|_| HttpResponse::error(400, "artifact_digest must be lowercase hex"))?;
    HumanDecision::new(NewDecision {
        id,
        reviewer: request.reviewer,
        role: request.role,
        subject: request.subject,
        artifact_digest: digest,
        decision: request.decision,
        comment: request.comment,
        decided_at: request.decided_at,
    })
    .map_err(|err| HttpResponse::error(422, &err.to_string()))
}

fn parse_json_body<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, HttpResponse> {
    serde_json::from_str(body).map_err(|err| HttpResponse::error(400, &err.to_string()))
}

fn parse_route(path: &str) -> Option<Route<'_>> {
    let segments: Vec<&str> = path.split('/').collect();
    match segments.as_slice() {
        ["api", "v1", "recovery", "review"] => Some(Route::RecoveryReview),
        ["api", "v1", "recovery", "questions"] => Some(Route::RecoveryQuestions),
        ["api", "v1", "recovery", "patch"] => Some(Route::RecoveryPatch),
        ["api", "v1", "recovery", "decisions"] => Some(Route::RecoveryDecisions),
        ["api", "v1", "protection"] => Some(Route::Protection),
        ["api", "v1", "protection", "tests", test] => Some(Route::ProtectionTest(test)),
        ["api", "v1", "protection", "flows", flow] => Some(Route::ProtectionFlow(flow)),
        ["api", "v1", "changes"] => Some(Route::Changes),
        ["api", "v1", "changes", change, "summary"] => Some(Route::Summary(change)),
        ["api", "v1", "requirements", requirement, "proofs"] => {
            Some(Route::RequirementProofs(requirement))
        }
        ["api", "v1", "findings", id] => Some(Route::Finding(id)),
        ["api", "v1", "runs", id] => Some(Route::Run(id)),
        ["api", "v1", "artifacts", id] => Some(Route::Artifact(id)),
        ["api", "v1", "human-decisions"] => Some(Route::HumanDecisions),
        ["api", "v1", "authoring", "draft"] => Some(Route::AuthorDraft),
        ["api", "v1", "authoring", "validate"] => Some(Route::AuthorValidate),
        ["api", "v1", "authoring", "preview"] => Some(Route::AuthorPreview),
        _ => None,
    }
}

fn ok<T: Serialize>(body: &T) -> HttpResponse {
    match serde_json::to_string(body) {
        Ok(json) => HttpResponse::new(200, json),
        Err(err) => HttpResponse::error(500, &err.to_string()),
    }
}

fn bus_error(err: &BusError) -> HttpResponse {
    let status = match err {
        BusError::NotFound(_) => 404,
        BusError::Ambiguous(_) => 409,
        BusError::Unknown { .. } | BusError::Identity(_) | BusError::InvalidInput(_) => 400,
        BusError::Spec(_) => 422,
        BusError::Runtime(_)
        | BusError::Intelligence(_)
        | BusError::Store(_)
        | BusError::Model(_) => 503,
    };
    HttpResponse::error(status, &err.to_string())
}

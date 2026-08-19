//! Exception-first Quality Studio API. Spec §31 and §58.
//!
//! Every endpoint is a projection of the shared command bus. The dashboard shows
//! unresolved exceptions, never hundreds of green cases, and a human decision is
//! always one reviewer against one subject.

use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use wvq_command_bus::{
    BusError, ChangesCommand, DebtCommand, DebtReply, EvidenceCommand, ExplainCommand,
    ProofSummary, QualityService, StatusCommand, VerifyCommand,
};
use wvq_domain::{
    ContentHash, HumanDecision, HumanDecisionId, HumanRole, NewDecision, VerificationDecision,
};
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
}

impl Route<'_> {
    fn method(self) -> &'static str {
        match self {
            Self::HumanDecisions => "POST",
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
}

impl Studio {
    /// Serve `service`, recording decisions and reading AI usage from `store`.
    #[must_use]
    pub fn new(service: Arc<dyn QualityService>, store: Store) -> Self {
        Self {
            service,
            store: Mutex::new(store),
        }
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

    fn record_decision(&self, body: &str) -> HttpResponse {
        let request: DecisionRequest = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(err) => return HttpResponse::error(400, &err.to_string()),
        };
        let Ok(id) = HumanDecisionId::new(&request.id) else {
            return HttpResponse::error(400, "decision id must be a non-empty token");
        };
        let Ok(digest) = ContentHash::new(&request.artifact_digest) else {
            return HttpResponse::error(400, "artifact_digest must be lowercase hex");
        };
        let decision = match HumanDecision::new(NewDecision {
            id,
            reviewer: request.reviewer,
            role: request.role,
            subject: request.subject,
            artifact_digest: digest,
            decision: request.decision,
            comment: request.comment,
            decided_at: request.decided_at,
        }) {
            Ok(value) => value,
            Err(err) => return HttpResponse::error(422, &err.to_string()),
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

fn parse_route(path: &str) -> Option<Route<'_>> {
    let segments: Vec<&str> = path.split('/').collect();
    match segments.as_slice() {
        ["api", "v1", "changes"] => Some(Route::Changes),
        ["api", "v1", "changes", change, "summary"] => Some(Route::Summary(change)),
        ["api", "v1", "requirements", requirement, "proofs"] => {
            Some(Route::RequirementProofs(requirement))
        }
        ["api", "v1", "findings", id] => Some(Route::Finding(id)),
        ["api", "v1", "runs", id] => Some(Route::Run(id)),
        ["api", "v1", "artifacts", id] => Some(Route::Artifact(id)),
        ["api", "v1", "human-decisions"] => Some(Route::HumanDecisions),
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
        BusError::Unknown { .. } | BusError::Identity(_) => 400,
        BusError::Spec(_) => 422,
    };
    HttpResponse::error(status, &err.to_string())
}

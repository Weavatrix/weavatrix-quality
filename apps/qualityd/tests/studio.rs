//! Task 23: exception-first Studio API, provenance-bearing decisions, no accept-all.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use qualityd::{HttpRequest, HttpResponse, Studio, serve};
use serde_json::Value;
use wvq_command_bus::{
    ApplicationSurfaceKind, EvidenceCell, EvidenceColumn, EvidenceNeed, EvidencePlan,
    EvidenceProducer, FakeService, ProducerOffer, ProofSummary, QualityService, SurfaceEvidenceRow,
};
use wvq_proof::{FlowView, ProtectionView, TestLineageView};
use wvq_spec_recovery::RecoveryDesk;
use wvq_store::Store;

static COUNTER: AtomicU32 = AtomicU32::new(0);

const DIGEST: &str = "abababababababababababababababababababababababababababababababab";

fn temp_store() -> Store {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("wvq-studio-{nanos}-{seq}"));
    std::fs::create_dir_all(&root).expect("temp dir");
    Store::open(&root).expect("open store")
}

fn studio_with(fake: &Arc<FakeService>) -> Studio {
    let service: Arc<dyn QualityService> = Arc::clone(fake) as Arc<dyn QualityService>;
    Studio::new(service, temp_store())
}

fn default_studio() -> Studio {
    studio_with(&Arc::new(FakeService::default()))
}

fn get(studio: &Studio, path: &str) -> HttpResponse {
    studio.handle(&HttpRequest {
        method: "GET".into(),
        path: path.into(),
        body: String::new(),
    })
}

fn post(studio: &Studio, path: &str, body: &str) -> HttpResponse {
    studio.handle(&HttpRequest {
        method: "POST".into(),
        path: path.into(),
        body: body.into(),
    })
}

fn json(response: &HttpResponse) -> Value {
    serde_json::from_str(&response.body).expect("response body is json")
}

fn proof(id: &str, requirement: &str, obligation: &str, verdict: &str) -> ProofSummary {
    ProofSummary {
        id: id.into(),
        requirement: requirement.into(),
        obligation: obligation.into(),
        verdict: verdict.into(),
    }
}

fn decision_body(id: &str, subject: &str, decision: &str) -> String {
    serde_json::json!({
        "id": id,
        "reviewer": "sergii",
        "role": "qa",
        "subject": subject,
        "artifact_digest": DIGEST,
        "decision": decision,
        "decided_at": "2026-08-20T09:00:00Z"
    })
    .to_string()
}

#[test]
fn changes_are_listed() {
    let response = get(&default_studio(), "/api/v1/changes");
    assert_eq!(response.status, 200);
    assert_eq!(json(&response)["changes"][0], "sankey-others");
}

#[test]
fn authoring_http_projects_the_complete_browser_program_lifecycle() {
    let studio = default_studio();
    let draft = post(
        &studio,
        "/api/v1/authoring/draft",
        r#"{"change":"live","base":"BASE","head":"HEAD","token_budget":8000}"#,
    );
    assert_eq!(draft.status, 200, "{}", draft.body);
    let draft = json(&draft);
    assert_eq!(draft["change"], "live");
    assert_eq!(draft["candidate"], Value::Null);
    assert_eq!(draft["obligations"][0]["id"], "others-visible");

    let program = serde_json::json!({"id": "generated-http-program"});
    let validated = post(
        &studio,
        "/api/v1/authoring/validate",
        &serde_json::json!({"change": "live", "program": program}).to_string(),
    );
    assert_eq!(validated.status, 200, "{}", validated.body);
    assert_eq!(json(&validated)["persisted"], false);

    let preview = post(
        &studio,
        "/api/v1/authoring/preview",
        &serde_json::json!({
            "change": "live",
            "program": {"id": "generated-http-program"},
            "screenshot": true,
            "trace": true
        })
        .to_string(),
    );
    assert_eq!(preview.status, 200, "{}", preview.body);
    let preview = json(&preview);
    assert_eq!(preview["passed"], true);
    assert_eq!(preview["program_persisted"], false);
    assert_eq!(preview["screenshot_handles"].as_array().unwrap().len(), 1);
    assert!(preview["trace_handle"].as_str().is_some());

    let recorded = post(
        &studio,
        "/api/v1/authoring/record",
        r#"{"change":"live","route":"/dashboard","headless":true}"#,
    );
    assert_eq!(recorded.status, 200, "{}", recorded.body);
    let recorded = json(&recorded);
    assert_eq!(recorded["useful"], true);
    assert_eq!(recorded["runtime_llm_tokens"], 0);
    assert_eq!(recorded["preview"]["passed"], true);

    let promoted = post(
        &studio,
        "/api/v1/authoring/promote",
        &serde_json::json!({
            "change": "live",
            "preview_id": preview["preview_id"],
            "program": {"id": "generated-http-program"}
        })
        .to_string(),
    );
    assert_eq!(promoted.status, 200, "{}", promoted.body);
    assert_eq!(json(&promoted)["persisted"], true);

    let healed = post(
        &studio,
        "/api/v1/authoring/heal",
        &serde_json::json!({
            "change": "live",
            "base": "BASE",
            "head": "HEAD",
            "program_id": "generated-http-program",
            "expected_program_revision": 1,
            "edits": [{
                "edit": "insert_wait",
                "after": 0,
                "condition": {"kind": "url", "route": "/ready"}
            }]
        })
        .to_string(),
    );
    assert_eq!(healed.status, 200, "{}", healed.body);
    assert_eq!(json(&healed)["program_revision"], 2);

    assert_eq!(get(&studio, "/api/v1/authoring/draft").status, 405);
    assert_eq!(
        post(
            &studio,
            "/api/v1/authoring/draft",
            r#"{"token_budget":8000,"unexpected":true}"#,
        )
        .status,
        400
    );
}

#[test]
fn dashboard_shows_exceptions_and_hides_pass_noise() {
    let fake = Arc::new(FakeService::default());
    fake.set_verdict("HUMAN_REQUIRED");
    fake.set_proofs(vec![
        proof(
            "p-1",
            "sankey.visual-limit-others",
            "others-visible",
            "PROVEN",
        ),
        proof(
            "p-2",
            "sankey.visual-limit-others",
            "others-count",
            "PROVEN",
        ),
        proof(
            "p-3",
            "sankey.refresh",
            "dialog-on-refresh",
            "HUMAN_REQUIRED",
        ),
    ]);
    let studio = studio_with(&fake);

    let body = json(&get(&studio, "/api/v1/changes/sankey-others/summary"));
    assert_eq!(body["obligations"], 3);
    assert_eq!(body["requirements"], 2);
    assert_eq!(body["proven"], 2);
    assert_eq!(body["suppressed_passing"], 2);
    assert_eq!(body["verdict"], "HUMAN_REQUIRED");

    let needs = body["needs_attention"]
        .as_array()
        .expect("needs_attention array");
    assert_eq!(needs.len(), 1, "green proofs must not reach the dashboard");
    assert_eq!(needs[0]["obligation"], "dialog-on-refresh");
    assert_eq!(needs[0]["intent"], "sankey.refresh");
    assert_eq!(needs[0]["surface"], "unmeasured");
    assert_eq!(needs[0]["protection"], "unmeasured");
    assert_eq!(needs[0]["failure_reel"], Value::Null);
    assert_eq!(needs[0]["cheapest_next"], Value::Null);
    assert_eq!(needs[0]["visual_region"], "unmeasured");
    assert_eq!(body["ai"], Value::Null, "unmeasured AI usage is not zero");
}

#[test]
fn exception_cards_project_a_matching_surface_and_never_invent_a_join() {
    let fake = Arc::new(FakeService::default());
    fake.set_proofs(vec![proof(
        "p-3",
        "sankey.refresh",
        "dialog-on-refresh",
        "HUMAN_REQUIRED",
    )]);
    fake.set_surface_evidence(wvq_command_bus::SurfaceEvidenceMatrixView {
        present: true,
        truncated: false,
        surfaces: vec![
            SurfaceEvidenceRow {
                surface: "dialog-on-refresh".into(),
                kind: ApplicationSurfaceKind::Endpoint,
                intent: EvidenceCell::Present,
                runtime: EvidenceCell::Absent,
                test: EvidenceCell::Present,
                proof: EvidenceCell::Unmeasured,
                coverage: EvidenceCell::Present,
                protection: EvidenceCell::Absent,
                ui: EvidenceCell::Unmeasured,
                a11y: EvidenceCell::Unmeasured,
                mutation: EvidenceCell::Absent,
            },
            SurfaceEvidenceRow {
                surface: "endpoint:POST /pay".into(),
                kind: ApplicationSurfaceKind::Endpoint,
                intent: EvidenceCell::Present,
                runtime: EvidenceCell::Present,
                test: EvidenceCell::Present,
                proof: EvidenceCell::Present,
                coverage: EvidenceCell::Present,
                protection: EvidenceCell::Present,
                ui: EvidenceCell::Present,
                a11y: EvidenceCell::Present,
                mutation: EvidenceCell::Present,
            },
        ],
    });
    fake.set_evidence_plan(wvq_command_bus::CheapestEvidencePlanView {
        present: true,
        truncated: false,
        gaps: vec![EvidencePlan {
            surface: "dialog-on-refresh".into(),
            kind: ApplicationSurfaceKind::Endpoint,
            column: EvidenceColumn::Protection,
            need: EvidenceNeed::MeasuredAbsent,
            cheapest: Some(EvidenceProducer::ExistingTestAdaptation),
            producers: vec![ProducerOffer {
                producer: EvidenceProducer::ExistingTestAdaptation,
                cost: 1,
            }],
        }],
    });
    let studio = studio_with(&fake);
    let body = json(&get(&studio, "/api/v1/changes/sankey-others/summary"));
    let needs = body["needs_attention"]
        .as_array()
        .expect("needs_attention array");
    assert_eq!(needs.len(), 1);
    assert_eq!(needs[0]["surface"], "dialog-on-refresh");
    assert_eq!(needs[0]["intent"], "sankey.refresh");
    assert_eq!(needs[0]["protection"], "absent");
    assert_eq!(needs[0]["proof"], "unmeasured");
    assert_eq!(needs[0]["runtime"], "absent");
    assert_eq!(needs[0]["cheapest_next"], "existing_test_adaptation");
    assert_eq!(needs[0]["source_candidates"][0], "existing_test_adaptation");
    assert_eq!(
        needs[0]["code_impact"], "unmeasured",
        "a neighbouring surface must not become this obligation's code impact"
    );
    assert_eq!(needs[0]["visual_region"], "unmeasured");
    assert_eq!(needs[0]["failure_reel"], Value::Null);
}

/// The dashboard carries the composite verdict, not just the proof token.
///
/// An axis that was never in scope and an axis that was in scope and not
/// measured must be distinguishable at a glance; folding both into "fine" is
/// how a coverage gap becomes invisible.
#[test]
fn the_dashboard_shows_every_axis_state_and_what_was_not_measured() {
    let fake = Arc::new(FakeService::default());
    fake.set_verdict("UNPROVEN");
    let studio = studio_with(&fake);

    let body = json(&get(&studio, "/api/v1/changes/sankey-others/summary"));
    assert_eq!(body["state"], "NOT_ENOUGH_EVIDENCE");
    assert_eq!(body["verdict"], "UNPROVEN", "the old token still ships");
    assert_eq!(body["blocking"], false, "missing evidence is not a failure");

    let axes: Vec<(String, String)> = body["axes"]
        .as_array()
        .expect("axes array")
        .iter()
        .map(|axis| {
            (
                axis["axis"].as_str().unwrap_or_default().to_owned(),
                axis["state"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        axes.iter()
            .map(|(axis, _)| axis.as_str())
            .collect::<Vec<_>>(),
        vec![
            "proof",
            "protection",
            "debt",
            "stability",
            "ai",
            "ui_integrity",
            "delta_triangle"
        ]
    );
    assert_eq!(
        axes.iter()
            .find(|(axis, _)| axis == "proof")
            .map(|(_, state)| state.as_str()),
        Some("unmeasured")
    );
    assert_eq!(
        axes.iter()
            .find(|(axis, _)| axis == "ui_integrity")
            .map(|(_, state)| state.as_str()),
        Some("not_applicable"),
        "an axis with no surface is not an axis with no evidence"
    );

    let reasons = body["blocking_reasons"].as_array().expect("reasons array");
    assert!(
        reasons.iter().any(|reason| reason["axis"] == "proof"),
        "{reasons:?}"
    );
    assert!(
        body["limitations"]
            .as_array()
            .expect("limitations array")
            .iter()
            .any(|item| item["axis"] == "proof"),
        "the gap is named"
    );

    // Exception-only UI projection: healthy elements are counted, never listed.
    let ui = &body["ui_integrity"];
    assert_eq!(ui["state"], "not_applicable");
    assert_eq!(ui["new"].as_array().map(Vec::len), Some(0));
    assert_eq!(ui["returned"].as_array().map(Vec::len), Some(0));
    assert_eq!(ui["suppressed_existing"], 0);
    assert_eq!(ui["truncated"], false);
}

#[test]
fn the_dashboard_projects_application_surfaces_without_gating() {
    let fake = Arc::new(FakeService::default());
    fake.set_verdict("PROVEN");
    fake.set_application_surface(wvq_command_bus::ApplicationSurfaceView {
        present: true,
        truncated: false,
        protected: vec!["endpoint:POST /pay".into()],
        partial: vec!["route:/checkout".into()],
        unmeasured: vec!["endpoint:GET /idle".into()],
    });
    let studio = studio_with(&fake);

    let body = json(&get(&studio, "/api/v1/changes/sankey-others/summary"));
    assert_eq!(body["blocking"], false);
    assert_eq!(body["verdict"], "PROVEN");
    let surfaces = &body["application_surface"];
    assert_eq!(surfaces["present"], true);
    assert_eq!(surfaces["protected"][0], "endpoint:POST /pay");
    assert_eq!(surfaces["partial"][0], "route:/checkout");
    assert_eq!(surfaces["unmeasured"][0], "endpoint:GET /idle");
}

#[test]
fn the_dashboard_projects_the_surface_evidence_matrix_without_gating() {
    let fake = Arc::new(FakeService::default());
    fake.set_verdict("PROVEN");
    fake.set_surface_evidence(wvq_command_bus::SurfaceEvidenceMatrixView {
        present: true,
        truncated: false,
        surfaces: vec![SurfaceEvidenceRow {
            surface: "endpoint:POST /pay".into(),
            kind: ApplicationSurfaceKind::Endpoint,
            intent: EvidenceCell::Present,
            runtime: EvidenceCell::Unmeasured,
            test: EvidenceCell::Present,
            proof: EvidenceCell::Unmeasured,
            coverage: EvidenceCell::Present,
            protection: EvidenceCell::Unmeasured,
            ui: EvidenceCell::Unmeasured,
            a11y: EvidenceCell::Unmeasured,
            mutation: EvidenceCell::Absent,
        }],
    });
    let studio = studio_with(&fake);
    let body = json(&get(&studio, "/api/v1/changes/sankey-others/summary"));
    assert_eq!(body["blocking"], false);
    let row = &body["surface_evidence"]["surfaces"][0];
    assert_eq!(row["surface"], "endpoint:POST /pay");
    assert_eq!(row["intent"], "present");
    assert_eq!(row["mutation"], "absent");
    assert_eq!(row["runtime"], "unmeasured");
}

#[test]
fn the_dashboard_projects_the_cheapest_evidence_plan_without_gating() {
    let fake = Arc::new(FakeService::default());
    fake.set_verdict("PROVEN");
    fake.set_evidence_plan(wvq_command_bus::CheapestEvidencePlanView {
        present: true,
        truncated: false,
        gaps: vec![EvidencePlan {
            surface: "endpoint:GET /admin".into(),
            kind: ApplicationSurfaceKind::Endpoint,
            column: EvidenceColumn::Protection,
            need: EvidenceNeed::MeasuredAbsent,
            cheapest: Some(EvidenceProducer::ExistingTestAdaptation),
            producers: vec![ProducerOffer {
                producer: EvidenceProducer::ExistingTestAdaptation,
                cost: 1,
            }],
        }],
    });
    let studio = studio_with(&fake);
    let body = json(&get(&studio, "/api/v1/changes/sankey-others/summary"));
    assert_eq!(body["blocking"], false);
    let gap = &body["evidence_plan"]["gaps"][0];
    assert_eq!(gap["surface"], "endpoint:GET /admin");
    assert_eq!(gap["column"], "protection");
    assert_eq!(gap["cheapest"], "existing_test_adaptation");
    assert_eq!(gap["producers"][0]["cost"], 1);
}

#[test]
fn requirement_drill_down_still_shows_green_proofs() {
    let fake = Arc::new(FakeService::default());
    fake.set_proofs(vec![
        proof(
            "p-1",
            "sankey.visual-limit-others",
            "others-visible",
            "PROVEN",
        ),
        proof("p-2", "sankey.refresh", "dialog-on-refresh", "UNPROVEN"),
    ]);
    let studio = studio_with(&fake);

    let body = json(&get(
        &studio,
        "/api/v1/requirements/sankey.visual-limit-others/proofs",
    ));
    let proofs = body["proofs"].as_array().expect("proofs array");
    assert_eq!(proofs.len(), 1);
    assert_eq!(proofs[0]["verdict"], "PROVEN");
    assert_eq!(
        get(&studio, "/api/v1/requirements/sankey.unknown/proofs").status,
        404
    );
}

#[test]
fn unknown_route_is_404_and_wrong_method_is_405() {
    let studio = default_studio();

    let unknown = get(&studio, "/api/v1/nope");
    assert_eq!(unknown.status, 404);
    assert_eq!(json(&unknown)["error"], "unknown route");

    let wrong_method = post(&studio, "/api/v1/changes", "{}");
    assert_eq!(wrong_method.status, 405);
    assert_eq!(
        json(&wrong_method)["error"],
        "method not allowed for this route"
    );

    assert_eq!(post(&studio, "/", "{}").status, 405);
    assert_eq!(get(&studio, "/api/v1/human-decisions").status, 405);
}

#[test]
fn cockpit_is_html_and_exception_first() {
    let studio = default_studio();
    let page = get(&studio, "/");
    assert_eq!(page.status, 200, "{}", page.body);
    assert_eq!(page.content_type, "text/html; charset=utf-8");
    assert!(
        page.body.contains("Weavatrix Quality"),
        "product name must be visible: {}",
        page.body
    );
    assert!(
        page.body.contains("NEEDS HUMAN"),
        "default view is the exception list: {}",
        page.body
    );
    assert!(
        !page.body.contains("accept_all"),
        "the cockpit must not offer implicit accept-all"
    );

    let index = get(&studio, "/index.html");
    assert_eq!(index.status, 200);
    assert_eq!(index.content_type, "text/html; charset=utf-8");

    let script = get(&studio, "/studio.js");
    assert_eq!(script.status, 200, "{}", script.body);
    assert_eq!(script.content_type, "text/javascript; charset=utf-8");
    assert!(
        script.body.contains("needs_attention"),
        "the cockpit must project the exception list, not invent verdicts"
    );
    assert!(
        script.body.contains("cheapest next"),
        "exception cards must show the cheapest next evidence, not only the verdict"
    );
    assert!(
        script.body.contains("failure reel"),
        "exception cards must name the failure reel even when it is unmeasured"
    );
    assert!(
        script.body.contains("/api/v1/human-decisions"),
        "a human decision must POST to the existing provenance endpoint"
    );
    assert!(
        !script.body.contains("accept_all"),
        "JavaScript must not invent a bulk accept"
    );

    let api = get(&studio, "/api/v1/changes");
    assert_eq!(api.status, 200);
    assert_eq!(api.content_type, "application/json");
    assert_eq!(json(&api)["changes"][0], "sankey-others");
}

#[test]
fn human_decision_is_recorded_with_provenance() {
    let studio = default_studio();
    let response = post(
        &studio,
        "/api/v1/human-decisions",
        &decision_body("hd-1", "others-visible", "observed_only"),
    );
    assert_eq!(response.status, 201);

    let body = json(&response);
    assert_eq!(body["reviewer"], "sergii");
    assert_eq!(body["role"], "qa");
    assert_eq!(body["subject"], "others-visible");
    assert_eq!(body["artifact_digest"], DIGEST);
    assert_eq!(body["decided_at"], "2026-08-20T09:00:00Z");
    assert_eq!(
        body["seal_eligible"], false,
        "observed_only never becomes normative"
    );
    assert_eq!(body["escalates"], false);
}

#[test]
fn escalation_is_visible_on_the_decision() {
    let studio = default_studio();
    let body = json(&post(
        &studio,
        "/api/v1/human-decisions",
        &decision_body("hd-2", "others-visible", "request_product_decision"),
    ));
    assert_eq!(body["escalates"], true);
    assert_eq!(body["seal_eligible"], false);
}

#[test]
fn implicit_accept_all_is_refused() {
    let studio = default_studio();

    let bulk_field = serde_json::json!({
        "id": "hd-3",
        "reviewer": "sergii",
        "role": "qa",
        "subject": "others-visible",
        "artifact_digest": DIGEST,
        "decision": "accept_as_intended",
        "decided_at": "2026-08-20T09:00:00Z",
        "accept_all": true
    })
    .to_string();
    assert_eq!(
        post(&studio, "/api/v1/human-decisions", &bulk_field).status,
        400,
        "an accept-all flag must not even parse"
    );

    for subject in ["*", "all", "a,b"] {
        assert_eq!(
            post(
                &studio,
                "/api/v1/human-decisions",
                &decision_body("hd-4", subject, "accept_as_intended")
            )
            .status,
            422,
            "subject {subject:?} approves more than one thing"
        );
    }
}

#[test]
fn malformed_decision_payloads_are_rejected() {
    let studio = default_studio();
    assert_eq!(post(&studio, "/api/v1/human-decisions", "{").status, 400);
    let bad_digest = serde_json::json!({
        "id": "hd-5",
        "reviewer": "sergii",
        "role": "qa",
        "subject": "others-visible",
        "artifact_digest": "NOT-HEX",
        "decision": "accept_as_intended",
        "decided_at": "2026-08-20T09:00:00Z"
    })
    .to_string();
    assert_eq!(
        post(&studio, "/api/v1/human-decisions", &bad_digest).status,
        400
    );
}

#[test]
fn recovery_screens_are_absent_until_a_desk_is_attached() {
    let studio = default_studio();
    for path in [
        "/api/v1/recovery/review",
        "/api/v1/recovery/questions",
        "/api/v1/recovery/patch",
    ] {
        let response = get(&studio, path);
        assert_eq!(response.status, 404, "{path}");
        assert_eq!(
            json(&response)["error"],
            "spec recovery is not enabled for this repository"
        );
    }
    assert_eq!(
        post(&studio, "/api/v1/recovery/decisions", "{}").status,
        404
    );
}

#[test]
fn an_attached_desk_serves_the_recovery_screens() {
    let studio = studio_with(&Arc::new(FakeService::default()))
        .with_recovery(Arc::new(Mutex::new(RecoveryDesk::new("sankey-others"))));

    let review = json(&get(&studio, "/api/v1/recovery/review"));
    assert_eq!(review["change"], "sankey-others");
    assert_eq!(review["blocked"], false);

    let questions = json(&get(&studio, "/api/v1/recovery/questions"));
    assert!(questions["for_qa"].as_array().is_some());

    let patch = json(&get(&studio, "/api/v1/recovery/patch"));
    assert!(
        patch["patch"]
            .as_str()
            .expect("patch text")
            .starts_with("# PROPOSED"),
        "the Studio patch preview is never presented as approved"
    );

    // A decision about a candidate the desk does not know is refused, not stored.
    let unknown = post(
        &studio,
        "/api/v1/recovery/decisions",
        &decision_body("hd-r1", "cand-unknown", "accept_as_intended"),
    );
    assert_eq!(unknown.status, 422);
}

#[test]
fn protection_screens_are_absent_until_a_view_is_attached() {
    let studio = default_studio();
    let response = get(&studio, "/api/v1/protection");
    assert_eq!(response.status, 404);
    assert_eq!(
        json(&response)["error"],
        "protection continuity has not been computed"
    );
}

#[test]
fn an_attached_view_answers_what_protected_this_before() {
    let view = ProtectionView {
        lineage: vec![TestLineageView {
            test: "auth-viewer.spec".into(),
            state: "unchanged".into(),
            matched_on: "test_name".into(),
            protection_changed: true,
            lost_flows: vec!["viewer-deny".into()],
            phantom: true,
            ..TestLineageView::default()
        }],
        flows: vec![FlowView {
            flow: "viewer-deny".into(),
            tests_before: vec!["auth-viewer.spec".into()],
            coverage_before: vec!["viewer-denied".into()],
            proof_before: vec!["P-811".into()],
            ..FlowView::default()
        }],
        ..ProtectionView::default()
    };
    let studio =
        studio_with(&Arc::new(FakeService::default())).with_protection(Arc::new(Mutex::new(view)));

    let report = json(&get(&studio, "/api/v1/protection"));
    assert_eq!(report["blocking"], false);
    assert_eq!(report["suppressed_healthy"], 0);

    let lineage = json(&get(&studio, "/api/v1/protection/tests/auth-viewer.spec"));
    assert_eq!(lineage["phantom"], true, "the phantom test is visible");
    assert_eq!(lineage["lost_flows"][0], "viewer-deny");

    let flow = json(&get(&studio, "/api/v1/protection/flows/viewer-deny"));
    assert_eq!(flow["tests_before"][0], "auth-viewer.spec");
    assert_eq!(flow["proof_before"][0], "P-811");
    assert!(
        flow["tests_after"].as_array().expect("array").is_empty(),
        "before and after sit side by side in one view"
    );

    assert_eq!(get(&studio, "/api/v1/protection/flows/unknown").status, 404);
    assert_eq!(get(&studio, "/api/v1/protection/tests/unknown").status, 404);
}

#[test]
fn studio_answers_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let server = std::thread::spawn(move || {
        let service: Arc<dyn QualityService> = Arc::new(FakeService::default());
        let studio = Studio::new(service, temp_store());
        serve(&listener, Some(1), |request| studio.handle(request)).expect("serve one request");
    });

    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .write_all(b"GET /api/v1/changes HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
        .expect("write request");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read response");
    server.join().expect("server thread");

    assert!(raw.starts_with("HTTP/1.1 200 OK"), "got: {raw}");
    assert!(raw.contains("Content-Type: application/json"));
    let body = raw.split("\r\n\r\n").nth(1).expect("response body");
    let value: Value = serde_json::from_str(body).expect("json body");
    assert_eq!(value["changes"][0], "sankey-others");
}

#[test]
fn cockpit_answers_html_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let server = std::thread::spawn(move || {
        let service: Arc<dyn QualityService> = Arc::new(FakeService::default());
        let studio = Studio::new(service, temp_store());
        serve(&listener, Some(1), |request| studio.handle(request)).expect("serve one request");
    });

    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
        .expect("write request");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read response");
    server.join().expect("server thread");

    assert!(raw.starts_with("HTTP/1.1 200 OK"), "got: {raw}");
    assert!(raw.contains("Content-Type: text/html; charset=utf-8"));
    assert!(raw.contains("NEEDS HUMAN"));
    assert!(!raw.contains("accept_all"));
}

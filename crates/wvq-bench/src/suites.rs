//! Representative v1 ecosystem cases. Costs are milliseconds (or runner-reported).

use std::collections::BTreeSet;
use wvq_intelligence::{ObligationNeed, TestCandidate};
use wvq_runtime::TestCaseResult;

use crate::case::{Ecosystem, FindingLabel, KnownBug, ShadowCase};

/// TypeScript frontend: overlapping Vitest-style suite, one recovered UI bug.
#[must_use]
pub fn ts_frontend_case() -> ShadowCase {
    ShadowCase {
        name: "ts-frontend-sankey".into(),
        ecosystem: Ecosystem::TsFrontend,
        candidates: vec![
            candidate("T-visual-limit", 40, ["others-visible", "overflow-grouped"]),
            candidate("T-storybook-sankey", 80, ["others-visible"]),
            candidate(
                "T-full-e2e-dashboard",
                1_200,
                ["others-visible", "overflow-grouped", "others-count"],
            ),
            candidate("T-a11y-chart", 90, ["others-visible"]),
            candidate("T-unrelated-settings", 50, ["settings-save"]),
            candidate("T-unrelated-login", 70, ["login-form"]),
        ],
        obligations: vec![need("others-visible", true), need("overflow-grouped", true)],
        bugs: vec![KnownBug {
            id: "overflow-groups-others".into(),
            recovering_test: "T-visual-limit".into(),
        }],
        findings: vec![
            finding("WVQ-ARCH-001:src/sankey.ts", true, true),
            finding("WVQ-SIZE-001:src/vendor.ts", false, true),
            finding("WVQ-COV-001:src/sankey.ts", true, true),
        ],
        planning_tokens: 0,
        runtime_tokens: 0,
        artifact_bytes: 48_000,
        human_touch_minutes: None,
        baseline_human_touch_minutes: None,
        escaped_regressions_delta: 0,
    }
}

/// Node/Bun backend: cheaper unit test preferred over a wide integration test.
#[must_use]
pub fn node_bun_backend_case() -> ShadowCase {
    ShadowCase {
        name: "bun-backend-add".into(),
        ecosystem: Ecosystem::NodeBunBackend,
        candidates: vec![
            candidate("T-unit-add", 12, ["add-numbers"]),
            candidate("T-unit-total", 15, ["add-numbers", "total-list"]),
            candidate(
                "T-integration-http",
                400,
                ["add-numbers", "total-list", "healthz"],
            ),
            candidate("T-unrelated-cron", 60, ["cron-tick"]),
        ],
        obligations: vec![need("add-numbers", true), need("total-list", true)],
        bugs: vec![KnownBug {
            id: "total-off-by-one".into(),
            recovering_test: "T-unit-total".into(),
        }],
        findings: vec![
            finding("WVQ-API-001:POST /add", true, true),
            finding("WVQ-DEAD-001:src/legacy.ts", true, false),
        ],
        planning_tokens: 0,
        runtime_tokens: 0,
        artifact_bytes: 8_192,
        human_touch_minutes: None,
        baseline_human_touch_minutes: None,
        escaped_regressions_delta: 0,
    }
}

/// Go service: `go test` cases, overflow failure is the recovered bug.
#[must_use]
pub fn go_service_case() -> ShadowCase {
    ShadowCase {
        name: "go-service-add".into(),
        ecosystem: Ecosystem::GoService,
        candidates: vec![
            candidate("TestAdd", 10, ["add"]),
            candidate("TestOverflow", 12, ["add", "overflow"]),
            candidate("TestSkip", 1, ["skip-helper"]),
            candidate("TestHTTPSuite", 350, ["add", "overflow", "healthz"]),
        ],
        obligations: vec![need("add", true), need("overflow", true)],
        bugs: vec![KnownBug {
            id: "overflow-wraps".into(),
            recovering_test: "TestOverflow".into(),
        }],
        findings: vec![
            finding("WVQ-API-001:GET /healthz", false, false),
            finding("WVQ-HIST-002:add.go", true, true),
        ],
        planning_tokens: 0,
        runtime_tokens: 0,
        artifact_bytes: 4_096,
        human_touch_minutes: None,
        baseline_human_touch_minutes: None,
        escaped_regressions_delta: 0,
    }
}

/// Overlay obligation mapping onto a normalized runner result (fixture-backed).
///
/// Each case becomes a candidate whose cost is the reported duration (1 ms minimum).
#[must_use]
pub fn case_from_runner(
    name: impl Into<String>,
    ecosystem: Ecosystem,
    cases: &[TestCaseResult],
    covers: &[(String, Vec<String>)],
    obligations: Vec<ObligationNeed>,
) -> ShadowCase {
    let cover_map: std::collections::BTreeMap<&str, Vec<String>> = covers
        .iter()
        .map(|(id, items)| (id.as_str(), items.clone()))
        .collect();
    let candidates = cases
        .iter()
        .map(|item| {
            let id = format!("{}::{}", item.suite, item.name);
            let covered = cover_map.get(id.as_str()).cloned().unwrap_or_default();
            TestCandidate {
                id,
                cost: item.duration_ms.unwrap_or(1).max(1),
                flake_penalty: 0,
                covers: covered.into_iter().collect(),
                explanation: vec!["from normalized runner evidence".into()],
            }
        })
        .collect();
    ShadowCase {
        name: name.into(),
        ecosystem,
        candidates,
        obligations,
        bugs: Vec::new(),
        findings: Vec::new(),
        planning_tokens: 0,
        runtime_tokens: 0,
        artifact_bytes: 0,
        human_touch_minutes: None,
        baseline_human_touch_minutes: None,
        escaped_regressions_delta: 0,
    }
}

fn candidate(id: &str, cost: u64, covers: impl IntoIterator<Item = &'static str>) -> TestCandidate {
    TestCandidate {
        id: id.to_owned(),
        cost,
        flake_penalty: 0,
        covers: covers
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>(),
        explanation: vec![format!("shadow candidate {id}")],
    }
}

fn need(id: &str, high_risk: bool) -> ObligationNeed {
    ObligationNeed {
        id: id.to_owned(),
        high_risk,
    }
}

fn finding(id: &str, expected: bool, observed: bool) -> FindingLabel {
    FindingLabel {
        id: id.to_owned(),
        expected,
        observed,
    }
}

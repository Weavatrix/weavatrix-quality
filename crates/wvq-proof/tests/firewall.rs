//! Task 23: AI Cost Firewall. Green path is free; exhaustion needs a human.

use wvq_proof::{
    AI_BUDGET_EXHAUSTED, AiBudget, AiCall, AiCallKind, AiCostFirewall, BudgetLimit, ProofVerdict,
};

fn budget() -> AiBudget {
    AiBudget {
        planning_tokens: 8_000,
        runtime_tokens: 0,
        browser_escape_calls: 2,
        vision_calls: 1,
        max_cost_micros: None,
    }
}

#[test]
fn ordinary_green_path_spends_zero_runtime_tokens() {
    let mut firewall = AiCostFirewall::new(budget());
    firewall
        .charge(&AiCall::tokens(AiCallKind::Planning, 1_800))
        .expect("planning is within budget");
    assert_eq!(firewall.usage().runtime_tokens, 0);
    assert_eq!(firewall.usage().planning_tokens, 1_800);
    assert!(!firewall.is_exhausted());
    assert_eq!(firewall.verdict(), None);
}

#[test]
fn runtime_call_over_budget_is_refused_and_needs_a_human() {
    let mut firewall = AiCostFirewall::new(budget());
    let refused = firewall
        .charge(&AiCall::tokens(AiCallKind::Runtime, 1))
        .expect_err("runtime budget is zero");
    assert_eq!(refused.reason, AI_BUDGET_EXHAUSTED);
    assert_eq!(refused.limit, BudgetLimit::RuntimeTokens);
    assert_eq!(refused.allowed, 0);
    assert_eq!(refused.requested, 1);
    assert!(firewall.is_exhausted());
    assert_eq!(firewall.verdict(), Some(ProofVerdict::HumanRequired));
}

#[test]
fn a_refused_call_records_nothing() {
    let mut firewall = AiCostFirewall::new(AiBudget {
        planning_tokens: 100,
        ..budget()
    });
    firewall
        .charge(&AiCall::tokens(AiCallKind::Planning, 90))
        .expect("first call fits");
    let before = firewall.usage();
    firewall
        .charge(&AiCall::tokens(AiCallKind::Planning, 20))
        .expect_err("second call overflows");
    assert_eq!(firewall.usage(), before);
}

#[test]
fn browser_escape_and_vision_calls_are_counted_and_capped() {
    let mut firewall = AiCostFirewall::new(AiBudget {
        runtime_tokens: 1_000,
        ..budget()
    });
    for _ in 0..2 {
        firewall
            .charge(&AiCall::tokens(AiCallKind::BrowserEscape, 10))
            .expect("two escapes are allowed");
    }
    let refused = firewall
        .charge(&AiCall::tokens(AiCallKind::BrowserEscape, 10))
        .expect_err("third escape exceeds the cap");
    assert_eq!(refused.limit, BudgetLimit::BrowserEscapeCalls);

    firewall
        .charge(&AiCall::tokens(AiCallKind::Vision, 5))
        .expect("one vision call is allowed");
    let refused = firewall
        .charge(&AiCall::tokens(AiCallKind::Vision, 5))
        .expect_err("second vision call exceeds the cap");
    assert_eq!(refused.limit, BudgetLimit::VisionCalls);

    let usage = firewall.usage();
    assert_eq!(usage.browser_escape_calls, 2);
    assert_eq!(usage.vision_calls, 1);
    assert_eq!(usage.runtime_tokens, 25);
}

#[test]
fn money_ceiling_refuses_before_spending() {
    let mut firewall = AiCostFirewall::new(AiBudget {
        max_cost_micros: Some(500),
        ..budget()
    });
    let refused = firewall
        .charge(&AiCall {
            kind: AiCallKind::Planning,
            tokens: 10,
            cost_micros: 900,
        })
        .expect_err("cost ceiling stops the call");
    assert_eq!(refused.limit, BudgetLimit::Cost);
    assert_eq!(firewall.usage().cost_micros, 0);
}

#[test]
fn ratio_is_reported_only_when_development_data_is_supplied() {
    let mut firewall = AiCostFirewall::new(budget());
    firewall
        .charge(&AiCall::tokens(AiCallKind::Planning, 2_000))
        .expect("planning fits");
    assert_eq!(firewall.ratio(0), None);
    let ratio = firewall.ratio(10_000).expect("development data supplied");
    assert_eq!(ratio.qa_tokens, 2_000);
    assert_eq!(ratio.percent, 20);
    assert!(ratio.within(20));
    assert!(!ratio.within(19));
}

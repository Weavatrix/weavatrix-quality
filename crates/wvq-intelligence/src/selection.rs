//! Greedy weighted set cover for minimal impacted regression.
//!
//! An exact solver is deferred until the shadow benchmark (Task 17) shows the
//! greedy plan is not good enough.

use std::collections::BTreeSet;

/// One test / session / program that can prove obligations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCandidate {
    /// Stable test id (`T14`, `file::name`, …).
    pub id: String,
    /// Execution cost (milliseconds or abstract units).
    pub cost: u64,
    /// Added to cost when ranking (flake history).
    pub flake_penalty: u64,
    /// Obligation ids this candidate can prove.
    pub covers: BTreeSet<String>,
    /// Provenance chain supplied by Weavatrix / spec mapping.
    pub explanation: Vec<String>,
}

/// An obligation the plan should cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObligationNeed {
    /// Obligation id.
    pub id: String,
    /// High/critical risk: never omit if any candidate covers it.
    pub high_risk: bool,
}

/// Input to [`select_minimal_plan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionInput {
    /// Candidate tests after Weavatrix ∩ coverage ∩ obligation filters.
    pub candidates: Vec<TestCandidate>,
    /// Obligations to cover.
    pub obligations: Vec<ObligationNeed>,
}

/// One selected test with its explanation chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedTest {
    /// Test id.
    pub id: String,
    /// Obligations this pick newly covered at selection time.
    pub covers: Vec<String>,
    /// Effective cost used (`cost + flake_penalty`).
    pub cost: u64,
    /// Why it was selected.
    pub explanation: Vec<String>,
}

/// Minimal execution plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionPlan {
    /// Tests to run, in greedy order.
    pub selected: Vec<SelectedTest>,
    /// High-risk obligations no candidate could cover.
    pub uncovered_mandatory: Vec<String>,
    /// Algorithm id. Exact solvers wait on a benchmark.
    pub algorithm: &'static str,
}

const MANDATORY_WEIGHT: u64 = 1_000;
const ALGORITHM: &str = "greedy-weighted-set-cover";

/// Deterministic greedy weighted set cover.
///
/// Mandatory (high-risk) obligations are weighted so they cannot lose to a
/// cheaper test that only covers optional items. Equivalent remaining cover
/// prefers lower effective cost, then lexicographic id.
#[must_use]
pub fn select_minimal_plan(input: SelectionInput) -> SelectionPlan {
    let mandatory: BTreeSet<String> = input
        .obligations
        .iter()
        .filter(|item| item.high_risk)
        .map(|item| item.id.clone())
        .collect();
    let mut remaining: BTreeSet<String> = input
        .obligations
        .iter()
        .map(|item| item.id.clone())
        .collect();
    let mut unused = input.candidates;
    unused.sort_by(|left, right| left.id.cmp(&right.id));
    let mut selected = Vec::new();

    loop {
        let Some((index, gain_mandatory, gain_optional)) = best_index(&unused, &remaining, &mandatory)
        else {
            break;
        };
        let candidate = unused.remove(index);
        let mut newly: Vec<String> = candidate
            .covers
            .iter()
            .filter(|id| remaining.contains(*id))
            .cloned()
            .collect();
        newly.sort();
        for id in &newly {
            remaining.remove(id);
        }
        let cost = effective_cost(&candidate);
        let mut explanation = candidate.explanation;
        explanation.push(format!("covers obligations: {}", newly.join(", ")));
        explanation.push(format!(
            "greedy gain: {gain_mandatory} mandatory + {gain_optional} optional / cost {cost}"
        ));
        selected.push(SelectedTest {
            id: candidate.id,
            covers: newly,
            cost,
            explanation,
        });
        if remaining.is_empty() {
            break;
        }
    }

    let uncovered_mandatory = mandatory
        .into_iter()
        .filter(|id| remaining.contains(id))
        .collect();
    SelectionPlan {
        selected,
        uncovered_mandatory,
        algorithm: ALGORITHM,
    }
}

fn best_index(
    unused: &[TestCandidate],
    remaining: &BTreeSet<String>,
    mandatory: &BTreeSet<String>,
) -> Option<(usize, u64, u64)> {
    let mut best: Option<(usize, u64, u64, u64, &str)> = None;
    for (index, candidate) in unused.iter().enumerate() {
        let mut gain_mandatory = 0_u64;
        let mut gain_optional = 0_u64;
        for id in &candidate.covers {
            if !remaining.contains(id) {
                continue;
            }
            if mandatory.contains(id) {
                gain_mandatory = gain_mandatory.saturating_add(1);
            } else {
                gain_optional = gain_optional.saturating_add(1);
            }
        }
        if gain_mandatory == 0 && gain_optional == 0 {
            continue;
        }
        let cost = effective_cost(candidate).max(1);
        let score = gain_mandatory
            .saturating_mul(MANDATORY_WEIGHT)
            .saturating_add(gain_optional);
        match best {
            None => best = Some((index, score, cost, gain_mandatory, candidate.id.as_str())),
            Some((_, best_score, best_cost, _, best_id)) => {
                if better(score, cost, candidate.id.as_str(), best_score, best_cost, best_id) {
                    best = Some((index, score, cost, gain_mandatory, candidate.id.as_str()));
                }
            }
        }
    }
    best.map(|(index, _, _, gain_m, _)| {
        let candidate = &unused[index];
        let gain_o = u64::try_from(
            candidate
                .covers
                .iter()
                .filter(|id| remaining.contains(*id) && !mandatory.contains(*id))
                .count(),
        )
        .unwrap_or(u64::MAX);
        (index, gain_m, gain_o)
    })
}

fn better(
    score: u64,
    cost: u64,
    id: &str,
    best_score: u64,
    best_cost: u64,
    best_id: &str,
) -> bool {
    let left = score.saturating_mul(best_cost);
    let right = best_score.saturating_mul(cost);
    if left != right {
        return left > right;
    }
    if cost != best_cost {
        return cost < best_cost;
    }
    id < best_id
}

fn effective_cost(candidate: &TestCandidate) -> u64 {
    candidate.cost.saturating_add(candidate.flake_penalty)
}

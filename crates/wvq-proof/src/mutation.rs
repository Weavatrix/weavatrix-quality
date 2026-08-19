//! Changed-region mutation. Survived mutants are proof weakness, not coverage.

use std::collections::BTreeSet;

/// Ecosystem for a mutant. v1: TS/JS and Go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutantEcosystem {
    /// TypeScript / JavaScript.
    TsJs,
    /// Go.
    Go,
}

/// TS/JS operators from spec §21.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsJsOperator {
    /// `>` ↔ `>=`
    CmpGtGe,
    /// `<` ↔ `<=`
    CmpLtLe,
    /// `===` ↔ `!==`
    EqNeq,
    /// `true` ↔ `false`
    BoolFlip,
    /// `&&` ↔ `||`
    AndOr,
    /// `+1` ↔ `-1`
    OffByOne,
    /// remove branch
    RemoveBranch,
    /// remove sort
    RemoveSort,
    /// wrong permission
    WrongPermission,
    /// omit callback
    OmitCallback,
    /// omit error propagation
    OmitError,
    /// wrong collection boundary
    CollectionBoundary,
}

/// Safe Go operators from spec §21.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoOperator {
    /// `err != nil` ↔ `err == nil`
    ErrNilFlip,
    /// boundary flip
    BoundaryFlip,
    /// return nil/zero
    ReturnZero,
    /// skip branch
    SkipBranch,
    /// ignore context
    IgnoreContext,
    /// invert boolean
    InvertBool,
}

/// One operator application on a changed region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutant {
    /// Mutant id.
    pub id: String,
    /// Ecosystem.
    pub ecosystem: MutantEcosystem,
    /// Operator token.
    pub operator: String,
    /// Changed region (`file:span`).
    pub region: String,
}

/// Outcome of one mutant against the selected suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutantStatus {
    /// A selected test failed.
    Killed,
    /// All selected tests passed.
    Survived,
}

/// Per-mutant result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutantResult {
    /// Mutant.
    pub mutant: Mutant,
    /// Killed or survived.
    pub status: MutantStatus,
    /// Tests actually run (selected only).
    pub tests_run: Vec<String>,
}

/// Attached to a Proof. Survived > 0 is weakness.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MutationSummary {
    /// Killed mutants.
    pub killed: u64,
    /// Survived mutants.
    pub survived: u64,
}

impl MutationSummary {
    /// From individual results.
    #[must_use]
    pub fn from_results(results: &[MutantResult]) -> Self {
        let mut killed = 0_u64;
        let mut survived = 0_u64;
        for item in results {
            match item.status {
                MutantStatus::Killed => killed = killed.saturating_add(1),
                MutantStatus::Survived => survived = survived.saturating_add(1),
            }
        }
        Self { killed, survived }
    }
}

/// Whether a selected test detects a mutant.
pub trait MutantOracle {
    /// True when the test fails under the mutant (kills it).
    fn test_fails(&self, mutant_id: &str, test_id: &str) -> bool;
}

/// TS/JS operators for one changed region. Empty region yields none.
#[must_use]
pub fn ts_js_mutants(region: &str) -> Vec<Mutant> {
    if region.is_empty() {
        return Vec::new();
    }
    [
        TsJsOperator::CmpGtGe,
        TsJsOperator::CmpLtLe,
        TsJsOperator::EqNeq,
        TsJsOperator::BoolFlip,
        TsJsOperator::AndOr,
        TsJsOperator::OffByOne,
        TsJsOperator::RemoveBranch,
        TsJsOperator::RemoveSort,
        TsJsOperator::WrongPermission,
        TsJsOperator::OmitCallback,
        TsJsOperator::OmitError,
        TsJsOperator::CollectionBoundary,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, operator)| Mutant {
        id: format!("ts-{index}-{region}"),
        ecosystem: MutantEcosystem::TsJs,
        operator: format!("{operator:?}"),
        region: region.to_owned(),
    })
    .collect()
}

/// Safe Go operators for one changed region.
#[must_use]
pub fn go_mutants(region: &str) -> Vec<Mutant> {
    if region.is_empty() {
        return Vec::new();
    }
    [
        GoOperator::ErrNilFlip,
        GoOperator::BoundaryFlip,
        GoOperator::ReturnZero,
        GoOperator::SkipBranch,
        GoOperator::IgnoreContext,
        GoOperator::InvertBool,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, operator)| Mutant {
        id: format!("go-{index}-{region}"),
        ecosystem: MutantEcosystem::Go,
        operator: format!("{operator:?}"),
        region: region.to_owned(),
    })
    .collect()
}

/// Run mutants against the selected suite only. Other tests are not invoked.
pub fn run_selected_mutants(
    mutants: &[Mutant],
    selected: &[String],
    oracle: &dyn MutantOracle,
) -> Vec<MutantResult> {
    let selected: BTreeSet<&str> = selected.iter().map(String::as_str).collect();
    mutants
        .iter()
        .map(|mutant| {
            let mut tests_run = Vec::new();
            let mut killed = false;
            for test in &selected {
                tests_run.push((*test).to_owned());
                if oracle.test_fails(&mutant.id, test) {
                    killed = true;
                }
            }
            MutantResult {
                mutant: mutant.clone(),
                status: if killed {
                    MutantStatus::Killed
                } else {
                    MutantStatus::Survived
                },
                tests_run,
            }
        })
        .collect()
}

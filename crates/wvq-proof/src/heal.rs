//! Safe healing: locators and waits only. Never a new business oracle.

use thiserror::Error;
use wvq_domain::{ObligationId, OracleSealId};
use wvq_runtime::{Target, TestAction, TestProgram, WaitCondition};

/// Why a heal was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HealError {
    /// Oracle seal does not match the program under repair.
    #[error("heal requires the same OracleSeal")]
    SealMismatch,
    /// Semantic assertions / obligations changed.
    #[error("heal cannot change semantic assertions")]
    AssertionChanged,
    /// Expected business result would move.
    #[error("heal cannot change expected results")]
    ExpectedResultChanged,
    /// Target recovery is not semantic.
    #[error("{0}")]
    Invalid(String),
}

/// Allowed automatic edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealEdit {
    /// Replace a target with a recovered semantic alias.
    Retarget {
        /// Step index.
        step: usize,
        /// Recovered target.
        target: Target,
    },
    /// Insert a deterministic wait after `step`.
    InsertWait {
        /// Insert after this index.
        after: usize,
        /// Wait condition.
        condition: WaitCondition,
    },
}

/// Versioned repair. The program id stays; `revision` increments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealedProgram {
    /// Repaired program.
    pub program: TestProgram,
    /// 1-based revision after this heal.
    pub revision: u32,
    /// Seal that must still hold.
    pub seal: OracleSealId,
}

/// Recover a target: keep semantic identity, allow test-id / CSS alias.
///
/// # Errors
///
/// `XPath`-like CSS or empty recovered identity.
pub fn recover_target(previous: &Target, observed: &Target) -> Result<Target, HealError> {
    if looks_like_xpath(observed.fallback_css.as_deref())
        || looks_like_xpath(previous.fallback_css.as_deref())
    {
        return Err(HealError::Invalid("XPath is not a healing identity".into()));
    }
    let recovered = Target {
        role: observed.role.clone().or_else(|| previous.role.clone()),
        accessible_name: observed
            .accessible_name
            .clone()
            .or_else(|| previous.accessible_name.clone()),
        label: observed.label.clone().or_else(|| previous.label.clone()),
        test_id: observed
            .test_id
            .clone()
            .or_else(|| previous.test_id.clone()),
        component_hint: observed
            .component_hint
            .clone()
            .or_else(|| previous.component_hint.clone()),
        scope: observed.scope.clone().or_else(|| previous.scope.clone()),
        fallback_css: observed
            .fallback_css
            .clone()
            .or_else(|| previous.fallback_css.clone()),
    };
    if recovered.role.is_none()
        && recovered.accessible_name.is_none()
        && recovered.label.is_none()
        && recovered.test_id.is_none()
        && recovered.component_hint.is_none()
        && recovered.fallback_css.is_none()
    {
        return Err(HealError::Invalid("recovered target is empty".into()));
    }
    if previous.role.is_some() && recovered.role != previous.role {
        return Err(HealError::ExpectedResultChanged);
    }
    if previous.accessible_name.is_some() && recovered.accessible_name != previous.accessible_name {
        return Err(HealError::ExpectedResultChanged);
    }
    Ok(recovered)
}

fn looks_like_xpath(css: Option<&str>) -> bool {
    css.is_some_and(|value| value.starts_with("//") || value.starts_with("/html"))
}

/// Apply allowed edits. Seal and assertions must be unchanged.
///
/// # Errors
///
/// Seal mismatch, assertion/result change, or invalid step index.
pub fn apply_heal(
    program: &TestProgram,
    seal: &OracleSealId,
    expected_seal: &OracleSealId,
    edits: &[HealEdit],
    current_revision: u32,
) -> Result<HealedProgram, HealError> {
    if seal != expected_seal {
        return Err(HealError::SealMismatch);
    }
    let before_asserts = assertions(program);
    let mut next = program.clone();
    for edit in edits {
        match edit {
            HealEdit::Retarget { step, target } => {
                retarget_step(&mut next, *step, target)?;
            }
            HealEdit::InsertWait { after, condition } => {
                if *after >= next.steps.len() {
                    return Err(HealError::Invalid("wait insert index out of range".into()));
                }
                next.steps.insert(
                    after.saturating_add(1),
                    TestAction::Wait {
                        condition: condition.clone(),
                    },
                );
            }
        }
    }
    if assertions(&next) != before_asserts || next.obligations != program.obligations {
        return Err(HealError::AssertionChanged);
    }
    if fill_values(&next) != fill_values(program) {
        return Err(HealError::ExpectedResultChanged);
    }
    next.validate()
        .map_err(|err| HealError::Invalid(err.to_string()))?;
    Ok(HealedProgram {
        program: next,
        revision: current_revision.saturating_add(1).max(1),
        seal: seal.clone(),
    })
}

fn retarget_step(program: &mut TestProgram, step: usize, target: &Target) -> Result<(), HealError> {
    let action = program
        .steps
        .get_mut(step)
        .ok_or_else(|| HealError::Invalid("retarget step out of range".into()))?;
    match action {
        TestAction::Activate { target: current }
        | TestAction::Fill {
            target: current, ..
        }
        | TestAction::Select {
            target: current, ..
        }
        | TestAction::Hover { target: current }
        | TestAction::Scroll { target: current }
        | TestAction::Drag {
            target: current, ..
        } => {
            *current = recover_target(current, target)?;
            Ok(())
        }
        TestAction::Assert { .. } => Err(HealError::AssertionChanged),
        _ => Err(HealError::Invalid(
            "heal can only retarget activate/fill/select/hover/scroll/drag".into(),
        )),
    }
}

fn assertions(program: &TestProgram) -> Vec<ObligationId> {
    let mut ids: Vec<ObligationId> = program
        .steps
        .iter()
        .filter_map(|step| match step {
            TestAction::Assert { obligation } => Some(obligation.clone()),
            _ => None,
        })
        .collect();
    ids.extend(program.obligations.iter().cloned());
    ids
}

fn fill_values(program: &TestProgram) -> Vec<String> {
    program
        .steps
        .iter()
        .filter_map(|step| match step {
            TestAction::Fill { value, .. } | TestAction::Select { value, .. } => {
                Some(value.clone())
            }
            _ => None,
        })
        .collect()
}

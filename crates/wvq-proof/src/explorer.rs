//! Model-less cheap explorer. Agent packet only after deterministic exhaustion.

use std::collections::BTreeSet;

/// Semantic control the explorer may activate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SemanticControl {
    /// Stable id.
    pub id: String,
    /// Role + accessible name (not CSS/`XPath`).
    pub semantic: String,
    /// Setup cost (higher is worse).
    pub setup_cost: u64,
    /// Already covered by existing tests/programs.
    pub already_covered: bool,
    /// Would cover an uncovered obligation.
    pub uncovers_obligation: bool,
    /// Leads to a novel behavior state.
    pub novel_state: bool,
    /// Graph-risk proximity 0–3.
    pub risk: u8,
    /// Boundary heuristic.
    pub boundary: bool,
    /// Historical bug similarity.
    pub historical: bool,
}

/// Depth / action budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerBudget {
    /// Max actions.
    pub max_actions: u32,
    /// Tarpit window.
    pub tarpit_after: u32,
}

/// Compact escape packet. No DOM/screenshot. 0 runtime tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerPacket {
    /// Goal.
    pub goal: String,
    /// Uncovered obligation, if any.
    pub uncovered_obligation: Option<String>,
    /// Current state digest.
    pub state_digest: String,
    /// Top remaining semantic controls.
    pub top_controls: Vec<String>,
    /// Last actions taken.
    pub last_actions: Vec<String>,
    /// Why deterministic search stopped.
    pub failed_candidates: Vec<String>,
    /// Always 0.
    pub runtime_tokens: u64,
}

/// One explorer decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerDecision {
    /// Take this control.
    Act(String),
    /// Budget or tarpit exhausted; optional agent escape.
    Exhausted(ExplorerPacket),
}

/// Deterministic explorer memory.
#[derive(Debug, Clone)]
pub struct Explorer {
    budget: ExplorerBudget,
    actions_taken: u32,
    last_actions: Vec<String>,
    seen_states: BTreeSet<String>,
    seen_obligations: u32,
    seen_code: u32,
    idle: u32,
}

impl Explorer {
    /// Start with an initial state digest.
    #[must_use]
    pub fn new(budget: ExplorerBudget, initial_state: impl Into<String>) -> Self {
        let mut seen_states = BTreeSet::new();
        seen_states.insert(initial_state.into());
        Self {
            budget,
            actions_taken: 0,
            last_actions: Vec::new(),
            seen_states,
            seen_obligations: 0,
            seen_code: 0,
            idle: 0,
        }
    }

    /// Score one control. Higher is better.
    #[must_use]
    pub fn score(control: &SemanticControl) -> i64 {
        let mut score = 0_i64;
        if control.uncovers_obligation {
            score += 100;
        }
        if control.novel_state {
            score += 50;
        }
        score += i64::from(control.risk).saturating_mul(10);
        if control.boundary {
            score += 5;
        }
        if control.historical {
            score += 5;
        }
        if control.already_covered {
            score -= 80;
        }
        score - i64::try_from(control.setup_cost).unwrap_or(i64::MAX)
    }

    /// Whether the last `tarpit_after` actions added nothing.
    #[must_use]
    pub fn is_tarpit(&self) -> bool {
        self.budget.tarpit_after > 0 && self.idle >= self.budget.tarpit_after
    }

    /// Choose the next control or emit an escape packet after exhaustion.
    pub fn step(
        &mut self,
        controls: &[SemanticControl],
        current_state: &str,
        uncovered_obligation: Option<&str>,
    ) -> ExplorerDecision {
        if self.actions_taken >= self.budget.max_actions || self.is_tarpit() {
            return ExplorerDecision::Exhausted(self.packet(
                controls,
                current_state,
                uncovered_obligation,
            ));
        }
        let mut ranked: Vec<&SemanticControl> = controls.iter().collect();
        ranked.sort_by(|left, right| {
            Self::score(right)
                .cmp(&Self::score(left))
                .then_with(|| left.id.cmp(&right.id))
        });
        let Some(pick) = ranked
            .iter()
            .find(|item| !self.last_actions.iter().any(|taken| taken == &item.id))
        else {
            return ExplorerDecision::Exhausted(self.packet(
                controls,
                current_state,
                uncovered_obligation,
            ));
        };
        let novel = self.seen_states.insert(current_state.to_owned());
        let mut gained = novel || pick.novel_state;
        if pick.uncovers_obligation {
            self.seen_obligations = self.seen_obligations.saturating_add(1);
            gained = true;
        }
        if pick.risk > 0 {
            self.seen_code = self.seen_code.saturating_add(1);
            gained = true;
        }
        if gained {
            self.idle = 0;
        } else {
            self.idle = self.idle.saturating_add(1);
        }
        self.actions_taken = self.actions_taken.saturating_add(1);
        self.last_actions.push(pick.id.clone());
        if self.last_actions.len() > 5 {
            self.last_actions.remove(0);
        }
        ExplorerDecision::Act(pick.id.clone())
    }

    fn packet(
        &self,
        controls: &[SemanticControl],
        current_state: &str,
        uncovered_obligation: Option<&str>,
    ) -> ExplorerPacket {
        let mut top: Vec<&SemanticControl> = controls.iter().collect();
        top.sort_by(|left, right| Self::score(right).cmp(&Self::score(left)));
        ExplorerPacket {
            goal: "explore after deterministic exhaustion".into(),
            uncovered_obligation: uncovered_obligation.map(ToOwned::to_owned),
            state_digest: current_state.to_owned(),
            top_controls: top
                .into_iter()
                .take(3)
                .map(|item| item.id.clone())
                .collect(),
            last_actions: self.last_actions.clone(),
            failed_candidates: vec![
                "budget".into(),
                "tarpit".into(),
                "no_untried_control".into(),
            ],
            runtime_tokens: 0,
        }
    }
}

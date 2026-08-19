//! Spec §26 AI Cost Firewall.
//!
//! The invariant is that an ordinary green path spends zero runtime tokens. A
//! refused decision never silently escalates to a bigger model: it becomes
//! `HUMAN_REQUIRED` with reason [`AI_BUDGET_EXHAUSTED`].

use thiserror::Error;

use crate::verdict::ProofVerdict;

/// Reason attached to the `HUMAN_REQUIRED` verdict this firewall produces.
pub const AI_BUDGET_EXHAUSTED: &str = "AI_BUDGET_EXHAUSTED";

/// Per-change or per-run ceiling. Spec §26.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AiBudget {
    /// Tokens allowed for compiling plans, obligations, and relations.
    pub planning_tokens: u64,
    /// Tokens allowed during execution. Zero on the ordinary green path.
    pub runtime_tokens: u64,
    /// Bounded semantic escapes from the deterministic browser explorer.
    pub browser_escape_calls: u32,
    /// Vision calls on unresolved cropped regions.
    pub vision_calls: u32,
    /// Optional hard money ceiling in micros.
    pub max_cost_micros: Option<u64>,
}

/// Which ceiling a call is charged against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCallKind {
    /// Compile-time reasoning. Rare.
    Planning,
    /// Runtime reasoning during execution.
    Runtime,
    /// Browser escape. Also charged against the runtime ceiling.
    BrowserEscape,
    /// Vision call. Also charged against the runtime ceiling.
    Vision,
}

/// One AI decision requesting budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiCall {
    /// Which ceiling applies.
    pub kind: AiCallKind,
    /// Tokens the decision would consume.
    pub tokens: u64,
    /// Money the decision would consume, in micros.
    pub cost_micros: u64,
}

impl AiCall {
    /// A call of `kind` costing `tokens` and no tracked money.
    #[must_use]
    pub fn tokens(kind: AiCallKind, tokens: u64) -> Self {
        Self {
            kind,
            tokens,
            cost_micros: 0,
        }
    }
}

/// Spent so far. Telemetry axes from spec §26.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AiUsage {
    /// Planning tokens consumed.
    pub planning_tokens: u64,
    /// Runtime tokens consumed. Stays zero on the green path.
    pub runtime_tokens: u64,
    /// Browser escapes taken.
    pub browser_escape_calls: u32,
    /// Vision calls taken.
    pub vision_calls: u32,
    /// Money consumed, in micros.
    pub cost_micros: u64,
}

impl AiUsage {
    /// Planning plus runtime tokens.
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.planning_tokens.saturating_add(self.runtime_tokens)
    }
}

/// Which ceiling stopped a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetLimit {
    /// Planning token ceiling.
    PlanningTokens,
    /// Runtime token ceiling.
    RuntimeTokens,
    /// Browser escape call ceiling.
    BrowserEscapeCalls,
    /// Vision call ceiling.
    VisionCalls,
    /// Money ceiling.
    Cost,
}

impl BudgetLimit {
    /// Stable token for transport and logs.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlanningTokens => "planning_tokens",
            Self::RuntimeTokens => "runtime_tokens",
            Self::BrowserEscapeCalls => "browser_escape_calls",
            Self::VisionCalls => "vision_calls",
            Self::Cost => "max_cost_micros",
        }
    }
}

/// A refused AI decision. Nothing was spent and no model was escalated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("{reason}: {} would exceed {allowed} (used {used}, requested {requested})", limit.as_str())]
pub struct BudgetExhausted {
    /// Always [`AI_BUDGET_EXHAUSTED`].
    pub reason: &'static str,
    /// Ceiling that stopped the call.
    pub limit: BudgetLimit,
    /// Configured ceiling.
    pub allowed: u64,
    /// Already consumed before this call.
    pub used: u64,
    /// What the call asked for.
    pub requested: u64,
}

/// QA tokens measured against development tokens. Integer percent, never a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenRatio {
    /// QA tokens spent.
    pub qa_tokens: u64,
    /// Development tokens supplied by the host.
    pub development_tokens: u64,
    /// `qa_tokens` as a percentage of `development_tokens`, rounded down.
    pub percent: u64,
}

impl TokenRatio {
    /// Whether the ratio is within `max_percent`.
    #[must_use]
    pub fn within(&self, max_percent: u64) -> bool {
        self.percent <= max_percent
    }
}

/// Enforces [`AiBudget`] and records [`AiUsage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCostFirewall {
    budget: AiBudget,
    usage: AiUsage,
    exhausted: bool,
}

impl AiCostFirewall {
    /// Start with a ceiling and no usage.
    #[must_use]
    pub fn new(budget: AiBudget) -> Self {
        Self {
            budget,
            usage: AiUsage::default(),
            exhausted: false,
        }
    }

    /// Configured ceiling.
    #[must_use]
    pub fn budget(&self) -> AiBudget {
        self.budget
    }

    /// Usage so far.
    #[must_use]
    pub fn usage(&self) -> AiUsage {
        self.usage
    }

    /// Whether any decision has been refused.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// `HUMAN_REQUIRED` once a decision was refused; otherwise no opinion.
    #[must_use]
    pub fn verdict(&self) -> Option<ProofVerdict> {
        if self.exhausted {
            Some(ProofVerdict::HumanRequired)
        } else {
            None
        }
    }

    /// Charge one call.
    ///
    /// On rejection nothing is recorded, the firewall latches exhausted, and the
    /// caller must escalate to a human rather than to another model.
    ///
    /// # Errors
    ///
    /// Returns [`BudgetExhausted`] when any applicable ceiling would be crossed.
    pub fn charge(&mut self, call: &AiCall) -> Result<(), BudgetExhausted> {
        self.check_call_ceiling(call)?;
        self.check_token_ceiling(call)?;
        self.check_cost_ceiling(call)?;
        self.record(call);
        Ok(())
    }

    /// QA-to-development token ratio.
    ///
    /// Returns `None` when no development data was supplied: missing evidence is
    /// never reported as a measured ratio.
    #[must_use]
    pub fn ratio(&self, development_tokens: u64) -> Option<TokenRatio> {
        if development_tokens == 0 {
            return None;
        }
        let qa_tokens = self.usage.total_tokens();
        Some(TokenRatio {
            qa_tokens,
            development_tokens,
            percent: qa_tokens.saturating_mul(100) / development_tokens,
        })
    }

    fn check_call_ceiling(&mut self, call: &AiCall) -> Result<(), BudgetExhausted> {
        let (limit, allowed, used) = match call.kind {
            AiCallKind::BrowserEscape => (
                BudgetLimit::BrowserEscapeCalls,
                self.budget.browser_escape_calls,
                self.usage.browser_escape_calls,
            ),
            AiCallKind::Vision => (
                BudgetLimit::VisionCalls,
                self.budget.vision_calls,
                self.usage.vision_calls,
            ),
            AiCallKind::Planning | AiCallKind::Runtime => return Ok(()),
        };
        if used >= allowed {
            return Err(self.refuse(limit, u64::from(allowed), u64::from(used), 1));
        }
        Ok(())
    }

    fn check_token_ceiling(&mut self, call: &AiCall) -> Result<(), BudgetExhausted> {
        let (limit, allowed, used) = match call.kind {
            AiCallKind::Planning => (
                BudgetLimit::PlanningTokens,
                self.budget.planning_tokens,
                self.usage.planning_tokens,
            ),
            AiCallKind::Runtime | AiCallKind::BrowserEscape | AiCallKind::Vision => (
                BudgetLimit::RuntimeTokens,
                self.budget.runtime_tokens,
                self.usage.runtime_tokens,
            ),
        };
        if used.saturating_add(call.tokens) > allowed {
            return Err(self.refuse(limit, allowed, used, call.tokens));
        }
        Ok(())
    }

    fn check_cost_ceiling(&mut self, call: &AiCall) -> Result<(), BudgetExhausted> {
        let Some(allowed) = self.budget.max_cost_micros else {
            return Ok(());
        };
        let used = self.usage.cost_micros;
        if used.saturating_add(call.cost_micros) > allowed {
            return Err(self.refuse(BudgetLimit::Cost, allowed, used, call.cost_micros));
        }
        Ok(())
    }

    fn record(&mut self, call: &AiCall) {
        match call.kind {
            AiCallKind::Planning => {
                self.usage.planning_tokens = self.usage.planning_tokens.saturating_add(call.tokens);
            }
            AiCallKind::Runtime => {
                self.usage.runtime_tokens = self.usage.runtime_tokens.saturating_add(call.tokens);
            }
            AiCallKind::BrowserEscape => {
                self.usage.runtime_tokens = self.usage.runtime_tokens.saturating_add(call.tokens);
                self.usage.browser_escape_calls = self.usage.browser_escape_calls.saturating_add(1);
            }
            AiCallKind::Vision => {
                self.usage.runtime_tokens = self.usage.runtime_tokens.saturating_add(call.tokens);
                self.usage.vision_calls = self.usage.vision_calls.saturating_add(1);
            }
        }
        self.usage.cost_micros = self.usage.cost_micros.saturating_add(call.cost_micros);
    }

    fn refuse(
        &mut self,
        limit: BudgetLimit,
        allowed: u64,
        used: u64,
        requested: u64,
    ) -> BudgetExhausted {
        self.exhausted = true;
        BudgetExhausted {
            reason: AI_BUDGET_EXHAUSTED,
            limit,
            allowed,
            used,
            requested,
        }
    }
}

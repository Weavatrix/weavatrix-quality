//! Inherent LiveService model command.

use super::super::access::*;
use super::super::persist_run::make_ai_usage_id;
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn model(&self, cmd: &ModelCommand) -> Result<ModelReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let kind = parse_model_kind(&cmd.kind)?;
        let mut policy = load_model_policy(&self.repo)?;
        if let Some(hints) = load_quality_contract(&self.repo, &compiled.change)?.ai {
            policy.budget.planning_tokens =
                policy.budget.planning_tokens.min(hints.planning_tokens);
            policy.budget.runtime_tokens = policy.budget.runtime_tokens.min(hints.runtime_tokens);
        }
        let store = self.store()?;
        let persisted = store
            .ai_usage_for_change(&compiled.change)
            .map_err(|err| BusError::Store(err.to_string()))?
            .unwrap_or_default();
        let usage = AiUsage {
            planning_tokens: persisted.planning_tokens,
            runtime_tokens: persisted.runtime_tokens,
            browser_escape_calls: u32::try_from(persisted.browser_escape_calls).map_err(|_| {
                BusError::Store("persisted browser escape count exceeds u32".into())
            })?,
            vision_calls: u32::try_from(persisted.vision_calls)
                .map_err(|_| BusError::Store("persisted vision call count exceeds u32".into()))?,
            cost_micros: persisted.cost_micros,
        };
        let mut firewall = AiCostFirewall::with_usage(policy.budget, usage);
        let reply = call_local_model(
            &policy.model,
            &LocalModelRequest {
                kind,
                prompt: cmd.prompt.clone(),
            },
            &mut firewall,
        )
        .map_err(|err| BusError::Model(err.to_string()))?;
        let total_tokens = reply.input_tokens.saturating_add(reply.output_tokens);
        let (planning_tokens, runtime_tokens, browser_escape_calls, vision_calls) = match kind {
            AiCallKind::Planning => (total_tokens, 0, 0, 0),
            AiCallKind::Runtime => (0, total_tokens, 0, 0),
            AiCallKind::BrowserEscape => (0, total_tokens, 1, 0),
            AiCallKind::Vision => (0, total_tokens, 0, 1),
        };
        store
            .put_ai_usage(
                &make_ai_usage_id(&compiled.change, &cmd.kind)?,
                &StoredAiUsage {
                    change_id: compiled.change.clone(),
                    run_id: None,
                    planning_tokens,
                    runtime_tokens,
                    browser_escape_calls,
                    vision_calls,
                    cost_micros: reply.cost_micros,
                },
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        Ok(ModelReply {
            change: compiled.change,
            kind: cmd.kind.clone(),
            model: reply.model,
            text: reply.text,
            input_tokens: reply.input_tokens,
            output_tokens: reply.output_tokens,
            cost_micros: reply.cost_micros,
        })
    }
}

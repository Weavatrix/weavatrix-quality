//! Inherent LiveService author draft, validate, and preview commands.

use super::super::access::*;
use super::super::protection_snapshot::ensure_complete_diff;
use super::LiveService;
use crate::replies::bound_items;

impl LiveService {
    pub(in crate::service) fn author_draft(
        &self,
        cmd: &AuthorDraftCommand,
    ) -> Result<AuthorDraftReply, BusError> {
        validate_authoring_budget(cmd.token_budget)?;
        let compiled = self.compiled(&cmd.change)?;
        let range = self.revision_range(&cmd.base, &cmd.head)?;
        let changed = changed_files(&self.repo, &range)?;
        if changed.is_empty() {
            return Err(BusError::Intelligence(format!(
                "revision range `{}` -> `{}` contains no changed code to author against",
                cmd.base, cmd.head
            )));
        }
        let revision = self.revision()?;
        let graph_diff = self.weavatrix_operation(
            &revision,
            "graph_diff",
            &json!({
                "base_ref": range.merge_base,
                "detail": "edges",
                "max_results": 100_000
            }),
        )?;
        ensure_complete_diff(&graph_diff)?;
        let change_impact = self.weavatrix_operation(
            &revision,
            "change_impact",
            &json!({
                "base_ref": range.merge_base,
                "depth": 6,
                "max_nodes": 100_000,
                "precision": "graph"
            }),
        )?;
        if change_impact
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(BusError::Intelligence(
                "change_impact was truncated; refusing a partial authoring packet".into(),
            ));
        }

        let changed_files = changed.all();
        let obligations = authoring_obligations(&compiled.obligations)?;
        let authority_tokens = authoring_authority_tokens(&changed_files, &obligations)?;
        if authority_tokens >= cmd.token_budget {
            return Err(BusError::Runtime(format!(
                "authoring token budget {} cannot contain the complete sealed authority (needs more than {authority_tokens})",
                cmd.token_budget
            )));
        }
        let context_items =
            authoring_context(&compiled.spec, &changed_files, &graph_diff, &change_impact);
        let (context, context_tokens, truncated) = bound_items(
            context_items,
            cmd.token_budget.saturating_sub(authority_tokens),
        );
        let mut reply = AuthorDraftReply {
            change: compiled.change.clone(),
            revision: revision.to_string(),
            base: range.base_ref,
            head: range.head_ref,
            changed_files,
            context,
            obligations,
            truncated,
            tokens_used: authority_tokens.saturating_add(context_tokens),
            token_budget: cmd.token_budget,
            candidate: None,
            model_usage: None,
        };

        if cmd.use_model {
            let model = self.model(&ModelCommand {
                change: compiled.change.clone(),
                kind: "planning".into(),
                prompt: authoring_model_prompt(&reply)?,
            })?;
            let candidate: Value = serde_json::from_str(&model.text).map_err(|err| {
                BusError::Model(format!(
                    "authoring model did not return one strict TestProgram JSON object: {err}"
                ))
            })?;
            let validated = validate_author_candidate(&self.repo, &compiled, &candidate)?;
            reply.candidate = Some(
                serde_json::to_value(&validated.program)
                    .map_err(|err| BusError::Runtime(err.to_string()))?,
            );
            reply.model_usage = Some(AuthorModelUsage {
                model: model.model,
                input_tokens: model.input_tokens,
                output_tokens: model.output_tokens,
                cost_micros: model.cost_micros,
            });
        }
        Ok(reply)
    }

    pub(in crate::service) fn author_validate(
        &self,
        cmd: &AuthorValidateCommand,
    ) -> Result<AuthorValidateReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let validated = validate_author_candidate(&self.repo, &compiled, &cmd.program)?;
        let program = serde_json::to_value(&validated.program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        Ok(AuthorValidateReply {
            change: compiled.change,
            seal_id: validated.seal_id,
            program_id: validated.program.id.to_string(),
            program,
            obligations: validated
                .program
                .obligations
                .iter()
                .map(ToString::to_string)
                .collect(),
            valid: true,
            persisted: false,
        })
    }

    pub(in crate::service) fn author_preview_controlled(
        &self,
        cmd: &AuthorPreviewCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorPreviewReply, BusError> {
        let compiled = self.compiled(&cmd.change)?;
        let _range = self.revision_range(&cmd.base, &cmd.head)?;
        let before = self.revision()?;
        let validated = validate_author_candidate(&self.repo, &compiled, &cmd.program)?;
        let canonical_program = validated.program.clone();
        let canonical_program_body = serde_json::to_vec(&canonical_program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let seal_id = validated.seal_id.clone();
        let policy = load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
            BusError::Runtime(
                "authoring preview requires a browser runtime in .weavatrix-quality/config.yaml"
                    .into(),
            )
        })?;
        let mut program = validated.program;
        program.evidence_policy.screenshot = if cmd.screenshot {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        program.evidence_policy.trace = if cmd.trace {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        let preview_token = author_preview_token(program.id.as_str())?;
        let evidence_dir = self
            .repo
            .join(".weavatrix-quality")
            .join("authoring-evidence")
            .join(&preview_token);
        let result = run_browser_program(
            &BrowserRunConfig {
                base_url: policy.base_url,
                browser: policy.browser,
                headless: policy.headless,
                timeout: policy.timeout,
                module_root: policy.module_root,
                runtime_dir: self
                    .repo
                    .join(".weavatrix-quality/runtime/playwright-runner"),
                evidence_dir: evidence_dir.clone(),
                viewport: None,
                // Authoring exercises one candidate program in isolation; UI
                // integrity is a base/head comparison with nothing to compare.
                ui_integrity: None,
                network: policy.network,
                cancel,
            },
            &program,
            &validated.oracles,
        )
        .map_err(|err| BusError::Runtime(err.to_string()))?;
        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during authoring preview: `{before}` -> `{after}`"
            )));
        }
        let store = self.store()?;
        let persisted = persist_author_preview(&store, &preview_token, &result)?;
        store
            .put_authoring_preview(
                &preview_token,
                canonical_program.id.as_str(),
                &compiled.change,
                before.as_str(),
                &seal_id,
                result.passed,
                &canonical_program_body,
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        let _ = std::fs::remove_dir(&evidence_dir);
        Ok(AuthorPreviewReply {
            preview_id: preview_token,
            change: compiled.change,
            revision: before.to_string(),
            program_id: program.id.to_string(),
            passed: result.passed,
            asserted: result.asserted,
            contradicted: result.contradicted,
            failure: result.failure,
            observation_handles: persisted.observation_handles,
            screenshot_handles: persisted.screenshot_handles,
            trace_handle: persisted.trace_handle,
            program_persisted: false,
        })
    }
}

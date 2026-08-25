//! Inherent LiveService author promote and heal commands.

use super::super::access::*;
use super::LiveService;

impl LiveService {
    pub(in crate::service) fn author_promote(
        &self,
        cmd: &AuthorPromoteCommand,
    ) -> Result<AuthorPromoteReply, BusError> {
        if cmd.preview_id.trim().is_empty() {
            return Err(BusError::InvalidInput(
                "preview_id must not be empty".into(),
            ));
        }
        let compiled = self.compiled(&cmd.change)?;
        let repository_revision = self.revision()?;
        let validated = validate_author_candidate(&self.repo, &compiled, &cmd.program)?;
        let program_body = serde_json::to_vec(&validated.program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let mut store = self.store()?;
        let (program_revision, created) = store
            .promote_authoring_preview(
                &cmd.preview_id,
                validated.program.id.as_str(),
                &compiled.change,
                repository_revision.as_str(),
                &validated.seal_id,
                &program_body,
            )
            .map_err(map_authoring_store_error)?;
        Ok(AuthorPromoteReply {
            change: compiled.change,
            revision: repository_revision.to_string(),
            seal_id: validated.seal_id,
            program_id: validated.program.id.to_string(),
            program_revision,
            persisted: true,
            created,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub(in crate::service) fn author_heal_controlled(
        &self,
        cmd: &AuthorHealCommand,
        cancel: Arc<AtomicBool>,
    ) -> Result<AuthorHealReply, BusError> {
        if cmd.program_id.trim().is_empty() || cmd.expected_program_revision == 0 {
            return Err(BusError::InvalidInput(
                "healing requires a program id and positive expected revision".into(),
            ));
        }
        if cmd.edits.is_empty() || cmd.edits.len() > 64 {
            return Err(BusError::InvalidInput(
                "healing requires between 1 and 64 bounded edits".into(),
            ));
        }
        let compiled = self.compiled(&cmd.change)?;
        let _range = self.revision_range(&cmd.base, &cmd.head)?;
        let before = self.revision()?;
        let mut store = self.store()?;
        let latest = store
            .latest_program_revision(&cmd.program_id)
            .map_err(|err| BusError::Store(err.to_string()))?
            .ok_or_else(|| BusError::NotFound(format!("browser TestProgram {}", cmd.program_id)))?;
        if latest != cmd.expected_program_revision {
            return Err(BusError::Ambiguous(format!(
                "program revision changed: expected {}, latest is {latest}",
                cmd.expected_program_revision
            )));
        }
        let (stored, body) = store
            .read_program_revision(&cmd.program_id, latest)
            .map_err(|err| BusError::Store(err.to_string()))?
            .ok_or_else(|| {
                BusError::NotFound(format!(
                    "browser TestProgram {} revision {latest}",
                    cmd.program_id
                ))
            })?;
        if stored.change_id != compiled.change {
            return Err(BusError::InvalidInput(
                "healing cannot move a program to another change".into(),
            ));
        }
        let candidate: Value = serde_json::from_slice(&body).map_err(|err| {
            BusError::Store(format!(
                "stored TestProgram {} revision {latest} is malformed: {err}",
                cmd.program_id
            ))
        })?;
        let validated = validate_author_candidate(&self.repo, &compiled, &candidate)?;
        if stored.seal != validated.seal_id {
            return Err(BusError::InvalidInput(
                "OracleSeal changed since promotion; a contradiction is not healable".into(),
            ));
        }
        let stored_seal =
            OracleSealId::new(&stored.seal).map_err(|err| BusError::Identity(err.to_string()))?;
        let current_seal = OracleSealId::new(&validated.seal_id)
            .map_err(|err| BusError::Identity(err.to_string()))?;
        let edits = cmd
            .edits
            .iter()
            .cloned()
            .map(|edit| match edit {
                AuthorHealEdit::Retarget { step, target } => HealEdit::Retarget { step, target },
                AuthorHealEdit::InsertWait { after, condition } => {
                    HealEdit::InsertWait { after, condition }
                }
            })
            .collect::<Vec<_>>();
        let healed = apply_heal(
            &validated.program,
            &stored_seal,
            &current_seal,
            &edits,
            latest,
        )
        .map_err(|err| BusError::InvalidInput(err.to_string()))?;
        let canonical_program = healed.program;
        let canonical_program_body = serde_json::to_vec(&canonical_program)
            .map_err(|err| BusError::Runtime(err.to_string()))?;
        let policy = load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
            BusError::Runtime(
                "safe healing requires a browser runtime in .weavatrix-quality/config.yaml".into(),
            )
        })?;
        let mut executable = canonical_program.clone();
        executable.evidence_policy.screenshot = if cmd.screenshot {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        executable.evidence_policy.trace = if cmd.trace {
            CaptureWhen::Always
        } else {
            CaptureWhen::Never
        };
        let preview_token = author_preview_token(&format!("heal-{}", cmd.program_id))?;
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
            &executable,
            &validated.oracles,
        )
        .map_err(|err| BusError::Runtime(err.to_string()))?;
        let after = self.revision()?;
        if before != after {
            return Err(BusError::Ambiguous(format!(
                "repository revision changed during safe-healing replay: `{before}` -> `{after}`"
            )));
        }
        let persisted = persist_author_preview(&store, &preview_token, &result)?;
        store
            .put_authoring_preview(
                &preview_token,
                canonical_program.id.as_str(),
                &compiled.change,
                before.as_str(),
                &validated.seal_id,
                result.passed,
                &canonical_program_body,
            )
            .map_err(|err| BusError::Store(err.to_string()))?;
        let (program_revision, created) = if result.passed {
            let (revision, created) = store
                .heal_authoring_preview(
                    &preview_token,
                    canonical_program.id.as_str(),
                    latest,
                    &compiled.change,
                    before.as_str(),
                    &validated.seal_id,
                    &canonical_program_body,
                )
                .map_err(map_authoring_store_error)?;
            (Some(revision), created)
        } else {
            (None, false)
        };
        let did_persist = program_revision.is_some();
        let _ = std::fs::remove_dir(&evidence_dir);
        Ok(AuthorHealReply {
            preview_id: preview_token,
            change: compiled.change,
            revision: before.to_string(),
            seal_id: validated.seal_id,
            program_id: canonical_program.id.to_string(),
            previous_program_revision: latest,
            program_revision,
            passed: result.passed,
            asserted: result.asserted,
            contradicted: result.contradicted,
            failure: result.failure,
            observation_handles: persisted.observation_handles,
            screenshot_handles: persisted.screenshot_handles,
            trace_handle: persisted.trace_handle,
            persisted: did_persist,
            created,
        })
    }
}

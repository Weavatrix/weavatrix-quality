//! Replay browser programs on base and measure UI at a viewport.

use super::super::access::*;
use super::super::persist_evidence::cap_browser_evidence;
use super::super::persist_ui_analyse::analyse_ui_snapshots;
use super::LiveService;

impl LiveService {
    /// Replay the exact head-selected programs against the merge-base runtime.
    ///
    /// The runtime coordinates (notably `base_url`) come from the base config,
    /// while program steps, seed, evidence policy, and sealed oracles are the
    /// exact head values already executed. This prevents a changed test from
    /// making the two sides incomparable and avoids treating preview origins
    /// as product behavior.
    pub(in crate::service) fn replay_base_browser_programs(
        &self,
        range: &RevisionRange,
        change: &str,
        head_policy: &BrowserPolicy,
        head_runs: &[(&ConfiguredBrowserProgram, BrowserProgramRun)],
        ui_policy: &UiIntegrityPolicy,
        run_evidence_policy: &str,
    ) -> Result<BaseBrowserReplay, BusError> {
        let worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let revision = WeavatrixProvider
            .analyze(&worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?
            .revision;
        let spec = optional_change(&worktree.path, change)?;
        let base_runtime =
            load_browser_runtime_with(&worktree.path, Some(head_policy.module_root.as_path()))?
                .ok_or_else(|| {
                    BusError::Runtime("merge base has no browser runtime configuration".into())
                })?;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut runs = Vec::new();
        for (configured, _) in head_runs {
            let mut executable = configured.program.clone();
            cap_browser_evidence(&mut executable, run_evidence_policy);
            // Binary capture is not an input to structured comparison and its
            // timestamped paths are not behavior. The paired replay keeps the
            // exact actions, seed, oracles, network, console, and storage
            // policy while avoiding orphaned base-worktree files.
            executable.evidence_policy.screenshot = CaptureWhen::Never;
            executable.evidence_policy.trace = CaptureWhen::Never;
            let result = run_browser_program_at(
                &BrowserRunConfig {
                    base_url: base_runtime.base_url.clone(),
                    browser: base_runtime.browser.clone(),
                    headless: base_runtime.headless,
                    timeout: base_runtime.timeout,
                    module_root: base_runtime.module_root.clone(),
                    runtime_dir: self
                        .repo
                        .join(".weavatrix-quality/runtime/playwright-runner"),
                    evidence_dir: worktree
                        .path
                        .join(".weavatrix-quality/browser-evidence")
                        .join(format!(
                            "delta-base-{}",
                            safe_file_token(configured.program.id.as_str())
                        )),
                    viewport: None,
                    ui_integrity: ui_collection_config(ui_policy, &configured.oracles),
                    network: head_policy.network.clone(),
                    cancel: Arc::clone(&cancel),
                },
                &executable,
                &configured.oracles,
                revision.as_str(),
            )
            .map_err(|err| BusError::Runtime(err.to_string()))?;
            runs.push(result);
        }
        Ok(BaseBrowserReplay {
            revision,
            spec,
            runs,
        })
    }

    /// Replay the configured browser programs at the merge base.
    pub(in crate::service) fn measure_base_ui(
        &self,
        range: &RevisionRange,
        compiled: &Compiled,
        policy: &UiIntegrityPolicy,
    ) -> Result<UiIntegritySnapshot, BusError> {
        // The browser engine is toolchain, not source: a fresh worktree has no
        // node_modules, and replaying base with a different engine would
        // confound the geometry being compared.
        let head_runtime =
            load_browser_policy(&self.repo, &compiled.obligations)?.ok_or_else(|| {
                BusError::Runtime("no browser runtime is configured for this repository".into())
            })?;
        let engine = head_runtime.module_root;
        let comparison_network = head_runtime.network;
        let worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let evidence = WeavatrixProvider
            .analyze(&worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?;
        let Some(browser) =
            load_browser_policy_with(&worktree.path, &compiled.obligations, Some(&engine))?
        else {
            // Base had no browser programs at all, so nothing here can be
            // compared. Report it rather than calling head's findings new.
            return Ok(UiIntegritySnapshot {
                revision: evidence.revision.to_string(),
                truncated: true,
                ..UiIntegritySnapshot::default()
            });
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let mut runs = Vec::new();
        for configured in &browser.programs {
            let result = run_browser_program_at(
                &BrowserRunConfig {
                    base_url: browser.base_url.clone(),
                    browser: browser.browser.clone(),
                    headless: browser.headless,
                    timeout: browser.timeout,
                    module_root: browser.module_root.clone(),
                    // The bridge is materialized once next to the engine.
                    runtime_dir: self
                        .repo
                        .join(".weavatrix-quality/runtime/playwright-runner"),
                    evidence_dir: worktree
                        .path
                        .join(".weavatrix-quality")
                        .join("browser-evidence")
                        .join(safe_file_token(configured.program.id.as_str())),
                    viewport: None,
                    ui_integrity: ui_collection_config(policy, &configured.oracles),
                    network: comparison_network.clone(),
                    cancel: Arc::clone(&cancel),
                },
                &configured.program,
                &configured.oracles,
                evidence.revision.as_str(),
            )
            .map_err(|err| BusError::Runtime(err.to_string()))?;
            runs.push((configured, result));
        }
        let borrowed = runs
            .iter()
            .map(|(configured, result)| (*configured, result.clone()))
            .collect::<Vec<_>>();
        Ok(analyse_ui_snapshots(&evidence.revision, policy, &borrowed)?.snapshot)
    }

    pub(in crate::service) fn measure_ui_at(
        &self,
        repo: &Path,
        revision: &RevisionId,
        browser: &BrowserPolicy,
        policy: &UiIntegrityPolicy,
        viewport: BrowserViewport,
        side: &str,
    ) -> Result<UiIntegritySnapshot, BusError> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut runs = Vec::new();
        for configured in &browser.programs {
            let result = run_browser_program_at(
                &BrowserRunConfig {
                    base_url: browser.base_url.clone(),
                    browser: browser.browser.clone(),
                    headless: browser.headless,
                    timeout: browser.timeout,
                    module_root: browser.module_root.clone(),
                    runtime_dir: self
                        .repo
                        .join(".weavatrix-quality/runtime/playwright-runner"),
                    evidence_dir: repo
                        .join(".weavatrix-quality")
                        .join("browser-evidence")
                        .join(format!(
                            "responsive-{side}-{}-{}",
                            viewport.width,
                            safe_file_token(configured.program.id.as_str())
                        )),
                    viewport: Some(viewport),
                    ui_integrity: ui_collection_config(policy, &configured.oracles),
                    network: browser.network.clone(),
                    cancel: Arc::clone(&cancel),
                },
                &configured.program,
                &configured.oracles,
                revision.as_str(),
            )
            .map_err(|err| BusError::Runtime(err.to_string()))?;
            runs.push((configured, result));
        }
        let borrowed = runs
            .iter()
            .map(|(configured, result)| (*configured, result.clone()))
            .collect::<Vec<_>>();
        Ok(analyse_ui_snapshots(revision, policy, &borrowed)?.snapshot)
    }
}

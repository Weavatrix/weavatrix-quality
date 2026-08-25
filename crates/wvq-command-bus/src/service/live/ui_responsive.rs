//! Responsive UI probes across base and head.

use super::super::access::*;
use super::super::persist_ui::responsive_probe_incomplete;
use super::LiveService;

impl LiveService {
    /// Probe the parsed CSS/container boundaries on base and head, then bisect
    /// only intervals whose measured finding sets disagree.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::service) fn measure_responsive_ui(
        &self,
        range: &RevisionRange,
        compiled: &Compiled,
        policy: &UiIntegrityPolicy,
        base_default: &UiIntegritySnapshot,
        head_default: &UiIntegritySnapshot,
        previously_fixed: &BTreeSet<String>,
    ) -> Result<(Vec<wvq_ui::ResponsiveFailureInterval>, bool), BusError> {
        let engine = load_browser_policy(&self.repo, &compiled.obligations)?
            .map(|browser| browser.module_root)
            .ok_or_else(|| {
                BusError::Runtime("no browser runtime is configured for this repository".into())
            })?;
        let base_worktree = TemporaryWorktree::create(&self.repo, &range.merge_base)?;
        let base_revision = WeavatrixProvider
            .analyze(&base_worktree.path)
            .map_err(|err| BusError::Intelligence(err.to_string()))?
            .revision;
        let mut base_browser =
            load_browser_policy_with(&base_worktree.path, &compiled.obligations, Some(&engine))?
                .ok_or_else(|| {
                    BusError::Runtime("base has no browser runtime configuration".into())
                })?;
        let head_browser =
            load_browser_policy_with(&self.repo, &compiled.obligations, Some(&engine))?
                .ok_or_else(|| {
                    BusError::Runtime("head has no browser runtime configuration".into())
                })?;
        // Base/head geometry must use the exact same network fixture. Runtime
        // coordinates come from each revision, but the head-selected replay
        // policy is the comparison authority just like the head TestProgram.
        base_browser.network = head_browser.network.clone();
        let head_revision = RevisionId::new(&head_default.revision)
            .map_err(|err| BusError::Identity(err.to_string()))?;

        let breakpoints = base_default
            .responsive_breakpoints
            .union(&head_default.responsive_breakpoints)
            .copied()
            .collect::<BTreeSet<_>>();
        let plan = responsive_probe_plan(&policy.responsive, &breakpoints);
        let mut truncated = plan.truncated
            || base_default.responsive_breakpoints_incomplete
            || head_default.responsive_breakpoints_incomplete;
        let mut probes = Vec::new();
        for width in plan.widths {
            probes.push(self.measure_responsive_probe_with_retry(
                width,
                &base_worktree.path,
                &base_revision,
                &base_browser,
                &head_revision,
                &head_browser,
                policy,
                previously_fixed,
            )?);
        }
        while let Some(width) = next_responsive_probe(&policy.responsive, &probes) {
            probes.push(self.measure_responsive_probe_with_retry(
                width,
                &base_worktree.path,
                &base_revision,
                &base_browser,
                &head_revision,
                &head_browser,
                policy,
                previously_fixed,
            )?);
        }
        probes.sort_by_key(|probe| probe.width);
        truncated |= probes
            .iter()
            .any(|probe| probe.delta.truncated || !probe.delta.unmeasured_states.is_empty());
        let exhaustive_policy = wvq_ui::ResponsivePolicy {
            max_probes: 128,
            ..policy.responsive
        };
        truncated |= next_responsive_probe(&exhaustive_policy, &probes).is_some();
        Ok((
            responsive_failure_intervals(&policy.responsive, &probes),
            truncated,
        ))
    }

    /// A browser can report a bounded transient collection limitation (for
    /// example, one state still settling) even when the repository is static.
    /// Retry that exact width once. A second incomplete measurement is kept and
    /// therefore still fails closed; retries never turn missing evidence into a
    /// pass.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::service) fn measure_responsive_probe_with_retry(
        &self,
        width: u32,
        base_repo: &Path,
        base_revision: &RevisionId,
        base_browser: &BrowserPolicy,
        head_revision: &RevisionId,
        head_browser: &BrowserPolicy,
        policy: &UiIntegrityPolicy,
        previously_fixed: &BTreeSet<String>,
    ) -> Result<ResponsiveProbe, BusError> {
        let first = self.measure_responsive_probe(
            width,
            base_repo,
            base_revision,
            base_browser,
            head_revision,
            head_browser,
            policy,
            previously_fixed,
        )?;
        if !responsive_probe_incomplete(&first) {
            return Ok(first);
        }
        self.measure_responsive_probe(
            width,
            base_repo,
            base_revision,
            base_browser,
            head_revision,
            head_browser,
            policy,
            previously_fixed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::service) fn measure_responsive_probe(
        &self,
        width: u32,
        base_repo: &Path,
        base_revision: &RevisionId,
        base_browser: &BrowserPolicy,
        head_revision: &RevisionId,
        head_browser: &BrowserPolicy,
        policy: &UiIntegrityPolicy,
        previously_fixed: &BTreeSet<String>,
    ) -> Result<ResponsiveProbe, BusError> {
        let viewport = BrowserViewport {
            width,
            height: policy.responsive.height,
        };
        let base = self.measure_ui_at(
            base_repo,
            base_revision,
            base_browser,
            policy,
            viewport,
            "base",
        )?;
        let head = self.measure_ui_at(
            &self.repo,
            head_revision,
            head_browser,
            policy,
            viewport,
            "head",
        )?;
        Ok(ResponsiveProbe {
            width,
            delta: ratchet_ui(&base, &head, previously_fixed, policy),
        })
    }
}

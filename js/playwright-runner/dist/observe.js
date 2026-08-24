/** Structured observation. Screenshots follow EvidencePolicy only. No AI. */
function allowed(when, failed) {
    if (when === "always")
        return true;
    if (when === "on_failure")
        return failed;
    return false;
}
export function filterObservation(observation, policy, failed) {
    const next = { ...observation };
    if (!allowed(policy.screenshot, failed)) {
        delete next.screenshot_handle;
        delete next.screenshot_path;
    }
    if (!allowed(policy.network, failed)) {
        next.network = [];
        next.network_requests = [];
        next.network_requests_truncated = false;
    }
    if (!allowed(policy.console, failed))
        next.console = [];
    if (!allowed(policy.storage, failed)) {
        next.storage = {};
        next.storage_available = false;
    }
    return next;
}

/** Structured observation. Screenshots follow EvidencePolicy only. No AI. */

export type CaptureWhen = "never" | "on_failure" | "always";

export type EvidencePolicy = {
  screenshot: CaptureWhen;
  trace: CaptureWhen;
  network: CaptureWhen;
  console: CaptureWhen;
  storage: CaptureWhen;
};

export type Observation = {
  route?: string;
  a11y_digest?: string;
  network: string[];
  network_requests: Array<{
    sequence: number;
    method: string;
    url: string;
    status?: number;
    resource_type?: string;
  }>;
  network_requests_truncated: boolean;
  console: string[];
  storage: Record<string, string>;
  storage_available?: boolean;
  viewport?: string;
  screenshot_handle?: string;
  screenshot_path?: string;
};

function allowed(when: CaptureWhen, failed: boolean): boolean {
  if (when === "always") return true;
  if (when === "on_failure") return failed;
  return false;
}

export function filterObservation(
  observation: Observation,
  policy: EvidencePolicy,
  failed: boolean,
): Observation {
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
  if (!allowed(policy.console, failed)) next.console = [];
  if (!allowed(policy.storage, failed)) {
    next.storage = {};
    next.storage_available = false;
  }
  return next;
}

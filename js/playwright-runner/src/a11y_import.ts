/** Collect axe-core / Storybook a11y JSON without keeping HTML. */

import type { Page } from "playwright";

const MAX_VIOLATIONS = 128;
const MAX_NODES = 8;
const MAX_TOKEN = 120;

export type A11yImportReport = {
  producer: "axe-core" | "storybook-a11y";
  /** True when the producer returned more violations or nodes than we keep. */
  truncated?: boolean;
  violations: Array<{
    id: string;
    impact?: string;
    nodes: Array<{ target: string[] }>;
  }>;
};

/** Axe/Storybook is optional. Failure is not the same as absence. */
export type A11yImportOutcome =
  | { status: "absent" }
  | { status: "failed"; error: string }
  | { status: "imported"; report: A11yImportReport };

function bound(raw: unknown): string {
  if (typeof raw !== "string") return "";
  const collapsed = raw.replace(/\s+/g, " ").trim();
  return collapsed.length > MAX_TOKEN ? collapsed.slice(0, MAX_TOKEN) : collapsed;
}

function sanitize(raw: unknown, producer: A11yImportReport["producer"]): A11yImportReport | undefined {
  if (!raw || typeof raw !== "object") return undefined;
  const record = raw as Record<string, unknown>;
  const nested =
    record.results && typeof record.results === "object"
      ? (record.results as Record<string, unknown>).violations
      : undefined;
  const list = Array.isArray(record.violations)
    ? record.violations
    : Array.isArray(nested)
      ? nested
      : [];
  let truncated = list.length > MAX_VIOLATIONS;
  const violations = list.slice(0, MAX_VIOLATIONS).flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const violation = item as Record<string, unknown>;
    const id = bound(violation.id);
    if (!id) return [];
    const nodesRaw = Array.isArray(violation.nodes) ? violation.nodes : [];
    if (nodesRaw.length > MAX_NODES) truncated = true;
    const nodes = nodesRaw.slice(0, MAX_NODES).map((node) => {
      const targets = node && typeof node === "object" && Array.isArray((node as { target?: unknown }).target)
        ? (node as { target: unknown[] }).target.map(bound).filter(Boolean)
        : [];
      return { target: targets };
    });
    const impact = bound(violation.impact);
    return impact
      ? [{ id, impact, nodes }]
      : [{ id, nodes }];
  });
  return truncated ? { producer, truncated: true, violations } : { producer, violations };
}

/**
 * Run axe if the page already loaded it (Storybook addon-a11y or a project
 * script). WVQ does not vendor axe-core. Absence is not a failure.
 */
export async function collectA11yImport(page: Page): Promise<A11yImportOutcome> {
  try {
    const raw = await page.evaluate(async () => {
      const axe = (globalThis as { axe?: { run?: (context: unknown, options: unknown) => Promise<unknown> } }).axe;
      if (typeof axe?.run !== "function") {
        const storybook = (globalThis as { __STORYBOOK_A11Y_RESULT__?: unknown }).__STORYBOOK_A11Y_RESULT__;
        return storybook === undefined ? null : { producer: "storybook-a11y", raw: storybook };
      }
      const result = await axe.run(document, { resultTypes: ["violations"] });
      return { producer: "axe-core", raw: result };
    });
    if (!raw || typeof raw !== "object") return { status: "absent" };
    const envelope = raw as { producer?: string; raw?: unknown };
    const producer = envelope.producer === "storybook-a11y" ? "storybook-a11y" : "axe-core";
    const report = sanitize(envelope.raw, producer);
    if (!report) return { status: "failed", error: `${producer} returned a report that could not be sanitised` };
    return { status: "imported", report };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const bounded = message.replace(/\s+/g, " ").trim().slice(0, MAX_TOKEN);
    return { status: "failed", error: bounded || "a11y producer threw" };
  }
}

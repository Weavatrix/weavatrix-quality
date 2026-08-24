/** Passive semantic recorder. No AI, XPath, request bodies, or raw form values. */

import type { BrowserContext, Page } from "playwright";
import type { Target, TestAction } from "./execute.js";

export type RecorderCapture = {
  action?: TestAction;
  limitation?: string;
};

export type RecorderHooks = {
  capture(capture: RecorderCapture): Promise<void>;
  finish(): void;
};

export type RecorderInstallConfig = {
  fixture_values: Record<string, string>;
  max_events: number;
  test_id_attribute?: string;
};

declare global {
  interface Window {
    __wvqRecorderInstalled?: boolean;
    __wvqRecordEvent?: (capture: RecorderCapture) => Promise<void>;
    __wvqRecordFinish?: () => Promise<void>;
  }
}

/** Install before navigation so natural application use is observed from first paint. */
export async function installSemanticRecorder(
  context: BrowserContext,
  page: Page,
  hooks: RecorderHooks,
  config: RecorderInstallConfig,
): Promise<void> {
  assertRecorderConfig(config);
  await page.exposeBinding("__wvqRecordEvent", async (_source, capture: unknown) => {
    await hooks.capture(assertRecorderCapture(capture));
  });
  await page.exposeBinding("__wvqRecordFinish", async () => hooks.finish());
  await context.addInitScript(pageRecorderInit, config);
  await page.evaluate(pageRecorderInit, config);
}

export function assertRecorderCapture(value: unknown): RecorderCapture {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("recorder capture must be an object");
  }
  const capture = value as RecorderCapture;
  const raw = JSON.stringify(capture);
  if (/xpath/i.test(raw)) throw new Error("XPath is not a recorded identity");
  if (capture.limitation !== undefined && typeof capture.limitation !== "string") {
    throw new Error("recorder limitation must be text");
  }
  if (capture.action !== undefined) assertRecordedAction(capture.action);
  if (!capture.action && !capture.limitation) {
    throw new Error("recorder capture must contain an action or limitation");
  }
  return capture;
}

function assertRecorderConfig(config: RecorderInstallConfig): void {
  if (!Number.isInteger(config.max_events) || config.max_events < 1 || config.max_events > 1_000) {
    throw new Error("recorder max_events must be between 1 and 1000");
  }
  for (const [key, value] of Object.entries(config.fixture_values)) {
    if (!key.trim() || typeof value !== "string") {
      throw new Error("recorder fixture values must have non-empty names and string values");
    }
  }
}

function assertRecordedAction(action: TestAction): void {
  if (!action || typeof action !== "object") throw new Error("recorded action is malformed");
  if (!["activate", "fill", "select", "press"].includes(action.action)) {
    throw new Error(`page recorder emitted unsupported action \`${action.action}\``);
  }
  if ("target" in action && action.target) assertTarget(action.target);
  if ((action.action === "fill" || action.action === "select") && !action.value.trim()) {
    throw new Error("recorded form action must reference a fixture name");
  }
}

function assertTarget(target: Target): void {
  if (target.scope) assertTarget(target.scope);
  if (
    !target.test_id &&
    !target.role &&
    !target.accessible_name &&
    !target.label &&
    !target.component_hint &&
    !target.fallback_css
  ) {
    throw new Error("recorded target needs a semantic identity");
  }
  if (Object.values(target).some((value) => typeof value === "string" && /xpath/i.test(value))) {
    throw new Error("XPath is not a recorded identity");
  }
}

/** Serialized into every same-origin document by Playwright. Keep it closure-free. */
function pageRecorderInit(config: RecorderInstallConfig): void {
  if (window.__wvqRecorderInstalled) return;
  window.__wvqRecorderInstalled = true;
  const fixtureEntries = Object.entries(config.fixture_values);
  const testIdAttribute = config.test_id_attribute || "data-testid";
  let emitted = 0;
  let lastFingerprint = "";
  let lastAt = 0;

  const compact = (value: string | null | undefined): string | undefined => {
    const normalized = value?.replace(/\s+/g, " ").trim();
    return normalized ? normalized.slice(0, 120) : undefined;
  };
  const cssEscape = (value: string): string => {
    if (globalThis.CSS?.escape) return globalThis.CSS.escape(value);
    return value.replace(/[^a-zA-Z0-9_-]/g, (part) => `\\${part}`);
  };
  const roleOf = (element: Element): string | undefined => {
    const explicit = compact(element.getAttribute("role"));
    if (explicit) return explicit;
    const tag = element.tagName.toLowerCase();
    if (tag === "button") return "button";
    if (tag === "a" && element.hasAttribute("href")) return "link";
    if (tag === "select") return "combobox";
    if (tag === "textarea") return "textbox";
    if (tag === "input") {
      const type = (element.getAttribute("type") || "text").toLowerCase();
      if (["button", "submit", "reset"].includes(type)) return "button";
      if (type === "checkbox") return "checkbox";
      if (type === "radio") return "radio";
      return "textbox";
    }
    if (tag === "dialog") return "dialog";
    return undefined;
  };
  const labelOf = (element: Element): string | undefined => {
    const aria = compact(element.getAttribute("aria-label"));
    if (aria) return aria;
    const labelledBy = element.getAttribute("aria-labelledby");
    if (labelledBy) {
      const value = labelledBy
        .split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent || "")
        .join(" ");
      const compacted = compact(value);
      if (compacted) return compacted;
    }
    if (element instanceof HTMLInputElement || element instanceof HTMLSelectElement || element instanceof HTMLTextAreaElement) {
      const labels = Array.from(element.labels || []).map((label) => label.textContent || "").join(" ");
      const compacted = compact(labels);
      if (compacted) return compacted;
    }
    return compact(element.textContent) || compact(element.getAttribute("title"));
  };
  const targetOf = (element: Element, includeScope = true): Target | undefined => {
    const target: Target = {};
    const testId = compact(element.getAttribute(testIdAttribute));
    const role = roleOf(element);
    const name = labelOf(element);
    const component = compact(element.getAttribute("data-component"));
    const id = compact(element.id);
    if (testId) target.test_id = testId;
    if (role) target.role = role;
    if (name) target.accessible_name = name;
    if (component) target.component_hint = component;
    if (!testId && !role && !name && !component && id) target.fallback_css = `#${cssEscape(id)}`;
    if (includeScope) {
      const scope = element.parentElement?.closest(
        `[role="dialog"],dialog,[${testIdAttribute}],[data-component],tr,li`,
      );
      const semanticScope = scope && scope !== element ? targetOf(scope, false) : undefined;
      if (semanticScope) target.scope = semanticScope;
    }
    return Object.keys(target).length > 0 ? target : undefined;
  };
  const semanticElement = (raw: EventTarget | null): Element | undefined => {
    if (!(raw instanceof Element)) return undefined;
    return raw.closest(
      `button,a[href],input,select,textarea,[role],[${testIdAttribute}],[data-component]`,
    ) || undefined;
  };
  const emit = (capture: RecorderCapture): void => {
    if (emitted >= config.max_events) {
      void window.__wvqRecordEvent?.({ limitation: "recording event budget exhausted" });
      void window.__wvqRecordFinish?.();
      return;
    }
    const fingerprint = JSON.stringify(capture);
    const now = Date.now();
    if (fingerprint === lastFingerprint && now - lastAt < 50) return;
    lastFingerprint = fingerprint;
    lastAt = now;
    emitted += 1;
    void window.__wvqRecordEvent?.(capture);
  };
  const fixtureFor = (value: string): string | undefined =>
    fixtureEntries.find(([, fixtureValue]) => fixtureValue === value)?.[0];

  document.addEventListener("click", (event) => {
    const element = semanticElement(event.target);
    if (!element || element instanceof HTMLSelectElement || element instanceof HTMLTextAreaElement) return;
    if (element instanceof HTMLInputElement && !["button", "submit", "reset", "checkbox", "radio"].includes(element.type)) return;
    const target = targetOf(element);
    if (target) emit({ action: { action: "activate", target } });
  }, true);
  document.addEventListener("change", (event) => {
    const element = semanticElement(event.target);
    if (!(element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement)) return;
    const target = targetOf(element);
    if (!target) return;
    const fixture = fixtureFor(element.value);
    if (!fixture) {
      emit({ limitation: `form value for ${target.test_id || target.accessible_name || target.role || "control"} has no named fixture and was not captured` });
      return;
    }
    emit({ action: element instanceof HTMLSelectElement
      ? { action: "select", target, value: fixture }
      : { action: "fill", target, value: fixture } });
  }, true);
  document.addEventListener("keydown", (event) => {
    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "e") {
      event.preventDefault();
      void window.__wvqRecordFinish?.();
      return;
    }
    if (!["Enter", "Escape"].includes(event.key)) return;
    const element = semanticElement(event.target);
    const target = element ? targetOf(element) : undefined;
    emit({ action: target
      ? { action: "press", key: event.key, target }
      : { action: "press", key: event.key } });
  }, true);
}

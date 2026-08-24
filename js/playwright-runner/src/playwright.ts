/** Actual Playwright adapter. Policy and sealed predicates arrive from Rust. */

import { createHash } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import type {
  Browser,
  BrowserContext,
  Locator,
  Page,
  Request,
} from "playwright";
import type { Driver, Target, WaitCondition } from "./execute.js";
import type { EvidencePolicy, Observation } from "./observe.js";
import {
  collectLayoutSnapshot,
  type CollectionResult,
  type UiIntegrityConfig,
} from "./ui_integrity.js";

export type PredicateTarget = Omit<Target, "component_hint">;

export type Predicate =
  | { kind: "visible" | "hidden" | "enabled" | "disabled"; target: PredicateTarget }
  | {
      kind: "text_equals" | "text_contains" | "value_equals";
      target: PredicateTarget;
      value: string;
    }
  | { kind: "route_equals" | "route_contains"; value: string }
  | { kind: "network_response"; method?: string; url_contains: string; status?: number }
  | { kind: "no_console_errors" }
  | { kind: "storage_equals"; area: "local" | "session"; key: string; value: string }
  | { kind: "storage_absent"; area: "local" | "session"; key: string }
  | { kind: "api_status"; operation: string; status: number }
  | { kind: "api_json_equals"; operation: string; pointer: string; value: unknown }
  | { kind: "unique"; target: PredicateTarget }
  | { kind: "max_multiplicity"; target: PredicateTarget; max: number }
  | { kind: "receives_events"; target: PredicateTarget; min_ratio_permille: number }
  | { kind: "inside_viewport"; target: PredicateTarget; margin_px: number }
  | { kind: "text_not_clipped"; target: PredicateTarget }
  | {
      kind: "no_overlap";
      target: PredicateTarget;
      with: PredicateTarget;
      max_ratio_permille: number;
    }
  | { kind: "all" | "any"; predicates: Predicate[] }
  | { kind: "not"; predicate: Predicate };

export type ProgramOracle = {
  obligation: string;
  condition?: Predicate;
  expected: Predicate;
};

export type FaultSpec =
  | { kind: "abort"; url_contains: string }
  | {
      kind: "http_response";
      url_contains: string;
      status: number;
      body?: string;
      headers?: Record<string, string>;
    }
  | { kind: "delay"; url_contains: string; delay_ms: number };

export type ApiOperation = {
  method: string;
  path: string;
  headers?: Record<string, string>;
};

export type BrowserProgram = {
  id: string;
  obligations: string[];
  preconditions?: Array<Record<string, unknown>>;
  steps: Array<Record<string, unknown>>;
  data?: Record<string, unknown>;
  faults?: Record<string, FaultSpec>;
  api_operations?: Record<string, ApiOperation>;
  evidence_policy?: EvidencePolicy;
};

export type BrowserConfig = {
  base_url: string;
  browser?: "chromium" | "firefox" | "webkit";
  headless?: boolean;
  timeout_ms?: number;
  viewport?: { width: number; height: number };
  evidence_dir: string;
};

type ResolvedBrowserConfig = Required<Omit<BrowserConfig, "viewport">> & {
  viewport: { width: number; height: number };
};

const MAX_NETWORK_REQUESTS = 2_048;

type NetworkEvent = { sequence: number; method: string; url: string; status?: number };
type ConsoleEvent = { type: string; text: string };
type ApiResult = { status: number; json?: unknown };

export class PlaywrightDriver implements Driver {
  readonly #browser: Browser;
  readonly #context: BrowserContext;
  readonly #page: Page;
  readonly #program: BrowserProgram;
  readonly #oracles: Map<string, ProgramOracle>;
  readonly #config: ResolvedBrowserConfig;
  readonly #network: NetworkEvent[] = [];
  readonly #requestEvents = new WeakMap<Request, NetworkEvent>();
  readonly #inflightMutations = new Set<Request>();
  readonly #console: ConsoleEvent[] = [];
  readonly #api = new Map<string, ApiResult>();
  readonly #featureFlags = new Map<string, string>();
  #failed = false;
  #closed = false;
  #traceStarted = false;
  #networkRequestsTruncated = false;
  #lastMutationChange = 0;

  private constructor(
    browser: Browser,
    context: BrowserContext,
    page: Page,
    program: BrowserProgram,
    oracles: ProgramOracle[],
    config: ResolvedBrowserConfig,
  ) {
    this.#browser = browser;
    this.#context = context;
    this.#page = page;
    this.#program = program;
    this.#oracles = new Map(oracles.map((oracle) => [oracle.obligation, oracle]));
    this.#config = config;
    page.on("request", (request) => {
      if (this.#network.length >= MAX_NETWORK_REQUESTS) {
        this.#networkRequestsTruncated = true;
        return;
      }
      const event: NetworkEvent = {
        sequence: this.#network.length + 1,
        method: request.method().toUpperCase(),
        url: request.url(),
      };
      this.#network.push(event);
      this.#requestEvents.set(request, event);
      if (isMutationMethod(event.method)) {
        this.#inflightMutations.add(request);
        this.#lastMutationChange = Date.now();
      }
    });
    page.on("response", (response) => {
      const event = this.#requestEvents.get(response.request());
      if (event) event.status = response.status();
    });
    const mutationFinished = (request: Request): void => {
      if (this.#inflightMutations.delete(request)) this.#lastMutationChange = Date.now();
    };
    page.on("requestfinished", mutationFinished);
    page.on("requestfailed", mutationFinished);
    page.on("console", (message) => {
      this.#console.push({ type: message.type(), text: message.text() });
    });
  }

  static async create(
    program: BrowserProgram,
    oracles: ProgramOracle[],
    rawConfig: BrowserConfig,
  ): Promise<PlaywrightDriver> {
    const config = validateConfig(rawConfig);
    validateProgram(program, oracles);
    const moduleRoot = process.env.WVQ_PLAYWRIGHT_MODULE_ROOT ?? process.cwd();
    const require = createRequire(join(moduleRoot, "package.json"));
    const modulePath = require.resolve("playwright");
    const loaded = await import(pathToFileURL(modulePath).href);
    const playwright = (loaded.default ?? loaded) as typeof import("playwright");
    const browserType = {
      chromium: playwright.chromium,
      firefox: playwright.firefox,
      webkit: playwright.webkit,
    }[config.browser];
    const browser = await browserType.launch({ headless: config.headless });
    const context = await browser.newContext({ viewport: config.viewport });
    context.setDefaultTimeout(config.timeout_ms);
    context.setDefaultNavigationTimeout(config.timeout_ms);
    const page = await context.newPage();
    const driver = new PlaywrightDriver(browser, context, page, program, oracles, config);
    const tracePolicy = program.evidence_policy?.trace ?? "never";
    if (tracePolicy !== "never") {
      await context.tracing.start({ screenshots: true, snapshots: true, sources: false });
      driver.#traceStarted = true;
    }
    return driver;
  }

  async navigate(route: string): Promise<void> {
    await this.#page.goto(this.#resolveUrl(route), { waitUntil: "domcontentloaded" });
  }

  async activate(target: Target): Promise<void> {
    await this.#locator(target).click();
  }

  async fill(target: Target, value: string): Promise<void> {
    await this.#locator(target).fill(this.#resolveScalar(value));
  }

  async select(target: Target, value: string): Promise<void> {
    await this.#locator(target).selectOption(this.#resolveScalar(value));
  }

  async press(key: string, target?: Target): Promise<void> {
    if (target) {
      await this.#locator(target).press(key);
    } else {
      await this.#page.keyboard.press(key);
    }
  }

  async wait(condition: WaitCondition): Promise<void> {
    if (condition.kind === "visible") {
      await this.#locator(condition.target).waitFor({ state: "visible" });
      return;
    }
    if (condition.kind === "url") {
      await this.#page.waitForURL((url) => routeOf(url).startsWith(condition.route));
      return;
    }
    const unknown: never = condition;
    throw new Error(`unknown wait condition \`${(unknown as { kind?: unknown }).kind}\``);
  }

  async setFeatureFlag(key: string, value: string): Promise<void> {
    this.#featureFlags.set(key, value);
    await this.#context.addInitScript(
      ({ featureKey, featureValue }) => localStorage.setItem(featureKey, featureValue),
      { featureKey: key, featureValue: value },
    );
    if (this.#page.url() !== "about:blank") {
      await this.#page.evaluate(
        ({ featureKey, featureValue }) => localStorage.setItem(featureKey, featureValue),
        { featureKey: key, featureValue: value },
      );
    }
  }

  async injectFault(name: string): Promise<void> {
    const fault = this.#program.faults?.[name];
    if (!fault) throw new Error(`unknown fault \`${name}\``);
    await this.#context.route("**/*", async (route) => {
      if (!route.request().url().includes(fault.url_contains)) {
        await route.fallback();
        return;
      }
      switch (fault.kind) {
        case "abort":
          await route.abort("failed");
          return;
        case "http_response":
          await route.fulfill({
            status: fault.status,
            body: fault.body ?? "",
            ...(fault.headers ? { headers: fault.headers } : {}),
          });
          return;
        case "delay":
          await new Promise((resolve) => setTimeout(resolve, fault.delay_ms));
          await route.continue();
          return;
      }
    });
  }

  async apiCall(name: string, input: string): Promise<void> {
    const operation = this.#program.api_operations?.[name];
    if (!operation) throw new Error(`unknown API operation \`${name}\``);
    if (!Object.hasOwn(this.#program.data ?? {}, input)) {
      throw new Error(`unknown API input \`${input}\``);
    }
    const response = await this.#context.request.fetch(this.#resolveUrl(operation.path), {
      method: operation.method,
      data: this.#program.data?.[input],
      failOnStatusCode: false,
      ...(operation.headers ? { headers: operation.headers } : {}),
    });
    let json: unknown;
    try {
      json = await response.json();
    } catch {
      json = undefined;
    }
    this.#api.set(name, { status: response.status(), json });
  }

  async assert(obligation: string): Promise<void> {
    const oracle = this.#oracles.get(obligation);
    if (!oracle) throw new Error(`assertion has no sealed oracle for \`${obligation}\``);
    if (oracle.condition) {
      const condition = await this.#evaluate(oracle.condition);
      if (!condition) {
        this.#failed = true;
        throw new Error(`condition_not_established:${obligation}`);
      }
    }
    if (oracle.expected.kind === "all") {
      for (const predicate of oracle.expected.predicates) {
        if (!(await this.#evaluate(predicate))) {
          this.#failed = true;
          throw new Error(
            `assertion_failed:${obligation}:sealed expectation ${predicate.kind} was not met`,
          );
        }
      }
      return;
    }
    if (!(await this.#evaluate(oracle.expected))) {
      this.#failed = true;
      throw new Error(`assertion_failed:${obligation}:sealed expectation was not met`);
    }
  }

  /**
   * Let a mutation and an immediate application-level retry finish inside the
   * action that caused them. Long polling and broken servers remain bounded by
   * a two-second ceiling; incomplete journals are still reported separately.
   */
  async settleAction(): Promise<void> {
    const deadline = Date.now() + Math.min(this.#config.timeout_ms, 2_000);
    const quietMs = 50;
    let observed = this.#network.length;
    let quietSince = Date.now();
    while (Date.now() < deadline) {
      await this.#page.waitForTimeout(25);
      if (this.#network.length !== observed) {
        observed = this.#network.length;
        quietSince = Date.now();
      }
      if (
        this.#inflightMutations.size === 0 &&
        Date.now() - Math.max(quietSince, this.#lastMutationChange) >= quietMs
      ) {
        return;
      }
    }
  }

  async observe(failed: boolean, captureScreenshot = true): Promise<Observation> {
    this.#failed ||= failed;
    const policy = this.#program.evidence_policy ?? defaultPolicy();
    const route = routeOf(new URL(this.#page.url()));
    const snapshot = await this.#page.locator("body").ariaSnapshot();
    const viewport = this.#page.viewportSize();
    const storageSnapshot = await this.#storageKeys();
    const observation: Observation = {
      route,
      a11y_digest: createHash("sha256").update(snapshot).digest("hex"),
      network: this.#network.map(
        (event) => `${event.method} ${event.url} ${event.status ?? "pending"}`,
      ),
      network_requests: this.#network.map((event) => ({ ...event })),
      network_requests_truncated: this.#networkRequestsTruncated,
      console: this.#console.map((event) => `${event.type}: ${event.text}`),
      storage: storageSnapshot.keys,
      storage_available: storageSnapshot.available,
    };
    if (viewport) observation.viewport = `${viewport.width}x${viewport.height}`;
    if (captureScreenshot && captureAllowed(policy.screenshot, this.#failed)) {
      await mkdir(this.#config.evidence_dir, { recursive: true });
      const path = join(
        this.#config.evidence_dir,
        `${safeName(this.#program.id)}-${Date.now()}.png`,
      );
      await this.#page.screenshot({ path, fullPage: true });
      observation.screenshot_path = path;
    }
    return observation;
  }

  /**
   * Collect one deterministic UI-integrity snapshot of the current state.
   *
   * The driver only measures: whether anything it records is a problem is
   * decided by `wvq-ui` in Rust. Collection makes no model or vision call.
   */
  async collectUi(
    identity: { revision: string; program?: string; step: number; stateDigest: string },
    config: UiIntegrityConfig | undefined,
  ): Promise<CollectionResult> {
    return collectLayoutSnapshot(
      this.#page,
      {
        revision: identity.revision,
        program: identity.program ?? this.#program.id,
        step: identity.step,
        stateDigest: identity.stateDigest,
      },
      config,
    );
  }

  async finish(): Promise<{ trace_path?: string }> {
    if (this.#closed) return {};
    let tracePath: string | undefined;
    if (this.#traceStarted) {
      const policy = this.#program.evidence_policy ?? defaultPolicy();
      if (captureAllowed(policy.trace, this.#failed)) {
        await mkdir(this.#config.evidence_dir, { recursive: true });
        const path = join(
          this.#config.evidence_dir,
          `${safeName(this.#program.id)}-${Date.now()}.zip`,
        );
        await this.#context.tracing.stop({ path });
        tracePath = path;
      } else {
        await this.#context.tracing.stop();
      }
    }
    await this.#context.close();
    await this.#browser.close();
    this.#closed = true;
    return tracePath ? { trace_path: tracePath } : {};
  }

  async cancel(): Promise<void> {
    if (this.#closed) return;
    await this.#context.close().catch(() => undefined);
    await this.#browser.close().catch(() => undefined);
    this.#closed = true;
  }

  #locator(target: Target): Locator {
    validateTarget(target);
    const root: Page | Locator = target.scope ? this.#locator(target.scope) : this.#page;
    if (target.test_id) return root.getByTestId(target.test_id);
    if (target.role) {
      const role = target.role as Parameters<Page["getByRole"]>[0];
      return target.accessible_name
        ? root.getByRole(role, { name: target.accessible_name })
        : root.getByRole(role);
    }
    if (target.label) return root.getByLabel(target.label, { exact: true });
    if (target.accessible_name) return root.getByText(target.accessible_name, { exact: true });
    if (target.component_hint) {
      return root.locator(`[data-component="${cssString(target.component_hint)}"]`);
    }
    return root.locator(target.fallback_css as string);
  }

  #resolveUrl(route: string): string {
    const base = new URL(this.#config.base_url);
    const resolved = new URL(route, base);
    if (resolved.origin !== base.origin) {
      throw new Error(`route must stay on configured origin ${base.origin}`);
    }
    return resolved.href;
  }

  #resolveScalar(reference: string): string {
    const value = this.#program.data?.[reference];
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      return String(value);
    }
    return reference;
  }

  async #evaluate(predicate: Predicate): Promise<boolean> {
    switch (predicate.kind) {
      case "visible":
        return this.#locator(predicate.target).isVisible();
      case "hidden":
        return this.#locator(predicate.target).isHidden();
      case "enabled":
        return this.#locator(predicate.target).isEnabled();
      case "disabled":
        return this.#locator(predicate.target).isDisabled();
      case "text_equals":
        return (await this.#locator(predicate.target).innerText()).trim() === predicate.value;
      case "text_contains":
        return (await this.#locator(predicate.target).innerText()).includes(predicate.value);
      case "value_equals":
        return (await this.#locator(predicate.target).inputValue()) === predicate.value;
      case "route_equals": {
        const current = this.#page.url();
        return current === predicate.value || routeOf(new URL(current)) === predicate.value;
      }
      case "route_contains":
        return this.#page.url().includes(predicate.value);
      case "network_response":
        return this.#network.some(
          (event) =>
            event.url.includes(predicate.url_contains) &&
            (!predicate.method || event.method === predicate.method.toUpperCase()) &&
            (!predicate.status || event.status === predicate.status),
        );
      case "no_console_errors":
        return !this.#console.some((event) => event.type === "error");
      case "storage_equals":
        return (await this.#storageValue(predicate.area, predicate.key)) === predicate.value;
      case "storage_absent":
        return (await this.#storageValue(predicate.area, predicate.key)) === null;
      case "api_status":
        return this.#api.get(predicate.operation)?.status === predicate.status;
      case "api_json_equals": {
        const json = this.#api.get(predicate.operation)?.json;
        return deepEqual(jsonPointer(json, predicate.pointer), predicate.value);
      }
      case "unique":
        return (await this.#locator(predicate.target).count()) === 1;
      case "max_multiplicity":
        return (await this.#locator(predicate.target).count()) <= predicate.max;
      case "receives_events":
        return (await this.#eventRatio(predicate.target)) >= predicate.min_ratio_permille;
      case "inside_viewport":
        return this.#insideViewport(predicate.target, predicate.margin_px);
      case "text_not_clipped":
        return this.#textNotClipped(predicate.target);
      case "no_overlap":
        return (
          (await this.#overlapRatio(predicate.target, predicate.with)) <=
          predicate.max_ratio_permille
        );
      case "all":
        for (const nested of predicate.predicates) if (!(await this.#evaluate(nested))) return false;
        return true;
      case "any":
        for (const nested of predicate.predicates) if (await this.#evaluate(nested)) return true;
        return false;
      case "not":
        return !(await this.#evaluate(predicate.predicate));
    }
  }

  /**
   * Share of probe points on the target that the browser reports as reaching
   * it, in permille. The target itself and anything inside it count; anything
   * else painting on top does not.
   */
  async #eventRatio(target: Target): Promise<number> {
    const handle = await this.#locator(target).elementHandle();
    if (!handle) return 0;
    try {
      return await handle.evaluate((element: Element) => {
        const box = element.getBoundingClientRect();
        if (box.width <= 0 || box.height <= 0) return 0;
        const inset = 2;
        const xs = [box.x + inset, box.x + box.width / 2, box.right - inset];
        const ys = [box.y + inset, box.y + box.height / 2, box.bottom - inset];
        const points: Array<[number, number]> = [
          [xs[1] as number, ys[1] as number],
          [xs[0] as number, ys[0] as number],
          [xs[2] as number, ys[0] as number],
          [xs[0] as number, ys[2] as number],
          [xs[2] as number, ys[2] as number],
        ];
        let received = 0;
        for (const [x, y] of points) {
          const top = document.elementsFromPoint(x, y)[0];
          if (top && (top === element || element.contains(top))) received += 1;
        }
        return Math.round((received / points.length) * 1000);
      });
    } finally {
      await handle.dispose();
    }
  }

  async #insideViewport(target: Target, margin: number): Promise<boolean> {
    const handle = await this.#locator(target).elementHandle();
    if (!handle) return false;
    try {
      return await handle.evaluate(
        (element: Element, slack: number) => {
          const box = element.getBoundingClientRect();
          if (box.width <= 0 || box.height <= 0) return false;
          return (
            box.x >= -slack &&
            box.y >= -slack &&
            box.right <= window.innerWidth + slack &&
            box.bottom <= window.innerHeight + slack
          );
        },
        margin,
      );
    } finally {
      await handle.dispose();
    }
  }

  async #textNotClipped(target: Target): Promise<boolean> {
    const handle = await this.#locator(target).elementHandle();
    if (!handle) return false;
    try {
      return await handle.evaluate((element: Element) => {
        // A scroll container is meant to hold more than it shows.
        const style = getComputedStyle(element);
        const scrolls =
          ["auto", "scroll", "overlay"].includes(style.overflowX) ||
          ["auto", "scroll", "overlay"].includes(style.overflowY);
        if (scrolls) return true;
        return (
          element.scrollWidth <= element.clientWidth + 1 &&
          element.scrollHeight <= element.clientHeight + 1
        );
      });
    } finally {
      await handle.dispose();
    }
  }

  /** Overlap of `other` on `target`, as permille of the target's own box. */
  async #overlapRatio(target: Target, other: Target): Promise<number> {
    const first = await this.#locator(target).elementHandle();
    const second = await this.#locator(other).elementHandle();
    if (!first || !second) {
      await first?.dispose();
      await second?.dispose();
      // A target that is not rendered cannot be overlapped.
      return 0;
    }
    try {
      return await first.evaluate((element: Element, counterpart: Element) => {
        const a = element.getBoundingClientRect();
        const b = counterpart.getBoundingClientRect();
        const area = a.width * a.height;
        if (area <= 0) return 0;
        const width = Math.max(0, Math.min(a.right, b.right) - Math.max(a.x, b.x));
        const height = Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.y, b.y));
        return Math.round(((width * height) / area) * 1000);
      }, second);
    } finally {
      await first.dispose();
      await second.dispose();
    }
  }

  async #storageValue(area: "local" | "session", key: string): Promise<string | null> {
    return this.#page.evaluate(
      ({ storageArea, storageKey }) =>
        (storageArea === "local" ? localStorage : sessionStorage).getItem(storageKey),
      { storageArea: area, storageKey: key },
    );
  }

  async #storageKeys(): Promise<{ keys: Record<string, string>; available: boolean }> {
    try {
      const keys = await this.#page.evaluate(() => {
        const values: Record<string, string> = {};
        for (let index = 0; index < localStorage.length; index += 1) {
          const key = localStorage.key(index);
          if (key) values[`local:${key}`] = "present";
        }
        for (let index = 0; index < sessionStorage.length; index += 1) {
          const key = sessionStorage.key(index);
          if (key) values[`session:${key}`] = "present";
        }
        return values;
      });
      return { keys, available: true };
    } catch {
      return { keys: {}, available: false };
    }
  }
}

function validateConfig(config: BrowserConfig): ResolvedBrowserConfig {
  const base = new URL(config.base_url);
  if (!['http:', 'https:'].includes(base.protocol)) {
    throw new Error("base_url must use http or https");
  }
  if (!config.evidence_dir) throw new Error("evidence_dir is required");
  const timeout = config.timeout_ms ?? 10_000;
  if (!Number.isInteger(timeout) || timeout < 1 || timeout > 120_000) {
    throw new Error("timeout_ms must be between 1 and 120000");
  }
  return {
    base_url: base.href,
    browser: config.browser ?? "chromium",
    headless: config.headless ?? true,
    timeout_ms: timeout,
    viewport: config.viewport ?? { width: 1280, height: 720 },
    evidence_dir: config.evidence_dir,
  };
}

function validateProgram(program: BrowserProgram, oracles: ProgramOracle[]): void {
  if (!program.id || !Array.isArray(program.steps) || !Array.isArray(program.obligations)) {
    throw new Error("malformed TestProgram");
  }
  const oracleIds = new Set(oracles.map((oracle) => oracle.obligation));
  for (const obligation of program.obligations) {
    if (!oracleIds.has(obligation)) {
      throw new Error(`program obligation \`${obligation}\` has no sealed predicate`);
    }
  }
}

function validateTarget(target: Target): void {
  if (target.scope) validateTarget(target.scope);
  if (
    !target.test_id &&
    !target.role &&
    !target.label &&
    !target.accessible_name &&
    !target.component_hint &&
    !target.fallback_css
  ) {
    throw new Error("target needs a semantic identity");
  }
  if (Object.values(target).some((value) => typeof value === "string" && /xpath/i.test(value))) {
    throw new Error("XPath is not a target identity");
  }
}

function defaultPolicy(): EvidencePolicy {
  return {
    screenshot: "never",
    trace: "never",
    network: "always",
    console: "always",
    storage: "on_failure",
  };
}

function captureAllowed(when: "never" | "on_failure" | "always", failed: boolean): boolean {
  return when === "always" || (when === "on_failure" && failed);
}

function routeOf(url: URL): string {
  return `${url.pathname}${url.search}${url.hash}`;
}

function safeName(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]/g, "-").slice(0, 100);
}

function isMutationMethod(method: string): boolean {
  return method === "POST" || method === "PUT" || method === "PATCH" || method === "DELETE";
}

function cssString(value: string): string {
  return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}

function jsonPointer(value: unknown, pointer: string): unknown {
  if (pointer === "") return value;
  if (!pointer.startsWith("/")) return undefined;
  return pointer
    .slice(1)
    .split("/")
    .map((part) => part.replaceAll("~1", "/").replaceAll("~0", "~"))
    .reduce<unknown>((current, part) => {
      if (!current || typeof current !== "object") return undefined;
      return (current as Record<string, unknown>)[part];
    }, value);
}

function deepEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

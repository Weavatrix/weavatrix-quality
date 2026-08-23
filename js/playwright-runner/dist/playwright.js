/** Actual Playwright adapter. Policy and sealed predicates arrive from Rust. */
import { createHash } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { collectLayoutSnapshot, } from "./ui_integrity.js";
export class PlaywrightDriver {
    #browser;
    #context;
    #page;
    #program;
    #oracles;
    #config;
    #network = [];
    #console = [];
    #api = new Map();
    #featureFlags = new Map();
    #failed = false;
    #closed = false;
    #traceStarted = false;
    constructor(browser, context, page, program, oracles, config) {
        this.#browser = browser;
        this.#context = context;
        this.#page = page;
        this.#program = program;
        this.#oracles = new Map(oracles.map((oracle) => [oracle.obligation, oracle]));
        this.#config = config;
        page.on("response", (response) => {
            this.#network.push({
                method: response.request().method(),
                url: response.url(),
                status: response.status(),
            });
        });
        page.on("console", (message) => {
            this.#console.push({ type: message.type(), text: message.text() });
        });
    }
    static async create(program, oracles, rawConfig) {
        const config = validateConfig(rawConfig);
        validateProgram(program, oracles);
        const moduleRoot = process.env.WVQ_PLAYWRIGHT_MODULE_ROOT ?? process.cwd();
        const require = createRequire(join(moduleRoot, "package.json"));
        const modulePath = require.resolve("playwright");
        const loaded = await import(pathToFileURL(modulePath).href);
        const playwright = (loaded.default ?? loaded);
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
    async navigate(route) {
        await this.#page.goto(this.#resolveUrl(route), { waitUntil: "domcontentloaded" });
    }
    async activate(target) {
        await this.#locator(target).click();
    }
    async fill(target, value) {
        await this.#locator(target).fill(this.#resolveScalar(value));
    }
    async select(target, value) {
        await this.#locator(target).selectOption(this.#resolveScalar(value));
    }
    async press(key, target) {
        if (target) {
            await this.#locator(target).press(key);
        }
        else {
            await this.#page.keyboard.press(key);
        }
    }
    async wait(condition) {
        if (condition.kind === "visible") {
            await this.#locator(condition.target).waitFor({ state: "visible" });
            return;
        }
        if (condition.kind === "url") {
            await this.#page.waitForURL((url) => routeOf(url).startsWith(condition.route));
            return;
        }
        const unknown = condition;
        throw new Error(`unknown wait condition \`${unknown.kind}\``);
    }
    async setFeatureFlag(key, value) {
        this.#featureFlags.set(key, value);
        await this.#context.addInitScript(({ featureKey, featureValue }) => localStorage.setItem(featureKey, featureValue), { featureKey: key, featureValue: value });
        if (this.#page.url() !== "about:blank") {
            await this.#page.evaluate(({ featureKey, featureValue }) => localStorage.setItem(featureKey, featureValue), { featureKey: key, featureValue: value });
        }
    }
    async injectFault(name) {
        const fault = this.#program.faults?.[name];
        if (!fault)
            throw new Error(`unknown fault \`${name}\``);
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
    async apiCall(name, input) {
        const operation = this.#program.api_operations?.[name];
        if (!operation)
            throw new Error(`unknown API operation \`${name}\``);
        if (!Object.hasOwn(this.#program.data ?? {}, input)) {
            throw new Error(`unknown API input \`${input}\``);
        }
        const response = await this.#context.request.fetch(this.#resolveUrl(operation.path), {
            method: operation.method,
            data: this.#program.data?.[input],
            failOnStatusCode: false,
            ...(operation.headers ? { headers: operation.headers } : {}),
        });
        let json;
        try {
            json = await response.json();
        }
        catch {
            json = undefined;
        }
        this.#api.set(name, { status: response.status(), json });
    }
    async assert(obligation) {
        const oracle = this.#oracles.get(obligation);
        if (!oracle)
            throw new Error(`assertion has no sealed oracle for \`${obligation}\``);
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
                    throw new Error(`assertion_failed:${obligation}:sealed expectation ${predicate.kind} was not met`);
                }
            }
            return;
        }
        if (!(await this.#evaluate(oracle.expected))) {
            this.#failed = true;
            throw new Error(`assertion_failed:${obligation}:sealed expectation was not met`);
        }
    }
    async observe(failed) {
        this.#failed ||= failed;
        const policy = this.#program.evidence_policy ?? defaultPolicy();
        const route = routeOf(new URL(this.#page.url()));
        const snapshot = await this.#page.locator("body").ariaSnapshot();
        const viewport = this.#page.viewportSize();
        const storageSnapshot = await this.#storageKeys();
        const observation = {
            route,
            a11y_digest: createHash("sha256").update(snapshot).digest("hex"),
            network: this.#network.map((event) => `${event.method} ${event.url} ${event.status}`),
            console: this.#console.map((event) => `${event.type}: ${event.text}`),
            storage: storageSnapshot.keys,
            storage_available: storageSnapshot.available,
        };
        if (viewport)
            observation.viewport = `${viewport.width}x${viewport.height}`;
        if (captureAllowed(policy.screenshot, this.#failed)) {
            await mkdir(this.#config.evidence_dir, { recursive: true });
            const path = join(this.#config.evidence_dir, `${safeName(this.#program.id)}-${Date.now()}.png`);
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
    async collectUi(identity, config) {
        return collectLayoutSnapshot(this.#page, {
            revision: identity.revision,
            program: identity.program ?? this.#program.id,
            step: identity.step,
            stateDigest: identity.stateDigest,
        }, config);
    }
    async finish() {
        if (this.#closed)
            return {};
        let tracePath;
        if (this.#traceStarted) {
            const policy = this.#program.evidence_policy ?? defaultPolicy();
            if (captureAllowed(policy.trace, this.#failed)) {
                await mkdir(this.#config.evidence_dir, { recursive: true });
                const path = join(this.#config.evidence_dir, `${safeName(this.#program.id)}-${Date.now()}.zip`);
                await this.#context.tracing.stop({ path });
                tracePath = path;
            }
            else {
                await this.#context.tracing.stop();
            }
        }
        await this.#context.close();
        await this.#browser.close();
        this.#closed = true;
        return tracePath ? { trace_path: tracePath } : {};
    }
    async cancel() {
        if (this.#closed)
            return;
        await this.#context.close().catch(() => undefined);
        await this.#browser.close().catch(() => undefined);
        this.#closed = true;
    }
    #locator(target) {
        validateTarget(target);
        const root = target.scope ? this.#locator(target.scope) : this.#page;
        if (target.test_id)
            return root.getByTestId(target.test_id);
        if (target.role) {
            const role = target.role;
            return target.accessible_name
                ? root.getByRole(role, { name: target.accessible_name })
                : root.getByRole(role);
        }
        if (target.label)
            return root.getByLabel(target.label, { exact: true });
        if (target.accessible_name)
            return root.getByText(target.accessible_name, { exact: true });
        if (target.component_hint) {
            return root.locator(`[data-component="${cssString(target.component_hint)}"]`);
        }
        return root.locator(target.fallback_css);
    }
    #resolveUrl(route) {
        const base = new URL(this.#config.base_url);
        const resolved = new URL(route, base);
        if (resolved.origin !== base.origin) {
            throw new Error(`route must stay on configured origin ${base.origin}`);
        }
        return resolved.href;
    }
    #resolveScalar(reference) {
        const value = this.#program.data?.[reference];
        if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
            return String(value);
        }
        return reference;
    }
    async #evaluate(predicate) {
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
                return this.#network.some((event) => event.url.includes(predicate.url_contains) &&
                    (!predicate.method || event.method === predicate.method.toUpperCase()) &&
                    (!predicate.status || event.status === predicate.status));
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
            case "all":
                for (const nested of predicate.predicates)
                    if (!(await this.#evaluate(nested)))
                        return false;
                return true;
            case "any":
                for (const nested of predicate.predicates)
                    if (await this.#evaluate(nested))
                        return true;
                return false;
            case "not":
                return !(await this.#evaluate(predicate.predicate));
        }
    }
    async #storageValue(area, key) {
        return this.#page.evaluate(({ storageArea, storageKey }) => (storageArea === "local" ? localStorage : sessionStorage).getItem(storageKey), { storageArea: area, storageKey: key });
    }
    async #storageKeys() {
        try {
            const keys = await this.#page.evaluate(() => {
                const values = {};
                for (let index = 0; index < localStorage.length; index += 1) {
                    const key = localStorage.key(index);
                    if (key)
                        values[`local:${key}`] = "present";
                }
                for (let index = 0; index < sessionStorage.length; index += 1) {
                    const key = sessionStorage.key(index);
                    if (key)
                        values[`session:${key}`] = "present";
                }
                return values;
            });
            return { keys, available: true };
        }
        catch {
            return { keys: {}, available: false };
        }
    }
}
function validateConfig(config) {
    const base = new URL(config.base_url);
    if (!['http:', 'https:'].includes(base.protocol)) {
        throw new Error("base_url must use http or https");
    }
    if (!config.evidence_dir)
        throw new Error("evidence_dir is required");
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
function validateProgram(program, oracles) {
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
function validateTarget(target) {
    if (target.scope)
        validateTarget(target.scope);
    if (!target.test_id &&
        !target.role &&
        !target.label &&
        !target.accessible_name &&
        !target.component_hint &&
        !target.fallback_css) {
        throw new Error("target needs a semantic identity");
    }
    if (Object.values(target).some((value) => typeof value === "string" && /xpath/i.test(value))) {
        throw new Error("XPath is not a target identity");
    }
}
function defaultPolicy() {
    return {
        screenshot: "never",
        trace: "never",
        network: "always",
        console: "always",
        storage: "on_failure",
    };
}
function captureAllowed(when, failed) {
    return when === "always" || (when === "on_failure" && failed);
}
function routeOf(url) {
    return `${url.pathname}${url.search}${url.hash}`;
}
function safeName(value) {
    return value.replace(/[^a-zA-Z0-9._-]/g, "-").slice(0, 100);
}
function cssString(value) {
    return value.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}
function jsonPointer(value, pointer) {
    if (pointer === "")
        return value;
    if (!pointer.startsWith("/"))
        return undefined;
    return pointer
        .slice(1)
        .split("/")
        .map((part) => part.replaceAll("~1", "/").replaceAll("~0", "~"))
        .reduce((current, part) => {
        if (!current || typeof current !== "object")
            return undefined;
        return current[part];
    }, value);
}
function deepEqual(left, right) {
    return JSON.stringify(left) === JSON.stringify(right);
}

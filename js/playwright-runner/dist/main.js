#!/usr/bin/env node
/** Stdio host. Speaks the Rust/TS golden protocol. */
import { stdin as input, stdout as output } from "node:process";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { decodeRequest } from "./protocol.js";
import { executeStep } from "./execute.js";
import { filterObservation } from "./observe.js";
import { PlaywrightDriver, } from "./playwright.js";
class BridgeSession {
    #initialized = false;
    #program;
    #driver;
    #failed = false;
    #finished = false;
    async handle(line) {
        let id = 0;
        try {
            const request = decodeRequest(line);
            id = request.id;
            let body = {};
            switch (request.method) {
                case "initialize": {
                    const schema = request.params.schema_v ?? 1;
                    if (schema !== 1)
                        throw new Error(`unknown bridge schema_v ${schema}`);
                    if (this.#initialized)
                        throw new Error("bridge is already initialized");
                    this.#initialized = true;
                    body = { schema_v: 1, engine: "playwright" };
                    break;
                }
                case "prepare": {
                    this.#requireInitialized();
                    if (this.#driver)
                        throw new Error("a program is already prepared");
                    const program = request.params.program;
                    const oracles = request.params.oracles;
                    const config = request.params.config;
                    if (!Array.isArray(oracles))
                        throw new Error("prepare requires params.oracles");
                    this.#program = program;
                    this.#driver = await PlaywrightDriver.create(program, oracles, config);
                    for (const action of program.preconditions ?? []) {
                        await executeStep(this.#driver, action);
                    }
                    body = { program: program.id, preconditions: program.preconditions?.length ?? 0 };
                    break;
                }
                case "prepare_recording": {
                    this.#requireInitialized();
                    if (this.#driver)
                        throw new Error("a program is already prepared");
                    const oracles = request.params.oracles;
                    const config = request.params.config;
                    const fixtureValues = request.params.fixture_values;
                    const route = request.params.route;
                    const session = request.params.session;
                    const maxEvents = request.params.max_events;
                    if (!Array.isArray(oracles))
                        throw new Error("prepare_recording requires params.oracles");
                    if (typeof route !== "string" || !route.startsWith("/")) {
                        throw new Error("prepare_recording route must be root-relative");
                    }
                    if (typeof session !== "string" || !session.trim()) {
                        throw new Error("prepare_recording requires params.session");
                    }
                    if (!fixtureValues || typeof fixtureValues !== "object" || Array.isArray(fixtureValues)) {
                        throw new Error("prepare_recording requires params.fixture_values");
                    }
                    if (!Number.isInteger(maxEvents)) {
                        throw new Error("prepare_recording requires integer params.max_events");
                    }
                    const program = {
                        id: session,
                        obligations: [],
                        steps: [],
                        data: fixtureValues,
                        evidence_policy: defaultPolicy(),
                    };
                    this.#program = program;
                    this.#driver = await PlaywrightDriver.create(program, oracles, config);
                    const started = await this.#driver.startRecording(route, {
                        fixture_values: fixtureValues,
                        max_events: Number(maxEvents),
                    });
                    body = { session, initial: started.initial };
                    break;
                }
                case "execute_step": {
                    const { program, driver } = this.#requirePrepared();
                    const index = request.params.index;
                    if (!Number.isInteger(index) || Number(index) < 0) {
                        throw new Error("execute_step requires a non-negative integer index");
                    }
                    const action = program.steps[Number(index)];
                    if (!action)
                        throw new Error("step index out of range");
                    try {
                        await executeStep(driver, action);
                    }
                    catch (error) {
                        this.#failed = true;
                        throw error;
                    }
                    body = { index, action: action.action };
                    break;
                }
                case "observe": {
                    const { program, driver } = this.#requirePrepared();
                    const failed = Boolean(request.params.failed) || this.#failed;
                    if (request.params.settle_action === true)
                        await driver.settleAction();
                    const captureScreenshot = request.params.capture_screenshot !== false;
                    const raw = await driver.observe(failed, captureScreenshot);
                    body = filterObservation(raw, program.evidence_policy ?? defaultPolicy(), failed);
                    break;
                }
                case "collect_ui": {
                    const { program, driver } = this.#requirePrepared();
                    const revision = request.params.revision;
                    const stateDigest = request.params.state_digest;
                    const step = request.params.step;
                    if (typeof revision !== "string" || revision === "") {
                        throw new Error("collect_ui requires params.revision");
                    }
                    if (typeof stateDigest !== "string" || stateDigest === "") {
                        throw new Error("collect_ui requires params.state_digest");
                    }
                    if (!Number.isInteger(step) || Number(step) < 0) {
                        throw new Error("collect_ui requires a non-negative integer step");
                    }
                    const result = await driver.collectUi({
                        revision,
                        program: program.id,
                        step: Number(step),
                        stateDigest,
                    }, request.params.config);
                    body = {
                        snapshot: result.snapshot,
                        limitations: result.limitations,
                        ...(result.a11y_import ? { a11y_import: result.a11y_import } : {}),
                    };
                    break;
                }
                case "poll_recording": {
                    const { driver } = this.#requirePrepared();
                    body = await driver.pollRecording();
                    break;
                }
                case "finish_recording": {
                    const { driver } = this.#requirePrepared();
                    const obligations = await driver.evaluateRecordedOracles();
                    const finished = await driver.finish();
                    this.#finished = true;
                    body = { obligations, ...finished };
                    break;
                }
                case "finish": {
                    const { driver } = this.#requirePrepared();
                    body = await driver.finish();
                    this.#finished = true;
                    break;
                }
                case "cancel": {
                    if (this.#driver)
                        await this.#driver.cancel();
                    this.#finished = true;
                    body = { cancelled: true };
                    break;
                }
                default: {
                    const unknown = request.method;
                    throw new Error(`unknown bridge method \`${unknown}\``);
                }
            }
            return JSON.stringify({ type: "ok", id, body });
        }
        catch (error) {
            return JSON.stringify({
                type: "error",
                id,
                error: error instanceof Error ? error.message : String(error),
            });
        }
    }
    #requireInitialized() {
        if (!this.#initialized)
            throw new Error("initialize must run first");
        if (this.#finished)
            throw new Error("bridge session has finished");
    }
    #requirePrepared() {
        this.#requireInitialized();
        if (!this.#program || !this.#driver)
            throw new Error("prepare must run first");
        return { program: this.#program, driver: this.#driver };
    }
}
const defaultSession = new BridgeSession();
export async function handle(line) {
    return defaultSession.handle(line);
}
export { BridgeSession };
function defaultPolicy() {
    return {
        screenshot: "never",
        trace: "never",
        network: "always",
        console: "always",
        storage: "on_failure",
    };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
    const session = new BridgeSession();
    const lines = createInterface({ input, crlfDelay: Infinity });
    for await (const line of lines) {
        output.write(`${await session.handle(line)}\n`);
    }
}

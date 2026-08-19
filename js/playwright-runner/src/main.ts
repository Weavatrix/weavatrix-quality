/** Stdio host. Speaks the Rust/TS golden protocol. No AI. */

import { stdin as input, stdout as output } from "node:process";
import { createInterface } from "node:readline";
import { decodeRequest } from "./protocol.ts";
import { executeStep, type Driver } from "./execute.ts";
import { filterObservation, type EvidencePolicy, type Observation } from "./observe.ts";

type Program = {
  steps: Array<{ action: string } & Record<string, unknown>>;
  evidence_policy?: EvidencePolicy;
};

const memory: { program?: Program; failed: boolean; route: string } = {
  failed: false,
  route: "",
};

const driver: Driver = {
  navigate(route) {
    memory.route = route;
  },
  activate() {},
  fill() {},
  press() {},
  assert() {},
};

function defaultPolicy(): EvidencePolicy {
  return {
    screenshot: "never",
    trace: "never",
    network: "always",
    console: "always",
    storage: "on_failure",
  };
}

function handle(line: string): string {
  try {
    const request = decodeRequest(line);
    switch (request.method) {
      case "initialize":
        return JSON.stringify({ type: "ok", id: request.id, body: { schema_v: 1 } });
      case "prepare":
        memory.program = request.params.program as Program;
        memory.failed = false;
        return JSON.stringify({ type: "ok", id: request.id, body: {} });
      case "execute_step": {
        const index = Number(request.params.index);
        const step = memory.program?.steps[index];
        if (!step) {
          return JSON.stringify({
            type: "error",
            id: request.id,
            error: "step index out of range",
          });
        }
        executeStep(driver, step as { action: string });
        return JSON.stringify({ type: "ok", id: request.id, body: {} });
      }
      case "observe": {
        const failed = Boolean(request.params.failed) || memory.failed;
        const raw: Observation = {
          route: memory.route || undefined,
          network: [],
          console: [],
          storage: {},
          screenshot_handle: "cas:unused",
        };
        const body = filterObservation(
          raw,
          memory.program?.evidence_policy ?? defaultPolicy(),
          failed,
        );
        return JSON.stringify({ type: "ok", id: request.id, body });
      }
      case "finish":
      case "cancel":
        return JSON.stringify({ type: "ok", id: request.id, body: {} });
      default:
        return JSON.stringify({
          type: "error",
          id: request.id,
          error: "unknown bridge method",
        });
    }
  } catch (err) {
    return JSON.stringify({
      type: "error",
      id: 0,
      error: err instanceof Error ? err.message : String(err),
    });
  }
}

export { handle };

if (import.meta.url === `file://${process.argv[1]}`) {
  const rl = createInterface({ input, crlfDelay: Infinity });
  rl.on("line", (line) => {
    output.write(`${handle(line)}\n`);
  });
}

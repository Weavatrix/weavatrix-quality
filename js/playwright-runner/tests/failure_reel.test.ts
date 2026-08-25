import assert from "node:assert/strict";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { actionTarget } from "../dist/execute.js";
import { decodeRequest } from "../dist/protocol.js";
import { handle } from "../dist/main.js";
import { PlaywrightDriver } from "../dist/playwright.js";
import { predicateTarget } from "../dist/failure_reel.js";

test("actionTarget names the IR target and skips navigate", () => {
  const target = { role: "button", accessible_name: "Pay" };
  assert.deepEqual(actionTarget({ action: "activate", target }), target);
  assert.deepEqual(actionTarget({ action: "fill", target, value: "x" }), target);
  assert.deepEqual(
    actionTarget({ action: "wait", condition: { kind: "visible", target } }),
    target,
  );
  assert.equal(actionTarget({ action: "navigate", route: "/" }), undefined);
  assert.equal(actionTarget({ action: "assert", obligation: "paid" }), undefined);
});

test("predicateTarget walks sealed spatial predicates", () => {
  const target = { role: "button", accessible_name: "Save" };
  assert.deepEqual(predicateTarget({ kind: "no_overlap", target, with: { role: "dialog" } }), target);
  assert.deepEqual(
    predicateTarget({ kind: "all", predicates: [{ kind: "visible", target }] }),
    target,
  );
  assert.equal(predicateTarget({ kind: "no_console_errors" }), undefined);
});

test("capture_failure_reel is a known method and still fails closed on the green path", async () => {
  decodeRequest(JSON.stringify({ method: "capture_failure_reel", id: 2, params: { step: 0 } }));
  const reply = JSON.parse(
    await handle(JSON.stringify({ method: "capture_failure_reel", id: 3, params: { step: 0 } })),
  );
  assert.equal(reply.type, "error");
  assert.match(reply.error, /prepare must run first|initialize must run first/);
});

async function serve(html: string): Promise<{ server: Server; port: number }> {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html" });
    response.end(html);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  return { server, port: address.port };
}

test("a failed activate captures after and highlighted frames, never on success", async () => {
  const { server, port } = await serve(`<!doctype html>
    <html><body>
      <button>Pay</button>
    </body></html>`);
  const evidence = await mkdtemp(join(tmpdir(), "wvq-reel-"));
  const program = {
    id: "reel-fixture",
    obligations: ["paid"],
    steps: [{ action: "navigate", route: "/" }],
  };
  const oracles = [{
    obligation: "paid",
    expected: { kind: "visible", target: { role: "button", accessible_name: "Missing" } },
  }];
  let driver;
  try {
    driver = await PlaywrightDriver.create(program, oracles, {
      base_url: `http://127.0.0.1:${port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 2_000,
      evidence_dir: evidence,
    });
    await driver.navigate("/");
    await assert.rejects(driver.assert("paid"), /assertion_failed:paid/);
    const reel = await driver.captureFailureReel(1, {
      action: "activate",
      target: { role: "button", accessible_name: "Pay" },
    });
    assert.ok(reel.after_path, "failed step must capture the after frame");
    assert.ok(reel.highlight_path, "locatable semantic target must be highlighted");
    assert.deepEqual(reel.limitations, []);
    const names = await readdir(evidence);
    assert.ok(names.some((name) => name.includes("reel") && name.includes("after")));
    assert.ok(names.some((name) => name.includes("reel") && name.includes("highlight")));
  } finally {
    await driver?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
});

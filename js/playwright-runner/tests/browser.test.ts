import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { executeStep } from "../dist/execute.js";
import { PlaywrightDriver } from "../dist/playwright.js";

test("real Playwright executes actions and sealed predicates", async () => {
  const server = createServer((request, response) => {
    if (request.url === "/api/details") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ ok: true }));
      return;
    }
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<!doctype html>
      <label>Name <input aria-label="Name"></label>
      <label>Mode <select aria-label="Mode"><option value="one">One</option><option value="two">Two</option></select></label>
      <button>Save</button><section role="status" hidden></section>
      <script>
        document.querySelector('button').addEventListener('click', async () => {
          const result = await fetch('/fault');
          const status = document.querySelector('[role=status]');
          status.hidden = false;
          status.textContent = document.querySelector('input').value + ':' + document.querySelector('select').value + ':' + result.status;
        });
      </script>`);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  const evidence = await mkdtemp(join(tmpdir(), "wvq-browser-"));
  const program = {
    id: "browser-real",
    obligations: ["saved", "conditional"],
    preconditions: [{ action: "set_feature_flag", key: "new-ui", value: "on" }],
    steps: [
      { action: "navigate", route: "/" },
      { action: "fill", target: { label: "Name" }, value: "Alice" },
      { action: "select", target: { label: "Mode" }, value: "two" },
      { action: "inject_fault", fault: "failed-save" },
      { action: "activate", target: { role: "button", accessible_name: "Save" } },
      { action: "wait", condition: { kind: "visible", target: { role: "status" } } },
      { action: "press", target: { label: "Name" }, key: "Enter" },
      { action: "api_call", operation: "details", input: "empty" },
      { action: "assert", obligation: "saved" },
    ],
    data: { empty: {} },
    faults: {
      "failed-save": {
        kind: "http_response",
        url_contains: "/fault",
        status: 503,
        body: "unavailable",
      },
    },
    api_operations: { details: { method: "GET", path: "/api/details" } },
    evidence_policy: {
      screenshot: "never",
      trace: "never",
      network: "always",
      console: "always",
      storage: "always",
    },
  };
  const oracles = [{
    obligation: "saved",
    expected: {
      kind: "all",
      predicates: [
        { kind: "visible", target: { role: "status" } },
        { kind: "text_contains", target: { role: "status" }, value: "Alice:two:503" },
        { kind: "storage_equals", area: "local", key: "new-ui", value: "on" },
        { kind: "network_response", method: "GET", url_contains: "/fault", status: 503 },
        { kind: "api_status", operation: "details", status: 200 },
        { kind: "api_json_equals", operation: "details", pointer: "/ok", value: true },
      ],
    },
  }, {
    obligation: "conditional",
    condition: { kind: "visible", target: { test_id: "missing-condition" } },
    expected: { kind: "no_console_errors" },
  }];
  let driver;
  try {
    driver = await PlaywrightDriver.create(program, oracles, {
      base_url: `http://127.0.0.1:${address.port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 10_000,
      evidence_dir: evidence,
    });
    for (const action of program.preconditions) await executeStep(driver, action);
    for (const action of program.steps) await executeStep(driver, action);
    const observation = await driver.observe(false);
    assert.equal(observation.route, "/");
    assert.match(observation.a11y_digest, /^[a-f0-9]{64}$/);
    assert.equal(observation.storage["local:new-ui"], "present");
    assert(observation.network.some((event) => event.endsWith(" 503")));
    assert.deepEqual(
      observation.network_requests.map((event) => event.sequence),
      observation.network_requests.map((_, index) => index + 1),
    );
    assert(
      observation.network_requests.some(
        (event) => event.url.endsWith("/fault") && event.status === 503,
      ),
    );
    assert(
      observation.network_requests.every(
        (event) => !("body" in event) && !("headers" in event),
      ),
    );
    assert.equal(observation.network_requests_truncated, false);
    await assert.rejects(
      driver.assert("conditional"),
      /condition_not_established:conditional/,
    );
  } finally {
    await driver?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
});

test("records a redacted API profile and replays it without calling upstream", async () => {
  let apiCalls = 0;
  const server = createServer((request, response) => {
    if (request.url?.startsWith("/api/profile?")) {
      apiCalls += 1;
      response.writeHead(200, { "content-type": "application/json; charset=utf-8" });
      response.end(JSON.stringify({
        ok: true,
        version: 7,
        email: "private@example.invalid",
        token: "secret-token-value",
      }));
      return;
    }
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<!doctype html><p role="status">loading</p><script>
      fetch('/api/profile?email=private%40example.invalid&page=1').then((response) => response.json()).then((value) => {
        document.querySelector('[role=status]').textContent = value.ok + ':' + value.version;
      });
    </script>`);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  const evidence = await mkdtemp(join(tmpdir(), "wvq-network-replay-"));
  const program = {
    id: "network-replay",
    obligations: ["profile-loaded"],
    steps: [
      { action: "navigate", route: "/" },
      { action: "wait", condition: { kind: "visible", target: { role: "status" } } },
      { action: "assert", obligation: "profile-loaded" },
    ],
    evidence_policy: {
      screenshot: "never",
      trace: "never",
      network: "always",
      console: "always",
      storage: "never",
    },
  };
  const oracles = [{
    obligation: "profile-loaded",
    expected: {
      kind: "text_equals",
      target: { role: "status" },
      value: "true:7",
    },
  }];
  let recorder;
  let replayer;
  try {
    recorder = await PlaywrightDriver.create(program, oracles, {
      base_url: `http://127.0.0.1:${address.port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 10_000,
      evidence_dir: evidence,
      network: { mode: "record" },
    });
    for (const action of program.steps) await executeStep(recorder, action);
    const recorded = await recorder.finish();
    recorder = undefined;
    assert.equal(apiCalls, 1);
    assert.equal(recorded.network_profile?.entries.length, 1);
    const serialized = JSON.stringify(recorded.network_profile);
    assert(!serialized.includes("private@example.invalid"));
    assert(!serialized.includes("secret-token-value"));
    assert(serialized.includes("[REDACTED]"));

    replayer = await PlaywrightDriver.create(program, oracles, {
      base_url: `http://127.0.0.1:${address.port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 10_000,
      evidence_dir: evidence,
      network: { mode: "replay", profile: recorded.network_profile },
    });
    for (const action of program.steps) await executeStep(replayer, action);
    assert.equal(apiCalls, 1, "strict replay must not call the recorded API upstream");
    assert.deepEqual(await replayer.evaluateRecordedOracles(), [{
      obligation: "profile-loaded",
      status: "passed",
    }]);
  } finally {
    await recorder?.finish();
    await replayer?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
});

test("strict replay distinguishes two POSTs to the same path by body digest", async () => {
  const seen: string[] = [];
  const server = createServer((request, response) => {
    if (request.url === "/api/save") {
      const chunks: Buffer[] = [];
      request.on("data", (chunk) => chunks.push(chunk as Buffer));
      request.on("end", () => {
        seen.push(Buffer.concat(chunks).toString("utf8"));
        response.writeHead(200, { "content-type": "application/json" });
        response.end(JSON.stringify({ ok: true, n: seen.length }));
      });
      return;
    }
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<!doctype html><p role="status">idle</p><script>
      fetch('/api/save', {method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({order:'42'})})
        .then((r) => r.json())
        .then(() => fetch('/api/save', {method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({theme:'dark'})}))
        .then((r) => r.json())
        .then((value) => { document.querySelector('[role=status]').textContent = 'saved:' + value.n; });
    </script>`);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  const evidence = await mkdtemp(join(tmpdir(), "wvq-body-identity-"));
  const program = {
    id: "body-identity",
    obligations: ["saved"],
    steps: [
      { action: "navigate", route: "/" },
      { action: "wait", condition: { kind: "visible", target: { role: "status" } } },
      { action: "assert", obligation: "saved" },
    ],
    evidence_policy: {
      screenshot: "never",
      trace: "never",
      network: "always",
      console: "always",
      storage: "never",
    },
  };
  const oracles = [{
    obligation: "saved",
    expected: {
      kind: "text_equals",
      target: { role: "status" },
      value: "saved:2",
    },
  }];
  let recorder;
  let replayer;
  try {
    recorder = await PlaywrightDriver.create(program, oracles, {
      base_url: `http://127.0.0.1:${address.port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 10_000,
      evidence_dir: evidence,
      network: { mode: "record" },
    });
    for (const action of program.steps) await executeStep(recorder, action);
    const recorded = await recorder.finish();
    recorder = undefined;
    assert.equal(seen.length, 2);
    assert.equal(recorded.network_profile?.schema_v, 2);
    assert.equal(recorded.network_profile?.entries.length, 2);
    const [first, second] = recorded.network_profile?.entries ?? [];
    assert.equal(first?.path, "/api/save");
    assert.equal(second?.path, "/api/save");
    assert.notEqual(first?.request_body_digest, second?.request_body_digest);

    replayer = await PlaywrightDriver.create(program, oracles, {
      base_url: `http://127.0.0.1:${address.port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 10_000,
      evidence_dir: evidence,
      network: { mode: "replay", profile: recorded.network_profile },
    });
    for (const action of program.steps) await executeStep(replayer, action);
    assert.equal(seen.length, 2, "strict replay must not call either POST upstream");
    assert.deepEqual(await replayer.evaluateRecordedOracles(), [{
      obligation: "saved",
      status: "passed",
    }]);
  } finally {
    await recorder?.finish();
    await replayer?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
});

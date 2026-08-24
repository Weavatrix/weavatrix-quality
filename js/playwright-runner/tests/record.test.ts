import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { PlaywrightDriver } from "../dist/playwright.js";

test("passive recorder captures semantic actions, redacts unknown values, and evaluates seals", async () => {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<!doctype html>
      <label>Name <input id="name"></label>
      <label>Secret <input id="secret"></label>
      <button data-testid="open-details">Open details</button>
      <section role="status" hidden></section>
      <script>
        document.querySelector('button').addEventListener('click', () => {
          const status = document.querySelector('[role=status]');
          status.hidden = false;
          status.textContent = 'Details for ' + document.querySelector('#name').value;
        });
        setTimeout(() => {
          const name = document.querySelector('#name');
          name.value = 'Alice';
          name.dispatchEvent(new Event('change', { bubbles: true }));
          const secret = document.querySelector('#secret');
          secret.value = 's3cr3t-private';
          secret.dispatchEvent(new Event('change', { bubbles: true }));
          document.querySelector('button').click();
          setTimeout(() => document.dispatchEvent(new KeyboardEvent('keydown', {
            key: 'E', ctrlKey: true, shiftKey: true, bubbles: true,
          })), 150);
        }, 100);
      </script>`);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  const evidence = await mkdtemp(join(tmpdir(), "wvq-record-"));
  const driver = await PlaywrightDriver.create(
    { id: "recording", obligations: [], steps: [], data: { name: "Alice" } },
    [{
      obligation: "details-visible",
      expected: {
        kind: "text_equals",
        target: { role: "status" },
        value: "Details for Alice",
      },
    }],
    {
      base_url: `http://127.0.0.1:${address.port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 10_000,
      evidence_dir: evidence,
    },
  );
  try {
    const started = await driver.startRecording("/", {
      fixture_values: { name: "Alice" },
      max_events: 20,
    });
    assert.equal(started.initial.route, "blank");
    const events = [];
    const limitations = [];
    for (let attempt = 0; attempt < 100; attempt += 1) {
      const poll = await driver.pollRecording();
      events.push(...poll.events);
      limitations.push(...poll.limitations);
      if (poll.done) break;
    }
    assert.deepEqual(events.map((event) => event.action.action), ["navigate", "fill", "activate"]);
    assert.equal(events[1]?.action.value, "name");
    assert.equal(events[2]?.action.target.test_id, "open-details");
    assert(limitations.some((item) => item.includes("has no named fixture")));
    assert(!JSON.stringify({ events, limitations }).includes("s3cr3t-private"));
    assert.deepEqual(await driver.evaluateRecordedOracles(), [{
      obligation: "details-visible",
      status: "passed",
    }]);
  } finally {
    await driver.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
});

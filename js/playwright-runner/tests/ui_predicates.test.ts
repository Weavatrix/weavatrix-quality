/**
 * The six sealed UI predicates, evaluated by real Chromium.
 *
 * Each one is asserted twice: once against a page where it holds and once
 * against a page where it does not. A predicate that can only pass is not an
 * oracle.
 */

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { PlaywrightDriver } from "../dist/playwright.js";

const PAGE = `<!doctype html>
<html><body style="margin:0;width:800px">
  <div data-testid="dialog">
    <button data-testid="save">Save</button>
  </div>
  <button data-testid="delete">Delete</button>
  <button data-testid="delete">Delete</button>

  <button data-testid="export" style="position:absolute;left:10px;top:200px;width:120px;height:40px">
    Export
  </button>

  <button data-testid="blocked" style="position:absolute;left:10px;top:300px;width:120px;height:40px">
    Blocked
  </button>
  <div data-testid="veil"
       style="position:absolute;left:0;top:290px;width:400px;height:60px;background:rgba(0,0,0,0.02)"></div>

  <button data-testid="offscreen" style="position:absolute;left:5000px;top:10px">Far away</button>

  <div data-testid="roomy" style="width:400px">Short label</div>
  <div data-testid="clipped"
       style="width:60px;overflow:hidden;white-space:nowrap;text-overflow:ellipsis">
    A label far too long for sixty pixels of box
  </div>
</body></html>`;

/**
 * One oracle per predicate under test, so `assert()` exercises the real sealed
 * path rather than a private helper.
 */
const ORACLES = [
  { obligation: "unique-save", expected: { kind: "unique", target: { test_id: "save" } } },
  { obligation: "unique-delete", expected: { kind: "unique", target: { test_id: "delete" } } },
  {
    obligation: "at-most-two-deletes",
    expected: { kind: "max_multiplicity", target: { test_id: "delete" }, max: 2 },
  },
  {
    obligation: "at-most-one-delete",
    expected: { kind: "max_multiplicity", target: { test_id: "delete" }, max: 1 },
  },
  {
    obligation: "export-receives-events",
    expected: {
      kind: "receives_events",
      target: { test_id: "export" },
      min_ratio_permille: 1000,
    },
  },
  {
    obligation: "blocked-receives-events",
    expected: {
      kind: "receives_events",
      target: { test_id: "blocked" },
      min_ratio_permille: 1000,
    },
  },
  {
    obligation: "export-inside-viewport",
    expected: { kind: "inside_viewport", target: { test_id: "export" }, margin_px: 0 },
  },
  {
    obligation: "offscreen-inside-viewport",
    expected: { kind: "inside_viewport", target: { test_id: "offscreen" }, margin_px: 0 },
  },
  {
    obligation: "roomy-text-not-clipped",
    expected: { kind: "text_not_clipped", target: { test_id: "roomy" } },
  },
  {
    obligation: "clipped-text-not-clipped",
    expected: { kind: "text_not_clipped", target: { test_id: "clipped" } },
  },
  {
    obligation: "export-clear-of-veil",
    expected: {
      kind: "no_overlap",
      target: { test_id: "export" },
      with: { test_id: "veil" },
      max_ratio_permille: 0,
    },
  },
  {
    obligation: "blocked-clear-of-veil",
    expected: {
      kind: "no_overlap",
      target: { test_id: "blocked" },
      with: { test_id: "veil" },
      max_ratio_permille: 0,
    },
  },
];

async function withDriver(
  run: (driver: PlaywrightDriver) => Promise<void>,
): Promise<void> {
  const server: Server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html" });
    response.end(PAGE);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  const evidence = await mkdtemp(join(tmpdir(), "wvq-pred-"));
  let driver;
  try {
    driver = await PlaywrightDriver.create(
      {
        id: "ui-predicates",
        obligations: ORACLES.map((oracle) => oracle.obligation),
        steps: [{ action: "navigate", route: "/" }],
      },
      ORACLES,
      {
        base_url: `http://127.0.0.1:${address.port}`,
        browser: "chromium",
        headless: true,
        timeout_ms: 15_000,
        evidence_dir: evidence,
      },
    );
    await driver.navigate("/");
    await run(driver);
  } finally {
    await driver?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
}

/** Whether the sealed assertion held, without swallowing bridge failures. */
async function holds(driver: PlaywrightDriver, obligation: string): Promise<boolean> {
  try {
    await driver.assert(obligation);
    return true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!message.startsWith("assertion_failed:")) throw error;
    return false;
  }
}

test("real Chromium evaluates every sealed UI predicate both ways", async () => {
  await withDriver(async (driver) => {
    // unique
    assert.equal(await holds(driver, "unique-save"), true, "one Save button is unique");
    assert.equal(
      await holds(driver, "unique-delete"),
      false,
      "two Delete buttons are not unique",
    );

    // max_multiplicity
    assert.equal(await holds(driver, "at-most-two-deletes"), true);
    assert.equal(await holds(driver, "at-most-one-delete"), false);

    // receives_events
    assert.equal(
      await holds(driver, "export-receives-events"),
      true,
      "an unobstructed button receives every probe",
    );
    assert.equal(
      await holds(driver, "blocked-receives-events"),
      false,
      "a button under a transparent veil receives none",
    );

    // inside_viewport
    assert.equal(await holds(driver, "export-inside-viewport"), true);
    assert.equal(await holds(driver, "offscreen-inside-viewport"), false);

    // text_not_clipped
    assert.equal(await holds(driver, "roomy-text-not-clipped"), true);
    assert.equal(await holds(driver, "clipped-text-not-clipped"), false);

    // no_overlap
    assert.equal(await holds(driver, "export-clear-of-veil"), true);
    assert.equal(await holds(driver, "blocked-clear-of-veil"), false);
  });
});

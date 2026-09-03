import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, "exact-head-validation.mjs");

test("observe-only step failure does not mark the exact-head manifest blocking", () => {
  const dir = mkdtempSync(join(tmpdir(), "wvq-exact-head-"));
  const out = join(dir, "exact-head-validation.json");
  const result = spawnSync(process.execPath, [script], {
    encoding: "utf8",
    env: {
      ...process.env,
      WVQ_VALIDATION_PATH: out,
      WVQ_STEP_PLAYWRIGHT: "success",
      WVQ_STEP_RECORDER: "success",
      WVQ_STEP_WORKSPACE: "success",
      WVQ_STEP_JAVASCRIPT: "success",
      WVQ_STEP_PACKAGE: "success",
      WVQ_STEP_CLIPPY: "success",
      WVQ_STEP_SPEC: "success",
      WVQ_STEP_DOCTOR: "success",
      WVQ_STEP_OBSERVE: "failure",
      GITHUB_SHA: "deadbeef",
    },
  });
  assert.equal(result.status, 0, result.stderr);
  const document = JSON.parse(readFileSync(out, "utf8"));
  assert.equal(document.steps.observe, "failure");
  assert.equal(document.blocking, false);
  assert.equal(document.steps.spec, "success");
});

test("a failed spec validate still blocks the exact-head manifest", () => {
  const dir = mkdtempSync(join(tmpdir(), "wvq-exact-head-"));
  const out = join(dir, "exact-head-validation.json");
  const result = spawnSync(process.execPath, [script], {
    encoding: "utf8",
    env: {
      ...process.env,
      WVQ_VALIDATION_PATH: out,
      WVQ_STEP_PLAYWRIGHT: "success",
      WVQ_STEP_RECORDER: "success",
      WVQ_STEP_WORKSPACE: "success",
      WVQ_STEP_JAVASCRIPT: "success",
      WVQ_STEP_PACKAGE: "success",
      WVQ_STEP_CLIPPY: "success",
      WVQ_STEP_SPEC: "failure",
      WVQ_STEP_DOCTOR: "success",
      WVQ_STEP_OBSERVE: "success",
    },
  });
  assert.equal(result.status, 1);
  const document = JSON.parse(readFileSync(out, "utf8"));
  assert.equal(document.blocking, true);
});

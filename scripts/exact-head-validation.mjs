#!/usr/bin/env node
// Exact-head validation manifest. Evidence of this commit's CI, not a STATUS claim.

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = process.env.WVQ_VALIDATION_PATH
  || join(root, "target", "exact-head-validation.json");

const steps = {
  playwright: process.env.WVQ_STEP_PLAYWRIGHT || "local",
  recorder: process.env.WVQ_STEP_RECORDER || "local",
  workspace: process.env.WVQ_STEP_WORKSPACE || "local",
  javascript: process.env.WVQ_STEP_JAVASCRIPT || "local",
  package: process.env.WVQ_STEP_PACKAGE || "local",
  clippy: process.env.WVQ_STEP_CLIPPY || "local",
  spec: process.env.WVQ_STEP_SPEC || "local",
  doctor: process.env.WVQ_STEP_DOCTOR || "local",
  observe: process.env.WVQ_STEP_OBSERVE || "local",
};

const blockingSteps = [
  "playwright",
  "recorder",
  "workspace",
  "javascript",
  "package",
  "clippy",
  "spec",
  "doctor",
];

const document = {
  schema_v: 1,
  product: "weavatrix-quality",
  commit: process.env.GITHUB_SHA || process.env.WVQ_COMMIT || "WORKTREE",
  workflow_run: process.env.GITHUB_RUN_ID || null,
  ref: process.env.GITHUB_REF || null,
  generated_at: new Date().toISOString(),
  steps,
  blocking: blockingSteps.some((name) => steps[name] === "failure"),
};

await mkdir(dirname(out), { recursive: true });
await writeFile(out, `${JSON.stringify(document, null, 2)}\n`);
process.stdout.write(`${out}\n`);
if (document.blocking) {
  process.exit(1);
}

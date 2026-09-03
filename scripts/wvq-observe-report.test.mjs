import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, "wvq-observe-report.mjs");

function run(doc) {
  const dir = mkdtempSync(join(tmpdir(), "wvq-observe-"));
  const file = join(dir, "verify.json");
  writeFileSync(file, `${JSON.stringify(doc)}\n`);
  return spawnSync(process.execPath, [script, file], { encoding: "utf8" });
}

test("observe-only verify JSON becomes GitHub notices and exit 0", () => {
  const result = run({
    command: "verify",
    body: {
      observe_only: true,
      state: "NOT_ENOUGH_EVIDENCE",
      verdict: "UNPROVEN",
      quality: {
        proof: { unproven_mandatory: ["unmeasured-never-clean"] },
        blocking_reasons: [
          {
            code: "WVQ-VERDICT-004",
            subject: "unmeasured-never-clean",
            detail: "high or critical obligation has no proof",
          },
        ],
      },
    },
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /::notice title=WVQ observe-only::NOT_ENOUGH_EVIDENCE \(UNPROVEN\)/);
  assert.match(result.stdout, /::notice title=WVQ-VERDICT-004::unmeasured-never-clean:/);
  assert.match(result.stdout, /CI exit stays 0/);
});

test("a blocking verify without observe-only is refused", () => {
  const result = run({
    command: "verify",
    body: { observe_only: false, state: "BLOCKED", verdict: "CONTRADICTED" },
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /observe_only/);
});

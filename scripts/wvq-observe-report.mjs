#!/usr/bin/env node
// Stage A observe-only reporter. Prints GitHub notices; never fails a verdict.

import { readFile } from "node:fs/promises";

const path = process.argv[2];
if (!path) {
  process.stderr.write("usage: node scripts/wvq-observe-report.mjs <verify.json>\n");
  process.exit(1);
}

const doc = JSON.parse(await readFile(path, "utf8"));
if (doc.command !== "verify" || !doc.body) {
  process.stderr.write("expected a wvq verify JSON document\n");
  process.exit(1);
}
const body = doc.body;
if (body.observe_only !== true) {
  process.stderr.write("observe report requires observe_only: true\n");
  process.exit(1);
}

const reasons = Array.isArray(body.quality?.blocking_reasons)
  ? body.quality.blocking_reasons
  : [];
const unproven = Array.isArray(body.quality?.proof?.unproven_mandatory)
  ? body.quality.proof.unproven_mandatory.length
  : 0;

process.stdout.write(
  `::notice title=WVQ observe-only::${github(body.state)} (${github(body.verdict)}), ${reasons.length} fired rule(s), ${unproven} unproven mandatory, CI exit stays 0\n`,
);
for (const reason of reasons.slice(0, 20)) {
  const title = github(reason.code || "WVQ");
  const text = github(`${reason.subject}: ${reason.detail}`);
  process.stdout.write(`::notice title=${title}::${text}\n`);
}

function github(value) {
  return String(value ?? "")
    .replaceAll("%", "%25")
    .replaceAll("\r", "%0D")
    .replaceAll("\n", "%0A");
}

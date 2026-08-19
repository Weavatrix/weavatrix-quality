import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { decodeRequest } from "../src/protocol.ts";
import { handle } from "../src/main.ts";
import { filterObservation } from "../src/observe.ts";

const fixtures = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "..",
  "fixtures",
  "browser",
);

test("golden initialize", () => {
  const line = readFileSync(join(fixtures, "protocol.initialize.json"), "utf8").trim();
  const req = decodeRequest(line);
  assert.equal(req.method, "initialize");
  assert.equal(req.id, 1);
});

test("unknown method fails closed", () => {
  const line = readFileSync(join(fixtures, "protocol.unknown.json"), "utf8").trim();
  assert.throws(() => decodeRequest(line), /unknown bridge method/);
});

test("screenshot omitted unless policy allows", () => {
  const filtered = filterObservation(
    {
      network: [],
      console: [],
      storage: {},
      screenshot_handle: "cas:shot",
    },
    {
      screenshot: "never",
      trace: "never",
      network: "always",
      console: "always",
      storage: "never",
    },
    true,
  );
  assert.equal(filtered.screenshot_handle, undefined);
});

test("handle initialize", () => {
  const reply = JSON.parse(
    handle(readFileSync(join(fixtures, "protocol.initialize.json"), "utf8").trim()),
  );
  assert.equal(reply.type, "ok");
});

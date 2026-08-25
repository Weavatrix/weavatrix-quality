import assert from "node:assert/strict";
import test from "node:test";
import {
  identifyRequestBytes,
  networkIdentity,
} from "../dist/request_identity.js";

test("json key order does not change the digest", () => {
  const left = identifyRequestBytes(
    "post",
    "/api/save",
    "application/json; charset=utf-8",
    Buffer.from('{"b":1,"a":{"z":2,"y":3}}'),
  );
  const right = identifyRequestBytes(
    "POST",
    "/api/save",
    "Application/JSON",
    Buffer.from('{"a":{"y":3,"z":2},"b":1}'),
  );
  assert.equal(left.body_digest, right.body_digest);
  assert.equal(
    networkIdentity("POST", "/api/save", left),
    networkIdentity("POST", "/api/save", right),
  );
  assert.match(networkIdentity("POST", "/api/save", left), /body:[a-f0-9]{64}/);
  assert.doesNotMatch(networkIdentity("POST", "/api/save", left), /"a"/);
});

test("a theme payload does not match a checkout payload on the same path", () => {
  const checkout = identifyRequestBytes(
    "POST",
    "/api/save",
    "application/json",
    Buffer.from('{"order":"42"}'),
  );
  const theme = identifyRequestBytes(
    "POST",
    "/api/save",
    "application/json",
    Buffer.from('{"theme":"dark"}'),
  );
  assert.notEqual(
    networkIdentity("POST", "/api/save", checkout),
    networkIdentity("POST", "/api/save", theme),
  );
});

test("graphql operations on the same path are distinct", () => {
  const checkout = identifyRequestBytes(
    "POST",
    "/graphql",
    "application/json",
    Buffer.from(
      '{"operationName":"Checkout","query":"query Checkout { order { id } }","variables":{"id":"1"}}',
    ),
  );
  const theme = identifyRequestBytes(
    "POST",
    "/graphql",
    "application/json",
    Buffer.from('{"query":"query Theme { palette }","variables":{}}'),
  );
  assert.equal(checkout.graphql?.operation_name, "Checkout");
  assert.equal(theme.graphql?.operation_name, "Theme");
  assert.notEqual(checkout.graphql?.query_digest, theme.graphql?.query_digest);
  assert.equal(checkout.body_digest, undefined);
  assert.doesNotMatch(
    networkIdentity("POST", "/graphql", checkout),
    /order \{ id \}/,
  );
});

test("graphql variable key order is canonical", () => {
  const left = identifyRequestBytes(
    "POST",
    "/graphql",
    "application/json",
    Buffer.from('{"query":"query Q($a:ID,$b:ID){n}","variables":{"b":2,"a":1}}'),
  );
  const right = identifyRequestBytes(
    "POST",
    "/graphql",
    "application/json",
    Buffer.from('{"query":"query Q($a:ID,$b:ID){n}","variables":{"a":1,"b":2}}'),
  );
  assert.equal(left.graphql?.variables_digest, right.graphql?.variables_digest);
});

test("graphql query whitespace does not change the digest", () => {
  const compact = identifyRequestBytes(
    "POST",
    "/graphql",
    "application/json",
    Buffer.from('{"query":"query Checkout { order { id } }"}'),
  );
  const spaced = identifyRequestBytes(
    "POST",
    "/graphql",
    "application/json",
    Buffer.from('{"query":"query   Checkout\\n{\\n  order { id }\\n}"}'),
  );
  assert.equal(compact.graphql?.query_digest, spaced.graphql?.query_digest);
});

test("empty bodies are identified without a digest", () => {
  const identity = identifyRequestBytes("GET", "/api/orders", "", undefined);
  assert.equal(identity.body_digest, undefined);
  assert.equal(identity.graphql, undefined);
  assert.equal(networkIdentity("GET", "/api/orders", identity), "GET /api/orders");
});

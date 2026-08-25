import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { PlaywrightDriver } from "../dist/playwright.js";
import { geometryMatches, resolveConfig, samplePoints } from "../dist/ui_integrity.js";

/** Serve one fixed HTML document on an ephemeral loopback port. */
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

async function collect(html: string, config: Record<string, unknown> = {}) {
  const { server, port } = await serve(html);
  const evidence = await mkdtemp(join(tmpdir(), "wvq-ui-"));
  const program = {
    id: "ui-fixture",
    obligations: ["rendered"],
    steps: [{ action: "navigate", route: "/" }],
  };
  const oracles = [{ obligation: "rendered", expected: { kind: "no_console_errors" } }];
  let driver;
  try {
    driver = await PlaywrightDriver.create(program, oracles, {
      base_url: `http://127.0.0.1:${port}`,
      browser: "chromium",
      headless: true,
      timeout_ms: 15_000,
      evidence_dir: evidence,
    });
    await driver.navigate("/");
    return await driver.collectUi(
      { revision: "rev-1", program: "ui-fixture", step: 0, stateDigest: "ab".repeat(32) },
      { enabled: true, ...config },
    );
  } finally {
    await driver?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
}

test("real Chromium collection produces a bounded, settled layout snapshot", async () => {
  const { snapshot, limitations } = await collect(`<!doctype html>
    <html><body>
      <main data-component="Toolbar">
        <button id="save" data-testid="save">Save</button>
        <button id="export" data-testid="export">Export</button>
      </main>
    </body></html>`);

  assert.equal(snapshot.schema_v, 2);
  assert.equal(snapshot.program, "ui-fixture");
  assert.equal(snapshot.step, 0);
  assert.equal(snapshot.route, "/");
  assert.deepEqual(limitations, [], "a static page must settle");
  assert.equal(snapshot.truncated, false);

  const save = snapshot.nodes.find((node) => node.dom_id === "save");
  assert(save, "the Save button must be a candidate");
  assert.equal(save.test_id, "save");
  assert.equal(save.role, "button");
  assert.equal(save.accessible_name, "Save");
  assert.equal(save.visible, true);
  assert.equal(save.interactive, true);
  assert.equal(save.enabled, true);
  assert.equal(save.pointer_events, true);
  assert(save.rects.length >= 1 && save.rects[0].width > 0);

  // Hit tests exist for enabled controls and resolve to the control itself.
  const samples = snapshot.hit_tests.filter((sample) => sample.target === save.id);
  assert(samples.length >= 5, `expected at least five probes, got ${samples.length}`);
  assert(
    samples.every((sample) => sample.topmost === save.id),
    "an unobstructed button owns every one of its probe points",
  );
});

test("accessibility facts are measured without exporting form values", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body>
      <button data-testid="unnamed"></button>
      <input data-testid="email" type="email" placeholder="Email address" value="private@example.invalid">
      <section role="region" aria-label="Paid"><div role="button" data-testid="checkout">Checkout</div></section>
      <section role="region" aria-label="Draft"><div role="button" data-testid="draft-checkout">Checkout</div></section>
      <div role="checkbox" data-testid="terms">Accept terms</div>
      <div role="dialog" aria-modal="true" data-testid="dialog"><button>Close</button></div>
    </body></html>`, {
    required_targets: [{
      role: "button",
      accessible_name: "Checkout",
      scope: { role: "region", accessible_name: "Paid" },
    }],
  });

  const unnamed = snapshot.nodes.find((node) => node.test_id === "unnamed");
  const email = snapshot.nodes.find((node) => node.test_id === "email");
  const checkout = snapshot.nodes.find((node) => node.test_id === "checkout");
  const draftCheckout = snapshot.nodes.find((node) => node.test_id === "draft-checkout");
  const terms = snapshot.nodes.find((node) => node.test_id === "terms");
  const dialog = snapshot.nodes.find((node) => node.test_id === "dialog");
  assert(unnamed && email && checkout && draftCheckout && terms && dialog);
  assert.equal(unnamed.tag, "button");
  assert.equal(unnamed.focusable, true);
  assert.equal(unnamed.accessible_name, undefined);
  assert.equal(email.input_type, "email");
  assert.equal(email.accessible_name, "Email address");
  assert.equal(email.label_associated, false, "a placeholder is not a label association");
  assert.equal(checkout.required_by_oracle, true);
  assert.equal(draftCheckout.required_by_oracle, false, "semantic scope stays exact");
  assert.equal(checkout.focusable, false);
  assert.equal(terms.aria_checked, undefined);
  assert.equal(dialog.modal, true);
  assert.equal(dialog.contains_focus, false);
  assert(!JSON.stringify(snapshot).includes("private@example.invalid"));
});

test("parsed media and container width breakpoints become bounded probe hints", async () => {
  const { snapshot, limitations } = await collect(`<!doctype html>
    <html><head><style media="(width <= 900px)">
      @media (max-width: 48rem) { body { padding: 1px } }
      main { container-type: inline-size }
      @container (inline-size < 640px) { button { width: 100% } }
    </style></head><body><main><button>Save</button></main></body></html>`, {
    responsive_breakpoints: true,
  });
  assert.deepEqual(snapshot.responsive_breakpoints, [640, 768, 900]);
  assert.equal(snapshot.responsive_breakpoints_complete, true);
  assert.deepEqual(limitations, []);
});

test("an overlay is recorded as the topmost node over the control it covers", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body>
      <button id="export" data-testid="export">Export</button>
      <div id="veil" style="position:fixed;inset:0;background:rgba(0,0,0,0.01)"></div>
    </body></html>`);

  const exportButton = snapshot.nodes.find((node) => node.dom_id === "export");
  const veil = snapshot.nodes.find((node) => node.dom_id === "veil");
  assert(exportButton && veil);
  const samples = snapshot.hit_tests.filter((sample) => sample.target === exportButton.id);
  assert(samples.length > 0);
  assert(
    samples.every((sample) => sample.topmost === veil.id),
    "the browser, not WVQ, decides what is on top",
  );
});

test("a pointer-events:none layer is recorded with pointer_events false", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body>
      <button id="export">Export</button>
      <div id="ghost" style="position:fixed;inset:0;pointer-events:none"></div>
    </body></html>`);
  const ghost = snapshot.nodes.find((node) => node.dom_id === "ghost");
  assert(ghost);
  assert.equal(ghost.pointer_events, false);
});

test("row scope is collected from data-entity so repeated actions stay distinct", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body>
      <ul>
        <li data-entity="order:1"><button>Delete</button></li>
        <li data-entity="order:2"><button>Delete</button></li>
      </ul>
    </body></html>`);
  const deletes = snapshot.nodes.filter((node) => node.accessible_name === "Delete");
  assert.equal(deletes.length, 2);
  assert.deepEqual(
    deletes.map((node) => node.entity_key).sort(),
    ["order:1", "order:2"],
  );
});

test("axe html never leaves the page", async () => {
  const { a11y_import } = await collect(`<!doctype html>
    <html><body>
      <button data-testid="pay">Pay</button>
      <script>
        window.axe = {
          run: async () => ({
            violations: [{
              id: "button-name",
              impact: "critical",
              nodes: [{
                html: "<button>secret-token-xyz</button>",
                failureSummary: "secret-token-xyz",
                target: ["[data-testid=pay]"]
              }]
            }]
          })
        };
      </script>
    </body></html>`);
  assert(a11y_import);
  assert.equal(a11y_import.producer, "axe-core");
  assert.equal(a11y_import.violations[0]?.id, "button-name");
  assert.deepEqual(a11y_import.violations[0]?.nodes[0]?.target, ["[data-testid=pay]"]);
  const encoded = JSON.stringify(a11y_import);
  assert.equal(encoded.includes("secret-token-xyz"), false);
  assert.equal(encoded.includes("\"html\""), false);
});

test("open shadow roots are entered", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body>
      <div id="host"></div>
      <script>
        const host = document.getElementById("host");
        const root = host.attachShadow({ mode: "open" });
        root.innerHTML = '<button id="shadow-pay" data-testid="shadow-pay">Pay</button>';
      </script>
    </body></html>`);
  const pay = snapshot.nodes.find((node) => node.test_id === "shadow-pay");
  assert(pay, "open shadow button must be a candidate");
  assert.equal(pay.accessible_name, "Pay");
  const host = snapshot.nodes.find((node) => node.dom_id === "host");
  assert(host);
  assert.equal(pay.parent, host.id);
});

test("same-origin iframe content is collected in top-level coordinates", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body style="margin:0">
      <iframe id="frame" style="position:absolute;left:40px;top:20px;width:200px;height:100px;border:0"
        srcdoc="<!doctype html><button id='inner' data-testid='inner-pay'>Pay</button>"></iframe>
    </body></html>`);
  const frame = snapshot.nodes.find((node) => node.dom_id === "frame");
  const pay = snapshot.nodes.find((node) => node.test_id === "inner-pay");
  assert(frame, "iframe itself is a surface");
  assert(pay, "same-origin iframe button must be a candidate");
  assert(pay.rects[0]);
  assert(
    pay.rects[0].x >= 40,
    `iframe child must be offset into the parent viewport, got x=${pay.rects[0].x}`,
  );
  assert.equal(pay.parent, frame.id);
});

test("cross-origin iframe is an opaque surface", async () => {
  const { snapshot, limitations } = await collect(`<!doctype html>
    <html><body>
      <iframe id="opaque" sandbox srcdoc="<!doctype html><button id='secret' data-testid='secret'>X</button>"></iframe>
    </body></html>`);
  assert.equal(
    snapshot.nodes.some((node) => node.test_id === "secret"),
    false,
    "opaque iframe content must not leak",
  );
  assert(snapshot.nodes.some((node) => node.dom_id === "opaque"));
  assert(
    limitations.some((item) => item.includes("opaque")),
    `${JSON.stringify(limitations)}`,
  );
});

test("clip_rect is the intersection of the whole overflow chain", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body style="margin:0">
      <div id="outer" style="overflow:hidden;width:40px;height:40px;position:relative">
        <div id="inner" style="overflow:hidden;width:80px;height:80px">
          <button id="pay" style="width:100px;height:100px">Pay</button>
        </div>
      </div>
    </body></html>`);
  const pay = snapshot.nodes.find((node) => node.dom_id === "pay");
  assert(pay);
  assert(pay.clip_rect, "a clipped button must carry clip_rect");
  assert(
    pay.clip_rect.width <= 41 && pay.clip_rect.height <= 41,
    `effective clip must be the 40x40 outer box, got ${pay.clip_rect.width}x${pay.clip_rect.height}`,
  );
});

test("text and document overflow metrics are collected", async () => {
  const { snapshot } = await collect(`<!doctype html>
    <html><body style="margin:0">
      <div id="cell" style="width:80px;overflow:hidden;white-space:nowrap;text-overflow:ellipsis">
        A very long piece of table text that cannot possibly fit
      </div>
      <div id="wide" style="width:2000px;height:10px"></div>
    </body></html>`);
  const cell = snapshot.nodes.find((node) => node.dom_id === "cell");
  assert(cell);
  assert(
    cell.text_scroll_width > cell.text_client_width,
    `expected clipped text, got ${cell.text_scroll_width} vs ${cell.text_client_width}`,
  );
  assert(
    snapshot.document.scroll_width > snapshot.document.client_width,
    "a 2000px child must make the document scroll horizontally",
  );
});

test("collection never records raw markup, form values, or unbounded text", async () => {
  const secret = "hunter2-super-secret-password";
  const { snapshot } = await collect(`<!doctype html>
    <html><body>
      <input id="password" type="password" value="${secret}" aria-label="Password">
      <p id="essay">${"lorem ipsum ".repeat(200)}</p>
    </body></html>`);
  const serialized = JSON.stringify(snapshot);
  assert(!serialized.includes(secret), "a form value must never reach evidence");
  assert(!serialized.includes("<input"), "raw markup must never reach evidence");
  for (const node of snapshot.nodes) {
    for (const field of [node.accessible_name, node.label, node.component_hint]) {
      if (field !== undefined) {
        assert(field.length <= 120, `field of ${field.length} chars exceeds the bound`);
      }
    }
  }
});

test("the node ceiling truncates instead of silently reporting a clean page", async () => {
  const rows = Array.from(
    { length: 60 },
    (_value, index) => `<button id="b${index}">Item ${index}</button>`,
  ).join("");
  const { snapshot, limitations } = await collect(
    `<!doctype html><html><body>${rows}</body></html>`,
    { max_nodes: 10 },
  );
  assert.equal(snapshot.nodes.length, 10);
  assert.equal(snapshot.truncated, true);
  assert(
    limitations.some((item) => item.includes("10-node ceiling")),
    `${JSON.stringify(limitations)}`,
  );
});

test("an animating layout is reported as unsettled rather than measured once", async () => {
  // A transform driven by JS outside the CSS animation system: freezing
  // animations cannot stop it, so the two reads must disagree.
  const { snapshot, limitations } = await collect(`<!doctype html>
    <html><body style="margin:0">
      <button id="drift" style="position:absolute;left:0">Drift</button>
      <script>
        let offset = 0;
        setInterval(() => {
          offset += 40;
          document.getElementById('drift').style.left = offset + 'px';
        }, 1);
      </script>
    </body></html>`);
  assert.equal(snapshot.truncated, true);
  assert(
    limitations.some((item) => item.includes("did not settle")),
    `${JSON.stringify(limitations)}`,
  );
});

test("collection reports zero runtime model tokens", async () => {
  const { snapshot } = await collect(
    `<!doctype html><html><body><button id="ok">OK</button></body></html>`,
  );
  // The whole collector is geometry arithmetic; nothing here can spend tokens.
  assert.equal(JSON.stringify(snapshot).includes("model"), false);
});

test("two screenshots of a focused input match after visual settle", async () => {
  const { server, port } = await serve(`<!doctype html>
    <html><body>
      <input id="name" value="Alice" autofocus>
    </body></html>`);
  const evidence = await mkdtemp(join(tmpdir(), "wvq-shot-"));
  let driver;
  try {
    driver = await PlaywrightDriver.create(
      {
        id: "visual-settle",
        obligations: ["rendered"],
        steps: [{ action: "navigate", route: "/" }],
        evidence_policy: {
          screenshot: "always",
          trace: "never",
          network: "never",
          console: "never",
          storage: "never",
        },
      },
      [{ obligation: "rendered", expected: { kind: "no_console_errors" } }],
      {
        base_url: `http://127.0.0.1:${port}`,
        browser: "chromium",
        headless: true,
        timeout_ms: 15_000,
        evidence_dir: evidence,
      },
    );
    await driver.navigate("/");
    const first = await driver.observe(false, true);
    await new Promise((resolve) => setTimeout(resolve, 600));
    const second = await driver.observe(false, true);
    assert(first.screenshot_path, "first observation must capture a png");
    assert(second.screenshot_path, "second observation must capture a png");
    const left = createHash("sha256")
      .update(await readFile(first.screenshot_path))
      .digest("hex");
    const right = createHash("sha256")
      .update(await readFile(second.screenshot_path))
      .digest("hex");
    assert.equal(left, right);
  } finally {
    await driver?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
});

test("config is clamped to the hard ceilings", () => {
  const resolved = resolveConfig({ max_nodes: 10_000_000, geometry_tolerance_px: -5 });
  assert.equal(resolved.max_nodes, 20_000);
  assert.equal(resolved.geometry_tolerance_px, 0);
  assert.equal(resolved.enabled, false, "collection is opt-in");
  assert.equal(resolved.test_id_attribute, "data-testid");
});

test("sample points cover the centre and corners, and grid a large target", () => {
  const small = samplePoints({ x: 0, y: 0, width: 20, height: 20 }, 2);
  assert.equal(small.length, 5);
  assert.deepEqual(small[0], { x: 10, y: 10 });

  const large = samplePoints({ x: 0, y: 0, width: 200, height: 100 }, 2);
  assert.equal(large.length, 9, "a large control gets a 3x3 grid");
});

test("geometry comparison honours the tolerance", () => {
  const node = (x: number) => [
    {
      id: "e1",
      rects: [{ x, y: 0, width: 10, height: 10 }],
      visible: true,
      interactive: false,
      enabled: true,
      pointer_events: true,
      scrollable: false,
      decorative: false,
      required_by_oracle: false,
    },
  ];
  assert.equal(geometryMatches(node(0), node(0.5), 1), true);
  assert.equal(geometryMatches(node(0), node(4), 1), false);
  assert.equal(geometryMatches(node(0), [], 1), false);
});

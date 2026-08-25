import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { PlaywrightDriver } from "../dist/playwright.js";

test("hover, scroll, and drag are first-class Playwright actions", async () => {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<!doctype html>
      <html><body style="margin:0;height:2400px">
        <button data-testid="file">File</button>
        <div data-testid="menu" hidden>Open</div>
        <button data-testid="footer" style="position:absolute;top:1800px">Footer</button>
        <div data-testid="chip" style="position:absolute;left:8px;top:80px;width:48px;height:24px;background:#333;color:#fff">Chip</div>
        <div data-testid="tray" style="position:absolute;left:200px;top:80px;width:120px;height:40px;border:1px solid #333"></div>
        <div data-testid="drop-status" hidden>Dropped</div>
        <script>
          const file = document.querySelector('[data-testid=file]');
          const menu = document.querySelector('[data-testid=menu]');
          file.addEventListener('mouseenter', () => { menu.hidden = false; });
          const chip = document.querySelector('[data-testid=chip]');
          const tray = document.querySelector('[data-testid=tray]');
          const dropped = document.querySelector('[data-testid=drop-status]');
          let dragging = false;
          chip.addEventListener('mousedown', () => { dragging = true; });
          document.addEventListener('mouseup', () => { dragging = false; });
          document.addEventListener('mousemove', (event) => {
            if (!dragging) return;
            chip.style.left = event.clientX - 24 + 'px';
            chip.style.top = event.clientY - 12 + 'px';
            const box = tray.getBoundingClientRect();
            if (event.clientX >= box.left && event.clientX <= box.right
                && event.clientY >= box.top && event.clientY <= box.bottom) {
              dropped.hidden = false;
            }
          });
        </script>
      </body></html>`);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert(address && typeof address === "object");
  const evidence = await mkdtemp(join(tmpdir(), "wvq-pointer-"));
  let driver;
  try {
    driver = await PlaywrightDriver.create(
      { id: "pointer-actions", obligations: ["ready"], steps: [{ action: "navigate", route: "/" }] },
      [{ obligation: "ready", expected: { kind: "no_console_errors" } }],
      {
        base_url: `http://127.0.0.1:${address.port}`,
        browser: "chromium",
        headless: true,
        timeout_ms: 8_000,
        evidence_dir: evidence,
      },
    );
    await driver.navigate("/");
    await driver.hover({ test_id: "file" });
    await driver.wait({ kind: "visible", target: { test_id: "menu" } });

    await driver.scroll({ test_id: "footer" });
    const afterScroll = await driver.collectUi(
      { revision: "rev-1", program: "pointer-actions", step: 1, stateDigest: "cd".repeat(32) },
      { enabled: true },
    );
    const footer = afterScroll.snapshot.nodes.find((node) => node.test_id === "footer");
    assert(footer, "footer must remain a collected node");
    assert.ok((footer.rects[0]?.y ?? 9_999) < 800, "scroll must bring the footer into view");

    await driver.drag({ test_id: "chip" }, { test_id: "tray" });
    await driver.wait({ kind: "visible", target: { test_id: "drop-status" } });
  } finally {
    await driver?.finish();
    await rm(evidence, { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
  }
});

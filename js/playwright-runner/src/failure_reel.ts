/** Diagnostic frames for a Playwright failure. Never a verdict. No AI. */

import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import type { Locator, Page } from "playwright";
import type { Target } from "./execute.js";
import { freezeAnimations, waitForFonts } from "./ui_integrity.js";

export type FailureReelCapture = {
  after_path?: string;
  highlight_path?: string;
  limitations: string[];
};

export type FailureReelRequest = {
  evidence_dir: string;
  program_id: string;
  step: number;
  target: Locator | null;
  target_applicable: boolean;
};

/** First semantic target nested in a sealed predicate. */
export function predicateTarget(predicate: unknown): Target | undefined {
  if (!predicate || typeof predicate !== "object" || Array.isArray(predicate)) return undefined;
  const value = predicate as { kind?: unknown; target?: unknown; predicates?: unknown; with?: unknown };
  if (value.target && typeof value.target === "object" && !Array.isArray(value.target)) {
    return value.target as Target;
  }
  if (value.with && typeof value.with === "object" && !Array.isArray(value.with)) {
    return value.with as Target;
  }
  if (Array.isArray(value.predicates)) {
    for (const child of value.predicates) {
      const found = predicateTarget(child);
      if (found) return found;
    }
  }
  if ("predicate" in value) return predicateTarget((value as { predicate?: unknown }).predicate);
  return undefined;
}

/**
 * Capture the after-frame and, when the target is still locatable, a highlighted
 * copy. Called only after a step failed. Passing runs never enter here.
 */
export async function captureFailureReelFrames(
  page: Page,
  request: FailureReelRequest,
): Promise<FailureReelCapture> {
  const limitations: string[] = [];
  await mkdir(request.evidence_dir, { recursive: true });
  await freezeAnimations(page);
  await waitForFonts(page, 2_000);

  const afterPath = reelPath(request, "after");
  await page.screenshot({
    path: afterPath,
    fullPage: true,
    animations: "disabled",
    caret: "hide",
  });

  if (!request.target_applicable) {
    limitations.push("target_not_applicable");
    return { after_path: afterPath, limitations };
  }
  if (!request.target) {
    limitations.push("target_not_located");
    return { after_path: afterPath, limitations };
  }

  const highlighted = await highlightLocator(request.target);
  if (!highlighted) {
    limitations.push("target_not_located");
    return { after_path: afterPath, limitations };
  }
  try {
    const highlightPath = reelPath(request, "highlight");
    await page.screenshot({
      path: highlightPath,
      fullPage: true,
      animations: "disabled",
      caret: "hide",
    });
    return { after_path: afterPath, highlight_path: highlightPath, limitations };
  } finally {
    await clearHighlight(request.target);
  }
}

export async function highlightLocator(locator: Locator): Promise<boolean> {
  const count = await locator.count();
  if (count !== 1) return false;
  const handle = await locator.elementHandle();
  if (!handle) return false;
  return handle.evaluate((node: Element) => {
    const el = node as HTMLElement;
    if (!el.style) return false;
    el.setAttribute("data-wvq-failure-highlight", "1");
    el.setAttribute("data-wvq-failure-outline", el.style.outline);
    el.setAttribute("data-wvq-failure-outline-offset", el.style.outlineOffset);
    el.style.outline = "3px solid #ff3b30";
    el.style.outlineOffset = "2px";
    return true;
  });
}

export async function clearHighlight(locator: Locator): Promise<void> {
  const handle = await locator.elementHandle().catch(() => null);
  if (!handle) return;
  await handle.evaluate((node: Element) => {
    const el = node as HTMLElement;
    if (!el.hasAttribute("data-wvq-failure-highlight")) return;
    el.style.outline = el.getAttribute("data-wvq-failure-outline") ?? "";
    el.style.outlineOffset = el.getAttribute("data-wvq-failure-outline-offset") ?? "";
    el.removeAttribute("data-wvq-failure-highlight");
    el.removeAttribute("data-wvq-failure-outline");
    el.removeAttribute("data-wvq-failure-outline-offset");
  }).catch(() => undefined);
}

function reelPath(request: FailureReelRequest, slot: string): string {
  return join(
    request.evidence_dir,
    `${safeName(request.program_id)}-reel-${request.step}-${slot}.png`,
  );
}

function safeName(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]/g, "-").slice(0, 100) || "program";
}

/**
 * Deterministic UI-integrity collection.
 *
 * This module measures; it never decides. Whether a duplicate, an occlusion, or
 * an overflow is a problem is Rust's answer â€” the policy, the detectors, and
 * the base/head ratchet all live in `wvq-ui`. Re-implementing any of that here
 * would give two answers to one question.
 *
 * Three properties matter more than coverage:
 *
 * - **Determinism.** Fonts are awaited, animations and transitions are frozen,
 *   and geometry is read twice and compared. An unsettled layout is reported as
 *   unstable rather than measured once and trusted.
 * - **Boundedness.** The candidate set is a filtered subset of the DOM, not the
 *   DOM. Node count, hit-test samples, and label length all have ceilings, and
 *   hitting one sets `truncated`.
 * - **Privacy.** No innerHTML, no form values, no cookies, no storage contents,
 *   no response bodies. Accessible names and labels are trimmed, collapsed, and
 *   cut to a fixed length before they ever leave the page.
 *
 * There is no AI here, and no vision. Everything below is `getBoundingClientRect`,
 * `getComputedStyle`, `elementsFromPoint`, and scroll-versus-client metrics.
 */

import type { Page } from "playwright";

/** Schema version Rust accepts. Bumping it is a breaking change. */
export const LAYOUT_SNAPSHOT_SCHEMA_V = 1;

/** Longest accessible name or label kept in evidence. Matches `wvq-ui`. */
export const MAX_LABEL_CHARS = 120;

/** Hard ceiling on collected nodes, whatever local policy asks for. */
export const HARD_MAX_NODES = 20_000;

/** Hard ceiling on hit-test samples in one snapshot. */
export const HARD_MAX_HIT_TESTS = 40_000;

export type Rect = { x: number; y: number; width: number; height: number };
export type Point = { x: number; y: number };

export type UiNode = {
  id: string;
  dom_id?: string;
  test_id?: string;
  role?: string;
  accessible_name?: string;
  label?: string;
  component_hint?: string;
  entity_key?: string;
  rects: Rect[];
  clip_rect?: Rect;
  visible: boolean;
  interactive: boolean;
  enabled: boolean;
  pointer_events: boolean;
  scrollable: boolean;
  decorative: boolean;
  position?: string;
  z_index?: number;
  stacking_context?: string;
  parent?: string;
  text_scroll_width?: number;
  text_client_width?: number;
  text_scroll_height?: number;
  text_client_height?: number;
};

export type HitTestSample = {
  target: string;
  point: Point;
  topmost?: string;
  stack: string[];
};

export type DocumentMetrics = {
  scroll_width: number;
  client_width: number;
  scroll_height: number;
  client_height: number;
};

export type LayoutSnapshot = {
  schema_v: number;
  revision: string;
  program: string;
  step: number;
  route: string;
  state_digest: string;
  viewport: { width: number; height: number };
  document: DocumentMetrics;
  nodes: UiNode[];
  hit_tests: HitTestSample[];
  truncated: boolean;
};

export type UiIntegrityConfig = {
  /** Whether collection runs at all. Off by default. */
  enabled?: boolean;
  /** Ceiling on collected nodes. Clamped to `HARD_MAX_NODES`. */
  max_nodes?: number;
  /** Geometry difference between the two reads that still counts as settled. */
  geometry_tolerance_px?: number;
  /** How long to wait for fonts and a settled layout, in milliseconds. */
  settle_timeout_ms?: number;
  /** Stable test attribute this project uses. */
  test_id_attribute?: string;
  /** Extra semantic targets sealed predicates name, so they are never dropped. */
  required_test_ids?: string[];
};

type ResolvedConfig = Required<Omit<UiIntegrityConfig, "required_test_ids">> & {
  required_test_ids: string[];
};

export function resolveConfig(config: UiIntegrityConfig | undefined): ResolvedConfig {
  const raw = config ?? {};
  const maxNodes = clampInt(raw.max_nodes ?? 5_000, 1, HARD_MAX_NODES);
  return {
    enabled: raw.enabled ?? false,
    max_nodes: maxNodes,
    geometry_tolerance_px: clampInt(raw.geometry_tolerance_px ?? 1, 0, 64),
    settle_timeout_ms: clampInt(raw.settle_timeout_ms ?? 2_000, 1, 30_000),
    test_id_attribute: raw.test_id_attribute ?? "data-testid",
    required_test_ids: (raw.required_test_ids ?? []).filter(
      (value) => typeof value === "string" && value.trim() !== "",
    ),
  };
}

function clampInt(value: number, low: number, high: number): number {
  if (!Number.isFinite(value)) return low;
  return Math.min(high, Math.max(low, Math.trunc(value)));
}

/** Everything one collection produced, plus why it may be incomplete. */
export type CollectionResult = {
  snapshot: LayoutSnapshot;
  /** Empty when the layout settled and every bound was respected. */
  limitations: string[];
};

/**
 * Collect one route/state/viewport.
 *
 * The caller supplies `revision`, `program`, `step`, and `stateDigest` so the
 * snapshot can be lined up against the same measurement point on the other
 * revision. Those four are the base/head comparison key; the DOM digest is
 * provenance only, because the change under review is what alters the markup.
 */
export async function collectLayoutSnapshot(
  page: Page,
  identity: { revision: string; program: string; step: number; stateDigest: string },
  config: UiIntegrityConfig | undefined,
): Promise<CollectionResult> {
  const resolved = resolveConfig(config);
  const limitations: string[] = [];

  await freezeAnimations(page);
  if (!(await waitForFonts(page, resolved.settle_timeout_ms))) {
    limitations.push("document.fonts.ready did not resolve before the settle timeout");
  }

  // Read the page twice. A layout still animating, lazily loading an image, or
  // reflowing after a late font swap will differ between the two reads, and a
  // single read of it would be a measurement of a transient state.
  const first = await readPage(page, resolved);
  const stable = await readPage(page, resolved);
  if (!geometryMatches(first.nodes, stable.nodes, resolved.geometry_tolerance_px)) {
    limitations.push(
      `layout did not settle: two reads ${resolved.geometry_tolerance_px}px apart disagreed`,
    );
  }
  if (stable.truncatedNodes) {
    limitations.push(
      `candidate set hit the ${resolved.max_nodes}-node ceiling; the snapshot is incomplete`,
    );
  }
  if (stable.truncatedHitTests) {
    limitations.push(`hit testing hit the ${HARD_MAX_HIT_TESTS}-sample ceiling`);
  }

  const viewport = page.viewportSize() ?? { width: 0, height: 0 };
  const snapshot: LayoutSnapshot = {
    schema_v: LAYOUT_SNAPSHOT_SCHEMA_V,
    revision: identity.revision,
    program: identity.program,
    step: identity.step,
    route: routeOf(page.url()),
    state_digest: identity.stateDigest,
    viewport,
    document: stable.document,
    nodes: stable.nodes,
    hit_tests: stable.hitTests,
    // Any limitation at all means this snapshot is not a clean measurement.
    truncated: limitations.length > 0,
  };
  return { snapshot, limitations };
}

/**
 * Stop time inside the page.
 *
 * Without this the same button is at two different places on two runs and the
 * ratchet reports a regression that is really an animation frame. The style is
 * additive and scoped to the automation run; it is never written to the app.
 */
async function freezeAnimations(page: Page): Promise<void> {
  await page.addStyleTag({
    content: `*, *::before, *::after {
      animation-delay: -1ms !important;
      animation-duration: 1ms !important;
      animation-iteration-count: 1 !important;
      transition-delay: -1ms !important;
      transition-duration: 1ms !important;
      scroll-behavior: auto !important;
      caret-color: transparent !important;
    }`,
  });
  // Drive any animation that was already running to its end state.
  await page.evaluate(() => {
    for (const animation of document.getAnimations()) {
      animation.finish();
    }
  });
}

async function waitForFonts(page: Page, timeoutMs: number): Promise<boolean> {
  try {
    return await page.evaluate(async (budget) => {
      const settled = await Promise.race([
        document.fonts.ready.then(() => true),
        new Promise<boolean>((resolve) => setTimeout(() => resolve(false), budget)),
      ]);
      return settled;
    }, timeoutMs);
  } catch {
    return false;
  }
}

type PageRead = {
  nodes: UiNode[];
  document: DocumentMetrics;
  hitTests: HitTestSample[];
  truncatedNodes: boolean;
  truncatedHitTests: boolean;
};

/**
 * One pass over the page, inside the browser.
 *
 * Traversal *and* hit testing happen in a single `evaluate`. Splitting them
 * would mean deriving node identities twice from two copies of the candidate
 * rule, which can drift; worse, the geometry and the hit-test results would
 * describe two different moments. One pass, one DOM state, one set of
 * identities.
 *
 * Everything it returns is a plain, bounded, JSON-safe value.
 */
async function readPage(page: Page, config: ResolvedConfig): Promise<PageRead> {
  return page.evaluate(
    ({ maxNodes, testIdAttribute, requiredTestIds, maxLabel, maxHitTests, inset }) => {
      const INTERACTIVE_TAGS = new Set([
        "A",
        "BUTTON",
        "INPUT",
        "SELECT",
        "TEXTAREA",
        "SUMMARY",
        "OPTION",
      ]);
      const INTERACTIVE_ROLES = new Set([
        "button",
        "link",
        "checkbox",
        "radio",
        "switch",
        "tab",
        "menuitem",
        "menuitemcheckbox",
        "menuitemradio",
        "option",
        "textbox",
        "combobox",
        "searchbox",
        "slider",
        "spinbutton",
      ]);
      const SURFACE_ROLES = new Set([
        "dialog",
        "alertdialog",
        "menu",
        "listbox",
        "alert",
        "status",
        "tooltip",
        "banner",
        "navigation",
      ]);

      /** Bound and redact any text before it leaves the page. */
      const text = (raw: string | null | undefined): string | undefined => {
        if (!raw) return undefined;
        const collapsed = raw.replace(/\s+/g, " ").trim();
        if (collapsed === "") return undefined;
        return collapsed.length > maxLabel ? collapsed.slice(0, maxLabel) : collapsed;
      };

      const roleOf = (element: Element): string | undefined => {
        const explicit = element.getAttribute("role");
        if (explicit) return text(explicit.split(/\s+/)[0] ?? explicit);
        const tag = element.tagName;
        if (tag === "BUTTON" || (tag === "INPUT" && (element as HTMLInputElement).type === "button"))
          return "button";
        if (tag === "A" && element.hasAttribute("href")) return "link";
        if (tag === "TEXTAREA") return "textbox";
        if (tag === "SELECT") return "combobox";
        if (tag === "INPUT") {
          const type = (element as HTMLInputElement).type;
          if (type === "checkbox" || type === "radio") return type;
          if (type === "submit" || type === "reset") return "button";
          return "textbox";
        }
        if (/^H[1-6]$/.test(tag)) return "heading";
        if (tag === "DIALOG") return "dialog";
        if (tag === "IMG") return "img";
        return undefined;
      };

      /**
       * Accessible name, from the attributes that carry it. Deliberately not
       * `innerText` for inputs: a text field's value is user data.
       */
      const nameOf = (element: Element): string | undefined => {
        const aria = element.getAttribute("aria-label");
        if (aria) return text(aria);
        const labelledBy = element.getAttribute("aria-labelledby");
        if (labelledBy) {
          const parts = labelledBy
            .split(/\s+/)
            .map((token) => document.getElementById(token)?.textContent ?? "")
            .join(" ");
          const resolved = text(parts);
          if (resolved) return resolved;
        }
        if (element.tagName === "IMG") return text(element.getAttribute("alt"));
        if (element.tagName === "INPUT" || element.tagName === "TEXTAREA") {
          const labels = (element as HTMLInputElement).labels;
          const first = labels?.[0];
          if (first) return text(first.textContent);
          return text(element.getAttribute("placeholder"));
        }
        const title = element.getAttribute("title");
        // `textContent` is only this element's own label when the element is a
        // control or a leaf. On a container it is the concatenated text of
        // everything inside, which is both noise and a needless copy of page
        // content: a list row would claim the name of the button it holds.
        const ownsItsText =
          INTERACTIVE_TAGS.has(element.tagName) || element.childElementCount === 0;
        const own = ownsItsText ? text(element.textContent) : undefined;
        return own ?? text(title);
      };

      const isEnabled = (element: Element): boolean =>
        !element.hasAttribute("disabled") &&
        element.getAttribute("aria-disabled") !== "true" &&
        !(element as HTMLInputElement).disabled;

      const scrollableStyle = (style: CSSStyleDeclaration): boolean =>
        ["auto", "scroll", "overlay"].includes(style.overflowX) ||
        ["auto", "scroll", "overlay"].includes(style.overflowY);

      const clipsChildren = (style: CSSStyleDeclaration): boolean =>
        ["hidden", "clip", "auto", "scroll"].includes(style.overflowX) ||
        ["hidden", "clip", "auto", "scroll"].includes(style.overflowY);

      const rectOf = (element: Element): Rect => {
        const box = element.getBoundingClientRect();
        return { x: box.x, y: box.y, width: box.width, height: box.height };
      };

      const nodes: UiNode[] = [];
      const assigned = new Map<Element, string>();
      let truncated = false;
      let counter = 0;

      /**
       * The candidate set. Collecting the whole DOM would be unbounded and
       * mostly noise, so an element earns a place by being something a
       * regression can break: an operable control, a machine identity a test
       * depends on, a surface that floats over other things, a node whose text
       * or box already overflows, or something a design system named.
       */
      const isCandidate = (element: Element, style: CSSStyleDeclaration): boolean => {
        if (element.id) return true;
        if (element.hasAttribute(testIdAttribute)) return true;
        if (INTERACTIVE_TAGS.has(element.tagName)) return true;
        const role = element.getAttribute("role");
        if (role && (INTERACTIVE_ROLES.has(role) || SURFACE_ROLES.has(role))) return true;
        if (element.tagName === "DIALOG") return true;
        if (element.hasAttribute("data-component") || element.hasAttribute("data-entity")) {
          return true;
        }
        if (scrollableStyle(style)) return true;
        if (
          element.scrollWidth > element.clientWidth ||
          element.scrollHeight > element.clientHeight
        ) {
          return true;
        }
        return ["fixed", "sticky", "absolute"].includes(style.position);
      };

      /** Nearest collected ancestor, so parent links stay inside the subset. */
      const nearestCollected = (element: Element): string | undefined => {
        let current = element.parentElement;
        while (current) {
          const found = assigned.get(current);
          if (found) return found;
          current = current.parentElement;
        }
        return undefined;
      };

      const nearestClip = (element: Element): Rect | undefined => {
        let current = element.parentElement;
        while (current) {
          const style = getComputedStyle(current);
          if (clipsChildren(style)) return rectOf(current);
          current = current.parentElement;
        }
        return undefined;
      };

      const nearestEntity = (element: Element): string | undefined => {
        let current: Element | null = element;
        while (current) {
          const entity = current.getAttribute("data-entity");
          if (entity) return text(entity);
          current = current.parentElement;
        }
        return undefined;
      };

      // Document order keeps the snapshot stable across runs; a parent is always
      // visited before its children, so `nearestCollected` can resolve.
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
      const required = new Set(requiredTestIds);
      let element = document.body as Element | null;
      while (element) {
        const style = getComputedStyle(element);
        const box = rectOf(element);
        const hidden =
          style.display === "none" ||
          style.visibility === "hidden" ||
          style.visibility === "collapse" ||
          Number(style.opacity) === 0;
        const testId = element.getAttribute(testIdAttribute) ?? undefined;
        const mustKeep = testId !== undefined && required.has(testId);

        if (mustKeep || isCandidate(element, style)) {
          if (nodes.length >= maxNodes) {
            truncated = true;
            break;
          }
          counter += 1;
          const nodeId = `e${counter}`;
          assigned.set(element, nodeId);
          const role = roleOf(element);
          const interactive =
            INTERACTIVE_TAGS.has(element.tagName) ||
            (role !== undefined && INTERACTIVE_ROLES.has(role)) ||
            element.hasAttribute("onclick") ||
            (element as HTMLElement).tabIndex >= 0;
          const clientRects = Array.from(element.getClientRects()).map((rect) => ({
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
          }));
          const node: UiNode = {
            id: nodeId,
            visible: !hidden && box.width > 0 && box.height > 0,
            interactive,
            enabled: isEnabled(element),
            pointer_events: style.pointerEvents !== "none",
            scrollable: scrollableStyle(style),
            decorative:
              element.getAttribute("aria-hidden") === "true" ||
              element.getAttribute("role") === "presentation" ||
              element.getAttribute("role") === "none",
            rects: clientRects.length > 0 ? clientRects : [box],
          };
          const domId = text(element.id);
          if (domId) node.dom_id = domId;
          const stableId = text(testId);
          if (stableId) node.test_id = stableId;
          if (role) node.role = role;
          const name = nameOf(element);
          if (name) node.accessible_name = name;
          const label = text(element.getAttribute("aria-label"));
          if (label) node.label = label;
          const component = text(element.getAttribute("data-component"));
          if (component) node.component_hint = component;
          const entity = nearestEntity(element);
          if (entity) node.entity_key = entity;
          const clip = nearestClip(element);
          if (clip) node.clip_rect = clip;
          if (style.position !== "static") node.position = style.position;
          const zIndex = Number.parseInt(style.zIndex, 10);
          if (Number.isFinite(zIndex)) node.z_index = zIndex;
          const parent = nearestCollected(element);
          if (parent) node.parent = parent;
          node.text_scroll_width = element.scrollWidth;
          node.text_client_width = element.clientWidth;
          node.text_scroll_height = element.scrollHeight;
          node.text_client_height = element.clientHeight;
          nodes.push(node);
        }
        element = walker.nextNode() as Element | null;
      }

      // ---- hit testing, against the identities just assigned ----------------

      const unionOf = (rects: Rect[]): Rect | undefined => {
        const usable = rects.filter((rect) => rect.width > 0 && rect.height > 0);
        const seed = usable[0];
        if (!seed) return undefined;
        let left = seed.x;
        let top = seed.y;
        let right = seed.x + seed.width;
        let bottom = seed.y + seed.height;
        for (const rect of usable.slice(1)) {
          left = Math.min(left, rect.x);
          top = Math.min(top, rect.y);
          right = Math.max(right, rect.x + rect.width);
          bottom = Math.max(bottom, rect.y + rect.height);
        }
        return { x: left, y: top, width: right - left, height: bottom - top };
      };

      /**
       * Centre plus four inset corners catches a control covered edge-on. A
       * target big enough for nine distinguishable points gets a 3x3 grid: a
       * large button can be half covered while its centre still responds, which
       * is exactly what a centre-only probe would call healthy.
       *
       * Kept in step with the exported `samplePoints` helper.
       */
      const pointsFor = (rect: Rect): Point[] => {
        const left = rect.x + inset;
        const centreX = rect.x + rect.width / 2;
        const right = rect.x + rect.width - inset;
        const top = rect.y + inset;
        const centreY = rect.y + rect.height / 2;
        const bottom = rect.y + rect.height - inset;
        if (rect.width >= inset * 12 && rect.height >= inset * 12) {
          const grid: Point[] = [];
          for (const y of [top, centreY, bottom]) {
            for (const x of [left, centreX, right]) grid.push({ x, y });
          }
          return grid;
        }
        return [
          { x: centreX, y: centreY },
          { x: left, y: top },
          { x: right, y: top },
          { x: left, y: bottom },
          { x: right, y: bottom },
        ];
      };

      const identify = (found: Element): string | undefined => {
        let current: Element | null = found;
        while (current) {
          const id = assigned.get(current);
          if (id) return id;
          current = current.parentElement;
        }
        return undefined;
      };

      const byId = new Map(nodes.map((node) => [node.id, node]));
      const hitTests: HitTestSample[] = [];
      let truncatedHitTests = false;
      for (const [candidate, nodeId] of assigned) {
        const node = byId.get(nodeId);
        if (!node || !node.visible || !node.interactive || !node.enabled) continue;
        if (!node.pointer_events) continue;
        const bounds = unionOf(node.rects);
        if (!bounds) continue;
        const points = pointsFor(bounds);
        if (hitTests.length + points.length > maxHitTests) {
          truncatedHitTests = true;
          break;
        }
        for (const point of points) {
          const stack = document
            .elementsFromPoint(point.x, point.y)
            .map(identify)
            .filter((value): value is string => value !== undefined);
          const sample: HitTestSample = { target: nodeId, point, stack };
          const topmost = stack[0];
          if (topmost) sample.topmost = topmost;
          hitTests.push(sample);
        }
        void candidate;
      }

      const root = document.documentElement;
      return {
        nodes,
        hitTests,
        truncatedNodes: truncated,
        truncatedHitTests,
        document: {
          scroll_width: root.scrollWidth,
          client_width: root.clientWidth,
          scroll_height: root.scrollHeight,
          client_height: root.clientHeight,
        },
      };
    },
    {
      maxNodes: config.max_nodes,
      testIdAttribute: config.test_id_attribute,
      requiredTestIds: config.required_test_ids,
      maxLabel: MAX_LABEL_CHARS,
      maxHitTests: HARD_MAX_HIT_TESTS,
      inset: 2,
    },
  );
}

/** Whether two reads of the same page agree on where everything is. */
export function geometryMatches(left: UiNode[], right: UiNode[], tolerance: number): boolean {
  if (left.length !== right.length) return false;
  for (let index = 0; index < left.length; index += 1) {
    const a = left[index];
    const b = right[index];
    if (!a || !b || a.id !== b.id || a.rects.length !== b.rects.length) return false;
    for (let rect = 0; rect < a.rects.length; rect += 1) {
      const first = a.rects[rect];
      const second = b.rects[rect];
      if (!first || !second) return false;
      if (
        Math.abs(first.x - second.x) > tolerance ||
        Math.abs(first.y - second.y) > tolerance ||
        Math.abs(first.width - second.width) > tolerance ||
        Math.abs(first.height - second.height) > tolerance
      ) {
        return false;
      }
    }
  }
  return true;
}

/**
 * Probe points on one control, exported so the sampling rule can be tested
 * without a browser. The in-page collector uses the same shape.
 *
 * Centre plus four inset corners catches a control covered edge-on. A target
 * big enough for nine distinguishable points gets a 3x3 grid: a large button
 * can be half covered while its centre still responds, which is exactly what a
 * centre-only probe would call healthy.
 */
export function samplePoints(rect: Rect, inset: number): Point[] {
  const [left, centreX, right] = [
    rect.x + inset,
    rect.x + rect.width / 2,
    rect.x + rect.width - inset,
  ];
  const [top, centreY, bottom] = [
    rect.y + inset,
    rect.y + rect.height / 2,
    rect.y + rect.height - inset,
  ];
  if (rect.width >= inset * 12 && rect.height >= inset * 12) {
    const grid: Point[] = [];
    for (const y of [top, centreY, bottom]) {
      for (const x of [left, centreX, right]) grid.push({ x, y });
    }
    return grid;
  }
  return [
    { x: centreX, y: centreY },
    { x: left, y: top },
    { x: right, y: top },
    { x: left, y: bottom },
    { x: right, y: bottom },
  ];
}

function routeOf(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.pathname}${parsed.search}${parsed.hash}`;
  } catch {
    return url;
  }
}

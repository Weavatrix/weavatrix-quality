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
/** Schema version Rust accepts. Bumping it is a breaking change. */
export const LAYOUT_SNAPSHOT_SCHEMA_V = 2;
/** Longest accessible name or label kept in evidence. Matches `wvq-ui`. */
export const MAX_LABEL_CHARS = 120;
/** Hard ceiling on collected nodes, whatever local policy asks for. */
export const HARD_MAX_NODES = 20_000;
/** Hard ceiling on hit-test samples in one snapshot. */
export const HARD_MAX_HIT_TESTS = 40_000;
export function resolveConfig(config) {
    const raw = config ?? {};
    const maxNodes = clampInt(raw.max_nodes ?? 5_000, 1, HARD_MAX_NODES);
    return {
        enabled: raw.enabled ?? false,
        max_nodes: maxNodes,
        geometry_tolerance_px: clampInt(raw.geometry_tolerance_px ?? 1, 0, 64),
        settle_timeout_ms: clampInt(raw.settle_timeout_ms ?? 2_000, 1, 30_000),
        test_id_attribute: raw.test_id_attribute ?? "data-testid",
        required_test_ids: (raw.required_test_ids ?? []).filter((value) => typeof value === "string" && value.trim() !== ""),
        required_targets: normalizeRequiredTargets(raw.required_targets ?? []),
        responsive_breakpoints: raw.responsive_breakpoints ?? false,
    };
}
function clampInt(value, low, high) {
    if (!Number.isFinite(value))
        return low;
    return Math.min(high, Math.max(low, Math.trunc(value)));
}
/**
 * Collect one route/state/viewport.
 *
 * The caller supplies `revision`, `program`, `step`, and `stateDigest` so the
 * snapshot can be lined up against the same measurement point on the other
 * revision. Those four are the base/head comparison key; the DOM digest is
 * provenance only, because the change under review is what alters the markup.
 */
export async function collectLayoutSnapshot(page, identity, config) {
    const resolved = resolveConfig(config);
    const limitations = [];
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
        limitations.push(`layout did not settle: two reads ${resolved.geometry_tolerance_px}px apart disagreed`);
    }
    if (stable.truncatedNodes) {
        limitations.push(`candidate set hit the ${resolved.max_nodes}-node ceiling; the snapshot is incomplete`);
    }
    if (stable.truncatedHitTests) {
        limitations.push(`hit testing hit the ${HARD_MAX_HIT_TESTS}-sample ceiling`);
    }
    const viewport = page.viewportSize() ?? { width: 0, height: 0 };
    const responsive = resolved.responsive_breakpoints
        ? await readResponsiveBreakpoints(page)
        : { widths: [], complete: true };
    if (!responsive.complete) {
        limitations.push("responsive breakpoint discovery could not inspect every applied stylesheet");
    }
    const snapshot = {
        schema_v: LAYOUT_SNAPSHOT_SCHEMA_V,
        revision: identity.revision,
        program: identity.program,
        step: identity.step,
        route: routeOf(page.url()),
        state_digest: identity.stateDigest,
        viewport,
        responsive_breakpoints: responsive.widths,
        responsive_breakpoints_complete: responsive.complete,
        document: stable.document,
        nodes: stable.nodes,
        hit_tests: stable.hitTests,
        // Any limitation at all means this snapshot is not a clean measurement.
        truncated: limitations.length > 0,
    };
    return { snapshot, limitations };
}
function normalizeRequiredTargets(targets) {
    if (!Array.isArray(targets) || targets.length > 256) {
        throw new Error("required accessibility targets exceed the 256-target ceiling");
    }
    const fields = ["role", "accessible_name", "label", "test_id", "component_hint"];
    const allowed = new Set([...fields, "scope", "fallback_css"]);
    const normalize = (target, depth) => {
        if (depth > 4)
            throw new Error("required accessibility target scope exceeds depth 4");
        if (!target || typeof target !== "object" || Array.isArray(target)) {
            throw new Error("required accessibility target must be an object");
        }
        for (const field of Object.keys(target)) {
            if (!allowed.has(field))
                throw new Error(`required accessibility target field ${field} is unknown`);
        }
        const result = {};
        for (const field of fields) {
            const value = target[field];
            if (value === undefined)
                continue;
            if (typeof value !== "string" || value.trim() === "" || value.length > MAX_LABEL_CHARS) {
                throw new Error(`required accessibility target ${field} is invalid`);
            }
            result[field] = value.trim();
        }
        if (target.fallback_css !== undefined) {
            if (typeof target.fallback_css !== "string" ||
                target.fallback_css.trim() === "" ||
                target.fallback_css.length > 512)
                throw new Error("required accessibility target fallback_css is invalid");
            result.fallback_css = target.fallback_css.trim();
        }
        if (target.scope !== undefined)
            result.scope = normalize(target.scope, depth + 1);
        if (Object.keys(result).every((field) => field === "scope")) {
            throw new Error("required accessibility target has no semantic identity");
        }
        return result;
    };
    const normalized = targets.map((target) => normalize(target, 0));
    return Array.from(new Map(normalized.map((target) => [JSON.stringify(target), target])).values());
}
/** Read parsed media/container width conditions; source text is never reparsed. */
async function readResponsiveBreakpoints(page) {
    return page.evaluate(() => {
        const widths = new Set();
        let complete = true;
        const rootPx = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
        const add = (raw, unit) => {
            const value = Number.parseFloat(raw);
            const pixels = unit.toLowerCase() === "px" ? value : value * rootPx;
            if (Number.isFinite(pixels)) {
                const rounded = Math.round(pixels);
                if (rounded >= 1 && rounded <= 16_384)
                    widths.add(rounded);
            }
        };
        const inspectCondition = (condition) => {
            const patterns = [
                /(?:min-|max-)?(?:width|inline-size)\s*:\s*(-?\d+(?:\.\d+)?)\s*(px|rem|em)/gi,
                /(?:width|inline-size)\s*[<>=]+\s*(-?\d+(?:\.\d+)?)\s*(px|rem|em)/gi,
                /(-?\d+(?:\.\d+)?)\s*(px|rem|em)\s*[<>=]+\s*(?:width|inline-size)/gi,
            ];
            for (const pattern of patterns) {
                for (const match of condition.matchAll(pattern)) {
                    const value = match[1];
                    const unit = match[2];
                    if (value && unit)
                        add(value, unit);
                }
            }
        };
        const visit = (rules) => {
            for (const rule of Array.from(rules)) {
                const candidate = rule;
                if (typeof candidate.conditionText === "string")
                    inspectCondition(candidate.conditionText);
                if (candidate.cssRules)
                    visit(candidate.cssRules);
            }
        };
        for (const sheet of Array.from(document.styleSheets)) {
            try {
                if (sheet.media.mediaText)
                    inspectCondition(sheet.media.mediaText);
                visit(sheet.cssRules);
            }
            catch {
                complete = false;
            }
        }
        return { widths: Array.from(widths).sort((a, b) => a - b).slice(0, 128), complete };
    });
}
/**
 * Stop time inside the page.
 *
 * Without this the same button is at two different places on two runs and the
 * ratchet reports a regression that is really an animation frame. The style is
 * additive and scoped to the automation run; it is never written to the app.
 */
async function freezeAnimations(page) {
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
async function waitForFonts(page, timeoutMs) {
    try {
        return await page.evaluate(async (budget) => {
            const settled = await Promise.race([
                document.fonts.ready.then(() => true),
                new Promise((resolve) => setTimeout(() => resolve(false), budget)),
            ]);
            return settled;
        }, timeoutMs);
    }
    catch {
        return false;
    }
}
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
async function readPage(page, config) {
    return page.evaluate(({ maxNodes, testIdAttribute, requiredTestIds, requiredTargets, maxLabel, maxHitTests, inset, }) => {
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
        const text = (raw) => {
            if (!raw)
                return undefined;
            const collapsed = raw.replace(/\s+/g, " ").trim();
            if (collapsed === "")
                return undefined;
            return collapsed.length > maxLabel ? collapsed.slice(0, maxLabel) : collapsed;
        };
        const roleOf = (element) => {
            const explicit = element.getAttribute("role");
            if (explicit)
                return text(explicit.split(/\s+/)[0] ?? explicit);
            const tag = element.tagName;
            if (tag === "BUTTON" || (tag === "INPUT" && element.type === "button"))
                return "button";
            if (tag === "A" && element.hasAttribute("href"))
                return "link";
            if (tag === "TEXTAREA")
                return "textbox";
            if (tag === "SELECT")
                return "combobox";
            if (tag === "INPUT") {
                const type = element.type;
                if (type === "checkbox" || type === "radio")
                    return type;
                if (type === "submit" || type === "reset")
                    return "button";
                return "textbox";
            }
            if (/^H[1-6]$/.test(tag))
                return "heading";
            if (tag === "DIALOG")
                return "dialog";
            if (tag === "IMG")
                return "img";
            return undefined;
        };
        /**
         * Accessible name, from the attributes that carry it. Deliberately not
         * `innerText` for inputs: a text field's value is user data.
         */
        const nameOf = (element) => {
            const aria = element.getAttribute("aria-label");
            if (aria)
                return text(aria);
            const labelledBy = element.getAttribute("aria-labelledby");
            if (labelledBy) {
                const parts = labelledBy
                    .split(/\s+/)
                    .map((token) => document.getElementById(token)?.textContent ?? "")
                    .join(" ");
                const resolved = text(parts);
                if (resolved)
                    return resolved;
            }
            if (element.tagName === "IMG")
                return text(element.getAttribute("alt"));
            if (element instanceof HTMLInputElement ||
                element instanceof HTMLSelectElement ||
                element instanceof HTMLTextAreaElement) {
                const labels = element.labels;
                const first = labels?.[0];
                if (first)
                    return text(first.textContent);
                if (element instanceof HTMLInputElement &&
                    ["button", "submit", "reset"].includes(element.type))
                    return text(element.value);
                return text(element.getAttribute("placeholder"));
            }
            const title = element.getAttribute("title");
            // `textContent` is only this element's own label when the element is a
            // control or a leaf. On a container it is the concatenated text of
            // everything inside, which is both noise and a needless copy of page
            // content: a list row would claim the name of the button it holds.
            const ownsItsText = INTERACTIVE_TAGS.has(element.tagName) ||
                INTERACTIVE_ROLES.has(roleOf(element) ?? "") ||
                element.childElementCount === 0;
            const own = ownsItsText ? text(element.textContent) : undefined;
            return own ?? text(title);
        };
        const isEnabled = (element) => !element.hasAttribute("disabled") &&
            element.getAttribute("aria-disabled") !== "true" &&
            !element.disabled;
        const labelAssociated = (element) => {
            if (text(element.getAttribute("aria-label")))
                return true;
            const labelledBy = element.getAttribute("aria-labelledby");
            if (labelledBy?.split(/\s+/).some((token) => Boolean(text(document.getElementById(token)?.textContent))))
                return true;
            if (element instanceof HTMLInputElement ||
                element instanceof HTMLSelectElement ||
                element instanceof HTMLTextAreaElement)
                return (element.labels?.length ?? 0) > 0;
            return false;
        };
        const naturallyFocusable = (element) => {
            if (!isEnabled(element))
                return false;
            if (element instanceof HTMLAnchorElement)
                return element.hasAttribute("href");
            if (element instanceof HTMLInputElement)
                return element.type !== "hidden";
            return element instanceof HTMLButtonElement ||
                element instanceof HTMLSelectElement ||
                element instanceof HTMLTextAreaElement ||
                element instanceof HTMLElement && element.tagName === "SUMMARY";
        };
        const focusable = (element) => {
            if (!isEnabled(element))
                return false;
            const explicit = element.getAttribute("tabindex");
            return naturallyFocusable(element) ||
                explicit !== null && Number.isInteger(Number(explicit)) && Number(explicit) >= 0;
        };
        const isModal = (element) => {
            if (element.getAttribute("aria-modal") === "true")
                return true;
            if (!(element instanceof HTMLDialogElement) || !element.open)
                return false;
            try {
                return element.matches(":modal");
            }
            catch {
                return false;
            }
        };
        const targetFacts = (element) => ({
            role: roleOf(element),
            name: nameOf(element),
            label: text(element.getAttribute("aria-label")),
            testId: text(element.getAttribute(testIdAttribute)),
            component: text(element.getAttribute("data-component")),
        });
        const requiredTargetMatches = (target, element) => {
            const facts = targetFacts(element);
            const comparisons = [
                [target.role, facts.role],
                [target.accessible_name, facts.name],
                [target.label, facts.label ?? facts.name],
                [target.test_id, facts.testId],
                [target.component_hint, facts.component],
            ];
            let named = false;
            for (const [expected, actual] of comparisons) {
                if (expected === undefined)
                    continue;
                named = true;
                if (expected !== actual)
                    return false;
            }
            if (target.fallback_css !== undefined) {
                named = true;
                try {
                    if (!element.matches(target.fallback_css))
                        return false;
                }
                catch {
                    return false;
                }
            }
            if (target.scope !== undefined) {
                let ancestor = element.parentElement;
                let matched = false;
                while (ancestor) {
                    if (requiredTargetMatches(target.scope, ancestor)) {
                        matched = true;
                        break;
                    }
                    ancestor = ancestor.parentElement;
                }
                if (!matched)
                    return false;
            }
            return named;
        };
        const scrollableStyle = (style) => ["auto", "scroll", "overlay"].includes(style.overflowX) ||
            ["auto", "scroll", "overlay"].includes(style.overflowY);
        const clipsChildren = (style) => ["hidden", "clip", "auto", "scroll"].includes(style.overflowX) ||
            ["hidden", "clip", "auto", "scroll"].includes(style.overflowY);
        const rectOf = (element) => {
            const box = element.getBoundingClientRect();
            return { x: box.x, y: box.y, width: box.width, height: box.height };
        };
        const nodes = [];
        const assigned = new Map();
        let truncated = false;
        let counter = 0;
        /**
         * The candidate set. Collecting the whole DOM would be unbounded and
         * mostly noise, so an element earns a place by being something a
         * regression can break: an operable control, a machine identity a test
         * depends on, a surface that floats over other things, a node whose text
         * or box already overflows, or something a design system named.
         */
        const isCandidate = (element, style) => {
            if (element.id)
                return true;
            if (element.hasAttribute(testIdAttribute))
                return true;
            if (INTERACTIVE_TAGS.has(element.tagName))
                return true;
            const role = element.getAttribute("role");
            if (role && (INTERACTIVE_ROLES.has(role) || SURFACE_ROLES.has(role)))
                return true;
            if (element.tagName === "DIALOG")
                return true;
            if (element.hasAttribute("data-component") || element.hasAttribute("data-entity")) {
                return true;
            }
            if (scrollableStyle(style))
                return true;
            if (element.scrollWidth > element.clientWidth ||
                element.scrollHeight > element.clientHeight) {
                return true;
            }
            return ["fixed", "sticky", "absolute"].includes(style.position);
        };
        /** Nearest collected ancestor, so parent links stay inside the subset. */
        const nearestCollected = (element) => {
            let current = element.parentElement;
            while (current) {
                const found = assigned.get(current);
                if (found)
                    return found;
                current = current.parentElement;
            }
            return undefined;
        };
        const nearestClip = (element) => {
            let current = element.parentElement;
            while (current) {
                const style = getComputedStyle(current);
                if (clipsChildren(style))
                    return rectOf(current);
                current = current.parentElement;
            }
            return undefined;
        };
        const nearestEntity = (element) => {
            let current = element;
            while (current) {
                const entity = current.getAttribute("data-entity");
                if (entity)
                    return text(entity);
                current = current.parentElement;
            }
            return undefined;
        };
        // Document order keeps the snapshot stable across runs; a parent is always
        // visited before its children, so `nearestCollected` can resolve.
        const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
        const required = new Set(requiredTestIds);
        let element = document.body;
        while (element) {
            const style = getComputedStyle(element);
            const box = rectOf(element);
            const hidden = style.display === "none" ||
                style.visibility === "hidden" ||
                style.visibility === "collapse" ||
                Number(style.opacity) === 0;
            const testId = element.getAttribute(testIdAttribute) ?? undefined;
            let semanticMustKeep = false;
            for (const target of requiredTargets) {
                if (requiredTargetMatches(target, element)) {
                    semanticMustKeep = true;
                    break;
                }
            }
            const mustKeep = (testId !== undefined && required.has(testId)) ||
                semanticMustKeep;
            if (mustKeep || isCandidate(element, style)) {
                if (nodes.length >= maxNodes) {
                    truncated = true;
                    break;
                }
                counter += 1;
                const nodeId = `e${counter}`;
                assigned.set(element, nodeId);
                const role = roleOf(element);
                const interactive = INTERACTIVE_TAGS.has(element.tagName) ||
                    (role !== undefined && INTERACTIVE_ROLES.has(role)) ||
                    element.hasAttribute("onclick") ||
                    element.tabIndex >= 0;
                const clientRects = Array.from(element.getClientRects()).map((rect) => ({
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                }));
                const node = {
                    id: nodeId,
                    visible: !hidden && box.width > 0 && box.height > 0,
                    interactive,
                    enabled: isEnabled(element),
                    pointer_events: style.pointerEvents !== "none",
                    scrollable: scrollableStyle(style),
                    decorative: element.getAttribute("aria-hidden") === "true" ||
                        element.getAttribute("role") === "presentation" ||
                        element.getAttribute("role") === "none",
                    required_by_oracle: false,
                    rects: clientRects.length > 0 ? clientRects : [box],
                };
                const domId = text(element.id);
                if (domId)
                    node.dom_id = domId;
                const stableId = text(testId);
                if (stableId)
                    node.test_id = stableId;
                if (role)
                    node.role = role;
                const name = nameOf(element);
                if (name)
                    node.accessible_name = name;
                const label = text(element.getAttribute("aria-label"));
                if (label)
                    node.label = label;
                const component = text(element.getAttribute("data-component"));
                if (component)
                    node.component_hint = component;
                node.tag = element.tagName.toLowerCase();
                if (element instanceof HTMLInputElement)
                    node.input_type = element.type.toLowerCase();
                node.focusable = !hidden && focusable(element);
                node.label_associated = labelAssociated(element);
                if (element instanceof HTMLButtonElement ||
                    element instanceof HTMLInputElement ||
                    element instanceof HTMLSelectElement ||
                    element instanceof HTMLTextAreaElement)
                    node.native_disabled = element.disabled;
                node.modal = isModal(element);
                const active = document.activeElement;
                node.contains_focus = Boolean(active && active !== document.body && element.contains(active));
                for (const [attribute, field] of [
                    ["aria-disabled", "aria_disabled"],
                    ["aria-checked", "aria_checked"],
                    ["aria-selected", "aria_selected"],
                    ["aria-pressed", "aria_pressed"],
                    ["aria-expanded", "aria_expanded"],
                ]) {
                    const value = text(element.getAttribute(attribute))?.toLowerCase();
                    if (value)
                        node[field] = value;
                }
                node.required_by_oracle = mustKeep;
                const entity = nearestEntity(element);
                if (entity)
                    node.entity_key = entity;
                const clip = nearestClip(element);
                if (clip)
                    node.clip_rect = clip;
                if (style.position !== "static")
                    node.position = style.position;
                const zIndex = Number.parseInt(style.zIndex, 10);
                if (Number.isFinite(zIndex))
                    node.z_index = zIndex;
                const parent = nearestCollected(element);
                if (parent)
                    node.parent = parent;
                node.text_scroll_width = element.scrollWidth;
                node.text_client_width = element.clientWidth;
                node.text_scroll_height = element.scrollHeight;
                node.text_client_height = element.clientHeight;
                nodes.push(node);
            }
            element = walker.nextNode();
        }
        // ---- hit testing, against the identities just assigned ----------------
        const unionOf = (rects) => {
            const usable = rects.filter((rect) => rect.width > 0 && rect.height > 0);
            const seed = usable[0];
            if (!seed)
                return undefined;
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
        const pointsFor = (rect) => {
            const left = rect.x + inset;
            const centreX = rect.x + rect.width / 2;
            const right = rect.x + rect.width - inset;
            const top = rect.y + inset;
            const centreY = rect.y + rect.height / 2;
            const bottom = rect.y + rect.height - inset;
            if (rect.width >= inset * 12 && rect.height >= inset * 12) {
                const grid = [];
                for (const y of [top, centreY, bottom]) {
                    for (const x of [left, centreX, right])
                        grid.push({ x, y });
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
        const identify = (found) => {
            let current = found;
            while (current) {
                const id = assigned.get(current);
                if (id)
                    return id;
                current = current.parentElement;
            }
            return undefined;
        };
        const byId = new Map(nodes.map((node) => [node.id, node]));
        const hitTests = [];
        let truncatedHitTests = false;
        for (const [candidate, nodeId] of assigned) {
            const node = byId.get(nodeId);
            if (!node || !node.visible || !node.interactive || !node.enabled)
                continue;
            if (!node.pointer_events)
                continue;
            const bounds = unionOf(node.rects);
            if (!bounds)
                continue;
            const points = pointsFor(bounds);
            if (hitTests.length + points.length > maxHitTests) {
                truncatedHitTests = true;
                break;
            }
            for (const point of points) {
                const stack = document
                    .elementsFromPoint(point.x, point.y)
                    .map(identify)
                    .filter((value) => value !== undefined);
                const sample = { target: nodeId, point, stack };
                const topmost = stack[0];
                if (topmost)
                    sample.topmost = topmost;
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
    }, {
        maxNodes: config.max_nodes,
        testIdAttribute: config.test_id_attribute,
        requiredTestIds: config.required_test_ids,
        requiredTargets: config.required_targets,
        maxLabel: MAX_LABEL_CHARS,
        maxHitTests: HARD_MAX_HIT_TESTS,
        inset: 2,
    });
}
/** Whether two reads of the same page agree on where everything is. */
export function geometryMatches(left, right, tolerance) {
    if (left.length !== right.length)
        return false;
    for (let index = 0; index < left.length; index += 1) {
        const a = left[index];
        const b = right[index];
        if (!a || !b || a.id !== b.id || a.rects.length !== b.rects.length)
            return false;
        for (let rect = 0; rect < a.rects.length; rect += 1) {
            const first = a.rects[rect];
            const second = b.rects[rect];
            if (!first || !second)
                return false;
            if (Math.abs(first.x - second.x) > tolerance ||
                Math.abs(first.y - second.y) > tolerance ||
                Math.abs(first.width - second.width) > tolerance ||
                Math.abs(first.height - second.height) > tolerance) {
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
export function samplePoints(rect, inset) {
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
        const grid = [];
        for (const y of [top, centreY, bottom]) {
            for (const x of [left, centreX, right])
                grid.push({ x, y });
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
function routeOf(url) {
    try {
        const parsed = new URL(url);
        return `${parsed.pathname}${parsed.search}${parsed.hash}`;
    }
    catch {
        return url;
    }
}

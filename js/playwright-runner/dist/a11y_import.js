/** Collect axe-core / Storybook a11y JSON without keeping HTML. */
const MAX_VIOLATIONS = 128;
const MAX_NODES = 8;
const MAX_TOKEN = 120;
function bound(raw) {
    if (typeof raw !== "string")
        return "";
    const collapsed = raw.replace(/\s+/g, " ").trim();
    return collapsed.length > MAX_TOKEN ? collapsed.slice(0, MAX_TOKEN) : collapsed;
}
function sanitize(raw, producer) {
    if (!raw || typeof raw !== "object")
        return undefined;
    const record = raw;
    const nested = record.results && typeof record.results === "object"
        ? record.results.violations
        : undefined;
    const list = Array.isArray(record.violations)
        ? record.violations
        : Array.isArray(nested)
            ? nested
            : [];
    const violations = list.slice(0, MAX_VIOLATIONS).flatMap((item) => {
        if (!item || typeof item !== "object")
            return [];
        const violation = item;
        const id = bound(violation.id);
        if (!id)
            return [];
        const nodesRaw = Array.isArray(violation.nodes) ? violation.nodes : [];
        const nodes = nodesRaw.slice(0, MAX_NODES).map((node) => {
            const targets = node && typeof node === "object" && Array.isArray(node.target)
                ? node.target.map(bound).filter(Boolean)
                : [];
            return { target: targets };
        });
        const impact = bound(violation.impact);
        return impact
            ? [{ id, impact, nodes }]
            : [{ id, nodes }];
    });
    return { producer, violations };
}
/**
 * Run axe if the page already loaded it (Storybook addon-a11y or a project
 * script). WVQ does not vendor axe-core.
 */
export async function collectA11yImport(page) {
    try {
        const raw = await page.evaluate(async () => {
            const axe = globalThis.axe;
            if (typeof axe?.run !== "function") {
                const storybook = globalThis.__STORYBOOK_A11Y_RESULT__;
                return storybook === undefined ? null : { producer: "storybook-a11y", raw: storybook };
            }
            const result = await axe.run(document, { resultTypes: ["violations"] });
            return { producer: "axe-core", raw: result };
        });
        if (!raw || typeof raw !== "object")
            return undefined;
        const envelope = raw;
        const producer = envelope.producer === "storybook-a11y" ? "storybook-a11y" : "axe-core";
        return sanitize(envelope.raw, producer);
    }
    catch {
        return undefined;
    }
}

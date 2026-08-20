/** Semantic manual recorder. No AI. `XPath` is not a recorded identity. */
export function recordIsEnabled() {
    return true;
}
export function assertSemanticEvent(event) {
    const raw = JSON.stringify(event);
    if (raw.includes('"xpath"')) {
        throw new Error("XPath is not a recorded identity");
    }
    return event;
}
export function isRedundant(beforeDigest, afterDigest) {
    return beforeDigest === afterDigest;
}

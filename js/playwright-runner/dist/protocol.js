/** Line protocol. Unknown methods fail closed. No AI. */
export const METHODS = [
    "initialize",
    "prepare",
    "prepare_recording",
    "execute_step",
    "observe",
    "collect_ui",
    "capture_failure_reel",
    "poll_recording",
    "finish_recording",
    "finish",
    "cancel",
];
export function decodeRequest(line) {
    const value = JSON.parse(line);
    if (typeof value.method !== "string") {
        throw new Error("missing method");
    }
    if (typeof value.id !== "number") {
        throw new Error("missing id");
    }
    if (!METHODS.includes(value.method)) {
        throw new Error(`unknown bridge method \`${value.method}\``);
    }
    const params = value.params && typeof value.params === "object" && !Array.isArray(value.params)
        ? value.params
        : {};
    return { method: value.method, id: value.id, params };
}

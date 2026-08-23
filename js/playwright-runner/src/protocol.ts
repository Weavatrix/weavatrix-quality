/** Line protocol. Unknown methods fail closed. No AI. */

export const METHODS = [
  "initialize",
  "prepare",
  "execute_step",
  "observe",
  "collect_ui",
  "finish",
  "cancel",
] as const;

export type Method = (typeof METHODS)[number];

export type BridgeRequest = {
  method: Method;
  id: number;
  params: Record<string, unknown>;
};

export function decodeRequest(line: string): BridgeRequest {
  const value = JSON.parse(line) as { method?: unknown; id?: unknown; params?: unknown };
  if (typeof value.method !== "string") {
    throw new Error("missing method");
  }
  if (typeof value.id !== "number") {
    throw new Error("missing id");
  }
  if (!METHODS.includes(value.method as Method)) {
    throw new Error(`unknown bridge method \`${value.method}\``);
  }
  const params =
    value.params && typeof value.params === "object" && !Array.isArray(value.params)
      ? (value.params as Record<string, unknown>)
      : {};
  return { method: value.method as Method, id: value.id, params };
}

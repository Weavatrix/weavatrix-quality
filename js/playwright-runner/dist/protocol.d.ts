/** Line protocol. Unknown methods fail closed. No AI. */
export declare const METHODS: readonly ["initialize", "prepare", "execute_step", "observe", "collect_ui", "finish", "cancel"];
export type Method = (typeof METHODS)[number];
export type BridgeRequest = {
    method: Method;
    id: number;
    params: Record<string, unknown>;
};
export declare function decodeRequest(line: string): BridgeRequest;

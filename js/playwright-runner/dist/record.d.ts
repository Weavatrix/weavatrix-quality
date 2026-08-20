/** Semantic manual recorder. No AI. `XPath` is not a recorded identity. */
export type RecordedTarget = {
    role?: string;
    accessible_name?: string;
    label?: string;
    test_id?: string;
    component_hint?: string;
    fallback_css?: string;
};
export type RecordedEvent = {
    action: string;
    target?: RecordedTarget;
    route?: string;
    value?: string;
    key?: string;
};
export declare function recordIsEnabled(): boolean;
export declare function assertSemanticEvent(event: RecordedEvent): RecordedEvent;
export declare function isRedundant(beforeDigest: string, afterDigest: string): boolean;

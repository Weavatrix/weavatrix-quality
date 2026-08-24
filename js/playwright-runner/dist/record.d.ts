/** Passive semantic recorder. No AI, XPath, request bodies, or raw form values. */
import type { BrowserContext, Page } from "playwright";
import type { TestAction } from "./execute.js";
export type RecorderCapture = {
    action?: TestAction;
    limitation?: string;
};
export type RecorderHooks = {
    capture(capture: RecorderCapture): Promise<void>;
    finish(): void;
};
export type RecorderInstallConfig = {
    fixture_values: Record<string, string>;
    max_events: number;
    test_id_attribute?: string;
};
declare global {
    interface Window {
        __wvqRecorderInstalled?: boolean;
        __wvqRecordEvent?: (capture: RecorderCapture) => Promise<void>;
        __wvqRecordFinish?: () => Promise<void>;
    }
}
/** Install before navigation so natural application use is observed from first paint. */
export declare function installSemanticRecorder(context: BrowserContext, page: Page, hooks: RecorderHooks, config: RecorderInstallConfig): Promise<void>;
export declare function assertRecorderCapture(value: unknown): RecorderCapture;

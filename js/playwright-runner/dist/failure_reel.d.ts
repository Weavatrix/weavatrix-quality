/** Diagnostic frames for a Playwright failure. Never a verdict. No AI. */
import type { Locator, Page } from "playwright";
import type { Target } from "./execute.js";
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
export declare function predicateTarget(predicate: unknown): Target | undefined;
/**
 * Capture the after-frame and, when the target is still locatable, a highlighted
 * copy. Called only after a step failed. Passing runs never enter here.
 */
export declare function captureFailureReelFrames(page: Page, request: FailureReelRequest): Promise<FailureReelCapture>;
export declare function highlightLocator(locator: Locator): Promise<boolean>;
export declare function clearHighlight(locator: Locator): Promise<void>;

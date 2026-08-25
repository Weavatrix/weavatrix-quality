/** Collect axe-core / Storybook a11y JSON without keeping HTML. */
import type { Page } from "playwright";
export type A11yImportReport = {
    producer: "axe-core" | "storybook-a11y";
    /** True when the producer returned more violations or nodes than we keep. */
    truncated?: boolean;
    violations: Array<{
        id: string;
        impact?: string;
        nodes: Array<{
            target: string[];
        }>;
    }>;
};
/** Axe/Storybook is optional. Failure is not the same as absence. */
export type A11yImportOutcome = {
    status: "absent";
} | {
    status: "failed";
    error: string;
} | {
    status: "imported";
    report: A11yImportReport;
};
/**
 * Run axe if the page already loaded it (Storybook addon-a11y or a project
 * script). WVQ does not vendor axe-core. Absence is not a failure.
 */
export declare function collectA11yImport(page: Page): Promise<A11yImportOutcome>;

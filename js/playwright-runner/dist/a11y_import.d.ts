/** Collect axe-core / Storybook a11y JSON without keeping HTML. */
import type { Page } from "playwright";
export type A11yImportReport = {
    producer: "axe-core" | "storybook-a11y";
    violations: Array<{
        id: string;
        impact?: string;
        nodes: Array<{
            target: string[];
        }>;
    }>;
};
/**
 * Run axe if the page already loaded it (Storybook addon-a11y or a project
 * script). WVQ does not vendor axe-core.
 */
export declare function collectA11yImport(page: Page): Promise<A11yImportReport | undefined>;

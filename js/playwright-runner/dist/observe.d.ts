/** Structured observation. Screenshots follow EvidencePolicy only. No AI. */
export type CaptureWhen = "never" | "on_failure" | "always";
export type EvidencePolicy = {
    screenshot: CaptureWhen;
    trace: CaptureWhen;
    network: CaptureWhen;
    console: CaptureWhen;
    storage: CaptureWhen;
};
export type Observation = {
    route?: string;
    a11y_digest?: string;
    network: string[];
    network_requests: Array<{
        sequence: number;
        method: string;
        url: string;
        status?: number;
        resource_type?: string;
        content_type?: string;
        body_digest?: string;
        graphql_operation?: string;
        graphql_query_digest?: string;
        graphql_variables_digest?: string;
    }>;
    network_requests_truncated: boolean;
    console: string[];
    storage: Record<string, string>;
    storage_available?: boolean;
    viewport?: string;
    screenshot_handle?: string;
    screenshot_path?: string;
};
export declare function filterObservation(observation: Observation, policy: EvidencePolicy, failed: boolean): Observation;

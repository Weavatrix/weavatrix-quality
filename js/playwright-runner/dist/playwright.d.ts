/** Actual Playwright adapter. Policy and sealed predicates arrive from Rust. */
import { type Driver, type Target, type WaitCondition } from "./execute.js";
import { type FailureReelCapture } from "./failure_reel.js";
import type { EvidencePolicy, Observation } from "./observe.js";
import { type RecorderInstallConfig } from "./record.js";
import { type CollectionResult, type UiIntegrityConfig } from "./ui_integrity.js";
export type PredicateTarget = Omit<Target, "component_hint">;
export type Predicate = {
    kind: "visible" | "hidden" | "enabled" | "disabled";
    target: PredicateTarget;
} | {
    kind: "text_equals" | "text_contains" | "value_equals";
    target: PredicateTarget;
    value: string;
} | {
    kind: "route_equals" | "route_contains";
    value: string;
} | {
    kind: "network_response";
    method?: string;
    url_contains: string;
    status?: number;
} | {
    kind: "no_console_errors";
} | {
    kind: "storage_equals";
    area: "local" | "session";
    key: string;
    value: string;
} | {
    kind: "storage_absent";
    area: "local" | "session";
    key: string;
} | {
    kind: "api_status";
    operation: string;
    status: number;
} | {
    kind: "api_json_equals";
    operation: string;
    pointer: string;
    value: unknown;
} | {
    kind: "unique";
    target: PredicateTarget;
} | {
    kind: "max_multiplicity";
    target: PredicateTarget;
    max: number;
} | {
    kind: "receives_events";
    target: PredicateTarget;
    min_ratio_permille: number;
} | {
    kind: "inside_viewport";
    target: PredicateTarget;
    margin_px: number;
} | {
    kind: "text_not_clipped";
    target: PredicateTarget;
} | {
    kind: "no_overlap";
    target: PredicateTarget;
    with: PredicateTarget;
    max_ratio_permille: number;
} | {
    kind: "all" | "any";
    predicates: Predicate[];
} | {
    kind: "not";
    predicate: Predicate;
};
export type ProgramOracle = {
    obligation: string;
    condition?: Predicate;
    expected: Predicate;
};
export type FaultSpec = {
    kind: "abort";
    url_contains: string;
} | {
    kind: "http_response";
    url_contains: string;
    status: number;
    body?: string;
    headers?: Record<string, string>;
} | {
    kind: "delay";
    url_contains: string;
    delay_ms: number;
};
export type ApiOperation = {
    method: string;
    path: string;
    headers?: Record<string, string>;
};
export type BrowserProgram = {
    id: string;
    obligations: string[];
    preconditions?: Array<Record<string, unknown>>;
    steps: Array<Record<string, unknown>>;
    data?: Record<string, unknown>;
    faults?: Record<string, FaultSpec>;
    api_operations?: Record<string, ApiOperation>;
    evidence_policy?: EvidencePolicy;
};
export type BrowserConfig = {
    base_url: string;
    browser?: "chromium" | "firefox" | "webkit";
    headless?: boolean;
    timeout_ms?: number;
    viewport?: {
        width: number;
        height: number;
    };
    evidence_dir: string;
    network?: NetworkPolicy;
};
export type NetworkReplayEntry = {
    method: string;
    path: string;
    status: number;
    content_type: string;
    body: string;
    request_content_type?: string;
    request_body_digest?: string;
    graphql_operation_name?: string;
    graphql_query_digest?: string;
    graphql_variables_digest?: string;
};
export type NetworkReplayProfile = {
    schema_v: 1 | 2;
    entries: NetworkReplayEntry[];
};
export type NetworkPolicy = {
    mode: "live" | "record" | "replay" | "hybrid";
    profile?: NetworkReplayProfile;
    redact_json_keys?: string[];
    max_entries?: number;
    max_body_bytes?: number;
    max_total_bytes?: number;
};
export type RecordedBrowserEvent = {
    action: Record<string, unknown>;
    observation: Observation;
};
export type RecordedBrowserPoll = {
    events: RecordedBrowserEvent[];
    limitations: string[];
    done: boolean;
};
export type RecordedOracleResult = {
    obligation: string;
    status: "passed" | "contradicted" | "condition_not_established";
};
export declare class PlaywrightDriver implements Driver {
    #private;
    private constructor();
    static create(program: BrowserProgram, oracles: ProgramOracle[], rawConfig: BrowserConfig): Promise<PlaywrightDriver>;
    /** Begin passive capture before opening the requested route. */
    startRecording(route: string, config: RecorderInstallConfig): Promise<{
        initial: Observation;
    }>;
    /** Drain events captured since the previous poll. */
    pollRecording(): Promise<RecordedBrowserPoll>;
    /** Evaluate existing sealed predicates at the exact final recorded state. */
    evaluateRecordedOracles(): Promise<RecordedOracleResult[]>;
    navigate(route: string): Promise<void>;
    activate(target: Target): Promise<void>;
    hover(target: Target): Promise<void>;
    scroll(target: Target): Promise<void>;
    drag(target: Target, to: Target): Promise<void>;
    fill(target: Target, value: string): Promise<void>;
    select(target: Target, value: string): Promise<void>;
    press(key: string, target?: Target): Promise<void>;
    wait(condition: WaitCondition): Promise<void>;
    setFeatureFlag(key: string, value: string): Promise<void>;
    injectFault(name: string): Promise<void>;
    apiCall(name: string, input: string): Promise<void>;
    assert(obligation: string): Promise<void>;
    /**
     * Let a mutation and an immediate application-level retry finish inside the
     * action that caused them. Long polling and broken servers remain bounded by
     * a two-second ceiling; incomplete journals are still reported separately.
     */
    settleAction(): Promise<void>;
    observe(failed: boolean, captureScreenshot?: boolean): Promise<Observation>;
    /**
     * Collect one deterministic UI-integrity snapshot of the current state.
     *
     * The driver only measures: whether anything it records is a problem is
     * decided by `wvq-ui` in Rust. Collection makes no model or vision call.
     */
    collectUi(identity: {
        revision: string;
        program?: string;
        step: number;
        stateDigest: string;
    }, config: UiIntegrityConfig | undefined): Promise<CollectionResult>;
    /**
     * Diagnostic frames for a failed step. Never called on the green path.
     * Overlay is restored before this returns so later observe/UI collection
     * cannot see it — the host calls this after both.
     */
    captureFailureReel(step: number, action: Record<string, unknown>): Promise<FailureReelCapture>;
    finish(): Promise<{
        trace_path?: string;
        network_profile?: NetworkReplayProfile;
        network_limitations?: string[];
    }>;
    cancel(): Promise<void>;
}

/**
 * Deterministic UI-integrity collection.
 *
 * This module measures; it never decides. Whether a duplicate, an occlusion, or
 * an overflow is a problem is Rust's answer â€” the policy, the detectors, and
 * the base/head ratchet all live in `wvq-ui`. Re-implementing any of that here
 * would give two answers to one question.
 *
 * Three properties matter more than coverage:
 *
 * - **Determinism.** Fonts are awaited, animations and transitions are frozen,
 *   and geometry is read twice and compared. An unsettled layout is reported as
 *   unstable rather than measured once and trusted.
 * - **Boundedness.** The candidate set is a filtered subset of the DOM, not the
 *   DOM. Node count, hit-test samples, and label length all have ceilings, and
 *   hitting one sets `truncated`.
 * - **Privacy.** No innerHTML, no form values, no cookies, no storage contents,
 *   no response bodies. Accessible names and labels are trimmed, collapsed, and
 *   cut to a fixed length before they ever leave the page.
 *
 * There is no AI here, and no vision. Everything below is `getBoundingClientRect`,
 * `getComputedStyle`, `elementsFromPoint`, and scroll-versus-client metrics.
 */
import type { Page } from "playwright";
import type { Target } from "./execute.js";
/** Schema version Rust accepts. Bumping it is a breaking change. */
export declare const LAYOUT_SNAPSHOT_SCHEMA_V = 2;
/** Longest accessible name or label kept in evidence. Matches `wvq-ui`. */
export declare const MAX_LABEL_CHARS = 120;
/** Hard ceiling on collected nodes, whatever local policy asks for. */
export declare const HARD_MAX_NODES = 20000;
/** Hard ceiling on hit-test samples in one snapshot. */
export declare const HARD_MAX_HIT_TESTS = 40000;
export type Rect = {
    x: number;
    y: number;
    width: number;
    height: number;
};
export type Point = {
    x: number;
    y: number;
};
export type UiNode = {
    id: string;
    dom_id?: string;
    test_id?: string;
    role?: string;
    accessible_name?: string;
    label?: string;
    tag?: string;
    input_type?: string;
    required_by_oracle: boolean;
    focusable?: boolean;
    label_associated?: boolean;
    native_disabled?: boolean;
    modal?: boolean;
    contains_focus?: boolean;
    aria_disabled?: string;
    aria_checked?: string;
    aria_selected?: string;
    aria_pressed?: string;
    aria_expanded?: string;
    component_hint?: string;
    entity_key?: string;
    rects: Rect[];
    clip_rect?: Rect;
    visible: boolean;
    interactive: boolean;
    enabled: boolean;
    pointer_events: boolean;
    scrollable: boolean;
    decorative: boolean;
    position?: string;
    z_index?: number;
    stacking_context?: string;
    parent?: string;
    text_scroll_width?: number;
    text_client_width?: number;
    text_scroll_height?: number;
    text_client_height?: number;
};
export type HitTestSample = {
    target: string;
    point: Point;
    topmost?: string;
    stack: string[];
};
export type DocumentMetrics = {
    scroll_width: number;
    client_width: number;
    scroll_height: number;
    client_height: number;
};
export type LayoutSnapshot = {
    schema_v: number;
    revision: string;
    program: string;
    step: number;
    route: string;
    state_digest: string;
    viewport: {
        width: number;
        height: number;
    };
    responsive_breakpoints: number[];
    responsive_breakpoints_complete: boolean;
    document: DocumentMetrics;
    nodes: UiNode[];
    hit_tests: HitTestSample[];
    truncated: boolean;
};
export type UiIntegrityConfig = {
    /** Whether collection runs at all. Off by default. */
    enabled?: boolean;
    /** Ceiling on collected nodes. Clamped to `HARD_MAX_NODES`. */
    max_nodes?: number;
    /** Geometry difference between the two reads that still counts as settled. */
    geometry_tolerance_px?: number;
    /** How long to wait for fonts and a settled layout, in milliseconds. */
    settle_timeout_ms?: number;
    /** Stable test attribute this project uses. */
    test_id_attribute?: string;
    /** Extra semantic targets sealed predicates name, so they are never dropped. */
    required_test_ids?: string[];
    /** Exact semantic targets sealed predicates depend on. */
    required_targets?: Target[];
    /** Discover parsed CSS/container width transitions for adaptive probing. */
    responsive_breakpoints?: boolean;
};
type ResolvedConfig = Required<Omit<UiIntegrityConfig, "required_test_ids" | "required_targets">> & {
    required_test_ids: string[];
    required_targets: Target[];
};
export declare function resolveConfig(config: UiIntegrityConfig | undefined): ResolvedConfig;
/** Everything one collection produced, plus why it may be incomplete. */
export type CollectionResult = {
    snapshot: LayoutSnapshot;
    /** Empty when the layout settled and every bound was respected. */
    limitations: string[];
};
/**
 * Collect one route/state/viewport.
 *
 * The caller supplies `revision`, `program`, `step`, and `stateDigest` so the
 * snapshot can be lined up against the same measurement point on the other
 * revision. Those four are the base/head comparison key; the DOM digest is
 * provenance only, because the change under review is what alters the markup.
 */
export declare function collectLayoutSnapshot(page: Page, identity: {
    revision: string;
    program: string;
    step: number;
    stateDigest: string;
}, config: UiIntegrityConfig | undefined): Promise<CollectionResult>;
/**
 * Stop time inside the page.
 *
 * Without this the same button is at two different places on two runs and the
 * ratchet reports a regression that is really an animation frame. The style is
 * additive and scoped to the automation run; it is never written to the app.
 */
/** Freeze CSS time and hide the caret so a later screenshot is not a random frame. */
export declare function freezeAnimations(page: Page): Promise<void>;
/** Bounded wait for webfonts. False means the timeout won, not that fonts failed. */
export declare function waitForFonts(page: Page, timeoutMs: number): Promise<boolean>;
/** Whether two reads of the same page agree on where everything is. */
export declare function geometryMatches(left: UiNode[], right: UiNode[], tolerance: number): boolean;
/**
 * Probe points on one control, exported so the sampling rule can be tested
 * without a browser. The in-page collector uses the same shape.
 *
 * Centre plus four inset corners catches a control covered edge-on. A target
 * big enough for nine distinguishable points gets a 3x3 grid: a large button
 * can be half covered while its centre still responds, which is exactly what a
 * centre-only probe would call healthy.
 */
export declare function samplePoints(rect: Rect, inset: number): Point[];
export {};

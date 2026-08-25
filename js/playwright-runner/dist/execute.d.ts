/** Deterministic step execution. Playwright is the host; this file has no AI. */
export type Target = {
    role?: string;
    accessible_name?: string;
    label?: string;
    test_id?: string;
    component_hint?: string;
    scope?: Target;
    fallback_css?: string;
};
export type WaitCondition = {
    kind: "visible";
    target: Target;
} | {
    kind: "url";
    route: string;
};
export type TestAction = {
    action: "navigate";
    route: string;
} | {
    action: "activate";
    target: Target;
} | {
    action: "fill";
    target: Target;
    value: string;
} | {
    action: "select";
    target: Target;
    value: string;
} | {
    action: "press";
    key: string;
    target?: Target;
} | {
    action: "wait";
    condition: WaitCondition;
} | {
    action: "set_feature_flag";
    key: string;
    value: string;
} | {
    action: "inject_fault";
    fault: string;
} | {
    action: "api_call";
    operation: string;
    input: string;
} | {
    action: "hover";
    target: Target;
} | {
    action: "scroll";
    target: Target;
} | {
    action: "drag";
    target: Target;
    to: Target;
} | {
    action: "upload";
    target: Target;
    fixture: string;
} | {
    action: "download";
    target: Target;
} | {
    action: "popup";
    target: Target;
} | {
    action: "switch_tab";
    route: string;
} | {
    action: "assert";
    obligation: string;
};
export type Driver = {
    navigate(route: string): Promise<void>;
    activate(target: Target): Promise<void>;
    fill(target: Target, value: string): Promise<void>;
    select(target: Target, value: string): Promise<void>;
    press(key: string, target?: Target): Promise<void>;
    wait(condition: WaitCondition): Promise<void>;
    setFeatureFlag(key: string, value: string): Promise<void>;
    injectFault(fault: string): Promise<void>;
    apiCall(operation: string, input: string): Promise<void>;
    hover(target: Target): Promise<void>;
    scroll(target: Target): Promise<void>;
    drag(target: Target, to: Target): Promise<void>;
    upload(target: Target, fixture: string): Promise<void>;
    download(target: Target): Promise<void>;
    popup(target: Target): Promise<void>;
    switchTab(route: string): Promise<void>;
    assert(obligation: string): Promise<void>;
};
/** Semantic target the IR named for this action, if any. */
export declare function actionTarget(action: TestAction): Target | undefined;
export declare function executeStep(driver: Driver, action: TestAction): Promise<void>;

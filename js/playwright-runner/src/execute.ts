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

export type WaitCondition =
  | { kind: "visible"; target: Target }
  | { kind: "url"; route: string };

export type TestAction =
  | { action: "navigate"; route: string }
  | { action: "activate"; target: Target }
  | { action: "fill"; target: Target; value: string }
  | { action: "select"; target: Target; value: string }
  | { action: "press"; key: string; target?: Target }
  | { action: "wait"; condition: WaitCondition }
  | { action: "set_feature_flag"; key: string; value: string }
  | { action: "inject_fault"; fault: string }
  | { action: "api_call"; operation: string; input: string }
  | { action: "hover"; target: Target }
  | { action: "scroll"; target: Target }
  | { action: "drag"; target: Target; to: Target }
  | { action: "upload"; target: Target; fixture: string }
  | { action: "download"; target: Target }
  | { action: "popup"; target: Target }
  | { action: "switch_tab"; route: string }
  | { action: "assert"; obligation: string };

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
export function actionTarget(action: TestAction): Target | undefined {
  switch (action.action) {
    case "activate":
    case "fill":
    case "select":
    case "hover":
    case "scroll":
    case "drag":
    case "upload":
    case "download":
    case "popup":
      return action.target;
    case "press":
      return action.target;
    case "wait":
      return action.condition.kind === "visible" ? action.condition.target : undefined;
    default:
      return undefined;
  }
}

export async function executeStep(driver: Driver, action: TestAction): Promise<void> {
  switch (action.action) {
    case "navigate":
      requireText("navigate route", action.route);
      await driver.navigate(action.route);
      return;
    case "activate":
      requireTarget("activate", action.target);
      await driver.activate(action.target);
      return;
    case "fill":
      requireTarget("fill", action.target);
      await driver.fill(action.target, action.value);
      return;
    case "select":
      requireTarget("select", action.target);
      await driver.select(action.target, action.value);
      return;
    case "press":
      requireText("press key", action.key);
      if (action.target) requireTarget("press", action.target);
      await driver.press(action.key, action.target);
      return;
    case "wait":
      if (!action.condition) throw new Error("wait needs a condition");
      await driver.wait(action.condition);
      return;
    case "set_feature_flag":
      requireText("feature flag key", action.key);
      await driver.setFeatureFlag(action.key, action.value);
      return;
    case "inject_fault":
      requireText("fault id", action.fault);
      await driver.injectFault(action.fault);
      return;
    case "api_call":
      requireText("API operation", action.operation);
      requireText("API input", action.input);
      await driver.apiCall(action.operation, action.input);
      return;
    case "hover":
      requireTarget("hover", action.target);
      await driver.hover(action.target);
      return;
    case "scroll":
      requireTarget("scroll", action.target);
      await driver.scroll(action.target);
      return;
    case "drag":
      requireTarget("drag", action.target);
      requireTarget("drag drop", action.to);
      await driver.drag(action.target, action.to);
      return;
    case "upload":
      requireTarget("upload", action.target);
      requireText("upload fixture", action.fixture);
      await driver.upload(action.target, action.fixture);
      return;
    case "download":
      requireTarget("download", action.target);
      await driver.download(action.target);
      return;
    case "popup":
      requireTarget("popup", action.target);
      await driver.popup(action.target);
      return;
    case "switch_tab":
      requireText("switch_tab route", action.route);
      await driver.switchTab(action.route);
      return;
    case "assert":
      requireText("assert obligation", action.obligation);
      await driver.assert(action.obligation);
      return;
    default: {
      const unknown: never = action;
      throw new Error(`unknown action \`${(unknown as { action?: unknown }).action}\``);
    }
  }
}

function requireText(label: string, value: unknown): asserts value is string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`${label} must be non-empty`);
  }
}

function requireTarget(action: string, target: unknown): asserts target is Target {
  if (!target || typeof target !== "object" || Array.isArray(target)) {
    throw new Error(`${action} needs a target`);
  }
}

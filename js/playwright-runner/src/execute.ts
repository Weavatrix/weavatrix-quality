/** Deterministic step execution. Playwright is the host; this file has no AI. */

export type Target = {
  role?: string;
  accessible_name?: string;
  label?: string;
  test_id?: string;
  component_hint?: string;
  fallback_css?: string;
};

export type TestAction = {
  action: string;
  route?: string;
  target?: Target;
  value?: string;
  key?: string;
  obligation?: string;
};

export type Driver = {
  navigate(route: string): void;
  activate(target: Target): void;
  fill(target: Target, value: string): void;
  press(key: string, target?: Target): void;
  assert(obligation: string): void;
};

export function executeStep(driver: Driver, action: TestAction): void {
  switch (action.action) {
    case "navigate":
      if (!action.route) throw new Error("navigate route must be non-empty");
      driver.navigate(action.route);
      return;
    case "activate":
      if (!action.target) throw new Error("activate needs a target");
      driver.activate(action.target);
      return;
    case "fill":
      if (!action.target) throw new Error("fill needs a target");
      driver.fill(action.target, action.value ?? "");
      return;
    case "press":
      if (!action.key) throw new Error("press key must be non-empty");
      driver.press(action.key, action.target);
      return;
    case "assert":
      if (!action.obligation) throw new Error("assert needs an obligation");
      driver.assert(action.obligation);
      return;
    case "select":
    case "wait":
    case "set_feature_flag":
    case "inject_fault":
    case "api_call":
      return;
    default:
      throw new Error(`unknown action \`${action.action}\``);
  }
}

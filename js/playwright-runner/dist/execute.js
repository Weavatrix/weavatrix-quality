/** Deterministic step execution. Playwright is the host; this file has no AI. */
/** Semantic target the IR named for this action, if any. */
export function actionTarget(action) {
    switch (action.action) {
        case "activate":
        case "fill":
        case "select":
            return action.target;
        case "press":
            return action.target;
        case "wait":
            return action.condition.kind === "visible" ? action.condition.target : undefined;
        default:
            return undefined;
    }
}
export async function executeStep(driver, action) {
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
            if (action.target)
                requireTarget("press", action.target);
            await driver.press(action.key, action.target);
            return;
        case "wait":
            if (!action.condition)
                throw new Error("wait needs a condition");
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
        case "assert":
            requireText("assert obligation", action.obligation);
            await driver.assert(action.obligation);
            return;
        default: {
            const unknown = action;
            throw new Error(`unknown action \`${unknown.action}\``);
        }
    }
}
function requireText(label, value) {
    if (typeof value !== "string" || value.trim() === "") {
        throw new Error(`${label} must be non-empty`);
    }
}
function requireTarget(action, target) {
    if (!target || typeof target !== "object" || Array.isArray(target)) {
        throw new Error(`${action} needs a target`);
    }
}

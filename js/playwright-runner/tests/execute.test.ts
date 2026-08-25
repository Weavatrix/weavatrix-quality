import assert from "node:assert/strict";
import test from "node:test";
import { executeStep } from "../dist/execute.js";

test("every TestProgram action reaches a real driver method", async () => {
  const calls: string[] = [];
  const driver = {
    async navigate() { calls.push("navigate"); },
    async activate() { calls.push("activate"); },
    async fill() { calls.push("fill"); },
    async select() { calls.push("select"); },
    async press() { calls.push("press"); },
    async wait() { calls.push("wait"); },
    async setFeatureFlag() { calls.push("set_feature_flag"); },
    async injectFault() { calls.push("inject_fault"); },
    async apiCall() { calls.push("api_call"); },
    async hover() { calls.push("hover"); },
    async scroll() { calls.push("scroll"); },
    async drag() { calls.push("drag"); },
    async upload() { calls.push("upload"); },
    async download() { calls.push("download"); },
    async popup() { calls.push("popup"); },
    async switchTab() { calls.push("switch_tab"); },
    async assert() { calls.push("assert"); },
  };
  const target = { role: "button", accessible_name: "Save" };
  const tray = { test_id: "tray" };
  const actions = [
    { action: "navigate", route: "/" },
    { action: "activate", target },
    { action: "fill", target, value: "value" },
    { action: "select", target, value: "one" },
    { action: "press", target, key: "Enter" },
    { action: "wait", condition: { kind: "visible", target } },
    { action: "set_feature_flag", key: "flag", value: "on" },
    { action: "inject_fault", fault: "offline" },
    { action: "api_call", operation: "load", input: "fixture" },
    { action: "hover", target },
    { action: "scroll", target },
    { action: "drag", target, to: tray },
    { action: "upload", target, fixture: "invoice" },
    { action: "download", target },
    { action: "popup", target },
    { action: "switch_tab", route: "/preview" },
    { action: "assert", obligation: "saved" },
  ];
  for (const action of actions) await executeStep(driver, action);
  assert.deepEqual(calls, actions.map((action) => action.action));
});

test("unknown actions fail instead of becoming no-ops", async () => {
  const driver = {};
  await assert.rejects(
    executeStep(driver, { action: "browser_magic" }),
    /unknown action/,
  );
});

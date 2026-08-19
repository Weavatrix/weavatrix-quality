/** Semantic manual recorder. No AI. `XPath` is not a recorded identity. */

export type RecordedTarget = {
  role?: string;
  accessible_name?: string;
  label?: string;
  test_id?: string;
  component_hint?: string;
  fallback_css?: string;
};

export type RecordedEvent = {
  action: string;
  target?: RecordedTarget;
  route?: string;
  value?: string;
  key?: string;
};

export function recordIsEnabled(): boolean {
  return true;
}

export function assertSemanticEvent(event: RecordedEvent): RecordedEvent {
  const raw = JSON.stringify(event);
  if (raw.includes('"xpath"')) {
    throw new Error("XPath is not a recorded identity");
  }
  return event;
}

export function isRedundant(beforeDigest: string, afterDigest: string): boolean {
  return beforeDigest === afterDigest;
}

/** Optional continuous observation journal. Rust owns admission and sealing. */

export const SCHEMA_V: 1
export const MAX_EVENTS: 1000

export type ContinuousAction =
  | { action: 'navigate'; route: string }
  | { action: 'activate'; target: { test_id?: string; role?: string; accessible_name?: string; label?: string } }
  | { action: 'fill'; target: { test_id?: string; role?: string; accessible_name?: string; label?: string }; value: string }
  | { action: 'select'; target: { test_id?: string; role?: string; accessible_name?: string; label?: string }; value: string }
  | { action: 'press'; key: string; target?: { test_id?: string; role?: string; accessible_name?: string; label?: string } }

export type ContinuousJournal = {
  schema_v: 1
  source: 'continuous'
  observed_only: true
  session_id: string
  data: Record<string, string>
  initial: { route: string }
  events: Array<{ action: ContinuousAction; after: { route: string } }>
  max_events: number
}

export function createJournal(options: {
  sessionId: string
  route: string
  data?: Record<string, string>
  maxEvents?: number
}): ContinuousJournal

export function appendEvent(
  journal: ContinuousJournal,
  event: { action: ContinuousAction; after: { route: string } },
): ContinuousJournal

export function serialize(journal: ContinuousJournal): string

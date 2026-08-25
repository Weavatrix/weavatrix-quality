/** Optional continuous observation journal. Policy lives in Rust. This file has no AI. */

export const SCHEMA_V = 1
export const MAX_EVENTS = 1_000

const ALLOWED = new Set(['navigate', 'activate', 'fill', 'select', 'press'])

export function createJournal({ sessionId, route, data = {}, maxEvents = 200 } = {}) {
  requireText('sessionId', sessionId)
  requireText('route', route)
  if (sessionId.includes('/') || sessionId.includes('\\') || sessionId.includes('..')) {
    throw new Error('sessionId must not be a path')
  }
  if (!Number.isInteger(maxEvents) || maxEvents < 1 || maxEvents > MAX_EVENTS) {
    throw new Error('maxEvents must be between 1 and 1000')
  }
  return {
    schema_v: SCHEMA_V,
    source: 'continuous',
    observed_only: true,
    session_id: sessionId,
    data: { ...data },
    initial: { route },
    events: [],
    max_events: maxEvents,
  }
}

export function appendEvent(journal, event) {
  assertJournal(journal)
  if (journal.events.length >= journal.max_events) {
    throw new Error('continuous journal exceeds max_events')
  }
  const action = event?.action
  const after = event?.after
  if (!action || typeof action !== 'object' || !ALLOWED.has(action.action)) {
    throw new Error(`continuous journal cannot include action \`${action?.action}\``)
  }
  if (!after?.route || typeof after.route !== 'string' || after.route.trim() === '') {
    throw new Error('continuous journal state needs a route')
  }
  const raw = JSON.stringify(event)
  if (/xpath/i.test(raw)) throw new Error('XPath is not a continuous journal identity')
  if ((action.action === 'fill' || action.action === 'select') && !Object.hasOwn(journal.data, action.value)) {
    throw new Error(`continuous journal fill/select names unknown data \`${action.value}\``)
  }
  journal.events.push({ action, after: { route: after.route } })
  return journal
}

export function serialize(journal) {
  assertJournal(journal)
  const { max_events: _max, ...document } = journal
  return JSON.stringify(document)
}

function assertJournal(journal) {
  if (!journal || journal.schema_v !== SCHEMA_V || journal.source !== 'continuous') {
    throw new Error('malformed continuous journal')
  }
  if (journal.observed_only !== true) {
    throw new Error('continuous journal must set observed_only true')
  }
}

function requireText(label, value) {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`${label} must be non-empty`)
  }
}

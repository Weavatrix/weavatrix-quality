import assert from 'node:assert/strict'
import test from 'node:test'
import { appendEvent, createJournal, serialize } from '../src/index.mjs'

test('emits an observed_only journal Rust can admit', () => {
  const journal = createJournal({ sessionId: 'staging-checkout', route: '/checkout' })
  appendEvent(journal, {
    action: { action: 'activate', target: { test_id: 'pay' } },
    after: { route: '/checkout/done' },
  })
  const document = JSON.parse(serialize(journal))
  assert.equal(document.schema_v, 1)
  assert.equal(document.source, 'continuous')
  assert.equal(document.observed_only, true)
  assert.equal(document.events.length, 1)
  assert.equal(document.max_events, undefined)
})

test('refuses xpath, unknown actions, and path-shaped session ids', () => {
  assert.throws(
    () => createJournal({ sessionId: '../secret', route: '/' }),
    /path/,
  )
  const journal = createJournal({ sessionId: 'ok', route: '/' })
  assert.throws(
    () => appendEvent(journal, {
      action: { action: 'activate', target: { xpath: '//button' } },
      after: { route: '/' },
    }),
    /XPath/,
  )
  assert.throws(
    () => appendEvent(journal, {
      action: { action: 'assert', obligation: 'paid' },
      after: { route: '/' },
    }),
    /cannot include action/,
  )
})

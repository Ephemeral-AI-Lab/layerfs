import assert from 'node:assert/strict'
import test from 'node:test'
import { compareNeeds, visibleFailures, worstTone } from '../src/client/storage/home-notices.ts'

test('Home priority is independent of inventory order, including ties', () => {
  const keys = ['setup', 'job:a', 'job:b', 'unavailable:m', 'unreachable:m', 'gone:m', 'record:m', 'needs:m', 'down:s', 'stopped:m']
  assert.deepEqual(keys.toReversed().map(key => ({ key })).sort(compareNeeds).map(n => n.key), keys)
})

test('only an actual later success clears the same failure; dismissed and removed Stores clear', () => {
  const failed = { id: 'a', kind: 'backup', store: 'books', ended: 10, outcome: 'failed' }
  const stores = [{ name: 'books' }]
  const unknown = { ...failed, id: 'b', ended: 11, outcome: 'unknown' }
  const success = { ...unknown, id: 'c', ended: 12, outcome: 'succeeded' }
  assert.deepEqual(visibleFailures([failed, unknown], stores, new Set()), [failed, unknown])
  assert.deepEqual(visibleFailures([failed, unknown, success], stores, new Set()), [])
  assert.deepEqual(visibleFailures([failed], stores, new Set(['a'])), [])
  assert.deepEqual(visibleFailures([failed], [], new Set()), [])
})

test('a tab reports the worst visible signal, healthy rows remain quiet', () => {
  assert.equal(worstTone([]), undefined)
  assert.equal(worstTone([undefined, 'warning']), 'warning')
  assert.equal(worstTone(['danger', 'warning']), 'danger')
})

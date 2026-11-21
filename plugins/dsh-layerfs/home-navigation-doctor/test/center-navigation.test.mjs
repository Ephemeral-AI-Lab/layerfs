import test from 'node:test'
import assert from 'node:assert/strict'
import * as tabs from '../src/client/storage/tabs.ts'

test('Center accepts existing routes and nested backup paths, rejects incomplete targets', () => {
  for (const id of ['health', 'logs', 'machines', 'overview', 'store:one', 'backup:bucket/a/b', 'project:s/p', 'sandbox:native/s']) assert.equal(tabs.isTabId(id), true, id)
  for (const id of ['project:s', 'sandbox:/s', 'backup:b/', 'store:', 'unknown']) assert.equal(tabs.isTabId(id), false, id)
})

test('removing a parent forgets its descendants in both directions and keeps the nearest previous page', () => {
  const h = { stack: ['logs', 'store:a', 'project:a/p', 'logs', 'store:another'], at: 2 }
  assert.deepEqual(tabs.forgetPages(h, id => tabs.underPage(id, 'store:a')), { stack: ['logs', 'store:another'], at: 0 })
  assert.equal(tabs.underPage('sandbox:m/s', 'machine:m'), true)
  assert.equal(tabs.underPage('backup:b/f/part', 'bucket:b'), true)
  assert.deepEqual(tabs.forgetPages({stack:['logs'], at:0}, () => true), {stack:['overview'], at:0})
})

test('history caps at fifty, ignores current page and drops forward entries on a new visit', () => {
  let h = { stack: ['overview'], at: 0 }
  for (let i = 0; i < 60; i++) h = tabs.visitPage(h, `store:${i}`)
  assert.equal(h.stack.length, 50)
  assert.equal(h.stack[0], 'store:10')
  assert.equal(tabs.visitPage(h, 'store:59'), h)
  assert.deepEqual(tabs.visitPage({...h, at:0}, 'health'), {stack:['store:10','health'], at:1})
})

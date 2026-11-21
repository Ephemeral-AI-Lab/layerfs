import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { defaultStoreName, locationsOf, preferredStore, slugify } from '../src/client/picker/location.ts'
import { sidebarProjects } from '../src/client/projects/recent.ts'

test('slugify turns free text into a project name', () => {
  assert.equal(slugify('My App'), 'my-app')
  assert.equal(slugify('  --Hello, World!!  '), 'hello-world')
  assert.equal(slugify('v1.2_beta'), 'v1.2_beta')
  assert.equal(slugify('x'.repeat(80)).length, 63)
  assert.equal(slugify('***'), '')
})

test('locations: native SQLite then PostgreSQL, never Scratch or other machines', () => {
  const view = { stores: [
    { name: 'scratch', kind: 'sqlite', machine: 'native' },
    { name: 'team', kind: 'postgresql', postgresql: { target: 'db.lan:5432/app' }, connectedFrom: [{ machine: null }, { machine: 'vm2' }] },
    { name: 'local', kind: 'sqlite', machine: 'native' },
    { name: 'far', kind: 'sqlite', machine: 'vm1' },
  ] }
  assert.deepEqual(locationsOf(view, 'scratch'), [
    { store: 'local', kind: 'sqlite', server: null, connectedFrom: [] },
    { store: 'team', kind: 'postgresql', server: 'db.lan:5432/app', connectedFrom: ['native', 'vm2'] },
  ])
  assert.equal(defaultStoreName(view), 'projects')
  assert.equal(defaultStoreName({ stores: [{ name: 'projects' }, { name: 'projects-2' }] }), 'projects-3')
})


test('the last Store is preselected while it is still offered', () => {
  const locations = [{ store: 'a' }, { store: 'b' }]
  assert.equal(preferredStore(locations, 'b'), 'b')
  assert.equal(preferredStore(locations, 'gone'), 'a')
  assert.equal(preferredStore([], null), '')
})

test('sidebar projects: projects with sessions, newest first, hidden ones out', () => {
  const catalog = [
    { key: 's1/app', name: 'app', store: 's1', projectId: 'p-app' },
    { key: 's2/app', name: 'app', store: 's2', projectId: 'p-app2' },
    { key: 'pg/api', name: 'api', store: 'pg', projectId: 'p-api', kind: 'postgresql' },
    { key: 's1/idle', name: 'idle', store: 's1', projectId: 'p-idle' },
  ]
  const recent = [
    { project: 'pg/api', projectId: 'p-api', lastActivity: 30, sessionIds: ['a1'] },
    { project: 's1/app', projectId: 'p-app', lastActivity: 50, newestSession: 45, sessionIds: ['b1'] },
    { project: 's2/app', lastActivity: 40, sessionIds: ['c1'] },
    { project: 's1/draft', lastActivity: 60, sessionIds: ['d1'] },
    { project: 's1/gone', lastActivity: 10, sessionIds: ['e1'] },
  ]
  const rows = sidebarProjects([...recent, { project: 's1/idle', lastActivity: 99, sessionIds: [] }], catalog, {})
  // An unused draft keeps a just-made project listed; a project without sessions never is.
  assert.deepEqual(rows.map((r) => r.row.key), ['s1/draft', 's1/app', 's2/app', 'pg/api', 's1/gone'])
  // The Store shows only when two listed projects share a name.
  assert.deepEqual(rows.map((r) => r.showStore), [false, true, true, false, false])
  assert.equal(rows[0].row.catalogState, 'removed')
  // Hidden until a newer session.
  // Hidden until a newer session: later activity on an old session does not bring it back.
  assert.ok(!sidebarProjects(recent, catalog, { 'p-app': 46 }).some((r) => r.row.key === 's1/app'))
  assert.ok(sidebarProjects(recent, catalog, { 'p-app': 44 }).some((r) => r.row.key === 's1/app'))
  // A project the catalog doesn't list yet, in a Store that exists, is not called removed.
  assert.equal(sidebarProjects(recent, catalog, {}, new Set(['s1'])).find((r) => r.row.key === 's1/gone').row.catalogState, undefined)
  // An unread catalog marks nothing removed.
  assert.equal(sidebarProjects(recent, null, {}).find((r) => r.row.key === 's1/gone').row.catalogState, undefined)
})

// DSH owns these nodes. Catch accidental reintroduction of either DOM workaround.
test('the Workspaces switch and Center launcher stay inside public extension points', () => {
  for (const path of ['projects/WorkspacesView.tsx', 'projects/workspaces.module.css', 'storage/index.tsx']) {
    const source = readFileSync(new URL(`../src/client/${path}`, import.meta.url), 'utf8')
    assert.doesNotMatch(source, /createPortal|MutationObserver|document\.(?:querySelector|addEventListener)|:global\(\[class\*=/, path)
  }
})

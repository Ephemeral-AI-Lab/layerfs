#!/usr/bin/env node
// Keyless live browser proof. Use a private dev server with dev/fake-llm.overlay.yml.
// Exercises normal model labels, script inspection, Stop and same-session tools.
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
import { readdir, readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { openDev, runTurn } from '../tools/e2e-lib.mjs'

const { browser, page, logs } = await openDev()
const op = (name, args = {}) => page.evaluate(([name, args]) => window.__dshLayerfs.op(name, args), [name, args])
const until = async (description, read, timeout = 240000, interval = 250) => {
  const deadline = Date.now() + timeout
  for (;;) {
    let value
    try { value = await read() } catch (error) {
      // A client bundle refresh retires the old Remote namespace. Re-observe
      // the same run; this retry is deliberately confined to read-only polls.
      if (!/Remote invocation .* was aborted/.test(String(error)) || Date.now() >= deadline) throw error
      await page.waitForTimeout(500)
      continue
    }
    if (value) return value
    if (Date.now() >= deadline) throw new Error(`Timed out: ${description}`)
    await page.waitForTimeout(interval)
  }
}
// Turns of a session that closed without a single step (an empty "Completed" turn).
const sessionHome = process.env.DSH_LAYERFS_SESSION_HOME ?? join(process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url)), 'project-sessions')
const findLog = async (directory, sessionId) => {
  for (const entry of await readdir(directory, { withFileTypes: true }).catch(() => [])) {
    if (!entry.isDirectory()) continue
    const path = join(directory, entry.name)
    if (entry.name === sessionId) return join(path, 'session.v4.jsonl')
    const found = await findLog(path, sessionId)
    if (found) return found
  }
}
const emptyTurns = async (sessionId) => {
  const file = await findLog(sessionHome, sessionId)
  assert.ok(file, `native log of ${sessionId}`)
  const rows = (await readFile(file, 'utf8')).trim().split('\n').map(line => { try { return JSON.parse(line) } catch { return {} } })
  const empty = []
  let open
  for (const row of rows) {
    if (row.type === 'turn/start') open = { turn: row.data.turn, steps: 0 }
    else if (row.type === 'step/start' && open) open.steps++
    else if (row.type === 'turn/end' && open) { if (!open.steps && row.data.reason?.kind !== 'aborted') empty.push({ turn: open.turn, reason: row.data.reason }); open = undefined }
  }
  return empty
}
const model = () => page.locator('button[aria-label^="Select model, current"]').first()
const mixed = process.env.DSH_SIMULATION_MIXED === '1'
const cycles = Number(process.env.DSH_SIMULATION_CYCLES ?? 1)
const launch = (options = {}) => op('simulation.launch', { project: 'design', machine: 'native', mixed, cycles, ordinaryModel: { provider: 'fake', model: 'scripted' }, ...options })
const settled = (id) => until('settled script', async () => {
  const value = await op('simulation.script', { sessionId: id })
  return ['completed', 'stopped', 'uncertain', 'failed'].includes(value.state) ? value : null
  // Each scripted bash call costs about a second of host tool overhead (360 calls per cycle).
}, Math.max(900000, cycles * (mixed ? 1200000 : 600000)))

async function continueNormally(id, marker) {
  const result = await runTurn(page, {
    newSession: false, model: 'Fake scripted',
    message: `bash: test -n "$DSH_LAYERFS_WS" && "$DSH_LAYERFS_CLI" workspace exec "$DSH_LAYERFS_WS" 'ls' --stream\nbash: echo ${marker} > ${marker}.txt`,
    expect: /done: \d+ commands/, timeoutMs: 60000,
  })
  assert.ok(result.ok, result.error)
  assert.doesNotMatch(result.transcript, /done: \d+ commands \([1-9]\d* failed\)/, 'ordinary followup tools succeed')
  assert.equal(result.sessionId, id, 'ordinary followup keeps the session identity')
  assert.match(result.transcript, new RegExp(marker), 'ordinary tool appears in the same conversation')
  const annotations = await op('session.annotations', { sessionId: id })
  assert.ok(annotations.invocations.some(row => row.operation === 'workspace.exec' && row.toolCallId && row.outcome === 'success'), 'continuation runs a successful real project tool')
}

let active
let preserveActive = false
try {
  if (process.env.DSH_SIMULATION_STOP_ONLY !== '1') {
  active = await launch()
  const initial = await op('simulation.script', { sessionId: active.sessionId })
  assert.ok(initial.script?.name)
  await until('script model name', async () => ((await model().getAttribute('aria-label')) ?? '').includes(initial.script.name))
  assert.equal(await page.getByRole('button', { name: 'View script', exact: true }).count(), 0, 'a running script has no script control')
  const complete = await settled(active.sessionId)
  assert.equal(complete.state, 'completed', JSON.stringify({ state: complete.state, error: complete.script?.error }))
  await page.getByRole('button', { name: 'View script', exact: true }).click({ timeout: 15000 })
  await page.getByRole('dialog', { name: initial.script.name, exact: true }).waitFor()
  assert.match(await page.getByRole('dialog').innerText(), /agentKey/)
  await page.getByRole('button', { name: 'Close', exact: true }).click()
  const transcript = await page.locator('body').innerText()
  assert.match(transcript, /Forking |I'll revise |Merging |Delegating /, 'assistant narration precedes tool calls')
  assert.doesNotMatch(transcript, /The requested project changes are saved/, 'final replies come from receipts')
  const root = (await op('runtime.status')).root
  const journal = (await readFile(join(root, 'simulations', active.runId, 'journal.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse)
  const sessionIds = [...new Set(journal.filter(row => row.type === 'binding').map(row => row.sessionId))]
  assert.ok(sessionIds.length > 1, 'the workload creates native parent and child sessions')
  if (cycles >= 3) {
    const childKeys = journal.filter(row => row.type === 'binding' && !row.agentKey.endsWith('-main')).map(row => row.agentKey)
    assert.ok(childKeys.some(key => journal.filter(row => row.type === 'turn-settled' && row.agentKey === key).length > 1), 'later assignments reuse one native child session')
  }
  const invocations = (await Promise.all(sessionIds.map(sessionId => op('session.annotations', { sessionId })))).flatMap(sidecar => sidecar.invocations)
  assert.ok(invocations.some(row => row.operation === 'branch.publish' && row.toolCallId && row.versions.some(version => version.kind === 'Layer' && version.effect === 'created')), 'real CLI publications label their exact tool calls')
  assert.ok(invocations.filter(row => row.operation === 'workspace.exec').every(row => row.versions.length === 0), 'workspace.exec never gains checkpoint links')
  if (mixed) assert.ok(invocations.some(row => row.operation === 'workspace.commit' && row.toolCallId && row.versions.some(version => version.kind === 'Commit' && version.effect === 'created')), 'Mixed workspace commits label their exact tool calls')
  await continueNormally(active.sessionId, `continuation-${Date.now()}`)
  await page.reload({ waitUntil: 'networkidle' })
  await page.waitForFunction(() => window.__dshLayerfs)
  assert.equal(await page.evaluate(() => window.__dshLayerfs.selectedSessionId()), active.sessionId)
  assert.equal((await op('simulation.script', { sessionId: active.sessionId })).state, 'completed', 'reloading does not replay scripts')
  console.log('Completed workload, inspected its script, reloaded and continued with ordinary tools.')
  }

  active = await launch({ dispatchDelayMs: 10000, cycles: 1 })
  await page.getByRole('button', { name: /^Stop(?:\s|$)/ }).first().waitFor()
  await page.getByRole('button', { name: /^Stop(?:\s|$)/ }).first().click()
  const stopped = await settled(active.sessionId)
  assert.equal(stopped.state, 'stopped', JSON.stringify({ state: stopped.state, error: stopped.script?.error }))
  await continueNormally(active.sessionId, `after-stop-${Date.now()}`)
  await page.waitForTimeout(1000)
  assert.equal((await op('simulation.script', { sessionId: active.sessionId })).state, 'stopped', 'normal input never restarts the script')
  assert.deepEqual(await emptyTurns(active.sessionId), [], 'Stop during a wait leaves no empty turn behind')
  console.log('Native Stop during a wait, then same-session tool continuation.')

  // Stop while a project command is in flight. The script ends for good
  // (stopped, or failed without a pin) and the session is not locked: its real
  // model is selected again and continues from the project's current files.
  active = await launch({ cycles: 1 })
  const journalPath = join((await op('runtime.status')).root, 'simulations', active.runId, 'journal.jsonl')
  const journalRows = async () => (await readFile(journalPath, 'utf8').catch(() => '')).trim().split('\n').filter(Boolean).map(line => { try { return JSON.parse(line) } catch { return null } }).filter(Boolean)
  const inflight = await until('an in-flight project command', async () => {
    const rows = await journalRows()
    const done = new Set(rows.filter(row => row.type === 'result').map(row => row.stepKey))
    return rows.find(row => row.type === 'intent' && row.origin === 'tool' && !done.has(row.stepKey)) ?? null
  }, 120000, 20)
  await op('simulation.stop', { sessionId: active.sessionId })
  const ended = await settled(active.sessionId)
  assert.ok(['stopped', 'failed'].includes(ended.state), JSON.stringify({ state: ended.state, error: ended.script?.error }))
  assert.doesNotMatch(ended.script?.error ?? '', /Reconcile/)
  const outcome = (await journalRows()).find(row => row.type === 'result' && row.stepKey === inflight.stepKey)
  console.log(`Stopped during ${inflight.stepKey} (${outcome ? (outcome.ok ? 'it completed' : 'cancelled/uncertain') : 'no result'}); script state ${ended.state}.`)
  assert.equal((await op('simulation.status', { sessionId: active.sessionId })).locked, false, 'a terminal script never locks its session')
  await until('real model restored', async () => ((await model().getAttribute('aria-label')) ?? '').includes('Fake scripted'), 30000)
  await continueNormally(active.sessionId, `after-mid-stop-${Date.now()}`)
  await page.waitForTimeout(1000)
  assert.equal((await op('simulation.script', { sessionId: active.sessionId })).state, ended.state, 'the script never runs again')
  assert.deepEqual(await emptyTurns(active.sessionId), [], 'Stop leaves no empty turn behind (child settlement notices never wake the root)')
  assert.deepEqual(logs.filter(line => line.startsWith('[pageerror]')), [])
  console.log('Simulation browser proof passed: native Stop during a wait and mid-command, each followed by same-session real-model tools.')
} catch (error) {
  preserveActive = /Timed out: settled script/.test(String(error))
  await page.screenshot({ path: join(process.env.DSH_LAYERFS_DEV_DIR ?? '.dev', 'simulation-failure.png'), fullPage: true }).catch(() => {})
  console.error((await page.locator('body').innerText()).slice(-6000))
  console.error(logs.join('\n'))
  throw error
} finally {
  if (active && !preserveActive) await op('simulation.stop', { sessionId: active.sessionId }).catch(() => {})
  await browser.close()
}

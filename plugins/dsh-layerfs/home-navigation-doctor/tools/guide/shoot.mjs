#!/usr/bin/env node
// Screenshots for the UI guide (docs/guide), taken on a dev server.
//   DSH_LAYERFS_DEV_DIR=<dev dir> node tools/guide/shoot.mjs tools/guide/shots/<page>.mjs [--only <file>] [--out docs/guide/img]
//
// A shot module exports `default` an array of shots (or `(h) => shots`):
//   { file: 'store-overview',            // -> <out>/store-overview.png
//     size: '1440x900',                  // viewport (default 1440x900)
//     run: async (page, h) => {},        // get the page into place (h: helpers below)
//     callouts: [(page) => locator, …],  // numbered 1, 2, 3… in this order (optional)
//     clip: (page) => locator,           // screenshot only this element (optional)
//     pad: 12 }                          // padding around clip (default 12)
// Callouts are drawn on the page itself (an overlay added before the shot and
// removed after it), so a retake after a UI change is one command. A callout
// whose element is missing fails that shot instead of drawing a stray number.
// Each shot prints `file: ok` or `file: FAIL <why>`; the exit code is 1 when any failed.
import { mkdirSync } from 'node:fs'
import { saveShot } from './shot-image.mjs'
import { join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { chromium } from 'playwright-core'
import { devUrl, dismiss } from '../ui.mjs'

const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
const args = process.argv.slice(2)
const flag = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback }
const modulePath = args[0]?.startsWith('--') ? undefined : args[0]
if (!modulePath) { console.error('usage: shoot.mjs <shots module> [--only <file>] [--out <dir>]'); process.exit(2) }
const only = flag('--only')
const out = resolve(flag('--out', fileURLToPath(new URL('../../docs/guide/img', import.meta.url))))
mkdirSync(out, { recursive: true })

/** Helpers handed to each shot's `run`. */
function helpers(page) {
  const bar = () => page.locator('header[aria-label="Home navigation"]')
  const h = {
    wait: (ms = 800) => page.waitForTimeout(ms),
    dismiss: () => dismiss(page),
    /** Open Center (the sidebar's LayerFS row) and wait for its nav bar. */
    async center() {
      await dismiss(page)
      if (!(await bar().isVisible().catch(() => false))) {
        await page.getByRole('navigation', { name: 'Global panels' }).getByRole('button', { name: 'LayerFS', exact: true }).first().click()
        await bar().waitFor({ timeout: 20000 })
      }
      await page.waitForTimeout(600)
    },
    /** Center's Home (its first breadcrumb). */
    async home() {
      await h.center()
      const root = bar().getByRole('navigation', { name: 'Location' }).getByRole('button', { name: 'Home', exact: true })
      if (await root.count()) { await root.first().click(); await page.waitForTimeout(800) }
    },
    /** Center's Jump to: type `query`, open the first hit (or the hit whose name is `pick`). */
    async jump(query, pick) {
      await h.center()
      await bar().getByRole('button', { name: 'Jump to' }).first().click()
      const input = page.getByRole('combobox', { name: 'Jump to' })
      await input.fill(query)
      await page.waitForTimeout(400)
      if (pick) await page.getByRole('option').filter({ hasText: pick }).first().click()
      else await input.press('Enter')
      await page.waitForTimeout(1500)
    },
    /** A tab by its accessible name, in the main column. */
    async tab(name) {
      await page.getByRole('tab', { name, exact: true }).first().click()
      await page.waitForTimeout(1200)
    },
    /** Open a conversation by session id. */
    async session(sessionId) {
      await page.evaluate((id) => window.__dshLayerfs?.openSession(id), sessionId)
      await page.waitForTimeout(2000)
    },
    /** Wait until `text` shows (string or RegExp). */
    async see(text, timeout = 20000) {
      await page.getByText(text).first().waitFor({ timeout })
    },
  }
  return h
}

const mod = await import(pathToFileURL(resolve(modulePath)).href)
const browser = await chromium.launch({ executablePath: CHROME, headless: true })
let failed = 0
const pages = new Map()
async function pageFor(size) {
  if (pages.has(size)) return pages.get(size)
  const [width, height] = size.split('x').map(Number)
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 2 })
  await page.addInitScript(() => { try { localStorage.setItem('dsh-layerfs:debug', '1') } catch {} })
  await page.goto(devUrl(), { waitUntil: 'networkidle' })
  await page.locator('[data-composer-input=true], [role=treeitem]').first().waitFor({ timeout: 30000 }).catch(() => {})
  await page.waitForTimeout(1500)
  await dismiss(page)
  pages.set(size, page)
  return page
}
try {
  const first = await pageFor('1440x900')
  const shots = typeof mod.default === 'function' ? await mod.default(helpers(first)) : mod.default
  for (const shot of shots) {
    if (only && shot.file !== only) continue
    const page = await pageFor(shot.size ?? '1440x900')
    const h = helpers(page)
    try {
      await shot.run?.(page, h)
      await saveShot(page, shot, join(out, `${shot.file}.png`))
      console.log(`${shot.file}: ok`)
    } catch (error) {
      failed++
      console.log(`${shot.file}: FAIL ${String(error?.message ?? error).split('\n')[0]}`)
      await page.screenshot({ path: join(out, `_failed-${shot.file}.png`) }).catch(() => {})
    } finally {
      await page.keyboard.press('Escape').catch(() => {})
      await page.waitForTimeout(300)
    }
  }
} finally {
  await browser.close()
}
process.exit(failed ? 1 : 0)

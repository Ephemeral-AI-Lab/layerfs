import { fileURLToPath } from 'node:url'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
/** The running dev server's authenticated URL (scripts/dev.mjs writes <dev dir>/url.txt). */
export function devUrl() {
  const dir = process.env.DSH_LAYERFS_DEV_DIR ?? fileURLToPath(new URL('../.dev', import.meta.url))
  return readFileSync(join(dir, 'url.txt'), 'utf8').trim()
}
// Shared browser helpers for tools/shot.mjs step modules.
export async function dismiss(page) {
  for (let i = 0; i < 4; i++) {
    const cont = page.getByRole('button', { name: 'Continue' })
    if (await cont.count()) { await cont.first().click(); await page.waitForTimeout(400); continue }
    const dialog = page.getByRole('dialog')
    if (await dialog.count()) {
      const close = dialog.getByRole('button', { name: /close/i })
      if (await close.count()) { await close.first().click(); await page.waitForTimeout(400); continue }
      await page.keyboard.press('Escape'); await page.waitForTimeout(400); continue
    }
    break
  }
}
/** Pages with their own button on Home's head card (not in the sidebar). */
const CENTER_PAGES = new Set(['Health', 'Logs', 'Settings'])
export async function openPanel(page, label) {
  await dismiss(page)
  if (CENTER_PAGES.has(label)) {
    const bar = page.locator('header[aria-label="Home navigation"]')
    if (!(await bar.isVisible().catch(() => false))) {
      await page.getByRole('navigation', { name: 'Global panels' }).getByRole('button', { name: 'LayerFS', exact: true }).first().click()
      await bar.waitFor({ timeout: 15000 })
    }
    // Home's head card holds the buttons: go up to Home first (its crumb is a button on any other page).
    const root = bar.getByRole('navigation', { name: 'Location' }).getByRole('button', { name: 'Home', exact: true })
    if (await root.count()) { await root.first().click(); await page.waitForTimeout(400) }
    await page.getByRole('navigation', { name: 'Health, Logs and Settings' }).getByRole('button', { name: new RegExp(`^${label}\\b`) }).first().click()
    await page.waitForTimeout(1500)
    return
  }
  const titled = page.locator('[class*=panelTitle]', { hasText: new RegExp(`^${label}$`) }).first()
  // At narrow widths the sidebar collapses to icons: fall back to the entry's accessible name.
  if (await titled.isVisible().catch(() => false)) await titled.click()
  else await page.getByRole('button', { name: label, exact: true }).first().click()
  await page.waitForTimeout(1500)
}

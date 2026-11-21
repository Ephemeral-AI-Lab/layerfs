// Shots for docs/guide/logs.md: Center's Logs page. Read only: Reset… is opened and closed, never confirmed.
const SESSION_ROW = (p) => p.locator('tbody tr').filter({ hasText: 'workspace commit' }).filter({ hasText: 'turn 3' }).first()

/** Center's Logs page, from Center's navigation bar (its tool group). */
async function logs(page, h) {
  await h.home()
  await page.locator('header[aria-label="Home navigation"]').getByRole('button', { name: 'Logs', exact: true }).click()
  await page.getByRole('table', { name: 'Calls' }).waitFor({ timeout: 20000 })
  await h.wait(1200)
  // later calls may push the fixture conversation's row onto an older page
  for (let i = 0; i < 8 && !(await SESSION_ROW(page).isVisible().catch(() => false)); i++) {
    const older = page.getByRole('button', { name: 'Load older calls' })
    if (!(await older.count())) break
    await older.click()
    await h.wait(1500)
  }
  await SESSION_ROW(page).scrollIntoViewIfNeeded().catch(() => {})
  // no stray hover tip from the last click
  await page.mouse.move(2, 600)
  await h.wait(500)
}
/** The detail of the fixture conversation's turn-3 checkpoint commit. */
async function detail(page, h) {
  await logs(page, h)
  await SESSION_ROW(page).locator('td').nth(2).click()
  await page.getByRole('dialog').getByText('Saved version').waitFor({ timeout: 10000 })
  await h.wait(1500)
}
const dialog = (p) => p.getByRole('dialog')

export default [
  {
    file: 'logs-overview',
    run: logs,
    callouts: [
      (p) => p.getByRole('button', { name: 'Refresh', exact: true }).last(),
      (p) => p.getByRole('button', { name: 'Reset…' }),
      (p) => p.getByRole('textbox', { name: 'Project (store/project)' }),
      (p) => p.getByRole('button', { name: 'Operation' }),
      (p) => p.getByRole('tablist', { name: 'Client' }),
      (p) => p.getByRole('tablist', { name: 'Outcome' }),
      (p) => p.getByRole('checkbox', { name: 'Writes only' }).locator('xpath=..'),
      (p) => SESSION_ROW(p).locator('td').nth(2),
      (p) => SESSION_ROW(p).locator('td').nth(5),
      (p) => SESSION_ROW(p).locator('td').nth(6),
    ],
  },
  {
    file: 'logs-detail',
    run: detail,
    callouts: [
      (p) => dialog(p).getByRole('button', { name: 'Previous call' }),
      (p) => dialog(p).getByRole('button', { name: 'Copy command' }),
      (p) => dialog(p).getByRole('region', { name: 'Details' }),
      (p) => dialog(p).getByRole('button', { name: 'Open session' }),
      (p) => dialog(p).getByRole('button', { name: 'Calls of this checkpoint' }),
      (p) => dialog(p).getByRole('button', { name: 'Calls of this session' }),
    ],
  },
  {
    file: 'logs-output',
    run: async (page, h) => {
      await detail(page, h)
      await dialog(page).getByRole('region', { name: 'Output' }).scrollIntoViewIfNeeded()
      await h.wait(600)
    },
    clip: (p) => dialog(p).getByRole('region', { name: 'Output' }),
  },
  {
    file: 'logs-session',
    run: async (page, h) => {
      await detail(page, h)
      await dialog(page).getByRole('button', { name: 'Calls of this session' }).click()
      await page.getByRole('button', { name: 'Show all calls' }).waitFor({ timeout: 10000 })
      await h.wait(1500)
    },
    callouts: [
      (p) => p.getByRole('button', { name: 'Show all calls' }).locator('xpath=..'),
      (p) => p.getByRole('checkbox', { name: 'Writes only' }).locator('xpath=..'),
    ],
  },
  {
    // the confirm only: Escape closes it after the shot; Reset is never pressed
    file: 'logs-reset',
    run: async (page, h) => {
      await logs(page, h)
      await page.getByRole('button', { name: 'Reset…' }).click()
      await page.getByRole('dialog', { name: 'Reset the call log?' }).waitFor({ timeout: 10000 })
      await h.wait(600)
    },
    clip: (p) => p.getByRole('dialog', { name: 'Reset the call log?' }),
  },
]

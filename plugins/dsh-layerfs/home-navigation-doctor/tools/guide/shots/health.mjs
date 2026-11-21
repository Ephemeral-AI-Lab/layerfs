// Shots for docs/guide/health.md: Center's Doctor page.
// It runs one read-only doctor (`store doctor guide-notes`) when the page holds no result for that
// Store yet, so the Stores tab shows a checked row and the fix LayerFS proposes. No fix is pressed.
const STORE = 'guide-notes'

/** Center's Doctor page, from Center's navigation bar (its tool group). */
async function doctor(page, h) {
  await h.home()
  await page.locator('header[aria-label="Home navigation"]').getByRole('button', { name: 'Doctor', exact: true }).click()
  await page.getByRole('button', { name: 'Check all' }).waitFor({ timeout: 20000 })
  await h.wait(1200)
}
const row = (p, kind, name) => p.locator(`[data-fid="row:${kind}:${name}"]`)
/** The Stores tab with guide-notes checked and open. */
async function storeOpen(page, h) {
  await doctor(page, h)
  await page.getByRole('tab', { name: /^Stores/ }).click()
  await h.wait(800)
  const check = page.locator(`[data-fid="check:store:${STORE}"]`)
  // only when nothing was checked yet: one store doctor, read only
  if (await row(page, 'store', STORE).getByText('Not checked').count()) {
    await check.click()
    await page.getByText('Not checked').first().waitFor({ state: 'detached', timeout: 60000 }).catch(() => {})
    await h.wait(2000)
  }
  if ((await row(page, 'store', STORE).getAttribute('aria-expanded')) !== 'true') await row(page, 'store', STORE).click()
  await h.wait(1000)
}

export default [
  {
    file: 'health-overview',
    run: doctor,
    callouts: [
      (p) => p.locator('[data-fid="head"]'),
      (p) => p.getByRole('button', { name: 'Check all' }),
      (p) => p.locator('[data-fid^="row:home:"]'),
      (p) => p.getByRole('tablist', { name: 'Kinds' }),
      (p) => row(p, 'machine', 'docker'),
      (p) => p.locator('[data-fid="marks:machine:docker"]'),
      (p) => p.locator('[data-fid="check:machine:docker"]'),
      (p) => p.getByText(/^Read from records:/),
    ],
  },
  {
    file: 'health-store',
    run: async (page, h) => {
      await storeOpen(page, h)
      // the fix's tip: the calls it runs, then the doctor that checks it again (hover only)
      await page.locator(`[data-fid="fix:store:${STORE}"]`).hover()
      await h.wait(1500)
    },
    callouts: [
      (p) => row(p, 'store', STORE),
      (p) => p.locator(`[data-fid="fix:store:${STORE}"]`),
      (p) => p.locator(`[data-fid="copycall:store:${STORE}"]`).locator('xpath=..'),
      (p) => p.locator(`[data-fid="copyfix:store:${STORE}"]`).locator('xpath=..'),
    ],
    clip: (p) => p.getByRole('tabpanel'),
    pad: 24,
  },
  {
    file: 'health-projects',
    run: async (page, h) => {
      await doctor(page, h)
      await page.getByRole('tab', { name: /^Projects/ }).click()
      await h.wait(1000)
      await page.locator('[data-fid="full:project:guide-notes/handbook"]').hover()
      await h.wait(1500)
    },
    callouts: [
      (p) => p.locator('[data-fid="full:project:guide-notes/handbook"]'),
      (p) => p.locator('[data-fid="check:project:guide-notes/handbook"]'),
    ],
  },
]

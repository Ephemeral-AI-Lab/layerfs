// Shots for docs/guide/presets.md: the Presets dialog ("Try a preset"), opened from Home's head card.
// Read only. Rebuild… and Stop machines… are opened and closed (Escape after the shot); their
// confirm buttons (Continue, Wipe and build, Stop machines) are never pressed. Rebuild comes last:
// its options dialog has a Continue button, which the helpers' dismiss() would press.

/** The Presets dialog, on one preset. */
async function presets(page, h, pick = 'Starter') {
  await h.home()
  await page.getByRole('group').getByRole('button', { name: 'Try a preset', exact: true }).click()
  await page.getByRole('navigation', { name: 'Presets' }).waitFor({ timeout: 20000 })
  await page.getByRole('navigation', { name: 'Presets' }).getByRole('button', { name: new RegExp(`^${pick}`) }).click()
  await h.wait(1500)
}
const row = (p) => p.getByRole('button', { name: /^bookshelf Neighborhood/ })

export default [
  {
    file: 'presets-overview',
    run: presets,
    callouts: [
      (p) => p.getByRole('navigation', { name: 'Presets' }),
      (p) => p.getByRole('navigation', { name: 'Presets' }).getByRole('button', { name: /^Starter/ }),
      (p) => p.locator('[class*=presets_status]').first(),
      (p) => p.getByRole('button', { name: 'View script', exact: true }),
      (p) => p.getByRole('button', { name: /^(Rebuild…|Build…)$/ }),
      (p) => p.getByRole('button', { name: 'More actions' }),
      (p) => row(p),
      (p) => p.getByRole('button', { name: 'View script for bookshelf' }),
      (p) => p.getByRole('region', { name: /^(Last build|Previous build)$/ }),
    ],
  },
  {
    file: 'presets-studio',
    run: (page, h) => presets(page, h, 'Studio'),
    callouts: [
      (p) => p.getByRole('navigation', { name: 'Presets' }).getByRole('button', { name: /^Studio/ }),
      (p) => p.getByRole('button', { name: /^(Rebuild…|Build…)$/ }),
      (p) => p.getByRole('region', { name: 'Project library' }),
    ],
  },
  {
    file: 'presets-resource',
    run: async (page, h) => {
      await presets(page, h)
      if ((await row(page).getAttribute('aria-expanded')) !== 'true') await row(page).click()
      await h.wait(800)
      const log = page.locator('summary').filter({ hasText: /^Log$/ })
      await log.click()
      await h.wait(800)
      await page.getByRole('region', { name: 'Selected resource' }).scrollIntoViewIfNeeded()
      await h.wait(400)
    },
    callouts: [
      (p) => p.getByRole('region', { name: 'Selected resource' }),
      (p) => p.locator('summary').filter({ hasText: /^Log$/ }),
    ],
  },
  {
    file: 'presets-script',
    run: async (page, h) => {
      await presets(page, h)
      await page.getByRole('button', { name: 'View script', exact: true }).click()
      await page.getByText(/^1–\d+ of [\d,]+ commands$/).waitFor({ timeout: 30000 })
      await h.wait(1000)
    },
    callouts: [
      (p) => p.getByText(/^1–\d+ of [\d,]+ commands$/),
      (p) => p.getByRole('button', { name: 'Copy this page' }),
      (p) => p.getByRole('button', { name: 'Download full plan' }),
      (p) => p.getByText('Reset environment', { exact: true }).first(),
    ],
  },
  {
    // the confirm only: Escape closes it; Stop machines is never pressed
    file: 'presets-stop',
    run: async (page, h) => {
      await presets(page, h)
      await page.getByRole('button', { name: 'More actions' }).click()
      await page.getByRole('menuitem', { name: 'Stop machines…' }).click()
      await page.getByRole('dialog', { name: /^Stop machines and delete project sessions\?$/ }).waitFor({ timeout: 60000 })
      await page.locator('summary').filter({ hasText: /^Machines to close$/ }).click()
      await h.wait(800)
    },
    clip: (p) => p.getByRole('dialog', { name: /^Stop machines and delete project sessions\?$/ }),
  },
  {
    // last (see the top): the options step only; Continue is never pressed
    file: 'presets-rebuild',
    run: async (page, h) => {
      await presets(page, h)
      await page.getByRole('button', { name: /^(Rebuild…|Build…)$/ }).click()
      await page.getByRole('dialog', { name: /^(Rebuild|Build) Starter\?$/ }).waitFor({ timeout: 10000 })
      await h.wait(600)
    },
    clip: (p) => p.getByRole('dialog', { name: /^(Rebuild|Build) Starter\?$/ }),
  },
]

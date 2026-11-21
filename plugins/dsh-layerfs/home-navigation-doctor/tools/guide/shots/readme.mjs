// Shots for docs/guide/README.md: the whole window, once on Center and once on a project conversation.
const SESSION = 'session-ef98eca3-a037-47dd-801a-4eb84d21d36b'

export default [
  {
    file: 'readme-center',
    run: async (page, h) => { await h.home(); await h.wait(2500) },
    callouts: [
      (p) => p.getByRole('navigation', { name: 'Global panels' }).getByRole('button', { name: 'LayerFS', exact: true }),
      (p) => p.locator('header[aria-label="Home navigation"]'),
      (p) => p.locator('header[aria-label="Home navigation"]').getByRole('button', { name: 'Doctor', exact: true }),
      (p) => p.getByRole('tablist').first(),
      (p) => p.getByRole('table').first(),
    ],
  },
  {
    file: 'readme-conversation',
    run: async (page, h) => {
      await h.session(SESSION)
      await page.getByRole('tab', { name: 'Trajectory', exact: true }).first().waitFor({ timeout: 20000 })
      await h.wait(2000)
    },
    callouts: [
      (p) => p.getByRole('tab', { name: 'Trajectory', exact: true }),
      (p) => p.getByRole('tab', { name: 'Files', exact: true }),
      (p) => p.getByRole('tab', { name: 'History', exact: true }),
      (p) => p.getByRole('tab', { name: 'Sandbox', exact: true }),
      (p) => p.getByRole('tab', { name: 'Stats', exact: true }),
    ],
  },
]

import { homedir } from 'node:os'

/** Draw numbered callouts over the given elements; returns a remover. */
async function drawCallouts(page, locators) {
  const boxes = []
  for (const [i, make] of locators.entries()) {
    const loc = make(page).first()
    await loc.waitFor({ state: 'visible', timeout: 8000 }).catch(() => { throw new Error(`callout ${i + 1}: element not visible`) })
    const box = await loc.boundingBox()
    if (!box) throw new Error(`callout ${i + 1}: no box`)
    boxes.push(box)
  }
  await page.evaluate((list) => {
    const layer = document.createElement('div')
    layer.id = 'guide-callouts'
    Object.assign(layer.style, { position: 'fixed', inset: '0', pointerEvents: 'none', zIndex: '2147483647' })
    list.forEach((b, i) => {
      const ring = document.createElement('div')
      Object.assign(ring.style, { position: 'fixed', left: `${b.x - 3}px`, top: `${b.y - 3}px`, width: `${b.width + 6}px`, height: `${b.height + 6}px`,
        border: '2px solid #e5484d', borderRadius: '6px', boxSizing: 'border-box' })
      const badge = document.createElement('div')
      badge.textContent = String(i + 1)
      const left = Math.max(2, b.x - 11), top = Math.max(2, b.y - 11)
      Object.assign(badge.style, { position: 'fixed', left: `${left}px`, top: `${top}px`, width: '22px', height: '22px', borderRadius: '11px',
        background: '#e5484d', color: '#fff', font: '600 12px/22px -apple-system, system-ui, sans-serif', textAlign: 'center',
        boxShadow: '0 0 0 2px #fff' })
      layer.append(ring, badge)
    })
    document.body.append(layer)
  }, boxes)
  return () => page.evaluate(() => document.getElementById('guide-callouts')?.remove())
}

/** Show this computer's home folder as `~` in text, titles and field values, so no account name lands in a shot. */
async function maskHome(page) {
  await page.evaluate((home) => {
    const swap = (s) => (s && s.includes(home) ? s.split(home).join('~') : s)
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT)
    for (let n = walker.nextNode(); n; n = walker.nextNode()) { const v = swap(n.nodeValue); if (v !== n.nodeValue) n.nodeValue = v }
    for (const el of document.querySelectorAll('input, textarea')) { const v = swap(el.value); if (v !== el.value) el.value = v }
  }, homedir())
}

/** Capture one numbered guide image from a prepared browser view. */
export async function saveShot(page, shot, path) {
  await maskHome(page)
  const remove = shot.callouts?.length ? await drawCallouts(page, shot.callouts) : null
  try {
    if (shot.clip) {
      const box = await shot.clip(page).first().boundingBox()
      if (!box) throw new Error('clip element not found')
      const pad = shot.pad ?? 12
      const vp = page.viewportSize()
      const x = Math.max(0, box.x - pad), y = Math.max(0, box.y - pad)
      await page.screenshot({ path, clip: { x, y, width: Math.min(vp.width - x, box.width + pad * 2), height: Math.min(vp.height - y, box.height + pad * 2) } })
    } else {
      await page.screenshot({ path })
    }
  } finally { await remove?.() }
}

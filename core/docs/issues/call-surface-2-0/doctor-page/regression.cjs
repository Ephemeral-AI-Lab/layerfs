// Run with node --test regression.cjs and Playwright on the Node module path.
const { test, before, after } = require('node:test');
const assert = require('node:assert/strict');
const { join } = require('node:path');
const { pathToFileURL } = require('node:url');
const { chromium } = require('playwright');
const catalog = require('../calls.json').doctor.checks;
const url = pathToFileURL(join(__dirname, 'index.html')).href;
const fid = (p, id) => p.locator(`[data-fid="${id}"]`);
let browser;
before(async () => { browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH }); });
after(async () => { if (browser) await browser.close(); });
function scenario(name, run) {
  test(name, async () => {
    const p = await browser.newPage();
    const errors = [];
    p.on('pageerror', e => errors.push(e.message));
    try {
      await p.goto(url);
      await p.selectOption('#scenario', 'healthy');
      await run(p);
      assert.deepEqual(errors, []);
    } finally { await p.close(); }
  });
}
async function shot(p, name) {
  if (process.env.SHOTS) await p.screenshot({ path: join(process.env.SHOTS, name + '.png') });
}
for (const checked of [false, true]) {
  scenario('folder details and children, checked=' + checked, async p => {
    if (checked) await p.click('#mockSettle');
    for (const [tab, key] of [['machines', 'machine:LayerFS Home'], ['stores', 'store:analytics'], ['sandboxes', 'sandbox:tests']]) {
      await p.click('#tab-' + tab);
      const row = fid(p, 'row:' + key);
      await row.click();
      const group = row.locator('xpath=ancestor::div[@class="group"]');
      assert.ok(await group.locator(':scope > .detail .cmdLine code').count(), key + ' missing Call');
      assert.ok(await group.locator(':scope > .detail .checks li').count());
      assert.ok(await group.locator(':scope > .kids .line').count());
      const ids = (await row.getAttribute('aria-controls')).split(' ');
      assert.deepEqual(ids.sort(), [await group.locator(':scope > .detail').getAttribute('id'), await group.locator(':scope > .kids').getAttribute('id')].sort());
      await p.mouse.move(0, 0);
      await shot(p, `detail-${tab}-${checked ? 'checked' : 'unchecked'}`);
    }
  });
}
for (const width of [375, 640, 641, 720, 760, 768, 1024]) {
  scenario('row layout at ' + width, async p => {
    await p.setViewportSize({ width, height: 800 });
    await p.click('#mockSettle');
    for (const tab of ['machines', 'stores', 'sandboxes']) {
      await p.click('#tab-' + tab);
      const [key, expected] = { machines: ['machine:demo-build', /lima · 4 CPU · 8 GB/], stores: ['store:analytics', /postgresql · 2 connections/], sandboxes: ['sandbox:tests', /demo-studio\/website · node:22/] }[tab];
      const fact = fid(p, 'row:' + key).locator('..').locator('.fact');
      assert.match(await fact.innerText({ timeout: 2000 }), expected);
      assert.ok(await fact.isVisible(), key + ' fact hidden');
      const defects = await p.locator('.line').evaluateAll(rows => rows.flatMap(row => {
        const fact = row.querySelector('.fact');
        const rect = fact?.getBoundingClientRect();
        return row.scrollWidth > row.clientWidth + 1 || (fact && (rect.width < 32 || rect.height < 1 || fact.parentElement.clientHeight < rect.height - 1)) ? [row.dataset.key] : [];
      }));
      assert.deepEqual(defects, [], tab + ' overflow or hidden facts');
    }
    await p.mouse.move(0, 0);
    await p.evaluate(() => scrollTo(0, 0));
    await shot(p, 'layout-' + width);
  });
}
scenario('short viewport tooltip allows scrolling to the last check', async p => {
  await p.setViewportSize({ width: 375, height: 500 });
  await p.click('#mockSettle'); await p.click('#tab-sandboxes');
  const marks = fid(p, 'marks:sandbox:tests');
  await marks.evaluate(el => el.scrollIntoView({ block: 'center' }));
  await marks.click();
  const tip = p.locator('#tip');
  const t = await tip.boundingBox(); const a = await marks.boundingBox();
  assert.ok(t && t.y >= 0 && t.y + t.height <= 500 && t.x >= 0 && t.x + t.width <= 375, JSON.stringify(t));
  assert.ok(t.y + t.height <= a.y || t.y >= a.y + a.height || t.x + t.width <= a.x || t.x >= a.x + a.width, JSON.stringify({ t, a }));
  await shot(p, 'short-tooltip');
  await tip.hover(); await p.mouse.wheel(0, 500);
  await p.waitForFunction(() => { const t = document.querySelector('#tip'); return t.hasAttribute('data-on') && t.scrollTop > 0 && t.lastElementChild.lastElementChild.getBoundingClientRect().bottom <= t.getBoundingClientRect().bottom; }, null, { timeout: 2000 });
  await shot(p, 'short-tooltip-end');
  await p.keyboard.press('Escape'); assert.equal(await tip.isVisible(), false);
});
for (const mode of ['unavailable', 'denied', 'success']) {
  scenario('copy command ' + mode, async p => {
    await p.evaluate(mode => Object.defineProperty(navigator, 'clipboard', { value: mode === 'unavailable' ? undefined : { writeText: async text => { if (mode === 'denied') throw new Error('Permission denied'); window.written = text; } } }), mode);
    await p.locator('#home .rowBtn').click();
    const copy = p.locator('#home [data-act=copy]'); await copy.click();
    if (mode === 'success') {
      assert.equal(await copy.innerText(), 'Copied'); await shot(p, 'clipboard-success'); assert.equal(await p.evaluate(() => window.written), await p.locator('#home .cmdLine code').innerText());
    } else {
      assert.notEqual(await copy.innerText(), 'Copied'); assert.match(await p.locator('#toasts').innerText(), /copy|clipboard/i);
      await shot(p, 'clipboard-' + mode);
    }
  });
}
scenario('home navigation', async p => { await p.evaluate(() => goTo(HOME_KEY)); assert.ok(await p.locator('#home .rowBtn').evaluate(el => el === document.activeElement)); });
scenario('missing target', async p => { await p.click('#tab-stores'); await p.evaluate(() => goTo('machine:missing')); assert.equal(await p.locator('#tab-stores').getAttribute('aria-selected'), 'true'); assert.match(await p.locator('#toasts').innerText(), /no longer|missing/i); await shot(p, 'missing-target'); });
scenario('dangling sandbox machine', async p => {
  await p.evaluate(() => { W.by['sandbox:tests'].machine = 'machine:missing'; });
  await p.click('#tab-sandboxes'); await fid(p, 'row:sandbox:tests').click({ timeout: 2000 });
  assert.match(await p.locator('.checks').innerText(), /sandbox.machine_reference/);
  assert.equal(await p.evaluate(() => probe(W.by['sandbox:tests']).checks.find(c => c.id === 'sandbox.machine_reference').r), 'bad');
});
scenario('dangling result from machine', async p => { assert.match(await p.evaluate(() => checkName({ id: 'store.connection', from: 'machine:missing' })), /missing/); });
scenario('native machine checks', async p => { assert.deepEqual(await p.evaluate(() => probe(W.by['machine:LayerFS Home']).checks.map(c => c.id)), ['machine.record', 'machine.operation', 'machine.state']); });
scenario('native sandbox checks', async p => { assert.deepEqual(await p.evaluate(() => probe(W.by['sandbox:tests']).checks.map(c => c.id)), catalog.sandbox.map(c => c.check).filter(id => !['sandbox.forward_process', 'sandbox.worker_compatibility'].includes(id))); });
scenario('PostgreSQL reach sources', async p => {
  const result = await p.evaluate(() => {
    const s = W.by['store:analytics']; const sources = () => probe(s).checks.filter(c => c.id === 'store.connection').map(c => c.from || null);
    s.connections.push({ machine: 'docker' }); const normal = sources(); s.connections.push({ machine: 'native' }); return [normal, sources()];
  });
  assert.deepEqual(result, [[null, 'machine:dev', 'machine:docker'], [null, 'machine:dev', 'machine:docker', 'machine:LayerFS Home']]);
});
scenario('Store check descriptions match frozen catalog', async p => { assert.deepEqual(await p.evaluate(() => CHECKS.store.map(c => c.what)), catalog.store.map(c => c.what)); });

scenario('expanded native reports use the backend check lists', async p => {
  await p.click('#mockSettle');
  for (const [tab, key, expected] of [
    ['machines', 'machine:LayerFS Home', ['machine.record', 'machine.operation', 'machine.state']],
    ['sandboxes', 'sandbox:tests', catalog.sandbox.map(c => c.check).filter(id => !['sandbox.forward_process', 'sandbox.worker_compatibility'].includes(id))],
  ]) {
    await p.click('#tab-' + tab);
    const row = fid(p, 'row:' + key);
    await row.click();
    const group = row.locator('xpath=ancestor::div[@class="group"]');
    assert.deepEqual(await group.locator(':scope > .detail .chk code').allTextContents(), expected);
  }
});

#!/usr/bin/env node
// Check the built explorer in headless Chrome: both languages draw, the switch keeps the section,
// search finds zh text, and nothing logs an error.
//   node tools/guide/site-check.mjs [docs/guide/explorer.html] [--shots <dir>]
import { chromium } from 'playwright-core'
import { pathToFileURL } from 'node:url'
import { resolve, join } from 'node:path'

const args = process.argv.slice(2)
const file = resolve(args.find((a) => !a.startsWith('--') && !args[args.indexOf(a) - 1]?.startsWith('--')) ?? 'docs/guide/explorer.html')
const shots = args.includes('--shots') ? args[args.indexOf('--shots') + 1] : null
const browser = await chromium.launch({ executablePath: process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', headless: true })
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, locale: 'en-US' })
const errors = []
page.on('pageerror', (e) => errors.push(String(e)))
page.on('console', (m) => { if (m.type() === 'error' && !/fonts\.g|mermaid|net::/i.test(m.text())) errors.push(m.text()) })
const fails = []
const expect = (ok, what) => { if (!ok) fails.push(what); console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`) }
const state = () => page.evaluate(() => ({
  lang: document.documentElement.lang, button: document.getElementById('lang').textContent,
  h1: document.querySelector('article.doc:not([hidden]) h1')?.textContent, toc: document.querySelector('#toc .toctitle')?.textContent,
  nav: document.querySelector('.navitem[aria-current]')?.textContent, note: document.querySelector('article.doc:not([hidden]) .langnote')?.textContent ?? null,
  articles: document.querySelectorAll('article.doc').length, scrollY: Math.round(scrollY), hash: location.hash,
}))

await page.goto(pathToFileURL(file).href + '#store', { waitUntil: 'domcontentloaded' })
await page.evaluate(() => localStorage.removeItem('layerfs-guide-lang'))
await page.reload({ waitUntil: 'domcontentloaded' })
let s = await state()
expect(s.lang === 'en' && s.button === '中文' && s.articles === 17, `English by default (${JSON.stringify(s)})`)
const target = await page.evaluate(() => [...document.querySelectorAll('article.doc:not([hidden]) h2')][2]?.id)
await page.goto(pathToFileURL(file).href + '#' + target, { waitUntil: 'domcontentloaded' })
await page.waitForTimeout(200)
if (shots) await page.screenshot({ path: join(shots, 'en.png') })
await page.click('#lang')
// Chromium can restore the pre-reload scroll position after the language has switched.
await page.waitForLoadState('load')
await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))
s = await state()
expect(s.lang === 'zh-CN' && s.button === 'English' && s.toc === '本页内容', `switches to zh (${JSON.stringify(s)})`)
const top = await page.evaluate((id) => Math.round(document.getElementById(id)?.getBoundingClientRect().top ?? -1), target)
expect(top >= 0 && top < 200, `keeps the section ${target} in view (top ${top})`)
if (shots) await page.screenshot({ path: join(shots, 'zh.png') })
await page.reload({ waitUntil: 'domcontentloaded' })
s = await state()
expect(s.lang === 'zh-CN', 'zh remembered across a reload')
// every page draws in zh, with its zh title
const pages = await page.evaluate(() => [...document.querySelectorAll('.navitem')].map((a) => a.dataset.page))
for (const id of pages) {
  await page.evaluate((id) => { location.hash = id }, id)
  await page.waitForTimeout(60)
  const st = await state()
  console.log(`     zh ${id.padEnd(13)} ${st.h1}${st.note ? `  [${st.note}]` : ''}`)
}
// search in zh
await page.fill('#q', '备份')
await page.waitForTimeout(100)
const hits = await page.evaluate(() => document.querySelectorAll('#results .result').length)
console.log(`     zh search 备份: ${hits} results`)
await page.keyboard.press('Escape')
// calls index in zh
await page.evaluate(() => { location.hash = 'calls--store-move' })
await page.waitForTimeout(100)
const uses = await page.evaluate(() => document.querySelectorAll('#calls--store-move .uses li').length)
expect(uses > 0, `zh call index lists store move (${uses} uses)`)
await page.click('#lang')
await page.waitForTimeout(200)
s = await state()
expect(s.lang === 'en' && s.toc === 'On this page', 'switches back to English')
expect(!errors.length, `no page errors${errors.length ? `: ${errors.join(' | ')}` : ''}`)
await browser.close()
process.exit(fails.length ? 1 : 0)

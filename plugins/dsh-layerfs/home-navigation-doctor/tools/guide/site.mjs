#!/usr/bin/env node
// Build the UI guide explorer: one HTML page that holds every docs/guide page,
// with grouped navigation, an outline, search, a screenshot viewer, and an index
// of every LayerFS call with the interactions that run it.
//   node tools/guide/site.mjs                      -> docs/guide/explorer.html (open it in a browser)
//   node tools/guide/site.mjs --fragment --out X   -> X without <html>/<head>/<body> (for an Artifact)
// Screenshots are referenced as img/<name>.png, next to the output file.
import { fileURLToPath } from 'node:url'
import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { createHash } from 'node:crypto'

const args = process.argv.slice(2)
const flag = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : fallback }
const dir = fileURLToPath(new URL('../../docs/guide/', import.meta.url))
const out = resolve(flag('--out', join(dir, 'explorer.html')))
const fragment = args.includes('--fragment')

/** The pages, in navigation order, grouped by where they live. */
const GROUPS = [
  { title: 'Start', pages: [['overview', 'README.md', 'Overview', 'What LayerFS adds, the map, how do I…']] },
  { title: 'In the DSH window', pages: [
    ['entry-points', 'entry-points.md', 'Entry points', 'Sidebar row, Workspaces switch, New Session, setup and project dialogs'],
    ['conversation', 'conversation.md', 'Conversation', 'A project session: Trajectory, Files, History, Sandbox, Stats'],
  ] },
  { title: 'Center pages', pages: [
    ['home', 'home.md', 'Home', 'Projects · Stores · Machines'],
    ['store', 'store.md', 'Store', 'Overview · Projects · Backups or Connections'],
    ['bucket', 'bucket.md', 'Bucket', 'Backup files and restores'],
    ['project', 'project.md', 'Project', 'Files · History · Sandboxes · Sessions · Stats'],
    ['machine', 'machine.md', 'Machine', 'Overview · Sandboxes · Stores · Images · Files · Worker log'],
    ['sandbox', 'sandbox.md', 'Sandbox', 'Overview · Workspaces · Files · Daemon log'],
  ] },
  { title: 'Center tools', pages: [
    ['doctor', 'health.md', 'Doctor', 'Checks and fixes'],
    ['logs', 'logs.md', 'Logs', 'Every LayerFS call the app made'],
    ['presets', 'presets.md', 'Presets', 'Demo libraries and resets'],
  ] },
  { title: 'Reference', pages: [
    ['components', 'components.md', 'Components', 'Parts shared by several pages'],
    ['calls', null, 'By LayerFS call', 'Every call and the interactions that run it'],
    ['cli-map', 'cli-map.md', 'CLI map', 'Every interaction → route → call'],
    ['cli-routes', 'cli-routes.md', 'Host routes', 'Generated from the host code'],
    ['contributing', 'CONTRIBUTING.md', 'Writing the guide', 'Template, screenshots, fixture'],
  ] },
]
const PAGES = GROUPS.flatMap((g) => g.pages.map(([id, file, title, blurb]) => ({ id, file, title, blurb, group: g.title })))

/**
 * Languages. English is docs/guide/*.md; Chinese is docs/guide/zh/*.md (docs/guide/zh/TRANSLATING.md
 * has the rules). A zh page gets the English section ids by position, so every link works in both.
 */
const LANGS = ['en', 'zh']
const I18N = {
  en: {
    label: 'English', groups: {}, pages: {},
    ui: {
      brand: 'LayerFS UI Guide', stats: '{p} pages · {s} screenshots', menu: 'Pages', search: 'Search pages, components, buttons, layerfs calls',
      searchLabel: 'Search the guide', onThisPage: 'On this page', prev: 'Previous', next: 'Next', close: 'Close', screenshot: 'Screenshot',
      prevShot: 'Previous screenshot', nextShot: 'Next screenshot', anchor: 'Link to this section', noresult: 'Nothing matches “{q}”.',
      switchTo: '中文', switchLabel: '切换到中文', nav: 'Guide pages',
      footer: 'Built from <code>docs/guide</code> by <code>tools/guide/site.mjs</code>.',
      kinds: { Page: 'Page', Section: 'Section', 'LayerFS call': 'LayerFS call', Row: 'Row' },
      read: 'read', interactions: (n) => `${n} interaction${n === 1 ? '' : 's'}`, callTitle: (c) => `Every interaction that runs ${c}`,
      callsLede: 'Every LayerFS call named in the guide\'s tables, grouped by family, with each interaction that runs it. <span class="tag read">read</span> marks calls a view makes to draw itself. Click an interaction to open its section.',
    },
  },
  zh: {
    label: '中文',
    groups: { Start: '开始', 'In the DSH window': 'DSH 窗口中', 'Center pages': 'Center 页面', 'Center tools': 'Center 工具', Reference: '参考' },
    pages: {
      overview: ['概览', 'LayerFS 带来了什么、全局地图、常见问题怎么做'],
      'entry-points': ['入口', '侧边栏行、Workspaces 切换、新建会话、设置和项目对话框'],
      conversation: ['对话', '项目会话：轨迹、文件、历史、Sandbox、统计'],
      home: ['主页', '项目 · Store · 机器'],
      store: ['Store', '概览 · 项目 · 备份或连接'],
      bucket: ['存储桶', '备份文件与恢复'],
      project: ['项目', '文件 · 历史 · Sandbox · 会话 · 统计'],
      machine: ['机器', '概览 · Sandbox · Store · 镜像 · 文件 · Worker 日志'],
      sandbox: ['Sandbox', '概览 · Workspace · 文件 · Daemon 日志'],
      doctor: ['诊断', '检查与修复'],
      logs: ['日志', '应用发出的每一次 LayerFS 调用'],
      presets: ['预设', '演示库与重置'],
      components: ['组件', '多个页面共用的部件'],
      calls: ['按 LayerFS 调用', '每个调用，以及触发它的操作'],
      'cli-map': ['CLI 对照表', '每个操作 → 路由 → 调用'],
      'cli-routes': ['主机路由', '由主机代码生成（英文）'],
      contributing: ['编写指南', '模板、截图、测试数据'],
    },
    ui: {
      brand: 'LayerFS 界面指南', stats: '{p} 页 · {s} 张截图', menu: '页面', search: '搜索页面、组件、按钮、layerfs 调用',
      searchLabel: '搜索指南', onThisPage: '本页内容', prev: '上一页', next: '下一页', close: '关闭', screenshot: '截图',
      prevShot: '上一张截图', nextShot: '下一张截图', anchor: '链接到此章节', noresult: '没有与“{q}”匹配的内容。',
      switchTo: 'English', switchLabel: 'Switch to English', nav: '指南页面',
      footer: '由 <code>tools/guide/site.mjs</code> 从 <code>docs/guide</code> 生成。',
      kinds: { Page: '页面', Section: '章节', 'LayerFS call': 'LayerFS 调用', Row: '行' },
      read: '读取', interactions: (n) => `${n} 个操作`, callTitle: (c) => `触发 ${c} 的所有操作`,
      callsLede: '指南各表格里出现的每个 LayerFS 调用，按类别分组，并列出触发它的每个操作。<span class="tag read">读取</span> 表示视图为了绘制自身而发出的调用。点击一个操作即可打开它所在的章节。',
      untranslated: '这一页还没有中文版，下面是英文原文。',
      englishOnly: '这一页由主机代码生成，只有英文版。',
      stale: '英文原文在翻译之后有过改动，这一页的部分内容可能已经过时。',
    },
  },
}
const names = (lang, page) => I18N[lang].pages[page.id] ?? [page.title, page.blurb]
const groupName = (lang, title) => I18N[lang].groups[title] ?? title
const sha12 = (file) => createHash('sha256').update(readFileSync(file)).digest('hex').slice(0, 12)
const BY_FILE = new Map(PAGES.filter((p) => p.file).map((p) => [p.file, p]))
const FAMILIES = ['home', 'machine', 'store', 'backup', 'project', 'branch', 'commit', 'file', 'sandbox', 'workspace']

// ------------------------------------------------------------------ markdown (the subset the guide uses)

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

/** GitHub-style slug, the same as tools/guide/check.mjs. */
function slugify(text) {
  return text.trim().toLowerCase().replace(/`/g, '').replace(/<[^>]+>/g, '').replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-')
}

/** Plain text of inline markdown (for search, outlines and slugs). */
function plain(md) {
  return md.replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*]/g, '').replace(/\\\|/g, '|').trim()
}

/** A guide link -> the explorer's hash (or null when it leads outside the guide). */
function route(href, page) {
  if (/^(https?:|mailto:)/.test(href)) return { external: href }
  const [raw, hash] = href.split('#')
  const path = raw.replace(/^\.\.\//, '') // a zh page reaches English-only files and img/ one folder up
  if (/^img\/.+\.png$/.test(path)) return { image: path }
  const target = path ? BY_FILE.get(path) : page
  if (!target) return null
  return { hash: hash ? `${target.id}--${hash}` : target.id }
}

function inline(md, ctx) {
  const codes = []
  let s = md.replace(/\\\|/g, '\u0001')
  s = s.replace(/`([^`]+)`/g, (_, c) => { codes.push(c); return `\u0000${codes.length - 1}\u0000` })
  s = esc(s)
  s = s.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_, alt, src) => figure(alt, src, ctx))
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, text, href) => {
    const r = route(href.replace(/&amp;/g, '&'), ctx.page)
    if (!r) return `<span class="xref" title="${esc(href)}">${text}</span>`
    if (r.external) return `<a href="${esc(r.external)}" target="_blank" rel="noopener">${text}</a>`
    if (r.image) return `<a href="#" class="shot-link" data-src="${esc(r.image)}" data-alt="${text.replace(/<[^>]+>/g, '')}">${text}</a>`
    return `<a href="#${r.hash}">${text}</a>`
  })
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>').replace(/(^|[^*\w])\*([^*\s][^*]*?)\*(?!\w)/g, '$1<em>$2</em>')
  s = s.replace(/\u0000(\d+)\u0000/g, (_, i) => {
    const c = codes[Number(i)].replace(/\u0001/g, '|')
    const call = ctx.callCell ? callOf(c) : null
    return call ? `<a class="call" href="#calls--${call.replace(' ', '-')}" title="${esc(ctx.ui.callTitle(call))}"><code>${esc(c)}</code></a>` : `<code>${esc(c)}</code>`
  })
  return s.replace(/\u0001/g, '|')
}

/** `store move <s> <m>` -> 'store move'; anything else -> null. */
function callOf(code) {
  const m = /^(?:layerfs\s+)?([a-z]+)\s+([a-z][a-z_-]*)/.exec(code.trim())
  return m && FAMILIES.includes(m[1]) ? `${m[1]} ${m[2].replace(/_/g, '-')}` : null
}

function figure(alt, href, ctx) {
  const src = href.replace(/^\.\.\//, '')
  if (!existsSync(join(dir, src))) return `<span class="missing">${alt}</span>`
  ctx.images.push(src)
  // Reserve the screenshot's space so lazy loading cannot move an anchored section.
  const png = readFileSync(join(dir, src))
  const width = png.readUInt32BE(16)
  const height = png.readUInt32BE(20)
  return `<button type="button" class="shot" data-src="${esc(src)}" data-alt="${alt}"><img src="${esc(src)}" alt="${alt}" width="${width}" height="${height}" loading="lazy"></button>`
}

function splitRow(line) {
  const cells = []
  let cur = '', tick = false
  const body = line.trim().replace(/^\|/, '').replace(/\|$/, '')
  for (let i = 0; i < body.length; i++) {
    const ch = body[i]
    if (ch === '\\' && body[i + 1] === '|') { cur += '\\|'; i++; continue }
    if (ch === '`') tick = !tick
    if (ch === '|' && !tick) { cells.push(cur.trim()); cur = ''; continue }
    cur += ch
  }
  cells.push(cur.trim())
  return cells
}

/** Markdown -> HTML for one page; collects headings, images, search entries and calls. */
function render(md, ctx) {
  const lines = md.replace(/<!--[\s\S]*?-->/g, '').split('\n')
  return blocks(lines, ctx)
}

function blocks(lines, ctx) {
  const html = []
  let i = 0
  const para = []
  const flush = () => { if (para.length) { html.push(`<p>${inline(para.join(' '), ctx)}</p>`); para.length = 0 } }
  while (i < lines.length) {
    const line = lines[i]
    const fence = /^(\s*)```(\w*)\s*$/.exec(line)
    if (fence) {
      flush()
      const lang = fence[2]
      const body = []
      i++
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i])) body.push(lines[i++].slice(fence[1].length))
      i++
      if (lang === 'mermaid') html.push(`<div class="diagram"><pre class="mermaid">${esc(body.join('\n'))}</pre></div>`)
      else html.push(`<pre class="code"${lang ? ` data-lang="${lang}"` : ''}><code>${esc(body.join('\n'))}</code></pre>`)
      continue
    }
    const h = /^(#{1,6})\s+(.*)$/.exec(line)
    if (h) {
      flush()
      const level = h[1].length
      const slug = ctx.slug(h[2])
      const id = `${ctx.page.id}--${slug}`
      if (level === 1) { ctx.title = plain(h[2]); i++; continue }
      if (level <= 3) ctx.headings.push({ level, id, text: plain(h[2]) })
      ctx.section = { id, text: plain(h[2]) }
      ctx.search.push({ kind: 'Section', text: plain(h[2]), hash: id, page: ctx.page.id })
      html.push(`<h${level} id="${id}"><a class="anchor" href="#${id}" aria-label="${ctx.ui.anchor}">#</a>${inline(h[2], ctx)}</h${level}>`)
      i++
      continue
    }
    if (/^\|.*\|\s*$/.test(line) && /^\|[\s:|-]+\|\s*$/.test(lines[i + 1] ?? '')) {
      flush()
      const head = splitRow(line)
      i += 2
      const rows = []
      while (i < lines.length && /^\|/.test(lines[i])) rows.push(splitRow(lines[i++]))
      html.push(table(head, rows, ctx))
      continue
    }
    if (/^>\s?/.test(line)) {
      flush()
      const body = []
      while (i < lines.length && /^>\s?/.test(lines[i])) body.push(lines[i++].replace(/^>\s?/, ''))
      html.push(`<div class="where">${blocks(body, ctx)}</div>`)
      continue
    }
    const li = /^(\s*)([-*]|\d+\.)\s+(.*)$/.exec(line)
    if (li && li[1].length === 0) {
      flush()
      const ordered = /\d/.test(li[2])
      const items = []
      while (i < lines.length) {
        const m = /^([-*]|\d+\.)\s+(.*)$/.exec(lines[i])
        if (!m || /\d/.test(m[1]) !== ordered) break
        const item = [m[2]]
        i++
        while (i < lines.length && (/^\s{2,}\S/.test(lines[i]) || (lines[i].trim() === '' && /^\s{2,}\S/.test(lines[i + 1] ?? '')))) {
          item.push(lines[i].replace(/^ {2,3}/, ''))
          i++
        }
        items.push(item)
      }
      const tag = ordered ? 'ol' : 'ul'
      html.push(`<${tag}>${items.map((item) => {
        const nested = item.length > 1 && item.slice(1).some((l) => /^\s*([-*]|\d+\.)\s|^\s*```/.test(l))
        if (!nested) return `<li>${inline(item.join(' ').replace(/\s+/g, ' '), ctx)}</li>`
        const first = []
        let k = 0
        while (k < item.length && !(k > 0 && /^\s*([-*]|\d+\.)\s|^\s*```/.test(item[k]))) first.push(item[k++])
        return `<li>${inline(first.join(' '), ctx)}${blocks(item.slice(k), ctx)}</li>`
      }).join('')}</${tag}>`)
      continue
    }
    if (line.trim() === '') { flush(); i++; continue }
    para.push(line.trim())
    i++
  }
  flush()
  return html.join('\n')
}

function table(head, rows, ctx) {
  const callCol = head.findIndex((c) => /LayerFS (call|调用)/i.test(c))
  const nameCol = head[0] === '#' ? 1 : 0
  const body = rows.map((r) => {
    const cells = head.map((_, k) => {
      const cell = r[k] ?? ''
      return `<td${k === callCol ? ' class="callcell"' : ''}>${inline(cell, { ...ctx, callCell: k === callCol })}</td>`
    }).join('')
    // a key table's row may start with its callout number ("4 **Create and open**")
    const name = plain(r[nameCol] ?? '').replace(/^\d+(?:\s*[,–-]\s*\d+)*\s+(?=\D)/, '')
    if (name && ctx.section) {
      ctx.search.push({ kind: head[nameCol] || 'Row', text: name, detail: plain(r.slice(nameCol + 1).join(' · ')).slice(0, 160), hash: ctx.section.id, page: ctx.page.id })
    }
    if (callCol >= 0 && ctx.section) {
      for (const m of (r[callCol] ?? '').matchAll(/`([^`]+)`/g)) {
        const call = callOf(m[1])
        if (!call) continue
        const where = head.findIndex((c) => /^(Where|位置)/i.test(c))
        ctx.calls.push({ call, read: /[(（]\s*(read|读取)/.test(r[callCol]), what: name, where: where >= 0 ? plain(r[where] ?? '') : '', hash: ctx.section.id, section: ctx.section.text, page: ctx.page.id })
      }
    }
    return `<tr>${cells}</tr>`
  }).join('\n')
  const wide = head.length >= 5 ? ' wide' : ''
  return `<div class="table${wide}"><table><thead><tr>${head.map((c, k) => `<th${k === callCol ? ' class="callcell"' : ''}>${inline(c, ctx)}</th>`).join('')}</tr></thead><tbody>${body}</tbody></table></div>`
}

// ------------------------------------------------------------------ build every page, once per language

/** One language's pages, search entries and call index. `enIds` gives a translation the English section ids. */
function build(lang, enIds) {
  const ui = I18N[lang].ui
  const search = [], calls = [], rendered = [], ids = new Map()
  for (const page of PAGES) {
    if (!page.file) continue
    const [ptitle, pblurb] = names(lang, page)
    const en = join(dir, page.file)
    let file = en, note = ''
    if (lang !== 'en') {
      const own = join(dir, lang, page.file)
      if (page.file === 'cli-routes.md') note = ui.englishOnly
      else if (!existsSync(own)) note = ui.untranslated
      else {
        file = own
        const m = /<!--\s*source:\s*([0-9a-f]+)\s*-->/.exec(readFileSync(own, 'utf8'))
        if (!m || m[1] !== sha12(en)) note = ui.stale
      }
    }
    const given = file !== en ? enIds.get(page.id) ?? [] : []
    const mine = []
    const seen = new Map()
    const ctx = {
      page, lang, ui, headings: [], images: [], search, calls, section: { id: page.id, text: ptitle }, title: ptitle,
      slug: (t) => {
        let s = given[mine.length]
        if (!s) { s = slugify(t); const n = seen.get(s) ?? 0; seen.set(s, n + 1); if (n) s = `${s}-${n}` }
        mine.push(s)
        return s
      },
    }
    const html = render(readFileSync(file, 'utf8'), ctx)
    if (given.length && given.length !== mine.length) console.warn(`${lang}/${page.file}: ${mine.length} headings, English has ${given.length}; section links may point to the wrong place`)
    ids.set(page.id, mine)
    rendered.push({ page, html: (note ? `<p class="langnote">${note}</p>\n` : '') + html, headings: ctx.headings, images: ctx.images.length })
    search.push({ kind: 'Page', text: ptitle, detail: pblurb, hash: page.id, page: page.id })
  }

  // the "By LayerFS call" page, from every table's LayerFS call column
  const byCall = new Map()
  for (const c of calls) {
    if (!byCall.has(c.call)) byCall.set(c.call, [])
    const list = byCall.get(c.call)
    if (!list.some((x) => x.what === c.what && x.hash === c.hash)) list.push(c)
  }
  const title = (id) => names(lang, PAGES.find((p) => p.id === id) ?? { id, title: id })[0]
  const callsPage = PAGES.find((p) => p.id === 'calls')
  const callHeadings = []
  const callHtml = [`<p class="lede">${ui.callsLede}</p>`]
  const readTag = `<span class="tag read">${ui.read}</span>`
  callHtml.push(`<nav class="families">${FAMILIES.filter((f) => [...byCall.keys()].some((k) => k.startsWith(`${f} `))).map((f) => `<a href="#calls--${f}">${f}</a>`).join('')}</nav>`)
  for (const family of FAMILIES) {
    const keys = [...byCall.keys()].filter((k) => k.startsWith(`${family} `)).sort()
    if (!keys.length) continue
    callHeadings.push({ level: 2, id: `calls--${family}`, text: family })
    callHtml.push(`<h2 id="calls--${family}"><a class="anchor" href="#calls--${family}">#</a><code>${family}</code></h2>`)
    for (const key of keys) {
      const id = `calls--${key.replace(' ', '-')}`
      const rows = byCall.get(key)
      const reads = rows.every((r) => r.read)
      search.push({ kind: 'LayerFS call', text: `layerfs ${key}`, detail: ui.interactions(rows.length), hash: id, page: 'calls' })
      callHtml.push(`<section class="callgroup" id="${id}"><h3><code>layerfs ${esc(key)}</code>${reads ? ` ${readTag}` : ''}<span class="count">${rows.length}</span></h3><ul class="uses">${rows.map((r) => `<li><a href="#${r.hash}"><span class="what">${esc(r.what || r.section)}</span><span class="in">${esc(title(r.page))} › ${esc(r.section)}${r.where ? ` · ${esc(r.where)}` : ''}</span></a>${r.read && !reads ? readTag : ''}</li>`).join('')}</ul></section>`)
    }
  }
  rendered.splice(PAGES.findIndex((p) => p.id === 'calls'), 0, { page: callsPage, html: callHtml.join('\n'), headings: callHeadings, images: 0 })
  search.push({ kind: 'Page', text: title('calls'), detail: names(lang, callsPage)[1], hash: 'calls', page: 'calls' })

  const nav = GROUPS.map((g) => `<div class="navgroup"><div class="navtitle">${groupName(lang, g.title)}</div>${g.pages.map(([id]) => { const [t, blurb] = names(lang, PAGES.find((p) => p.id === id)); return `<a class="navitem" href="#${id}" data-page="${id}"><span class="navname">${t}</span><span class="navblurb">${blurb}</span></a>` }).join('')}</div>`).join('') + `<div class="navfoot">${ui.footer}</div>`
  const pager = (id) => {
    const i = PAGES.findIndex((p) => p.id === id)
    const prev = PAGES[i - 1], next = PAGES[i + 1]
    return `<nav class="pager">${prev ? `<a href="#${prev.id}" class="prev"><span>${ui.prev}</span>${title(prev.id)}</a>` : '<span></span>'}${next ? `<a href="#${next.id}" class="next"><span>${ui.next}</span>${title(next.id)}</a>` : ''}</nav>`
  }
  const main = rendered.map((r) => `<article class="doc" id="page-${r.page.id}" data-page="${r.page.id}" hidden>
<header class="dochead"><div class="eyebrow">${groupName(lang, r.page.group)}</div><h1>${esc(title(r.page.id))}</h1></header>
${r.html}
${pager(r.page.id)}
</article>`).join('\n')
  const shots = rendered.reduce((n, r) => n + r.images, 0)
  const { interactions, callTitle, ...strings } = ui
  return {
    ids, nav, main, shots, calls: byCall.size,
    data: {
      outlines: Object.fromEntries(rendered.map((r) => [r.page.id, r.headings])), search,
      pages: PAGES.map((p) => ({ id: p.id, title: title(p.id) })),
      ui: { ...strings, stats: ui.stats.replace('{p}', PAGES.length).replace('{s}', shots) },
    },
  }
}

const built = {}
for (const lang of LANGS) built[lang] = build(lang, built.en?.ids)
const shots = built.en.shots
const data = JSON.stringify(Object.fromEntries(LANGS.map((l) => [l, built[l].data]))).replace(/</g, '\\u003c')
const templates = LANGS.map((l) => `<template id="nav-${l}">${built[l].nav}</template>\n<template id="main-${l}">${built[l].main}</template>`).join('\n')


const body = `<title>LayerFS UI Guide</title>
<meta name="claude:viewport-fit-sides" content="cover">
<script>
// The router scrolls to the translated fragment. A late reload restore would overwrite it.
history.scrollRestoration = 'manual'
</script>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600&family=IBM+Plex+Mono:wght@400;500&family=Space+Grotesk:wght@500;600&display=swap">
<style>
/* Layout: a docs shell. Left, the pages grouped by where they live in the app; middle, the page; right, its outline. */
:root {
  --bg: #fbfbfd; --surface: #ffffff; --sunken: #f2f3f7; --fg: #1b1e27; --muted: #5d6474; --faint: #8a90a0;
  --line: #e2e4ec; --line-strong: #cfd3de; --accent: #3550d8; --accent-soft: #e9edff; --accent-fg: #ffffff;
  --call: #0d6e5a; --call-soft: #e3f4ef; --read: #6b5a00; --read-soft: #f6efcc; --mark: #e5484d;
  --shadow: 0 1px 2px rgb(20 24 40 / 6%), 0 8px 24px rgb(20 24 40 / 8%);
  --f-cjk: "PingFang SC", "Hiragino Sans GB", "Noto Sans SC", "Microsoft YaHei";
  --f-display: "Space Grotesk", "IBM Plex Sans", var(--f-cjk), system-ui, sans-serif;
  --f-body: "IBM Plex Sans", var(--f-cjk), system-ui, -apple-system, "Segoe UI", sans-serif;
  --f-mono: "IBM Plex Mono", ui-monospace, "SF Mono", Menlo, monospace;
  --nav-w: 272px; --toc-w: 232px; --top-h: 56px;
}
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) {
  --bg: #12141a; --surface: #181b23; --sunken: #1f232d; --fg: #e6e8ef; --muted: #a0a6b5; --faint: #747b8c;
  --line: #2a2f3b; --line-strong: #3a4050; --accent: #8ea0ff; --accent-soft: #232a4d; --accent-fg: #10131c;
  --call: #6fd6bb; --call-soft: #16302b; --read: #e6cf6a; --read-soft: #34301a; --mark: #ff6b6f;
  --shadow: 0 1px 2px rgb(0 0 0 / 30%), 0 8px 24px rgb(0 0 0 / 35%); color-scheme: dark } }
:root[data-theme="dark"] {
  --bg: #12141a; --surface: #181b23; --sunken: #1f232d; --fg: #e6e8ef; --muted: #a0a6b5; --faint: #747b8c;
  --line: #2a2f3b; --line-strong: #3a4050; --accent: #8ea0ff; --accent-soft: #232a4d; --accent-fg: #10131c;
  --call: #6fd6bb; --call-soft: #16302b; --read: #e6cf6a; --read-soft: #34301a; --mark: #ff6b6f;
  --shadow: 0 1px 2px rgb(0 0 0 / 30%), 0 8px 24px rgb(0 0 0 / 35%); color-scheme: dark }
* { box-sizing: border-box }
html, body { background: var(--bg); color: var(--fg) }
body { font: 15px/1.6 var(--f-body); margin: 0; -webkit-font-smoothing: antialiased }
body::before { content: ""; position: fixed; inset: 0 0 auto; height: env(safe-area-inset-top, 0px); z-index: 30; background: var(--surface) }
a { color: var(--accent); text-decoration: none }
a:hover { text-decoration: underline }
:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 4px }
code { font: 0.86em/1.4 var(--f-mono); background: var(--sunken); border: 0.5px solid var(--line); border-radius: 4px; padding: 0.05em 0.35em; overflow-wrap: anywhere }

.top { position: sticky; top: env(safe-area-inset-top, 0px); z-index: 20; height: var(--top-h); display: flex; align-items: center; gap: 16px;
  padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px));
  background: var(--surface); border-bottom: 0.5px solid var(--line) }
.brand { display: flex; align-items: center; gap: 10px; font: 600 16px/1 var(--f-display); color: var(--fg); white-space: nowrap }
.brand:hover { text-decoration: none }
.brand svg { flex: none }
.brand small { font: 500 12px/1 var(--f-body); color: var(--muted); letter-spacing: 0.02em }
.lang { flex: none; border: 0.5px solid var(--line-strong); background: var(--surface); color: var(--fg); border-radius: 8px; height: 34px; padding: 0 12px; font: 500 13px var(--f-body); cursor: pointer }
@media (hover: hover) { .lang:hover { border-color: var(--accent); color: var(--accent) } }
.langnote { max-width: 72ch; background: var(--read-soft); color: var(--read); border-radius: 10px; padding: 8px 14px; font-size: 14px }
.menu { display: none; border: 0.5px solid var(--line-strong); background: var(--surface); color: var(--fg); border-radius: 8px; height: 34px; padding: 0 12px; font: 500 13px var(--f-body); cursor: pointer }
.searchbox { margin-left: auto; position: relative; width: min(440px, 100%) }
.searchbox input { width: 100%; height: 36px; border-radius: 9px; border: 0.5px solid var(--line-strong); background: var(--sunken); color: var(--fg); font: 14px var(--f-body); padding: 0 64px 0 34px }
.searchbox input:focus { outline: none; border-color: var(--accent); background: var(--surface) }
.searchbox .glass { position: absolute; left: 11px; top: 10px; color: var(--faint) }
.searchbox kbd { position: absolute; right: 10px; top: 8px; font: 500 11px/18px var(--f-mono); color: var(--faint); border: 0.5px solid var(--line-strong); border-radius: 5px; padding: 0 5px; background: var(--surface) }
.results { position: absolute; top: 42px; right: 0; width: min(560px, calc(100vw - 32px)); max-height: min(70vh, 560px); overflow: auto; background: var(--surface); border: 0.5px solid var(--line-strong); border-radius: 12px; box-shadow: var(--shadow); padding: 6px }
.result { display: grid; grid-template-columns: 92px 1fr; gap: 2px 12px; padding: 8px 10px; border-radius: 8px; color: var(--fg); cursor: pointer }
.result:hover, .result[aria-selected="true"] { background: var(--accent-soft); text-decoration: none }
.result .kind { grid-row: span 2; font: 500 11px/20px var(--f-body); color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em }
.result .rt { font-weight: 500; overflow-wrap: anywhere }
.result .rd { font-size: 12.5px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap }
.result mark { background: transparent; color: var(--accent); font-weight: 600 }
.noresult { padding: 14px; color: var(--muted) }

.shell { display: grid; grid-template-columns: var(--nav-w) minmax(0, 1fr) var(--toc-w); align-items: start }
.nav { position: sticky; top: calc(var(--top-h) + env(safe-area-inset-top, 0px)); height: calc(100vh - var(--top-h) - env(safe-area-inset-top, 0px)); overflow: auto; padding: 20px 12px 40px max(16px, env(safe-area-inset-left, 0px)); border-right: 0.5px solid var(--line); display: flex; flex-direction: column; gap: 18px }
.navtitle { font: 600 11px/1 var(--f-body); text-transform: uppercase; letter-spacing: 0.07em; color: var(--faint); padding: 0 10px 8px }
.navitem { display: flex; flex-direction: column; gap: 1px; padding: 7px 10px; border-radius: 8px; color: var(--fg) }
.navitem:hover { background: var(--sunken); text-decoration: none }
.navitem[aria-current="page"] { background: var(--accent-soft) }
.navitem[aria-current="page"] .navname { color: var(--accent) }
.navname { font-weight: 500; font-size: 14px }
.navblurb { font-size: 12px; line-height: 1.35; color: var(--muted) }
.navfoot { margin-top: auto; padding: 0 10px; font-size: 12px; color: var(--faint) }

main { min-width: 0; padding-block: 28px 80px; padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px)) }
.doc { max-width: 1060px; margin: 0 auto; padding: 0 16px }
.doc > p, .doc > ul, .doc > ol, .doc > .where, .doc > h2, .doc > h3, .doc > h4, .doc .lede { max-width: 72ch }
.dochead { margin-bottom: 18px }
.eyebrow { font: 600 11px/1 var(--f-body); text-transform: uppercase; letter-spacing: 0.08em; color: var(--accent); margin-bottom: 10px }
h1 { font: 600 34px/1.15 var(--f-display); margin: 0; letter-spacing: -0.01em; text-wrap: balance }
h2 { font: 600 22px/1.25 var(--f-display); margin: 44px 0 12px; padding-top: 18px; border-top: 0.5px solid var(--line); text-wrap: balance; scroll-margin-top: calc(var(--top-h) + 16px) }
h3 { font: 600 17px/1.3 var(--f-display); margin: 28px 0 8px; scroll-margin-top: calc(var(--top-h) + 16px) }
h4 { font: 600 15px/1.3 var(--f-body); margin: 20px 0 6px }
h2, h3, h4 { position: relative }
.anchor { position: absolute; left: -1.1em; color: var(--faint); opacity: 0; font-weight: 400 }
h2:hover .anchor, h3:hover .anchor { opacity: 1; text-decoration: none }
p { margin: 10px 0 }
ul, ol { padding-left: 1.4em; margin: 10px 0 }
li { margin: 4px 0 }
li > ul, li > ol, li > pre { margin: 6px 0 }
.where { background: var(--surface); border: 0.5px solid var(--line); border-radius: 10px; padding: 4px 16px; color: var(--muted); font-size: 14px }
.where strong { color: var(--fg) }
.lede { color: var(--muted) }
.xref { border-bottom: 1px dotted var(--faint) }

pre.code { background: var(--sunken); border: 0.5px solid var(--line); border-radius: 10px; padding: 12px 14px; overflow-x: auto; font: 13px/1.55 var(--f-mono) }
pre.code code { background: none; border: 0; padding: 0; font-size: inherit }
.diagram { background: var(--surface); border: 0.5px solid var(--line); border-radius: 12px; padding: 16px; overflow-x: auto; margin: 14px 0 }
.diagram pre.mermaid { margin: 0; background: none; font: 12px var(--f-mono); color: var(--muted); white-space: pre }

.shot { display: block; width: 100%; padding: 0; margin: 16px 0; border: 0.5px solid var(--line-strong); border-radius: 12px; overflow: hidden; background: var(--surface); box-shadow: var(--shadow); cursor: zoom-in }
.shot img { display: block; width: 100%; height: auto }
td .shot, li .shot { margin: 6px 0 }
p > .shot { margin: 16px 0 }
.shot-link { border-bottom: 1px dashed currentColor }
.shot-link:hover { text-decoration: none }

.table { overflow-x: auto; margin: 14px 0 20px; border: 0.5px solid var(--line); border-radius: 12px; background: var(--surface) }
table { border-collapse: collapse; width: 100%; font-size: 13.5px; line-height: 1.5 }
.table.wide table { min-width: 860px }
th { position: sticky; top: 0; background: var(--sunken); text-align: left; font: 600 12px/1.3 var(--f-body); color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; padding: 9px 12px; border-bottom: 0.5px solid var(--line-strong) }
td { padding: 9px 12px; border-top: 0.5px solid var(--line); vertical-align: top }
tr:first-child td { border-top: 0 }
td:first-child { font-weight: 500 }
th.callcell { color: var(--call) }
td.callcell { background: color-mix(in srgb, var(--call-soft) 45%, transparent) }
a.call code { color: var(--call); background: var(--call-soft); border-color: transparent }
a.call:hover { text-decoration: none }
a.call:hover code { outline: 1px solid var(--call) }
td code { font-size: 0.84em }

.pager { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin-top: 56px }
.pager a { display: flex; flex-direction: column; gap: 2px; padding: 12px 16px; border: 0.5px solid var(--line-strong); border-radius: 10px; color: var(--fg); font-weight: 500; background: var(--surface) }
.pager a:hover { border-color: var(--accent); text-decoration: none }
.pager a span { font-size: 12px; color: var(--muted); font-weight: 400 }
.pager .next { text-align: right; grid-column: 2 }

.toc { position: sticky; top: calc(var(--top-h) + env(safe-area-inset-top, 0px)); max-height: calc(100vh - var(--top-h) - env(safe-area-inset-top, 0px)); overflow: auto; padding: 28px max(16px, env(safe-area-inset-right, 0px)) 40px 16px; font-size: 13px }
.toc .toctitle { font: 600 11px/1 var(--f-body); text-transform: uppercase; letter-spacing: 0.07em; color: var(--faint); margin-bottom: 10px }
.toc a { display: block; padding: 4px 0 4px 10px; border-left: 2px solid var(--line); color: var(--muted); line-height: 1.35 }
.toc a.l3 { padding-left: 22px; font-size: 12.5px }
.toc a:hover { color: var(--fg); text-decoration: none }
.toc a.on { color: var(--accent); border-left-color: var(--accent) }

.families { display: flex; flex-wrap: wrap; gap: 8px; margin: 16px 0 8px }
.families a { font: 500 13px var(--f-mono); padding: 4px 10px; border-radius: 999px; background: var(--call-soft); color: var(--call) }
.callgroup { border: 0.5px solid var(--line); border-radius: 12px; background: var(--surface); margin: 12px 0; scroll-margin-top: calc(var(--top-h) + 16px) }
.callgroup:target { border-color: var(--call); box-shadow: 0 0 0 3px var(--call-soft) }
.callgroup h3 { display: flex; align-items: center; gap: 8px; margin: 0; padding: 10px 14px; border-bottom: 0.5px solid var(--line); font-size: 15px }
.callgroup h3 code { color: var(--call); background: none; border: 0; font-size: 14px; padding: 0 }
.callgroup .count { margin-left: auto; font: 500 12px var(--f-body); color: var(--muted); font-variant-numeric: tabular-nums }
.uses { list-style: none; margin: 0; padding: 4px 0 }
.uses li { display: flex; align-items: baseline; gap: 8px; margin: 0; padding: 0 14px }
.uses li a { flex: 1; display: flex; flex-wrap: wrap; gap: 2px 10px; padding: 6px 0; color: var(--fg); min-width: 0 }
.uses li + li { border-top: 0.5px solid var(--line) }
.uses .what { font-weight: 500 }
.uses .in { color: var(--muted); font-size: 12.5px }
.tag { font: 600 10.5px/1 var(--f-body); text-transform: uppercase; letter-spacing: 0.05em; padding: 3px 6px; border-radius: 5px; white-space: nowrap }
.tag.read { background: var(--read-soft); color: var(--read) }

.viewer { position: fixed; inset: 0; z-index: 50; background: rgb(10 12 18 / 94%); display: grid; grid-template-rows: auto 1fr; padding: max(12px, env(safe-area-inset-top, 0px)) max(12px, env(safe-area-inset-right, 0px)) max(12px, env(safe-area-inset-bottom, 0px)) max(12px, env(safe-area-inset-left, 0px)) }
.viewer .vbar { display: flex; align-items: center; gap: 10px; color: #f1f2f6; padding: 4px 4px 10px; font-size: 14px }
.viewer .vcap { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap }
.viewer button { background: rgb(255 255 255 / 12%); color: #f1f2f6; border: 0; border-radius: 8px; height: 32px; min-width: 32px; padding: 0 10px; font: 500 13px var(--f-body); cursor: pointer }
.viewer button:hover { background: rgb(255 255 255 / 22%) }
.viewer .vimg { overflow: auto; display: grid; place-items: center; min-height: 0 }
.viewer img { max-width: 100%; max-height: 100%; border-radius: 8px; background: #fff; cursor: zoom-in }
.viewer.full img { max-width: none; max-height: none; width: 2880px; cursor: zoom-out }

@media (max-width: 1240px) { .shell { grid-template-columns: var(--nav-w) minmax(0, 1fr) } .toc { display: none } }
@media (max-width: 900px) {
  .shell { grid-template-columns: minmax(0, 1fr) }
  .menu { display: inline-block }
  .nav { position: fixed; z-index: 40; top: calc(var(--top-h) + env(safe-area-inset-top, 0px)); left: 0; bottom: 0; height: auto; width: min(320px, 88vw); background: var(--surface); box-shadow: var(--shadow); transform: translateX(-105%); transition: transform 0.2s ease }
  body.navopen .nav { transform: none }
  .brand small { display: none }
  .doc { padding: 0 }
  h1 { font-size: 28px }
  .anchor { display: none }
}
@media (max-width: 520px) { .searchbox kbd { display: none } .searchbox input { padding-right: 12px } .brand span { display: none } }
@media (prefers-reduced-motion: reduce) { * { transition: none !important; scroll-behavior: auto !important } }
</style>

<header class="top">
  <button type="button" class="menu" id="menu" aria-expanded="false" aria-controls="nav" data-t="menu">Pages</button>
  <a class="brand" href="#overview">
    <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3 3 20h4.2L12 10.8 16.8 20H21L12 3Z" fill="var(--accent)"/><path d="M8.6 20 12 13.6 15.4 20H8.6Z" fill="var(--call)"/></svg>
    <span data-t="brand">LayerFS UI Guide</span><small data-t="stats"></small>
  </a>
  <div class="searchbox" role="search">
    <svg class="glass" width="15" height="15" viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" fill="none" stroke="currentColor" stroke-width="2"/><path d="m20 20-4-4" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
    <input id="q" type="search" autocomplete="off" aria-controls="results">
    <kbd>⌘K</kbd>
    <div class="results" id="results" role="listbox" hidden></div>
  </div>
  <button type="button" class="lang" id="lang" lang=""></button>
</header>
<div class="shell">
  <nav class="nav" id="nav"></nav>
  <main id="main"></main>
  <aside class="toc" id="toc" aria-label="On this page"></aside>
</div>
<div class="viewer" id="viewer" hidden role="dialog" aria-modal="true">
  <div class="vbar"><span class="vcap" id="vcap"></span><span id="vpos"></span><button type="button" id="vprev">←</button><button type="button" id="vnext">→</button><button type="button" id="vclose" data-t="close">Close</button></div>
  <div class="vimg" id="vimg"><img id="vpic" alt=""></div>
</div>
${templates}
<script id="guide-data" type="application/json">${data}</script>
<script>
(() => {
  const all = JSON.parse(document.getElementById('guide-data').textContent)
  const KEY = 'layerfs-guide-lang'
  let lang = (() => {
    try { const l = localStorage.getItem(KEY); if (all[l]) return l } catch (e) {}
    return (navigator.languages || [navigator.language || '']).some((l) => /^zh/i.test(l)) ? 'zh' : 'en'
  })()
  let data = all[lang]
  const pageIds = all.en.pages.map((p) => p.id).sort((a, b) => b.length - a.length)
  const $ = (s, r = document) => r.querySelector(s)
  const $$ = (s, r = document) => [...r.querySelectorAll(s)]
  let current = null

  // ---- routing: #page or #page--section
  function parse(hash) {
    const h = decodeURIComponent((hash || '').replace(/^#/, ''))
    if (!h) return { page: 'overview' }
    const page = pageIds.find((id) => h === id || h.startsWith(id + '--'))
    return page ? { page, target: h === page ? null : h } : { page: 'overview' }
  }
  function show({ page, target }) {
    if (page !== current) {
      for (const a of $$('article.doc')) a.hidden = a.dataset.page !== page
      $$('.navitem').forEach((n) => n.dataset.page === page ? n.setAttribute('aria-current', 'page') : n.removeAttribute('aria-current'))
      current = page
      buildToc(page)
      // the local build draws a page's diagrams the first time it shows (a hidden diagram lays out wrong);
      // an Artifact draws them itself
      const pending = $$('#page-' + page + ' pre.mermaid:not([data-processed])')
      if (pending.length && window.mermaid?.run) window.mermaid.run({ nodes: pending }).catch(() => {})
      document.title = (data.pages.find((p) => p.id === page)?.title ?? data.pages[0].title) + ' · ' + data.ui.brand
    }
    document.body.classList.remove('navopen')
    $('#menu').setAttribute('aria-expanded', 'false')
    const el = target && document.getElementById(target)
    if (el) el.scrollIntoView({ block: 'start' })
    else window.scrollTo(0, 0)
    spy()
  }
  window.addEventListener('hashchange', () => show(parse(location.hash)))

  // ---- outline and scroll spy
  function buildToc(page) {
    const items = data.outlines[page] || []
    $('#toc').innerHTML = items.length ? '<div class="toctitle">' + escapeHtml(data.ui.onThisPage) + '</div>' + items.map((h) => '<a class="l' + h.level + '" href="#' + h.id + '">' + escapeHtml(h.text) + '</a>').join('') : ''
  }
  function spy() {
    const links = $$('#toc a')
    if (!links.length) return
    let on = links[0]
    for (const a of links) {
      const el = document.getElementById(a.getAttribute('href').slice(1))
      if (el && el.getBoundingClientRect().top < 120) on = a
    }
    links.forEach((a) => a.classList.toggle('on', a === on))
  }
  window.addEventListener('scroll', () => requestAnimationFrame(spy), { passive: true })

  // ---- search
  const input = $('#q'), box = $('#results')
  let hits = [], at = 0
  const escapeHtml = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c])
  function score(e, words) {
    const t = e.text.toLowerCase(), d = (e.detail || '').toLowerCase()
    let s = 0
    for (const w of words) {
      if (t.startsWith(w)) s += 6
      else if (t.includes(w)) s += 4
      else if (d.includes(w)) s += 1
      else return 0
    }
    return s + ({ Page: 3, 'LayerFS call': 2, Section: 1 }[e.kind] || 0)
  }
  function mark(text, words) {
    let out = escapeHtml(text)
    for (const w of words) if (w.length > 1) out = out.replace(new RegExp('(' + w.replace(/[.*+?^\${}()|[\\]\\\\]/g, '\\\\$&') + ')', 'ig'), '<mark>$1</mark>')
    return out
  }
  function runSearch() {
    const words = input.value.toLowerCase().split(/\\s+/).filter(Boolean)
    if (!words.length) { box.hidden = true; return }
    const seen = new Set()
    hits = data.search.map((e) => [score(e, words), e]).filter(([s]) => s > 0).sort((a, b) => b[0] - a[0]).map(([, e]) => e)
      .filter((e) => { const k = e.kind + e.text + e.hash; if (seen.has(k)) return false; seen.add(k); return true }).slice(0, 40)
    at = 0
    const title = (id) => data.pages.find((p) => p.id === id)?.title ?? id
    box.innerHTML = hits.length ? hits.map((e, i) => '<a class="result" role="option" href="#' + e.hash + '" aria-selected="' + (i === 0) + '"><span class="kind">' + escapeHtml(data.ui.kinds[e.kind] || e.kind) + '</span><span class="rt">' + mark(e.text, words) + '</span><span class="rd">' + escapeHtml(title(e.page)) + (e.detail ? ' · ' + mark(e.detail, words) : '') + '</span></a>').join('') : '<div class="noresult">' + escapeHtml(data.ui.noresult).replace('{q}', escapeHtml(input.value)) + '</div>'
    box.hidden = false
  }
  function pick(i) { const e = hits[i]; if (!e) return; box.hidden = true; input.blur(); if (location.hash === '#' + e.hash) show(parse(location.hash)); else location.hash = e.hash }
  input.addEventListener('input', runSearch)
  input.addEventListener('focus', () => { if (input.value) runSearch() })
  input.addEventListener('keydown', (ev) => {
    const rows = $$('.result', box)
    if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
      ev.preventDefault(); at = Math.max(0, Math.min(rows.length - 1, at + (ev.key === 'ArrowDown' ? 1 : -1)))
      rows.forEach((r, i) => r.setAttribute('aria-selected', String(i === at))); rows[at]?.scrollIntoView({ block: 'nearest' })
    } else if (ev.key === 'Enter') { ev.preventDefault(); pick(at) }
    else if (ev.key === 'Escape') { box.hidden = true; input.blur() }
  })
  box.addEventListener('click', (ev) => { const a = ev.target.closest('.result'); if (!a) return; ev.preventDefault(); pick($$('.result', box).indexOf(a)) })
  document.addEventListener('click', (ev) => { if (!ev.target.closest('.searchbox')) box.hidden = true })
  document.addEventListener('keydown', (ev) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key.toLowerCase() === 'k') { ev.preventDefault(); input.focus(); input.select() }
    else if (ev.key === '/' && document.activeElement === document.body) { ev.preventDefault(); input.focus() }
  })

  // ---- screenshots
  const viewer = $('#viewer'), pic = $('#vpic')
  let shots = [], si = 0
  function openShot(src, alt) {
    const page = $('article.doc:not([hidden])')
    shots = [...new Map($$('.shot, .shot-link', page).map((b) => [b.dataset.src, b.dataset.alt])).entries()]
    si = Math.max(0, shots.findIndex(([s]) => s === src))
    if (si === 0 && shots[0]?.[0] !== src) { shots.unshift([src, alt]); si = 0 }
    paint(); viewer.hidden = false; viewer.classList.remove('full'); $('#vclose').focus()
  }
  function paint() { const [src, alt] = shots[si]; pic.src = src; pic.alt = alt || ''; $('#vcap').textContent = alt || src; $('#vpos').textContent = (si + 1) + ' / ' + shots.length }
  function step(d) { si = (si + d + shots.length) % shots.length; viewer.classList.remove('full'); paint() }
  function closeShot() { viewer.hidden = true; pic.removeAttribute('src') }
  document.addEventListener('click', (ev) => {
    const b = ev.target.closest('.shot, .shot-link')
    if (b) { ev.preventDefault(); openShot(b.dataset.src, b.dataset.alt) }
  })
  $('#vprev').onclick = () => step(-1)
  $('#vnext').onclick = () => step(1)
  $('#vclose').onclick = closeShot
  pic.onclick = () => viewer.classList.toggle('full')
  viewer.addEventListener('click', (ev) => { if (ev.target === viewer || ev.target.id === 'vimg') closeShot() })
  document.addEventListener('keydown', (ev) => {
    if (viewer.hidden) return
    if (ev.key === 'Escape') closeShot()
    else if (ev.key === 'ArrowRight') step(1)
    else if (ev.key === 'ArrowLeft') step(-1)
  })

  // ---- phone navigation
  $('#menu').onclick = () => { const open = document.body.classList.toggle('navopen'); $('#menu').setAttribute('aria-expanded', String(open)) }

  // ---- language: each language's nav and pages live in a <template>; a switch swaps them and keeps the place
  function setLang(l) {
    const reading = window.scrollY > 80 && $('#toc a.on')?.getAttribute('href').slice(1)
    lang = l
    data = all[l]
    document.documentElement.lang = l === 'zh' ? 'zh-CN' : 'en'
    $('#nav').replaceChildren(document.getElementById('nav-' + l).content.cloneNode(true))
    $('#main').replaceChildren(document.getElementById('main-' + l).content.cloneNode(true))
    for (const el of $$('[data-t]')) el.textContent = data.ui[el.dataset.t]
    input.placeholder = data.ui.search
    input.setAttribute('aria-label', data.ui.searchLabel)
    $('#nav').setAttribute('aria-label', data.ui.nav)
    $('#toc').setAttribute('aria-label', data.ui.onThisPage)
    $('.brand').setAttribute('aria-label', data.ui.brand)
    viewer.setAttribute('aria-label', data.ui.screenshot)
    $('#vprev').setAttribute('aria-label', data.ui.prevShot)
    $('#vnext').setAttribute('aria-label', data.ui.nextShot)
    const btn = $('#lang')
    btn.textContent = data.ui.switchTo
    btn.lang = l === 'zh' ? 'en' : 'zh-CN'
    btn.setAttribute('aria-label', data.ui.switchLabel)
    box.hidden = true
    const page = current || parse(location.hash).page
    current = null
    show({ page, target: reading || parse(location.hash).target })
  }
  $('#lang').onclick = () => {
    const next = lang === 'zh' ? 'en' : 'zh'
    try { localStorage.setItem(KEY, next) } catch (e) {}
    setLang(next)
  }

  setLang(lang)
})()
</script>`

const html = fragment ? body : `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<style>[hidden]{display:none!important} img{max-width:100%}</style>
<script src="https://cdn.jsdelivr.net/npm/mermaid@10.9.1/dist/mermaid.min.js"></script>
<script>window.mermaid?.initialize({ startOnLoad: false, theme: matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'neutral' })</script>
</head>
<body>
${body}
</body>
</html>
`
writeFileSync(out, html)
console.log(`${out}: ${PAGES.length} pages, ${shots} screenshots, ${built.en.calls} LayerFS calls; ${LANGS.map((l) => `${l} ${built[l].data.search.length} search entries`).join(", ")}`)

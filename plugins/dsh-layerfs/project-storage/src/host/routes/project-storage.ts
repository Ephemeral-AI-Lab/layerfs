/**
 * A project's own storage, from the counters every saved Commit carries
 * (`new_canonical`, `new_stored`, `new_metadata`, see LayerFS's
 * `layerfs-sdk-observe/references/storage-metrics.md`). LayerFS keeps byte
 * totals per Store only; summed over one project's Commits these counters are
 * the bytes that project's saves wrote first, and over every project they add
 * up to the Store's `store.stats` totals.
 *
 * No LayerFS call sums them, so the host keeps a ledger per project in
 * `<root>/project-storage/`: one row per Commit id (a Commit never changes).
 * A refresh lists the Branches, walks each moved Branch newest first until a
 * Commit the ledger has, reads only the new Commits (`branch.get_commit`) and
 * counts main's publications. The first read of a long history runs in the
 * background; the route answers at once with what the ledger holds and the
 * progress of the refresh.
 */
import { createHash } from 'node:crypto'
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import type { Lfs } from '../layerfs/cli.ts'
import { pages } from './util.ts'

/** Commits read at once. */
const WIDTH = 8
/** Rows per `branch.commits` page (a smaller page when one does not fit). */
const PAGE_SIZES = [100, 25, 5]
/** Newest saves the page draws as bars. */
const RECENT = 30
/** Points on the growth line. */
const GROWTH_POINTS = 40
/** A finished refresh is reused this long before the next one starts. */
const FRESH_MS = 15_000
/** Messages are kept this long. */
const MESSAGE = 120

/** One Commit: [time, via, logical, new_canonical, new_stored, new_metadata, branch, message]. */
type Row = [number | null, string | null, number, number, number, number, string, string]

interface Ledger {
  version: 1
  project: string
  created: number | null
  /** Commit id -> its row. */
  commits: Record<string, Row>
  /** Branch -> the head whose walk finished. */
  heads: Record<string, string>
  /** main's publication Layers: how many, their trees' raw bytes, the newest counted. */
  publications: { count: number, logical: number, head: string | null }
  updatedAt: number | null
}

export interface StorageProgress { branches: number, branchesTotal: number, saves: number }

interface Job { promise: Promise<void>, progress: StorageProgress }

const digest = (value: string) => createHash('sha256').update(value).digest('hex')
const num = (value: unknown): number => typeof value === 'number' && Number.isFinite(value) ? value : 0
const isCommit = (c: any) => (c?.ref?.kind ?? (String(c?.id).startsWith('32') ? 'Layer' : 'Commit')) === 'Commit'

/** Runs `fn` over `items`, `width` at a time. */
async function each<T>(items: T[], width: number, fn: (item: T) => Promise<void>): Promise<void> {
  let next = 0
  const lane = async () => { while (next < items.length) await fn(items[next++]) }
  await Promise.all(Array.from({ length: Math.min(width, items.length) }, lane))
}

export class ProjectStorageLedgers {
  readonly directory: string
  private readonly home: string
  private readonly jobs = new Map<string, Job>()
  private readonly errors = new Map<string, any>()
  private readonly finished = new Map<string, number>()

  constructor(root: string, home: string, private readonly lfs: () => Lfs, private readonly log: (line: string) => void = () => {}, private readonly now: () => number = Date.now) {
    this.directory = join(root, 'project-storage')
    this.home = resolve(home ?? root)
  }

  /** A project record's ledger file: a recreated project (another `created`) starts a new one. */
  path(project: string, created: number | null): string { return join(this.directory, `${digest(`${this.home}\0${project}\0${created ?? ''}`)}.json`) }

  async read(project: string, created: number | null): Promise<Ledger> {
    try {
      const value = JSON.parse(await readFile(this.path(project, created), 'utf8'))
      if (value.version === 1 && value.project === project && value.created === created && value.commits && value.heads && value.publications) return value
    } catch (error: any) {
      if (error.code !== 'ENOENT') this.log(`[host] project storage ledger of ${project}: ${error?.message ?? error}`)
    }
    return { version: 1, project, created, commits: {}, heads: {}, publications: { count: 0, logical: 0, head: null }, updatedAt: null }
  }

  private async write(ledger: Ledger): Promise<void> {
    await mkdir(this.directory, { recursive: true })
    const file = this.path(ledger.project, ledger.created), temp = `${file}.${process.pid}.tmp`
    await writeFile(temp, JSON.stringify(ledger))
    await rename(temp, file)
  }

  /** Starts a refresh unless one runs or one finished a moment ago; returns the running one. */
  refresh(project: string, created: number | null, force = false): Job | null {
    const key = this.path(project, created)
    const running = this.jobs.get(key)
    if (running) return running
    if (!force && this.now() - (this.finished.get(key) ?? 0) < FRESH_MS) return null
    const progress: StorageProgress = { branches: 0, branchesTotal: 0, saves: 0 }
    const promise = this.build(project, created, progress)
      .then(() => { this.errors.delete(key) }, (error: any) => {
        this.errors.set(key, { code: error?.code ?? 'Failed', message: error?.message ?? String(error), detail: error?.detail ?? null })
        this.log(`[host] project storage of ${project}: ${error?.message ?? error}`)
      })
      .finally(() => { this.jobs.delete(key); this.finished.set(key, this.now()) })
    const job = { promise, progress }
    this.jobs.set(key, job)
    return job
  }

  /** Waits for the running refresh, if any (tests and shutdown). */
  async settle(project: string, created: number | null): Promise<void> { await this.jobs.get(this.path(project, created))?.promise }

  async close(): Promise<void> { await Promise.all([...this.jobs.values()].map((job) => job.promise)) }

  private async walk(project: string, branch: string, head: string, known: (id: string) => boolean): Promise<any[]> {
    for (const size of PAGE_SIZES) {
      try {
        const rows: any[] = []
        for (let page = 1; ; page++) {
          const listed = await this.lfs().branch.commits(project, branch, { page, page_size: size, at_commit: head })
          let reached = false
          for (const c of listed.items ?? []) {
            if (known(c.id)) { reached = true; break }
            rows.push(c)
          }
          if (reached || !listed.has_more) return rows
        }
      } catch (error: any) {
        // a page whose reply does not fit is refused whole: list smaller pages
        if (error?.code !== 'Capacity' || size === PAGE_SIZES.at(-1)) throw error
      }
    }
    return []
  }

  private async build(project: string, created: number | null, progress: StorageProgress): Promise<void> {
    const lfs = this.lfs()
    const ledger = await this.read(project, created)
    const branches: any[] = await pages((p) => lfs.branch.list(project, p))
    const side = branches.filter((b) => b.name !== 'main' && b.head)
    progress.branchesTotal = side.length
    let dirty = false, lastWrite = this.now()
    for (const b of side) {
      const head: string = b.saved_head?.id ?? b.head
      if (ledger.heads[b.name] !== head) {
        const fresh = (await this.walk(project, b.name, head, (id) => id in ledger.commits)).filter(isCommit)
        await each(fresh, WIDTH, async (c) => {
          const r = await lfs.branch.get_commit(project, b.name, c.id)
          ledger.commits[c.id] = [typeof r.time === 'number' ? r.time : null, r.via ?? null, num(r.logical), num(r.new_canonical), num(r.new_stored), num(r.new_metadata), b.name, String(r.message ?? '').slice(0, MESSAGE)]
          progress.saves++
        })
        ledger.heads[b.name] = head
        dirty = true
      }
      progress.branches++
      // keep progress across a restart of a long first read
      if (dirty && this.now() - lastWrite > 5000) { await this.write(ledger); dirty = false; lastWrite = this.now() }
    }
    // main's publications: Layers, newest first, down to the newest one counted
    const main = branches.find((b) => b.name === 'main')
    const mainHead: string | null = main ? main.saved_head?.id ?? main.base ?? null : null
    if (mainHead && mainHead !== ledger.publications.head) {
      const layers = await this.walk(project, 'main', mainHead, (id) => id === ledger.publications.head)
      ledger.publications = {
        count: ledger.publications.count + layers.length,
        logical: ledger.publications.logical + layers.reduce((sum, c) => sum + num(c.logical), 0),
        head: mainHead,
      }
      dirty = true
    }
    ledger.updatedAt = this.now()
    await this.write(ledger)
  }

  /**
   * ProjectStorage: the ledger's totals (null before its first read finishes),
   * the newest saves, the growth of packed bytes, and the running refresh.
   */
  async view(project: string, created: number | null, options: { refresh?: boolean } = {}) {
    const job = this.refresh(project, created, options.refresh === true)
    const key = this.path(project, created)
    const ledger = await this.read(project, created)
    const rows = Object.entries(ledger.commits).sort((a, b) => (a[1][0] ?? 0) - (b[1][0] ?? 0))
    const totals = ledger.updatedAt === null ? null : {
      saves: rows.length,
      merges: rows.filter(([, r]) => r[1] === 'merge').length,
      branches: Object.keys(ledger.heads).length,
      publications: ledger.publications.count,
      commitLogical: rows.reduce((sum, [, r]) => sum + r[2], 0),
      publicationLogical: ledger.publications.logical,
      canonical: rows.reduce((sum, [, r]) => sum + r[3], 0),
      stored: rows.reduce((sum, [, r]) => sum + r[4], 0),
      metadata: rows.reduce((sum, [, r]) => sum + r[5], 0),
    }
    const growth: number[] = []
    if (rows.length) {
      let sum = 0, at = 0
      for (let i = 0; i < GROWTH_POINTS; i++) {
        const end = Math.ceil((i + 1) * rows.length / GROWTH_POINTS)
        for (; at < end; at++) sum += rows[at][1][4]
        growth.push(sum)
      }
    }
    return {
      project,
      totals,
      growth,
      recent: rows.slice(-RECENT).map(([id, r]) => ({ id, time: r[0], via: r[1], logical: r[2], stored: r[4], branch: r[6], message: r[7] })),
      updatedAt: ledger.updatedAt,
      building: job ? { ...job.progress } : null,
      error: job ? null : this.errors.get(key) ?? null,
    }
  }
}

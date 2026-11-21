/**
 * The machine page's tabs, pure (MachineTabs.tsx draws them): which tabs a
 * machine page has, the Sandboxes tab's cards (each Sandbox's record, the
 * sampler's newest values and its run folder on disk, joined by name, filtered
 * and ordered), and what the Overview's "needs you" notices count.
 */
import { sandboxGone, type MachineSandbox } from './model.ts'
import type { SandboxSpace } from './space.ts'

export type MachineTab = 'overview' | 'sandboxes' | 'stores' | 'images' | 'files' | 'log'

/**
 * The tabs a machine page has, in order: Images needs a machine that keeps
 * Sandbox image copies (Stage 1, not macOS native), Files machine.files,
 * Worker log machine.log and a worker log. The hour of processor and memory
 * is on the Overview, where the disk goes a dialog from it (no Usage or Disk
 * tab).
 */
export function machineTabs(has: { images: boolean, files: boolean, log: boolean }): MachineTab[] {
  return ['overview', ...(has.images ? ['images' as const] : []),
    ...(has.files ? ['files' as const] : []), 'sandboxes', 'stores', ...(has.log ? ['log' as const] : [])]
}

/** What the Sandboxes tab needs of one sampler row (metrics.ts `sandboxTable`): its memory now, null when not read. */
export interface MetricsRowLike { name: string, closedAt: number | null, memory?: number | null }

/** One Sandbox card: `record` null for a Sandbox closed within the sampler's hour (only its values remain). */
export interface SandboxCard<M extends MetricsRowLike> { name: string, record: MachineSandbox | null, metrics: M | null, space: SandboxSpace | null }

/** The Sandboxes tab's filters: every recorded Sandbox, the running ones, those that need you, and those closed within the hour. */
export type SandboxFilter = 'all' | 'running' | 'attention' | 'closed'
export const SANDBOX_FILTERS: readonly SandboxFilter[] = ['all', 'running', 'attention', 'closed']

/** The Sandboxes tab's orders: as LayerFS lists them, by name, the most disk first, the most memory first. */
export type SandboxSort = 'listed' | 'name' | 'disk' | 'memory'
export const SANDBOX_SORTS: readonly SandboxSort[] = ['listed', 'name', 'disk', 'memory']

/** An unknown size sorts after every known one; ties by name. */
function bySize<M extends MetricsRowLike>(size: (c: SandboxCard<M>) => number | null | undefined) {
  return (a: SandboxCard<M>, b: SandboxCard<M>): number => {
    const x = size(a) ?? null, y = size(b) ?? null
    if (x == null && y == null) return a.name.localeCompare(b.name)
    if (x == null) return 1
    if (y == null) return -1
    return y - x || a.name.localeCompare(b.name)
  }
}

/**
 * The Sandboxes tab's cards for one filter and order, and how many each
 * filter holds (for its chip). Recorded Sandboxes are joined by name with
 * their sampler values and disk row; those that need you always come first,
 * then the chosen order. "Closed" holds only Sandboxes the sampler saw close
 * within its hour, the most recently closed first; the other filters hold
 * only recorded ones.
 */
export function sandboxCards<M extends MetricsRowLike>(records: readonly MachineSandbox[], metrics: readonly M[], space: readonly SandboxSpace[] | null,
  needsYou: (b: MachineSandbox) => boolean, filter: SandboxFilter, sort: SandboxSort): { cards: SandboxCard<M>[], counts: Record<SandboxFilter, number> } {
  const byName = new Map(metrics.map((r) => [r.name, r]))
  const disk = new Map((space ?? []).map((r) => [r.name, r]))
  const recorded = new Set(records.map((b) => b.name))
  const closed = metrics.filter((r) => r.closedAt != null && !recorded.has(r.name))
  const counts: Record<SandboxFilter, number> = {
    all: records.length,
    running: records.filter((b) => b.state === 'ready').length,
    attention: records.filter(needsYou).length,
    closed: closed.length,
  }
  if (filter === 'closed') {
    const cards = closed.map((r) => ({ name: r.name, record: null, metrics: r, space: null }))
    return { cards: cards.sort((a, b) => (b.metrics.closedAt ?? 0) - (a.metrics.closedAt ?? 0) || a.name.localeCompare(b.name)), counts }
  }
  const kept = records.filter((b) => filter === 'all' || (filter === 'running' ? b.state === 'ready' : needsYou(b)))
  const cards: SandboxCard<M>[] = kept.map((b) => ({ name: b.name, record: b, metrics: byName.get(b.name) ?? null, space: disk.get(b.name) ?? null }))
  const order = sort === 'name' ? (a: SandboxCard<M>, b: SandboxCard<M>) => a.name.localeCompare(b.name)
    : sort === 'disk' ? bySize<M>((c) => c.space?.bytes)
      : sort === 'memory' ? bySize<M>((c) => c.metrics?.memory)
        : null
  const sorted = order ? [...cards].sort(order) : cards
  return { cards: [...sorted.filter((c) => needsYou(c.record!)), ...sorted.filter((c) => !needsYou(c.record!))], counts }
}

/**
 * What the Overview flags: gone Sandboxes (their daemon ended with the
 * machine, their run folder stays until closed) with the bytes LayerFS
 * measured for them (null when not measured), and the other Sandboxes that
 * need you.
 */
export function sandboxAttention(records: readonly MachineSandbox[], space: readonly SandboxSpace[] | null, needsYou: (b: MachineSandbox) => boolean): { gone: number, goneBytes: number | null, needsYou: number } {
  const gone = records.filter(sandboxGone)
  const disk = new Map((space ?? []).map((r) => [r.name, r.bytes]))
  const measured = gone.map((b) => disk.get(b.name)).filter((n): n is number => n != null)
  return {
    gone: gone.length,
    goneBytes: measured.length ? measured.reduce((a, b) => a + b, 0) : null,
    needsYou: records.filter((b) => !sandboxGone(b) && needsYou(b)).length,
  }
}

// ---------------------------------------------------------------- Overview

/** What the Overview needs of one sampler row (metrics.ts `sandboxTable`). */
export interface MemoryRowLike { gap: string | null, row: { live: boolean | null, usage: { memoryCurrent: number | null } | null } | null }

/** The memory the machine's live Sandboxes use now, summed over those whose memory was read; null when none was. */
export function sandboxMemory(rows: readonly MemoryRowLike[]): number | null {
  let sum: number | null = null
  for (const r of rows) {
    const v = r.gap || r.row?.live === false ? null : r.row?.usage?.memoryCurrent ?? null
    if (v != null) sum = (sum ?? 0) + v
  }
  return sum
}

/**
 * A processor share (0–1) as the Overview prints it, the unit apart: "0.4"
 * under 10 %, whole numbers above, "0" for none and "<0.1" for a trace —
 * never the bare "<1 %" that hides the difference between idle and busy.
 */
export function cpuShare(share: number): string {
  const pct = share * 100
  if (!(pct > 0)) return '0'
  if (pct < 0.1) return '<0.1'
  if (pct < 10) return pct.toFixed(1)
  return String(Math.round(Math.min(pct, 100)))
}

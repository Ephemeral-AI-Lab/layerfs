import { failureSuperseded } from './model.ts'
import type { StorageJob } from './types.ts'

// The notice order is independent of platform order, severity and polling order.
const priority: Record<string, number> = {
  setup: 0, job: 1, unavailable: 2, unreachable: 3, gone: 4, record: 5,
  needs: 6, down: 7, pg: 7, update: 7, stopped: 8,
}
export function compareNeeds(a: { key: string }, b: { key: string }): number {
  return priority[a.key.split(':')[0]] - priority[b.key.split(':')[0]] || a.key.localeCompare(b.key)
}

export function visibleFailures(jobs: readonly StorageJob[], stores: readonly { name: string }[], dismissed: ReadonlySet<string>) {
  const successes = jobs.filter(j => !j.error && (!j.outcome || j.outcome === 'succeeded'))
  return jobs.filter((j): j is StorageJob & { ended: number } => j.ended != null && !dismissed.has(j.id)
    && (!!j.error || !!j.outcome && j.outcome !== 'succeeded') && !failureSuperseded(j, successes, stores))
}

type Tone = 'danger' | 'warning' | undefined
export function worstTone(tones: readonly Tone[]): Tone {
  return tones.includes('danger') ? 'danger' : tones.includes('warning') ? 'warning' : undefined
}

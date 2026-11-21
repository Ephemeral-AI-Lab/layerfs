/**
 * The Projects tab's search and sort (rules in project-list.ts): a search box
 * ("/" focuses it, Escape clears it) and a sort menu. Both are remembered in
 * this browser; a viewer without storage simply starts from Last session and no search.
 */
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { IconSearchOutlineRegular, Input } from '@deepseek-ai/dsh-client-ui-primitives'
import { cachedAnswer } from '../ui/use-query.ts'
import { MenuSelect } from '../ui/index.tsx'
import type { StorageT } from './locale.ts'
import { DEFAULT_SORT, PROJECT_SORTS, sizeCacheKey, sizeOrder, type ProjectSize, type ProjectSort } from './project-list.ts'
import type { StoreRow } from './types.ts'
import css from './HomeCards.module.css'

const QUERY_KEY = 'dsh-layerfs:projects-query'
const SORT_KEY = 'dsh-layerfs:projects-sort'

function recall(key: string): string | null {
  try { return localStorage.getItem(key) } catch { return null }
}
function keep(key: string, value: string): void {
  try { if (value) localStorage.setItem(key, value); else localStorage.removeItem(key) } catch { /* private window or blocked storage */ }
}

export interface ProjectFinder {
  query: string
  setQuery: (q: string) => void
  sort: ProjectSort
  setSort: (s: ProjectSort) => void
  /** The Size order, worked out when Size is picked or the page refreshed; null otherwise (Last session's is worked out by the table, which reads the sessions). */
  frozen: readonly string[] | null
}

/** Search and sort state for the Projects tab. `refreshKey` is the page's Refresh: Size is ordered again then. */
export function useProjectFinder(stores: readonly StoreRow[], refreshKey: number): ProjectFinder {
  const [query, setQueryState] = useState(() => recall(QUERY_KEY) ?? '')
  const [sort, setSortState] = useState<ProjectSort>(() => {
    const s = recall(SORT_KEY)
    return PROJECT_SORTS.includes(s as ProjectSort) ? s as ProjectSort : DEFAULT_SORT
  })
  const [frozen, setFrozen] = useState<readonly string[] | null>(null)
  // the latest projects, read when the order is worked out (a storage event alone does not reorder)
  const keys = useRef<string[]>([])
  keys.current = stores.flatMap((s) => s.projects.map((p) => `${s.name}/${p}`))
  useEffect(() => {
    if (sort !== 'size') { setFrozen(null); return }
    setFrozen(sizeOrder(keys.current, (k) => cachedAnswer<ProjectSize>(sizeCacheKey(k), true)?.stored ?? null))
  }, [sort, refreshKey])
  return {
    query, sort, frozen,
    setQuery: (q) => { setQueryState(q); keep(QUERY_KEY, q) },
    setSort: (s) => { setSortState(s); keep(SORT_KEY, s === DEFAULT_SORT ? '' : s) },
  }
}

/** The bar above the project cards. */
export function ProjectFinderBar({ finder, t }: { finder: ProjectFinder, t: StorageT }): ReactNode {
  const input = useRef<HTMLInputElement>(null)
  // "/" focuses the search, unless typing somewhere already
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey || e.defaultPrevented) return
      const el = e.target as HTMLElement | null
      if (el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))) return
      // only while the bar is on screen (another panel may hide the page, which stays mounted)
      if (!input.current || input.current.offsetParent === null) return
      e.preventDefault()
      input.current.focus()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  const labels: Record<ProjectSort, string> = { session: t('ovSortSession'), default: t('ovSortDefault'), size: t('ovSortSize'), name: t('ovSortName') }
  return (
    <div className={css.finder} role="search" aria-label={t('ovFinderLabel')}>
      <Input ref={input} className={css.finderSearch} icon={<IconSearchOutlineRegular size={14} />} type="search" value={finder.query}
        placeholder={t('ovSearchProjects')} aria-label={t('ovSearchProjects')} autoComplete="off" spellCheck={false}
        onChange={(e) => finder.setQuery(e.target.value)}
        onKeyDown={(e) => { if (e.key === 'Escape' && finder.query) { e.preventDefault(); e.stopPropagation(); finder.setQuery('') } }} />
      {finder.query && <button type="button" className={css.clear} onClick={() => { finder.setQuery(''); input.current?.focus() }}>{t('ovClearSearch')}</button>}
      {/* the sort sits at the right end */}
      <span className={css.finderSpacer} />
      <MenuSelect className={css.finderSort} label={t('ovSortLabel')} value={finder.sort} onChange={finder.setSort}
        options={PROJECT_SORTS.map((s) => ({ value: s, label: labels[s] }))} />
    </div>
  )
}

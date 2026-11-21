/**
 * The Doctor page: the home as one tree, read from records, and what the
 * LayerFS 2.0 doctors last said about each thing in it.
 *
 * Above the tabs is this install (the home doctor's row). Each tab lists one
 * kind (machines, Stores, projects, Sandboxes, backup buckets); a row opens to
 * what is directly on it, so the same thing is found under its machine and in
 * its own tab. Opening the page runs no doctor: every row reads Not checked
 * until Check, Full check or Check all has run. The model is `model.ts`, the
 * calls are `use-doctor.ts`, the words are `words.ts`.
 */
import { usePageReading } from '../storage/PaneBar.tsx'
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button, IconRefreshOutlineRegular, Modal } from '@deepseek-ai/dsh-client-ui-primitives'
import type { HostError, LayerfsApi } from '../api.ts'
import { MachinesGlyph, SandboxesGlyph, StoreGlyph } from '../machines/icons.tsx'
import { LayerfsGlyph } from '../overview/glyph.tsx'
import type { StorageT } from '../storage/locale.ts'
import { BucketGlyph, refusalLabels } from '../storage/parts.tsx'
import { Contained, Empty, LoadError, Loading, Page } from '../ui/index.tsx'
import { TabBar } from '../ui/TabBar.tsx'
import type { DoctorT } from './locale.ts'
import { allOffWhy, allPlan, headOf, tabOf, tabsOf, viewOf, TABS, type RowFix, type TabKey, type Thing } from './model.ts'
import { Btn, Detail, Node, Row, StatusSignal, type RowContext } from './Row.tsx'
import { TipLayer, useToast } from './tips.tsx'
import { useDoctor } from './use-doctor.ts'
import { wordsOf, type Words } from './words.ts'
import css from './DoctorPage.module.css'

const TAB_LABEL: Record<TabKey, string> = { machines: 'dpTabMachines', stores: 'dpTabStores', projects: 'dpTabProjects', sandboxes: 'dpTabSandboxes', backups: 'dpTabBackups' }

function TabIcon({ tab }: { tab: TabKey }): ReactNode {
  if (tab === 'machines') return <MachinesGlyph size={15} />
  if (tab === 'stores') return <StoreGlyph size={15} />
  if (tab === 'projects') return <LayerfsGlyph size={15} mono />
  if (tab === 'sandboxes') return <SandboxesGlyph size={15} />
  return <BucketGlyph size={15} />
}

/** A tab's dot: what starts in it at its worst (a stopped machine marks its tab; a check that runs spins). */
const TAB_TONE = { bad: 'danger', warn: 'warning', nc: 'idle', run: 'ongoing' } as const

function Body({ api, t, ts, refreshKey }: { api: LayerfsApi, t: DoctorT, ts: StorageT, refreshKey: number }): ReactNode {
  const w = useMemo(() => wordsOf(t), [t])
  const toast = useToast()
  const doctor = useDoctor(api, (what) => toast(w.toast(what)), refreshKey)
  usePageReading(doctor.loading)
  const { state } = doctor
  const [tab, setTab] = useState<TabKey>('machines')
  const [open, setOpen] = useState<Record<string, boolean>>({})
  const [flash, setFlash] = useState<string | null>(null)
  const [confirm, setConfirm] = useState<{ thing: Thing, fix: RowFix } | null>(null)
  const [now, setNow] = useState(() => Date.now())
  const panel = useId()
  const root = useRef<HTMLDivElement | null>(null)
  /** Where the focus goes after the next render (a row another tab holds, a button that takes another's place). */
  const focusNext = useRef<string | null>(null)
  /** The control that last had the focus: when it leaves the page, its row takes the focus. */
  const lastFocus = useRef<string | null>(null)

  // the clock of a Full check's elapsed time: only while one runs
  // (and while a call waits for its turn at the host, so its row can say so: F12)
  const timing = Object.values(state.runs).some((r) => r.kind === 'full' || !r.spawned)
  useEffect(() => {
    if (!timing) return
    setNow(Date.now())
    const timer = setInterval(() => setNow(Date.now()), 500)
    return () => clearInterval(timer)
  }, [timing])

  useEffect(() => {
    if (!flash) return
    const timer = setTimeout(() => setFlash(null), 1600)
    return () => clearTimeout(timer)
  }, [flash])

  // focus is kept: a control that left the page hands it to its row (or its tab), never to the document
  useLayoutEffect(() => {
    const el = root.current
    if (!el) return
    const find = (fid: string) => el.querySelector<HTMLElement>(`[data-fid="${CSS.escape(fid)}"]`)
    if (focusNext.current) {
      const want = focusNext.current
      const target = find(want) ?? (want.includes(':') ? find(`row:${want.slice(want.indexOf(':') + 1)}`) : null)
      focusNext.current = null
      if (target && target !== document.activeElement) { target.focus({ preventScroll: true }); target.scrollIntoView({ block: 'nearest' }) }
      return
    }
    const last = lastFocus.current
    if (last && (document.activeElement === document.body || document.activeElement === null) && !find(last) && last.includes(':')) {
      // its row went too (a deleted project): the tab that listed it takes the focus
      const next = find(`row:${last.slice(last.indexOf(':') + 1)}`) ?? find(`tab:${tab}`)
      next?.focus({ preventScroll: true })
    }
    // where the focus is after this render (a focus set while the window is in the background sends no event)
    const active = document.activeElement
    if (active instanceof HTMLElement && el.contains(active) && active.dataset.fid) lastFocus.current = active.dataset.fid
  })

  const on = useMemo(() => ({
    toggle: (key: string) => setOpen((o) => ({ ...o, [key]: !o[key] })),
    check: (thing: Thing) => {
      // pressing Check opens the folder: the things it covers land in sight
      if (thing.kind !== 'home' && (state.world?.kids[thing.key]?.length ?? 0) > 0) setOpen((o) => ({ ...o, [thing.key]: true }))
      doctor.check(thing)
    },
    full: doctor.full,
    stop: (thing: Thing) => { focusNext.current = `full:${thing.key}`; doctor.stop(thing) },
    fix: (thing: Thing, fix: RowFix) => { if (fix.fix?.kind === 'delete') setConfirm({ thing, fix }); else doctor.fix(thing, fix) },
    /** A waiting row's root cause: open the path to its row and focus it. */
    goTo: (key: string) => {
      const thing = state.world?.by[key]
      if (!thing) return
      if (!root.current?.querySelector(`[data-rows] [data-fid="${CSS.escape(`row:${key}`)}"]`)) {
        setTab(tabOf(thing))
        if (thing.kind === 'workspace' && thing.parent) setOpen((o) => ({ ...o, [thing.parent!]: true }))
      }
      setFlash(key)
      focusNext.current = `row:${key}`
    },
  }), [doctor, state.world])

  if (!state.world) {
    const error = doctor.error as HostError | undefined
    if (doctor.loading || error === undefined) return <Loading label={w.t('dpLoading')} />
    if (error.code === 'Unsupported') return <Empty title={w.t('dpUnsupportedTitle')}>{w.t('dpUnsupported')}</Empty>
    return <LoadError title={w.t('dpLoadFailed')} error={error} labels={refusalLabels(ts)} retryLabel={w.t('dpRetry')} onRetry={doctor.reload} />
  }

  const world = state.world
  const ctx: RowContext = { state, w, open, flash, now, on }
  const head = headOf(state)
  const h = head.headline
  const headline = h.kind === 'checking' ? w.t('dpHeadChecking', { done: h.done, total: h.total })
    : h.kind === 'problems' ? w.n('dpProblems', h.n) : h.kind === 'notices' ? w.n('dpNotices', h.n)
      : h.kind === 'notChecked' ? w.word('nc') : h.kind === 'noProblems' ? w.t('dpHeadNoProblems') : w.t('dpHeadAllClear')
  const counts = [
    head.counts.ok ? w.t('dpCountOk', { n: head.counts.ok }) : '',
    head.counts.problem && h.kind !== 'problems' ? w.n('dpProblems', head.counts.problem) : '',
    head.counts.notice && h.kind !== 'notices' ? w.n('dpNotices', head.counts.notice) : '',
    head.counts.run && h.kind !== 'checking' ? w.t('dpCountChecking', { n: head.counts.run }) : '',
    head.counts.nc && h.kind !== 'notChecked' ? w.t('dpCountNc', { n: head.counts.nc }) : '',
  ].filter(Boolean).join(' · ')
  const plan = allPlan(state)
  const allWhy = state.all ? null : allOffWhy(state)
  const allWhyText = !allWhy ? null : w.t(allWhy.why === 'fix' ? 'dpAllOffFix' : allWhy.why === 'full' ? 'dpAllOffFull' : 'dpAllOffCheck', { name: allWhy.thing ? w.nameOf(allWhy.thing) : '' })
  const home = world.by[world.homeKey]!
  const homeOpen = !!open[home.key]
  const hv = viewOf(state, home, 0, homeOpen)
  const tabs = tabsOf(state)
  const kind = TABS.find(([key]) => key === tab)![1]
  const rows = world.things.filter((x) => x.kind === kind)

  return (
    <div className={css.root} ref={root} onFocus={(e) => { const fid = (e.target as HTMLElement).dataset?.fid; if (fid) lastFocus.current = fid }}>
      <header className={css.head}>
        <div className={css.headRow}>
          <div>
            <h1 className={css.title}>
              <span className={css.sr}>{w.t('dpTitle')}: </span>
              <StatusSignal fid="head" st={head.state} size={10} className={css.headSig} tip={{ title: headline, lines: [w.t('dpHeadTip')], cmds: ['layerfs home get --with tree'] }}>{headline}</StatusSignal>
            </h1>
            {counts ? <p className={css.intro}>{counts}</p> : null}
          </div>
          <div className={css.headActions}>
            <Button variant="ghost" size="sm" aria-label={ts('refresh')} title={ts('refresh')}
              icon={<IconRefreshOutlineRegular size={15} />} onClick={doctor.reload} />
            <Btn variant="primary" width="all" fid="checkAll" busy={!!state.all} off={!!allWhy} onPress={doctor.checkAll}
              tip={{
                title: w.t('dpCheckAll'), wide: true, lines: [allWhyText ?? w.t('dpCheckAllTip'), w.t('dpCheckAllLeftOut')],
                cmds: ['layerfs home doctor', ...(plan?.stores.length ? [`layerfs store doctor ${w.t('dpEachStore')} ×${plan.stores.length}`] : []), ...(plan?.sandboxes.length ? [`layerfs sandbox doctor ${w.t('dpEachSandbox')} ×${plan.sandboxes.length}`] : [])],
              }}>
              {w.t('dpCheckAll')}
            </Btn>
          </div>
        </div>
        <div className={css.homeBox}>
          <Row v={hv} open={homeOpen} ctx={ctx} />
          {homeOpen ? <Detail v={hv} ctx={ctx} /> : null}
        </div>
      </header>

      <TabBar label={w.t('dpTabsLabel')} active={tab} onSelect={(key) => setTab(key as TabKey)}
        tabs={tabs.map((x) => {
          const said = w.tabSaid(x)
          return {
            key: x.key, label: w.t(TAB_LABEL[x.key]), count: x.count, icon: <TabIcon tab={x.key} />,
            tone: x.mark ? TAB_TONE[x.mark] : undefined, hint: said.length ? said.join(' · ') : undefined,
            id: `${panel}-tab-${x.key}`, controls: panel, fid: `tab:${x.key}`,
          }
        })} />
      <div className={css.card} role="tabpanel" id={panel} aria-labelledby={`${panel}-tab-${tab}`} data-rows="">
        {rows.length ? rows.map((x) => <div key={x.key} className={css.group}><Node thing={x} depth={0} ctx={ctx} /></div>) : <p className={css.none}>{w.t('dpTabEmpty')}</p>}
      </div>

      {confirm ? (
        <Contained><Modal open onClose={() => setConfirm(null)} title={w.t('dpDeleteTitle', { name: confirm.thing.name })} closeLabel={w.t('close')}
          footer={(
            <div className={css.modalActions}>
              <Button variant="outline" onClick={() => setConfirm(null)}>{w.t('dpCancel')}</Button>
              <Button variant="primary" data-fid="confirm-delete" onClick={() => { const c = confirm; setConfirm(null); focusNext.current = `row:${c.thing.parent ?? c.thing.key}`; doctor.fix(c.thing, c.fix) }}>{w.t('dpFixDelete')}</Button>
            </div>
          )}>
          <p className={css.modalBody}>{splitOn(w.t('dpDeleteBody', { command: '\u0000' }), <code>{confirm.fix.line}</code>)}</p>
        </Modal></Contained>
      ) : null}
    </div>
  )
}

/** A sentence with one piece of code in it, at the placeholder. */
function splitOn(text: string, code: ReactNode): ReactNode {
  const [before, after] = text.split('\u0000')
  return <>{before}{code}{after ?? ''}</>
}

/** `refreshKey`: the top bar's Refresh; each new value reads the tree again. */
export function DoctorPage({ api, t, ts, refreshKey = 0 }: { api: LayerfsApi, t: DoctorT, ts: StorageT, refreshKey?: number }): ReactNode {
  return (
    <Page label={t('healthTitle')}>
      <TipLayer><Body api={api} t={t} ts={ts} refreshKey={refreshKey} /></TipLayer>
    </Page>
  )
}

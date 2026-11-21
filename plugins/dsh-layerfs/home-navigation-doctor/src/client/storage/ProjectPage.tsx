/**
 * One project: Files (Branch, tree, saved file), History (Commits and their
 * diffs), Sandboxes (the live Sandbox map), Sessions (the DSH sessions bound to it) and Stats.
 */
import { useId, useState, type ReactNode } from 'react'
import clsx from 'clsx'
import { Button, IconPlusOutlineRegular, IconRefreshOutlineRegular, MenuItemButton, StateDot, Tooltip } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, HostError, type LayerfsApi } from '../api.ts'
import { ProjectFiles } from '../files/ProjectFiles.tsx'
import { ProjectHistory } from '../files/ProjectHistory.tsx'
import { ago, bytes, when } from '../files/format.ts'
import { EmptyState } from '../files/States.tsx'
import type { Translate } from '../files/types.ts'
import { errorCode, useQuery } from '../files/use-query.ts'
import { Card, Fact, Facts, LoadError, Loading, MoreMenu, Refusal, Section } from '../ui/index.tsx'
import { TabBar } from '../ui/TabBar.tsx'
import type { StorageDialog } from './dialogs.tsx'
import type { StorageT } from './locale.ts'
import { refusalLabels } from './parts.tsx'
import type { ProjectState, SessionRow, StoreRow } from './types.ts'
import { brokenWords, sessionsUnavailable, storeTip } from './model.ts'
import { StoreFixAction } from './StoreCard.tsx'
import css from './Storage.module.css'
import { ProjectStorage } from './ProjectStorage.tsx'
import { ProjectSandboxes, type ProjectMapShare } from './ProjectSandboxes.tsx'

type Tab = 'files' | 'history' | 'sandboxes' | 'sessions' | 'stats'

export interface ProjectPageProps {
  refreshKey?: number
  api: LayerfsApi
  t: StorageT
  /** The project key, `<store>/<project>`. */
  project: string
  /** Opens a dialog of the Storage page (delete, close a Sandbox). */
  onDialog: (dialog: StorageDialog) => void
  /** The project is gone: back to its Store. */
  onRemoved: () => void
  /** Opens the owning Store's page. */
  onOpenStore: (store: string) => void
  /** What the Sandboxes map needs of the conversation views. */
  map: ProjectMapShare
  /** The project's Store as the Storage page last read it: a broken one (no saved login…) is shown instead of failed reads. */
  store?: StoreRow | undefined
}

/**
 * The project's Store is broken (UI pass 2 F4: a PostgreSQL Store with no
 * saved login): its word, LayerFS's sentence as the tip and its fix (Save a
 * login…) as the one action, instead of a failed read with a code and Retry.
 */
function StoreBroken({ s, t }: { s: StoreRow, t: StorageT }): ReactNode {
  return (
    <div className={css.storeBroken} role="alert" data-store-broken={s.reason ?? ''}>
      <p className={css.storeBrokenWord} title={storeTip(s, t)}><StateDot state="error" />{brokenWords(s, t)}</p>
      <StoreFixAction store={s} t={t} />
    </div>
  )
}

export function ProjectPage({ refreshKey = 0, api, t, project, onDialog, onRemoved, onOpenStore, map, store }: ProjectPageProps): ReactNode {
  const name = project.slice(project.indexOf('/') + 1)
  const [tab, setTab] = useState<Tab>('files')
  // One prefix per page: several projects can be open at once.
  const ids = useId()
  const [localTick, setTick] = useState(0)
  const tick = localTick + refreshKey
  const [open, setOpen] = useState<{ branch?: string, pin?: string, path?: string, n: number }>({ n: 0 })
  const tt = t as Translate
  const tabs: { value: Tab, label: string }[] = [
    { value: 'files', label: t('tabFiles') },
    { value: 'history', label: t('tabHistory') },
    { value: 'sandboxes', label: t('tabSandboxes') },
    { value: 'sessions', label: t('tabSessions') },
    { value: 'stats', label: t('tabStats') },
  ]
  const remove = () => onDialog({
    kind: 'confirm', title: t('removeProjectTitle', { name }), body: t('removeProjectBody'), action: t('deleteAction'), name,
    run: () => api.call('DELETE', `/api/projects/${enc(project)}`).then(onRemoved),
  })
  return (
    <div className={css.projectPage}>
      <h1 className={css.srOnly}>{name}</h1>
      {/* the shared tab bar: labels never squeeze; on a phone the tabs scroll with fades at their cut edges and the actions take the next line */}
      <TabBar className={css.projectTabs} label={t('projectTabs')} active={tab} onSelect={(key) => setTab(key as Tab)}
        tabs={tabs.map((x) => ({ key: x.value, label: x.label, id: `${ids}-${x.value}`, controls: `${ids}-${x.value}-panel` }))}
        actions={(
          <>
            <Tooltip label={t('refreshProject')} side="bottom" portal delayMs={500}>
              <Button variant="ghost" size="sm" className={css.paneIconButton} aria-label={t('refreshProject')} icon={<IconRefreshOutlineRegular size={14} />} onClick={() => setTick((n) => n + 1)} />
            </Tooltip>
            <MoreMenu label={t('projectActions', { name })}>{(close) => <MenuItemButton danger onSelect={() => { close(); remove() }}>{t('removeProject')}</MenuItemButton>}</MoreMenu>
          </>
        )} />
      <div role="tabpanel" id={`${ids}-${tab}-panel`} aria-labelledby={`${ids}-${tab}`} className={clsx(css.tabPanel, (tab === 'files' || tab === 'history') && css.fillPanel)}>
        {(tab === 'files' || tab === 'history') && store?.state === 'broken' ? (
          <StoreBroken s={store} t={t} />
        ) : tab === 'files' ? (
          <ProjectFiles key={`${open.n}`} api={api} project={project} t={tt} tick={tick} initialBranch={open.branch} initialPin={open.pin} initialPath={open.path} />
        ) : tab === 'history' ? (
          <ProjectHistory api={api} project={project} t={tt} tick={tick}
            onOpenFile={(branch, path, pin) => { setOpen((o) => ({ branch, path, pin, n: o.n + 1 })); setTab('files') }} />
        ) : tab === 'sandboxes' ? (
          <ProjectSandboxes key={project} api={api} t={t} share={map} project={project} tick={tick} onDialog={onDialog} />
        ) : tab === 'sessions' ? (
          <Sessions api={api} t={t} project={project} tick={tick} store={store} />
        ) : (
          <Stats api={api} t={t} project={project} tick={tick} onOpenStore={onOpenStore} />
        )}
      </div>
    </div>
  )
}

const isUnknownOp = (e: unknown) => {
  const code = errorCode(e)
  return code === 'layerfs/unknown-op' || (e instanceof HostError && /unknown-op|no dsh-layerfs operation/.test(`${e.code} ${e.message}`))
}

/** A session's binding state in words; an unknown state shows as the host gave it. */
function sessionStateWords(state: string | null, t: StorageT): string {
  if (state === 'pending') return t('sessionStatePending')
  if (state === 'bound') return t('sessionStateBound')
  return state ?? '—'
}

function Sessions({ api, t, project, tick, store }: { api: LayerfsApi, t: StorageT, project: string, tick: number, store?: StoreRow | undefined }): ReactNode {
  const q = useQuery(async () => {
    const value = await api.op<SessionRow[] | { sessions: SessionRow[] }>('sessions.list', { project })
    return Array.isArray(value) ? value : value?.sessions ?? []
  }, [api, project], { refresh: [tick], cache: `project-page-sessions:${project}` })
  const [busy, setBusy] = useState<string | null>(null)
  const [failed, setFailed] = useState<unknown>(undefined)
  const now = Date.now()
  const act = (id: string, work: () => Promise<unknown>) => {
    setBusy(id)
    setFailed(undefined)
    work().then(() => q.reload(), setFailed).finally(() => setBusy(null))
  }
  if (q.error && isUnknownOp(q.error)) return <Card><EmptyState title={t('sessionsUnsupported')}>{t('sessionsUnsupportedBody')}</EmptyState></Card>
  if (q.error && !q.data) return <LoadError title={t('sessionsFailed')} error={q.error} labels={refusalLabels(t)} retryLabel={t('retry')} onRetry={q.reload} />
  if (!q.data) return <Loading label={t('loading')} />
  // a broken or blocked Store: its sessions are unavailable until the fix (owner Q-R3): the word, its tip and the one fix; no Open or New session
  const down = sessionsUnavailable(store)
  const create = down ? undefined : (
    <Button variant="primary" size="sm" icon={busy === 'new' ? <StateDot state="ongoing" size={13} /> : <IconPlusOutlineRegular size={13} />} disabled={busy !== null}
      onClick={() => act('new', () => api.openProjectSession(project))}>{t('sessionsNew')}</Button>
  )
  return (
    <Section title={t('sessions')} count={String(q.data.length)} actions={q.data.length ? create : undefined}>
      {down && store ? <StoreBroken s={store} t={t} /> : null}
      {failed !== undefined ? <Refusal error={failed} labels={refusalLabels(t)} /> : null}
      {q.data.length === 0 ? (
        <Card><EmptyState title={t('sessionsNone')} action={create}>{t('sessionsNoneBody')}</EmptyState></Card>
      ) : (
        <Card className={css.tableCard}>
          <table className={css.table}>
            <thead><tr><th>{t('colSession')}</th><th>{t('colState')}</th><th className={`${css.num} ${css.wide}`} title={t('colCheckpointsTitle')}>{t('colCheckpoints')}</th><th className={css.num}>{t('colUpdated')}</th><th><span className={css.srOnly}>{t('sessionsOpen')}</span></th></tr></thead>
            <tbody>
              {q.data.map((s) => (
                <tr key={s.sessionId}>
                  <td><span className={css.rowTitle}>{s.title || t('sessionUntitled')}</span></td>
                  <td className={css.muted}>{sessionStateWords(s.state, t)}</td>
                  <td className={`${css.num} ${css.wide} ${css.muted}`}>{s.checkpoints ?? '—'}</td>
                  <td className={`${css.num} ${css.muted}`} title={when(s.updatedAt)}>{s.updatedAt ? ago(s.updatedAt, now, t as Translate) : '—'}</td>
                  <td className={css.actionsCell}>
                    {down ? null : <Button variant="outline" size="sm" disabled={busy !== null} icon={busy === s.sessionId ? <StateDot state="ongoing" size={13} /> : undefined}
                      onClick={() => act(s.sessionId, () => api.op('sessions.open', { sessionId: s.sessionId }))}>{t('sessionsOpen')}</Button>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </Section>
  )
}

function Stats({ api, t, project, tick, onOpenStore }: { api: LayerfsApi, t: StorageT, project: string, tick: number, onOpenStore: (store: string) => void }): ReactNode {
  return <ProjectStorage key={project} api={api} t={t} project={project} tick={tick} onOpenStore={onOpenStore} />
}

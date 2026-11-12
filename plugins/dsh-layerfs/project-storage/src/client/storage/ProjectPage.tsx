/**
 * One project: Files (Branch, tree, saved file), History (Commits and their
 * diffs), Sandboxes, Sessions (the DSH sessions bound to it) and Stats.
 */
import { useState, type ReactNode } from 'react'
import clsx from 'clsx'
import { Button, IconPlusOutlineRegular, IconRefreshOutlineRegular, MenuItemButton, SegmentedTabs, StateDot, Tag, Tooltip } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, HostError, type LayerfsApi } from '../api.ts'
import { ProjectFiles } from '../files/ProjectFiles.tsx'
import { ProjectHistory } from '../files/ProjectHistory.tsx'
import { ago, bytes, when } from '../files/format.ts'
import { EmptyState } from '../files/States.tsx'
import type { Translate } from '../files/types.ts'
import { errorCode, useQuery } from '../files/use-query.ts'
import { Card, Fact, Facts, LoadError, Loading, MoreMenu, Refusal, Section, StateChip, type Tone } from '../ui/index.tsx'
import type { StorageDialog } from './dialogs.tsx'
import type { StorageT } from './locale.ts'
import { PaneBar } from './PaneBar.tsx'
import { refusalLabels } from './parts.tsx'
import type { ProjectState, SandboxInfo, SessionRow } from './types.ts'
import css from './Storage.module.css'
import { ProjectStorage } from './ProjectStorage.tsx'

type Tab = 'files' | 'history' | 'sandboxes' | 'sessions' | 'stats'

export interface ProjectPageProps {
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
}

export function ProjectPage({ api, t, project, onDialog, onRemoved, onOpenStore }: ProjectPageProps): ReactNode {
  const name = project.slice(project.indexOf('/') + 1)
  const [tab, setTab] = useState<Tab>('files')
  const [tick, setTick] = useState(0)
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
      <PaneBar title={name}
        actions={(
          <>
            <Tooltip label={t('refreshProject')} side="bottom" portal delayMs={500}>
              <Button variant="ghost" size="sm" className={css.paneIconButton} aria-label={t('refreshProject')} icon={<IconRefreshOutlineRegular size={14} />} onClick={() => setTick((n) => n + 1)} />
            </Tooltip>
            <MoreMenu label={t('storeActions', { name })}>{(close) => <MenuItemButton danger onSelect={() => { close(); remove() }}>{t('removeProject')}</MenuItemButton>}</MoreMenu>
          </>
        )}>
        <SegmentedTabs size="sm" className={css.tabList} label={t('projectTabs')} value={tab} onChange={setTab}
          items={tabs.map((x) => ({ value: x.value, label: x.label, id: `lfs-project-${x.value}`, panelId: `lfs-project-${x.value}-panel` })) as [never, ...never[]]} />
      </PaneBar>
      <div role="tabpanel" id={`lfs-project-${tab}-panel`} aria-labelledby={`lfs-project-${tab}`} className={clsx(css.tabPanel, (tab === 'files' || tab === 'history') && css.fillPanel)}>
        {tab === 'files' ? (
          <ProjectFiles key={`${open.n}`} api={api} project={project} t={tt} tick={tick} initialBranch={open.branch} initialPin={open.pin} initialPath={open.path} />
        ) : tab === 'history' ? (
          <ProjectHistory api={api} project={project} t={tt} tick={tick}
            onOpenFile={(branch, path, pin) => { setOpen((o) => ({ branch, path, pin, n: o.n + 1 })); setTab('files') }} />
        ) : tab === 'sandboxes' ? (
          <Sandboxes api={api} t={t} project={project} tick={tick} onDialog={onDialog} />
        ) : tab === 'sessions' ? (
          <Sessions api={api} t={t} project={project} tick={tick} />
        ) : (
          <Stats api={api} t={t} project={project} tick={tick} onOpenStore={onOpenStore} />
        )}
      </div>
    </div>
  )
}

const sandboxTone = (state: string): Tone => state === 'running' ? 'success' : state === 'creating' || state === 'closing' ? 'warning' : state === 'unresolved' ? 'danger' : 'neutral'

function Sandboxes({ api, t, project, tick, onDialog }: { api: LayerfsApi, t: StorageT, project: string, tick: number, onDialog: (d: StorageDialog) => void }): ReactNode {
  const [closed, setClosed] = useState(0)
  const q = useQuery(() => api.call<SandboxInfo[]>('GET', `/api/projects/${enc(project)}/sandboxes`), [api, project, tick, closed])
  if (q.error && !q.data) return <LoadError title={t('projectFailed')} error={q.error} labels={refusalLabels(t)} retryLabel={t('retry')} onRetry={q.reload} />
  if (!q.data) return <Loading label={t('loading')} />
  if (!q.data.length) return <Card><EmptyState title={t('sandboxesNone')}>{t('sandboxesNoneBody')}</EmptyState></Card>
  return (
    <Card className={css.tableCard}>
      <ul className={css.rows}>
        {q.data.map((b) => (
          <li key={b.id ?? b.name} className={css.row}>
            <span className={css.rowMain}>
              <span className={css.rowTitle}>{b.name}</span>
              <span className={css.rowSub}>
                {[b.machine, b.runtime ?? b.platform, b.unread ? t('sandboxWorkspacesUnread') : t('sandboxWorkspaces', { n: b.workspaces.length })].filter(Boolean).join(' · ')}
              </span>
              {b.workspaces.length ? (
                <span className={css.wsList}>{b.workspaces.map((w) => <Tag key={w.ws} tone="outline">{`${w.path} · ${w.branch}`}</Tag>)}</span>
              ) : null}
              {b.refusal ? <span className={css.rowError}>{b.refusal.code}: {b.refusal.message}</span> : null}
            </span>
            <StateChip tone={sandboxTone(b.state)} label={b.state} live={b.state === 'creating' || b.state === 'closing'} op={b.op ? `${b.op.name} · ${b.op.phase}` : undefined} />
            <Button variant="outline" size="sm" disabled={b.state === 'gone' || b.state === 'closing'} onClick={() => onDialog({
              kind: 'confirm', title: t('closeSandboxConfirm', { name: b.name }), body: t('closeSandboxBody'), action: t('closeSandbox'),
              run: () => api.call('DELETE', `/api/sandboxes/${enc(b.name)}`).then(() => setClosed((n) => n + 1)),
            })}>{t('closeSandbox')}</Button>
          </li>
        ))}
      </ul>
    </Card>
  )
}

const isUnknownOp = (e: unknown) => {
  const code = errorCode(e)
  return code === 'layerfs/unknown-op' || (e instanceof HostError && /unknown-op|no dsh-layerfs operation/.test(`${e.code} ${e.message}`))
}

function Sessions({ api, t, project, tick }: { api: LayerfsApi, t: StorageT, project: string, tick: number }): ReactNode {
  const q = useQuery(async () => {
    const value = await api.op<SessionRow[] | { sessions: SessionRow[] }>('sessions.list', { project })
    return Array.isArray(value) ? value : value?.sessions ?? []
  }, [api, project, tick])
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
  const create = (
    <Button variant="primary" size="sm" icon={busy === 'new' ? <StateDot state="ongoing" size={13} /> : <IconPlusOutlineRegular size={13} />} disabled={busy !== null}
      onClick={() => act('new', () => api.openProjectSession(project))}>{t('sessionsNew')}</Button>
  )
  return (
    <Section title={t('sessions')} count={String(q.data.length)} actions={q.data.length ? create : undefined}>
      {failed !== undefined ? <Refusal error={failed} labels={refusalLabels(t)} /> : null}
      {q.data.length === 0 ? (
        <Card><EmptyState title={t('sessionsNone')} action={create}>{t('sessionsNoneBody')}</EmptyState></Card>
      ) : (
        <Card className={css.tableCard}>
          <table className={css.table}>
            <thead><tr><th>{t('colSession')}</th><th>{t('colState')}</th><th className={`${css.num} ${css.wide}`}>{t('colCheckpoints')}</th><th className={css.num}>{t('colUpdated')}</th><th><span className={css.srOnly}>{t('sessionsOpen')}</span></th></tr></thead>
            <tbody>
              {q.data.map((s) => (
                <tr key={s.sessionId}>
                  <td><span className={css.rowTitle}>{s.title || t('sessionUntitled')}</span></td>
                  <td className={css.muted}>{s.state ?? '—'}</td>
                  <td className={`${css.num} ${css.wide} ${css.muted}`}>{s.checkpoints ?? '—'}</td>
                  <td className={`${css.num} ${css.muted}`} title={when(s.updatedAt)}>{s.updatedAt ? ago(s.updatedAt, now, t as Translate) : '—'}</td>
                  <td className={css.actionsCell}>
                    <Button variant="outline" size="sm" disabled={busy !== null} icon={busy === s.sessionId ? <StateDot state="ongoing" size={13} /> : undefined}
                      onClick={() => act(s.sessionId, () => api.op('sessions.open', { sessionId: s.sessionId }))}>{t('sessionsOpen')}</Button>
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

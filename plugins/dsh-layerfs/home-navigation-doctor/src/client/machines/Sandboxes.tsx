/**
 * What a Sandbox's tab and the machine pages share: reading a Sandbox
 * (sandbox.get), its menu (Delete is sandbox.remove; Force delete is a separate
 * explicit action), and the New Sandbox dialog that creates one on a machine
 * from an eligible project.
 */
import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { Button, IconChevronRightOutlineRegular, IconCloseOutlineRegular, IconRightUpOutlineRegular, Input, MenuItemButton, Modal } from '@deepseek-ai/dsh-client-ui-primitives'
import { Card, Field, MenuSelect, MoreMenu, Notice, Refusal, StateChip, type Choice } from '../ui/index.tsx'
import { useMachinesContext } from './context.tsx'
import { SandboxGlyph } from './icons.tsx'
import { ImageMenu, defaultPick, findPick, imagePlan, useSandboxImages } from './ImageMenu.tsx'
import { FreeSpaceDialog } from './MachineImages.tsx'
import {
  BUILT_IN, SANDBOX_NAME, clearingCall, opWords, projectRef, shortImage, stateLook, supports,
  type MachineInfo, type MachineSandbox, type ProjectSummary, type SandboxInfo,
} from './model.ts'
import { useQuery } from '../ui/use-query.ts'
import css from './machines.module.css'

const POLL_MS = 15_000
/** While LayerFS says a command runs in one of its Workspaces. */
const RUNNING_POLL_MS = 5_000
/** How long after a command's start its record is read. */
const STARTED_MS = 800

/**
 * sandbox.get at mount, after its state or count changes, on this host's
 * `sandbox-command` events for it, and while it runs and the window is
 * visible every 15 s, or every 5 s only while a command runs in one of its
 * Workspaces (sandbox.get dials the daemon: layerfs-project#179 B.2).
 */
export function useSandboxRead(name: string, state: string | undefined, count: number | null | undefined, refreshKey = 0): { data: SandboxInfo | null, error: unknown, loading: boolean, reload: () => void } {
  const { readSandbox, api } = useMachinesContext()
  const query = useQuery(() => readSandbox(name), [api, name], { cache: `sandbox:${name}`, refresh: [state, count, refreshKey] })
  const { data, reload: read } = query
  // a command of this host started or ended here: what runs has changed
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined
    const off = api.onEvent('sandbox-command', (event) => {
      const data = event.data as { sandbox?: string, state?: string } | undefined
      if (data?.sandbox !== name || document.visibilityState !== 'visible') return
      clearTimeout(timer)
      // a start is read a moment later, once LayerFS has written the command's record
      timer = setTimeout(() => void read(), data.state === 'ready' ? STARTED_MS : 500)
    })
    return () => { clearTimeout(timer); off() }
  }, [api, name, read])
  const busy = !!data?.workspaces.some((w) => (w.running?.length ?? 0) > 0)
  useEffect(() => {
    if (state !== 'ready') return
    const timer = setInterval(() => { if (document.visibilityState === 'visible') void read() }, busy ? RUNNING_POLL_MS : POLL_MS)
    return () => clearInterval(timer)
  }, [read, state, busy])
  return { ...query, data: data ?? null }
}

/** Normal close reconciles unknown results; explicit force bypasses worker reads. */
export function useClose(name: string): { busy: boolean, error: unknown, unknown: string[] | null, close: (force: boolean) => Promise<boolean>, forceAllowed: boolean, uncertain: boolean } {
  const { closeSandbox, readSandbox } = useMachinesContext()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<unknown>(null)
  const [unknown, setUnknown] = useState<string[] | null>(null)
  const refusal = error as { code?: string, detail?: { running?: unknown[] } } | null
  const close = async (force: boolean): Promise<boolean> => {
    setBusy(true)
    try {
      if (!force && refusal?.code === 'Unknown') { await readSandbox(name); setError(null); return false }
      setError(null)
      setUnknown(await closeSandbox(name, force))
      return true
    } catch (e) {
      setError(e)
      return false
    } finally {
      setBusy(false)
    }
  }
  const forceAllowed = refusal?.code === 'InUse' || (refusal?.code === 'Busy' && !!refusal.detail?.running?.length)
  return { busy, error, unknown, close, forceAllowed, uncertain: refusal?.code === 'Unknown' }
}

/** Normal Delete, Read after uncertainty, and explicit Force delete. */
export function SandboxMenu({ name, op, closing, onOpen, onDeleted }: { name: string, op: MachineSandbox['op'], closing: ReturnType<typeof useClose>, onOpen?: () => void, onDeleted?: () => void }): ReactNode {
  const { t } = useMachinesContext()
  const { busy, close, uncertain } = closing
  const [confirm, setConfirm] = useState<'normal' | 'force' | null>(null)
  return (
    <>
      <MoreMenu label={t('moreForSandbox', { name })} disabled={busy}>
        {(dismiss) => (
          <>
            {onOpen && <MenuItemButton icon={<IconChevronRightOutlineRegular size={14} />} onSelect={() => { dismiss(); onOpen() }}>{t('open')}</MenuItemButton>}
            <MenuItemButton danger separatorBefore={!!onOpen} icon={<IconCloseOutlineRegular size={14} />} disabled={busy || (!!op && !op.uncertain)}
              onSelect={() => { dismiss(); if (uncertain) void close(false); else setConfirm('normal') }}>
              {uncertain ? t('readSandbox') : t('deleteSandbox')}
            </MenuItemButton>
            <MenuItemButton danger separatorBefore icon={<IconCloseOutlineRegular size={14} />} disabled={busy}
              onSelect={() => { dismiss(); setConfirm('force') }}>
              {t('forceDeleteSandbox')}
            </MenuItemButton>
          </>
        )}
      </MoreMenu>
      {confirm && (
        <Modal open onClose={() => { if (!busy) setConfirm(null) }} title={t(confirm === 'force' ? 'forceDeleteTitle' : 'deleteTitle', { name })} closeLabel={t('close')} className={css.dialog}
          description={t(confirm === 'force' ? 'forceDeleteBody' : 'deleteBody', { name })}
          footer={(
            <>
              <Button variant="outline" disabled={busy} onClick={() => setConfirm(null)}>{t('cancel')}</Button>
              <Button variant="primary" className={css.danger} disabled={busy}
                onClick={() => void close(confirm === 'force').then((ok) => { setConfirm(null); if (ok) onDeleted?.() })}>
                {busy ? t('deleting') : t(confirm === 'force' ? 'forceDeleteAction' : 'deleteAction')}
              </Button>
            </>
          )} />
      )}
    </>
  )
}

/**
 * New Sandbox on this machine: SQLite projects whose Store is here, or
 * PostgreSQL projects when the machine supports them; the Image menu; and a
 * button that says what happens (Create, or Start and create on a stopped
 * machine). The host creates in the background: the dialog closes on
 * admission and the machine's Sandboxes list shows the create's row.
 */
export function NewSandboxDialog({ machine, taken: existing, image, onClose }: { machine: MachineInfo, taken: string[], /** An image Id to preselect (the Images tab's New Sandbox on a stored image). */ image?: string, onClose: () => void }): ReactNode {
  const { t, tw, projects: readProjects, createSandbox, readSandbox, refusal, openStorageTab, useCreations } = useMachinesContext()
  const creations = useCreations((all) => all)
  // names a create still running (or failed, until dismissed) holds
  const taken = [...existing, ...Object.values(creations).filter((c) => c.phase !== 'done').map((c) => c.input.name)]
  const images = useSandboxImages(machine)
  const answer = images.data ?? null
  // macOS native runs Sandboxes on the Mac itself: no Image row (its backend says so before the answer does)
  const host = answer ? answer.mode === 'host' : machine.backend === 'native'
  // undefined: not chosen, the default (D7) follows the answer; null: LayerFS's default
  const [chosen, setChosen] = useState<string | null | undefined>(image)
  const pick = host ? null : chosen === undefined ? defaultPick(answer) : chosen === null ? null : findPick(answer, chosen) ?? defaultPick(answer)
  const plan = imagePlan(answer, pick, machine.state)
  const imagesPending = !host && !answer && images.loading
  const imagesFailed = !host && !answer && !images.loading && images.error != null
  const [projects, setProjects] = useState<{ data: ProjectSummary[] | null, error: unknown, loading: boolean }>({ data: null, error: null, loading: true })
  const loadProjects = useCallback(() => {
    setProjects({ data: null, error: null, loading: true })
    readProjects().then((data) => setProjects({ data, error: null, loading: false }), (error) => setProjects({ data: null, error, loading: false }))
  }, [readProjects])
  useEffect(() => { loadProjects() }, [loadProjects])
  const eligible = (projects.data ?? []).filter((p) => (p.kind === 'sqlite' ? p.machine === machine.name && supports(machine, 'sqlite') !== false : p.kind === 'postgresql' && supports(machine, 'postgresql_sandbox') === true))
  const [project, setProject] = useState('')
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<unknown>(null)
  const [observation, setObservation] = useState('')
  const uncertain = (error as { code?: string } | null)?.code === 'Unknown'
  const choose = (key: string) => {
    setProject(key)
    const stem = (eligible.find((p) => p.key === key)?.name ?? 'sandbox').replace(/[^a-z0-9_.-]/g, '-').replace(/^[^a-z0-9]+/, '').slice(0, 50) || 'sandbox'
    for (let n = 1; ; n++) if (!taken.includes(`${stem}-${n}`)) { setName(`${stem}-${n}`); break }
    setError(null)
  }
  const valid = SANDBOX_NAME.test(name) && !BUILT_IN.has(name) && !taken.includes(name)
  const ready = !busy && !uncertain && valid && !imagesPending && eligible.some((p) => p.key === project)
  // Free space (a Docker pick whose copy does not fit): a dialog on top of this one, so the form is kept (D14)
  const [freeing, setFreeing] = useState(false)
  const need = pick?.from === 'docker' && answer?.docker?.available ? answer.docker.images.find((r) => r.id === pick.id)?.need ?? null : null
  const make = async () => {
    if (!ready) return
    if (plan.label === 'free') { setFreeing(true); return }
    setBusy(true)
    setError(null)
    try {
      // always the picked row's id; none when the images could not be read (LayerFS's default)
      await createSandbox(project, { name, machine: machine.name, ...(pick ? { image: pick.id } : {}), ...(plan.label === 'start' ? { start: true } : {}) })
      onClose()
    } catch (e) { setError(e) } finally { setBusy(false) }
  }
  return (
    <Modal open onClose={() => { if (!busy) onClose() }} title={t('newSandbox')} description={t('newSandboxBody', { machine: machine.name })} closeLabel={t('close')} className={css.dialog} contentClassName={css.dialogContent}
      footer={(
        <>
          <Button variant="outline" disabled={busy} onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" disabled={!ready} onClick={() => void make()}>
            {busy ? t('creating') : t(plan.label === 'start' ? 'startAndCreate' : plan.label === 'free' ? 'freeSpace' : 'create')}
          </Button>
        </>
      )}>
      <form className={css.form} onSubmit={(e) => { e.preventDefault(); void make() }}>
        {projects.error != null
          ? (
            <>
              <Refusal error={projects.error} labels={refusal} className={css.errorText} />
              <div><Button type="button" size="sm" variant="outline" onClick={loadProjects}>{t('retryProjects')}</Button></div>
            </>
          )
          : projects.loading
            ? <p role="status" className={css.muted}>{t('projectsLoading')}</p>
            : eligible.length === 0
              ? (
                <>
                  <p role="status" className={css.muted}>{t('projectsNone', { machine: machine.name })}</p>
                  <div><Button type="button" size="sm" variant="outline" icon={<IconRightUpOutlineRegular size={13} />} onClick={() => { onClose(); openStorageTab('overview') }}>{t('openStorage')}</Button></div>
                </>
              )
              : (
                <>
                  <Field label={t('project')}>
                    <MenuSelect label={t('project')} value={project} placeholder={t('chooseProject')} disabled={busy || uncertain}
                      options={eligible.map((p) => ({ value: p.key, label: p.key }))} onChange={choose} />
                  </Field>
                  <Field label={t('name')} hint={t('sandboxNameHint')} error={name && !valid ? t('errSandboxName') : null}>
                    <Input value={name} onChange={(e) => setName(e.target.value)} disabled={busy || uncertain} spellCheck={false} maxLength={63} />
                  </Field>
                  {host
                    ? <p className={css.muted}>{t('nativeOwnSystem')}</p>
                    : (
                      <Field label={t('image')} hint={plan.note && !imagesFailed ? tw(plan.note) : undefined}>
                        <ImageMenu machine={machine} images={images} value={pick?.id ?? null} disabled={busy || uncertain}
                          onChange={(next) => setChosen(next?.id ?? null)} />
                      </Field>
                    )}
                  {imagesFailed && (
                    <>
                      <Refusal error={images.error} labels={refusal} compact className={css.errorText} />
                      <div><Button type="button" size="sm" variant="ghost" onClick={images.reload}>{t('imagesRetry')}</Button></div>
                    </>
                  )}
                  <p className={css.muted}>{t('machineIs', { machine: machine.name })}</p>
                </>
              )}
        {error != null && <Refusal error={error} labels={refusal} className={css.errorText} />}
        {uncertain && (
          <Notice>
            <p>{t('createUncertain')}</p>
            <div>
              <Button type="button" size="sm" variant="outline" disabled={busy} onClick={async () => {
                setBusy(true)
                try { const current = await readSandbox(name); setObservation(t('sandboxStateIs', { state: current.state ?? t('stateUnknown') })) } catch (e) { setObservation(e instanceof Error ? e.message : String(e)) } finally { setBusy(false) }
              }}>{t('readSandbox')}</Button>
            </div>
            {observation && <p role="status">{observation}</p>}
          </Notice>
        )}
      </form>
      {freeing && <FreeSpaceDialog machine={machine} need={need} onClose={(freed) => { setFreeing(false); if (freed) images.reload() }} />}
    </Modal>
  )
}

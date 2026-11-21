/**
 * Storage's dialogs: one small form per action. A PostgreSQL password or a
 * bucket's keys are either a reference (env:… or file:/path) or typed here;
 * a typed secret goes to the host, which keeps it in an owner-only file and
 * gives LayerFS that file's reference.
 */
import { useState, type ReactNode } from 'react'
import clsx from 'clsx'
import { Button, Checkbox, Input, SegmentedControl, StateDot } from '@deepseek-ai/dsh-client-ui-primitives'
import { enc, type LayerfsApi } from '../api.ts'
import { bytes } from '../files/format.ts'
import { useQuery } from '../files/use-query.ts'
import { Contained, Field, MenuSelect, Notice, Refusal, usePressGuard } from '../ui/index.tsx'
import type { StorageT } from './locale.ts'
import { backupStateOf, bucketParts, canMove, inHome, needsLogin, pgServerLogin, storeStateWords, type BackupState, type Reconnect } from './model.ts'
import { BucketProviderGlyph } from '../machines/icons.tsx'
import { LocationPicker, placeName, StartFirst } from '../projects/setup/MachineTiles.tsx'
import { defaultMachine, machinesFor, stoppedOf } from '../projects/setup/model.ts'
import { placeStore, startMachine, waitJob } from '../projects/setup/run.ts'
import { Signal } from './Signal.tsx'
import { FormModal, locationWords, refusalLabels, StoreGlyph, TextField, TileChoice } from './parts.tsx'
import type { MachinesView, PgRotation, ProjectInitJob, StorageJob, StorageView, StoreRow } from './types.ts'
import { ConnectDatabaseDialog } from './ConnectDatabase.tsx'
import { PG_URL_LOGIN, PgPasswordFields, usePgAuths, usePgPassword } from './pg-fields.tsx'
import { parsePgUrl } from './machine-stores.ts'
import { BackUpDialog, BackupsDialog } from './BackupDialogs.tsx'
import css from './Storage.module.css'

const NAME = /^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/

/** Which dialog is open, and on what. */
export type StorageDialog =
  | { kind: 'new-store', machine?: string | undefined }
  /**
   * Connect a database (ConnectDatabase.tsx), the one PostgreSQL connect dialog. `machine`: a machine page's
   * (new databases run on it); `server`: open on a new database on that server (`host:port`); `check`: open
   * on the check of a Store from a machine (where it runs unless named).
   */
  | { kind: 'connect', machine?: string, server?: string, check?: { store: string, machine?: string } }
  /** Edit a PostgreSQL Store's connection, or (`machine`) that machine's own. */
  | { kind: 'pg-edit', store: string, machine?: string }
  /** A new password for a saved PostgreSQL login: its Stores named, each checked again after (spec v23). */
  | { kind: 'pg-rotate', auth: string }
  | { kind: 'add-project', store: string }
  | { kind: 'push', store: StoreRow }
  | { kind: 'to-machine', machine: string }
  | { kind: 'move', store: StoreRow }
  /** `known`: the buckets when Add a bucket left Back up, so it comes back on the new one; `pick`: the bucket it starts on. */
  | { kind: 'backup', store: StoreRow, known?: string[], pick?: string }
  | { kind: 'backups', store: string }
  | { kind: 'restore', bucket: string, file: string, store?: string | null }
  /** `then`: the dialog Add a bucket returns to, closed or saved; `again`: Connect again… on a bucket whose key was refused (its name, and its URL when LayerFS's fix names it). */
  | { kind: 'bucket', then?: StorageDialog, again?: Reconnect }
  /** `plain`: the action removes nothing, so its button is not the danger one (Compact). */
  /** `steps`: what the confirm runs, listed in order (a fix chain); `cancelled`: it was closed without confirming. */
  | { kind: 'confirm', title: string, body: string, action: string, name?: string, force?: ConfirmForce, plain?: boolean, steps?: string[], cancelled?: () => void, run: (force: boolean) => Promise<unknown> }

/** An optional Force checkbox on a confirm: checked, the action runs with force and reads `action`. */
export interface ConfirmForce { label: string, action: string }

interface DialogsProps {
  api: LayerfsApi
  t: StorageT
  dialog: StorageDialog | null
  view: StorageView | undefined
  onClose: () => void
  /** The view changed: read it again. */
  done: () => void
  /** Swaps this dialog for another (Add Store's New Store and Connect a database). */
  onDialog?: (dialog: StorageDialog) => void
}

/** The open dialog, if any. */
export function StorageDialogs(props: DialogsProps): ReactNode {
  // a press inside the dialog stays there: it never reaches the page row or card under it through the portal (UI pass 3 F6)
  return props.dialog ? <Contained><OpenDialog {...props} /></Contained> : null
}

function OpenDialog({ api, t, dialog, view, onClose, done, onDialog }: DialogsProps): ReactNode {
  if (!dialog) return null
  const after = <T,>(p: Promise<T>) => p.then((v) => { done(); return v })
  switch (dialog.kind) {
    case 'new-store': return <NewStoreDialog api={api} t={t} view={view} opened={dialog.machine} onClose={onClose} done={done} onDialog={onDialog} />
    case 'connect': return <ConnectDatabaseDialog api={api} t={t} view={view} machine={dialog.machine} server={dialog.server} check={dialog.check} onClose={onClose} done={done} />
    case 'pg-edit': return <EditConnectionDialog api={api} t={t} view={view} store={dialog.store} machine={dialog.machine} onClose={onClose} done={done} />
    case 'pg-rotate': return <RotateLoginDialog api={api} t={t} view={view} auth={dialog.auth} onClose={onClose} done={done} />
    case 'add-project': return <NewProjectDialog api={api} t={t} store={dialog.store} onClose={onClose} done={done} />
    case 'push':
    case 'move': return <PlaceDialog api={api} t={t} kind={dialog.kind} store={dialog.store} onClose={onClose} done={done} />
    case 'to-machine': return <ToMachineDialog api={api} t={t} view={view} machine={dialog.machine} onClose={onClose} done={done} onDialog={onDialog} />
    case 'backup': return <BackUpDialog api={api} t={t} store={dialog.store} known={dialog.known} pick={dialog.pick} view={view} onClose={onClose} done={done} onDialog={onDialog} />
    case 'backups': return <BackupsDialog api={api} t={t} store={dialog.store} view={view} onClose={onClose} done={done} onDialog={onDialog} />
    case 'restore': return <RestoreDialog api={api} t={t} dialog={dialog} view={view} onClose={onClose} done={done} />
    case 'bucket': return <BucketDialog api={api} t={t} again={dialog.again} onClose={dialog.then && onDialog ? () => onDialog(dialog.then!) : onClose} done={done} />
    case 'confirm': return <ConfirmDialog t={t} title={dialog.title} body={dialog.body} action={dialog.action} name={dialog.name} force={dialog.force} plain={dialog.plain} steps={dialog.steps}
      onClose={() => { dialog.cancelled?.(); onClose() }} run={(force) => after(dialog.run(force))} />
  }
}

// ---------- New Store ----------

type NewPane = 'empty' | 'restore'

/** A new SQLite Store, empty or restored from a bucket. PostgreSQL goes on to Connect a database. */
function NewStoreDialog({ api, t, view, opened, onClose, done, onDialog }: {
  api: LayerfsApi, t: StorageT, view: StorageView | undefined, onClose: () => void, done: () => void
  /** The machine whose page opened the dialog: the default place when it runs and is not broken. */
  opened?: string | undefined
  onDialog?: ((dialog: StorageDialog) => void) | undefined
}): ReactNode {
  const [pane, setPane] = useState<NewPane>('empty')
  const [name, setName] = useState('')
  const buckets = view?.buckets ?? []
  const [bucket, setBucket] = useState(buckets[0]?.name ?? '')
  const [file, setFile] = useState('')
  const [listing, setListing] = useState(false)
  const [listError, setListError] = useState<unknown>(undefined)
  // Where the new Store lives. LayerFS makes it in the home; another machine is a move after.
  const machines = machinesFor(view, 'sqlite')
  // never a broken machine or one Create would start: such a machine is the person's pick, its start named (StartFirst)
  const fallback = defaultMachine(machines, undefined, opened)
  const [placeChoice, setPlace] = useState<string | null>(null)
  const place = placeChoice ?? fallback
  const words = locationWords(t, view)
  const where = placeName(place, words)
  /** A Store this dialog made whose move failed: a retry only finishes that. */
  const [made, setMade] = useState<string | null>(null)
  const [stage, setStage] = useState<string | undefined>(undefined)
  const chosen = buckets.find((b) => b.name === bucket)
  const files = chosen?.files ? [...new Set(chosen.files.map((f) => f.file))].map((f) => ({ file: f, bytes: chosen.files!.find((x) => x.file === f)?.bytes ?? null, stores: chosen.files!.filter((x) => x.file === f && x.store).map((x) => x.store!) })) : null
  const taken = !!name.trim() && name.trim() !== made && !!view?.stores.some((s) => s.name === name.trim())
  const list = async () => {
    setListing(true)
    setListError(undefined)
    try { await api.call('GET', `/api/buckets/${enc(bucket)}/files`) } catch (e) { setListError(e) } finally { setListing(false); done() }
  }
  const choose = (f: string) => {
    setFile(f)
    const base = f.replace(/\.[^.]+$/, '')
    setName(view?.stores.some((s) => s.name === base) ? `${base}-restored` : base)
  }
  const nameOk = NAME.test(name.trim()) && !taken
  const missing = [
    !name.trim() && t('name'),
    pane === 'restore' && !bucket && t('bucket'),
    pane === 'restore' && !file && t('storeFile'),
  ].filter(Boolean) as string[]
  /** Make the Store in the home, then (another machine) move the Store there; a stopped machine is started first by its own Start, never here. */
  const placed = async (make: () => Promise<unknown>, making: string) => {
    const store = name.trim()
    if (made !== store) {
      setStage(making)
      const result = await make()
      if (place === 'native') return
      // a restore is a job: the Store is there once it ends
      if (result && typeof result === 'object' && 'id' in result && 'ended' in result) await waitJob(api, result as StorageJob)
      setMade(store)
      done()
    }
    setStage(t('nsPlacing', { store, machine: where }))
    await placeStore(api, store, place, true)
    setMade(null)
  }
  const moveOnly = made !== null && made === name.trim() && place !== 'native'
  const form = {
    empty: {
      action: moveOnly ? t('nsMoveTo', { machine: where }) : t('createStore'), description: t('newStoreEmptyBody'),
      submit: () => placed(() => api.call('POST', '/api/stores', { name: name.trim() }), t('nsCreating', { store: name.trim() })),
    },
    restore: {
      action: moveOnly ? t('nsMoveTo', { machine: where }) : t('restoreStore'), description: t('newStoreRestoreBody'),
      submit: () => placed(() => api.call('POST', `/api/buckets/${enc(bucket)}/restore`, { store: name.trim(), file }), t('nsRestoring', { store: name.trim() })),
    },
  }[pane]
  // the machine the Store goes to, when it is not running: started only by its own Start below
  const stopped = stoppedOf([place], view)
  // The name's rule shows only once a typed name breaks it.
  const nameError = taken ? t('nameTaken') : name.trim() && !NAME.test(name.trim()) ? t('storeNameHint') : null
  return (
    <FormModal open onClose={onClose} t={t} title={t('newStore')} description={form.description} action={form.action}
      valid={!missing.length && nameOk && stopped.length === 0} hint={missing.length ? t('stillNeeded', { what: missing.join(', ') }) : undefined}
      busyHint={stage}
      submit={() => form.submit().then(done)}>
      {/* a PostgreSQL Store is connected, not made: its tile opens Connect a database */}
      {onDialog ? (
        <TileChoice label={t('kindOfStore')} value="sqlite" onChange={(v) => { if (v === 'postgresql') { onClose(); onDialog({ kind: 'connect' }) } }}
          options={[{ value: 'sqlite', label: t('sqlite'), icon: <StoreGlyph kind="sqlite" /> }, { value: 'postgresql', label: t('postgresql'), icon: <StoreGlyph kind="postgresql" /> }]} />
      ) : null}
      <SegmentedControl<NewPane> id="lfs-new-store-sqlite" className={css.selfStart} value={pane} onChange={setPane} label={t('sqlite')}
        options={[{ value: 'empty', label: t('storeEmpty') }, { value: 'restore', label: t('storeRestore') }]} />
      {pane === 'restore' ? (
        buckets.length === 0 ? <Notice tone="warning">{t('noBucketForRestore')}</Notice> : (
          <>
            <Field label={t('bucket')}>
              <div className={css.inline}>
                <MenuSelect value={bucket} label={t('bucket')} placeholder={t('chooseBucket')} className={css.grow}
                  options={buckets.map((b) => ({ value: b.name, label: b.name }))} onChange={(v) => { setBucket(v); setFile('') }} />
                <Button variant="outline" size="sm" disabled={!bucket || listing} onClick={() => { void list() }}
                  icon={listing ? <StateDot state="ongoing" size={13} /> : undefined}>{t('listBucket')}</Button>
              </div>
            </Field>
            {listError !== undefined ? <Refusal error={listError} labels={refusalLabels(t)} compact /> : null}
            <div className={css.choiceList} role="radiogroup" aria-label={t('storeFile')}>
              {!files?.length ? <p className={css.choiceEmpty}>{t('noFilesListed')}</p> : files.map((f) => (
                <button key={f.file} type="button" role="radio" aria-checked={file === f.file} className={css.choiceRow} data-checked={file === f.file || undefined} onClick={() => choose(f.file)}>
                  <span className={css.radio} aria-hidden="true" />
                  <span className={css.choiceMain}>{f.file}</span>
                  {f.stores.length ? <span className={css.choiceMeta}>{f.stores.join(', ')}</span> : null}
                  <span className={css.choiceMeta}>{bytes(f.bytes)}</span>
                </button>
              ))}
            </div>
          </>
        )
      ) : null}
      <TextField label={pane === 'restore' ? t('newStoreName') : t('name')} value={name} onChange={setName} mono autoFocus={pane !== 'restore'} placeholder="work" error={nameError} />
      <Field label={t('nsLocation')} hint={t('nsLocationHint')} group>
        <LocationPicker machines={machines} value={place} onChange={setPlace} words={words} label={t('nsLocation')} fallback={fallback} compact />
      </Field>
      {/* creating starts no machine: a stopped one has its own Start here */}
      <StartFirst names={stopped} start={(m) => startMachine(api, m)} onStarted={done}
        words={{ stopped: (machine) => t('nsStoppedFirst', { machine }), start: (machine) => t('startMachine', { machine }), refused: (machine) => t('nsStartRefused', { machine }) }} />
      {moveOnly ? <p className={css.formNote}>{t('nsMade', { store: made, machine: where })}</p> : null}
    </FormModal>
  )
}

// ---------- PostgreSQL: edit a connection, choose where it runs ----------

/**
 * Edit a PostgreSQL Store's connection: its address (store.connect), its
 * login (another saved login, or a new password for the current one:
 * store.update / store.save-auth) and its synchronous_commit. With
 * `machine`, that machine's own address instead (store.connect --machine),
 * which LayerFS checks from the machine before it saves anything. Only what
 * changed is sent. A refusal stays under the form. A Store that names no
 * saved login gets the save-login form instead ({@link SaveLoginDialog}).
 */
function EditConnectionDialog(props: {
  api: LayerfsApi, t: StorageT, view: StorageView | undefined, store: string, machine?: string | undefined, onClose: () => void, done: () => void
}): ReactNode {
  const s = props.view?.stores.find((x) => x.name === props.store)
  if (s && !props.machine && needsLogin(s)) return <SaveLoginDialog api={props.api} t={props.t} store={s} onClose={props.onClose} done={props.done} />
  return <ConnectionDialog {...props} />
}

function ConnectionDialog({ api, t, view, store, machine, onClose, done }: {
  api: LayerfsApi, t: StorageT, view: StorageView | undefined, store: string, machine?: string | undefined, onClose: () => void, done: () => void
}): ReactNode {
  const s = view?.stores.find((x) => x.name === store)
  const own = machine ? s?.ownConnections?.find((c) => c.machine === machine) : undefined
  const before = machine ? own?.url ?? '' : s?.postgresql?.url ?? ''
  const [url, setUrl] = useState(before)
  // the current login is shown picked; a new password rotates it (its user is the login's), another login replaces it
  const authBefore = s?.postgresql?.auth ?? ''
  const auths = usePgAuths(api)
  const pw = usePgPassword(authBefore ? 'saved' : 'entered', authBefore)
  const syncBefore = s?.postgresql?.synchronousCommit ?? ''
  const [sync, setSync] = useState(syncBefore)
  if (!s || !view) return null
  const place = machine === 'native' ? t('layerfsHome') : machine
  const urlChanged = url.trim() !== before && !!url.trim()
  const loginUser = pw.user.trim() || auths.find((a) => a.auth === authBefore)?.user || ''
  const pwChanged = !machine && (pw.how === 'entered' ? !!pw.value && !!loginUser : !pw.missing && !(pw.how === 'saved' && pw.auth === authBefore))
  const syncChanged = !machine && !!sync && sync !== syncBefore
  const urlError = PG_URL_LOGIN.test(url.trim()) ? t('pgPasswordInUrl') : null
  const login = () => (pw.how === 'entered' ? { user: loginUser, passwordValue: pw.value } : pw.body())
  const body = () => ({
    ...(machine ? { machine } : {}), ...(urlChanged ? { url: url.trim() } : {}), ...(pwChanged ? login() : {}), ...(syncChanged ? { synchronousCommit: sync } : {}),
  })
  return (
    <FormModal open onClose={onClose} t={t} title={machine ? t('pgEditOwnTitle', { store, machine: place ?? '' }) : t('pgEditTitle', { store })}
      description={machine ? t('pgEditOwnBody', { machine: place ?? '' }) : t('pgEditBody')} action={machine ? t('cdSaveAndCheck') : t('save')}
      valid={(urlChanged || pwChanged || syncChanged) && !urlError && !pw.error(t) && (!!machine || !!url.trim())}
      hint={!urlChanged && !pwChanged && !syncChanged ? t('pgEditNothing') : undefined}
      busyHint={machine ? t('checkingFrom', { machine: place ?? '' }) : undefined}
      submit={() => api.call('POST', `/api/stores/${enc(store)}/connection`, body()).then(done)}>
      {machine ? <p className={css.formNote}>{t('pgOwnNote', { machine: place ?? '', target: s.postgresql?.target ?? '' })}</p> : null}
      <TextField label={machine ? t('cdOwnUrl', { machine: place ?? '' }) : t('pgUrl')} value={url} onChange={setUrl} mono placeholder="postgresql://user@host:5432/db"
        hint={machine ? t('pgUrlHint') : undefined} error={urlError} autoFocus />
      {machine ? null : <PgPasswordFields id="lfs-pg-edit-password" pw={pw} t={t} auths={auths} />}
      {!machine && authBefore && !pwChanged ? <p className={css.formNote}>{t('pgPasswordKept')}</p> : null}
      {machine ? null : (
        <Field label={t('syncCommit')}>
          <MenuSelect value={sync} label={t('syncCommit')} placeholder={t('syncCommitDefault')}
            options={['off', 'local', 'remote_write', 'on', 'remote_apply'].map((v) => ({ value: v, label: v }))} onChange={setSync} />
        </Field>
      )}
    </FormModal>
  )
}

/**
 * Save a login for a PostgreSQL Store that names none (LayerFS's fix:
 * `store save-auth <auth> --user <user> --password-stdin && store update
 * <store> --auth <auth>`). Prefilled for the Store: the login's name is the
 * Store's when no saved login has it, the user is the one its address names.
 * The password is the person's to type; nothing is guessed. Or a saved login
 * is picked (`store update --auth` alone). The host saves and names it.
 */
function SaveLoginDialog({ api, t, store: s, onClose, done }: { api: LayerfsApi, t: StorageT, store: StoreRow, onClose: () => void, done: () => void }): ReactNode {
  const auths = usePgAuths(api)
  const pw = usePgPassword('entered', '', null, parsePgUrl(s.postgresql?.url ?? '')?.user ?? '')
  const [typed, setTyped] = useState<string | null>(null)
  const taken = new Set(auths.map((a) => a.auth))
  // a Store naming a login the home lacks (`missing`) gets that login back under its own name; one naming none, the server's (V23-12)
  const named = s.postgresql?.auth ?? null
  const fallbackName = named ?? pgServerLogin(s.postgresql?.server ?? null) ?? s.name
  const authName = (typed ?? (taken.has(fallbackName) ? '' : fallbackName)).trim()
  const entered = pw.how === 'entered'
  const nameError = !entered || !authName ? null : taken.has(authName) ? t('pgLoginNameTaken', { auth: authName }) : NAME.test(authName) ? null : t('storeNameHint')
  const valid = entered ? !!authName && !nameError && !pw.missing : !pw.missing
  const body = () => (entered ? { authName, user: pw.user.trim(), passwordValue: pw.value } : pw.body())
  return (
    <FormModal open onClose={onClose} t={t} title={t('pgLoginTitle', { store: s.name })} description={t(auths.length ? 'pgLoginBody' : 'pgLoginBodyNew', { target: s.postgresql?.target ?? s.name })}
      action={t('save')} valid={valid} submit={() => api.call('POST', `/api/stores/${enc(s.name)}/connection`, body()).then(done)}>
      <PgPasswordFields id="lfs-pg-login-password" pw={pw} t={t} auths={auths} />
      {entered ? <TextField label={t('pgLoginName')} value={typed ?? authName} onChange={setTyped} mono hint={t('pgLoginNameHint')} error={nameError} /> : null}
    </FormModal>
  )
}

/**
 * A new password for saved login `auth` (spec v23 §5.1): the dialog names
 * every Store that uses it, the password goes to LayerFS on stdin (`store
 * save-auth` under its own name rotates it), then each of those Stores is
 * checked from this computer, one dot each. A failed check is a result:
 * its dot and LayerFS's sentence as the tip.
 */
function RotateLoginDialog({ api, t, view, auth, onClose, done }: {
  api: LayerfsApi, t: StorageT, view: StorageView | undefined, auth: string, onClose: () => void, done: () => void
}): ReactNode {
  const login = view?.auths?.find((a) => a.auth === auth)
  const [user, setUser] = useState(login?.user ?? '')
  const [password, setPassword] = useState('')
  const [result, setResult] = useState<PgRotation | null>(null)
  const stores = login?.stores ?? []
  if (result) {
    const failed = result.checks.filter((c) => !c.ok).length
    return (
      <FormModal open onClose={onClose} t={t} title={t('pgRotateTitle', { auth })} description={failed ? t('pgRotateSomeFailed', { n: failed }) : t('pgRotateDone')}
        action={t('cdDone')} valid submit={async () => onClose()}>
        <ul className={css.rotateChecks} data-rotate-checks>
          {result.checks.map((c) => (
            <li key={c.store} className={css.rotateCheck} data-store={c.store} data-ok={c.ok || undefined} title={c.error?.message}>
              <StateDot state={c.ok ? 'done' : 'error'} size={9} />
              <span className={css.mono}>{c.store}</span>
              <span className={css.choiceMeta}>{c.ok ? t('pgReached') : t('pgCheckFailed')}</span>
            </li>
          ))}
        </ul>
      </FormModal>
    )
  }
  return (
    <FormModal open onClose={onClose} t={t} title={t('pgRotateTitle', { auth })}
      description={stores.length ? t('pgRotateBody', { stores: stores.join(', ') }) : t('pgRotateBodyNone')}
      action={stores.length ? t('pgRotateAction') : t('save')} valid={!!password && !!user.trim()} busyHint={t('pgRotateChecking')} keepOpen
      submit={() => api.call<PgRotation>('POST', `/api/store-auths/${enc(auth)}/rotate`, { user: user.trim(), passwordValue: password }).then((r) => { done(); setResult(r) })}>
      <TextField label={t('pgUser')} value={user} onChange={setUser} mono />
      <Field label={t('pgPasswordValue')} hint={t('pgPasswordNote')}>
        <Input className={css.monoInput} value={password} onChange={(e) => setPassword(e.target.value)} type="password" aria-label={t('pgPasswordValue')} autoComplete="off" spellCheck={false} autoFocus />
      </Field>
    </FormModal>
  )
}

// ---------- Add project ----------

function NewProjectDialog({ api, t, store, onClose, done }: { api: LayerfsApi, t: StorageT, store: string, onClose: () => void, done: () => void }): ReactNode {
  const [name, setName] = useState('')
  const [source, setSource] = useState('')
  const [job, setJob] = useState<ProjectInitJob | null>(null)
  const sourceError = source.trim() && !source.trim().startsWith('/') ? t('sourceInvalid') : null
  const submit = async () => {
    let current = job && !job.ended ? await api.call<ProjectInitJob>('GET', `/api/project-inits/${enc(job.id)}`)
      : await api.call<ProjectInitJob>('POST', '/api/projects', { name: name.trim(), store, ...(source.trim() ? { source: source.trim() } : {}) })
    setJob(current)
    const deadline = Date.now() + 120_000
    while (!current.ended) {
      if (Date.now() > deadline) throw new Error(t('creationRunning'))
      await new Promise((r) => setTimeout(r, 500))
      current = await api.call<ProjectInitJob>('GET', `/api/project-inits/${enc(current.id)}`)
      setJob(current)
    }
    done()
    if (current.error) throw Object.assign(new Error(current.error.message), current.error)
  }
  const running = job && !job.ended
  return (
    <FormModal open onClose={onClose} t={t} title={t('addProjectTitle', { store })} description={t('addProjectBody')}
      action={running ? t('checkCreation') : t('createProject')} valid={!!running || (NAME.test(name.trim()) && !sourceError)} submit={submit}>
      <TextField label={t('projectName')} value={name} onChange={setName} mono autoFocus placeholder="my-project" hint={t('projectNameHint')} />
      <TextField label={t('sourceFolder')} value={source} onChange={setSource} mono placeholder="/Users/me/code/my-project" hint={t('sourceFolderHint')} error={sourceError} />
      {job && !job.ended ? (
        <p className={css.progressLine} role="status">
          <StateDot state="ongoing" size={13} />
          {t('creating', { name: job.name, step: job.step })}
          {job.progress?.files != null ? ` · ${t('creatingCounts', { files: job.progress.files, bytes: bytes(job.progress.bytes ?? null) })}` : ''}
        </p>
      ) : null}
    </FormModal>
  )
}

// ---------- Push / Move ----------

function PlaceDialog({ api, t, kind, store: s, onClose, done }: { api: LayerfsApi, t: StorageT, kind: 'push' | 'move', store: StoreRow, onClose: () => void, done: () => void }): ReactNode {
  const machines = useQuery(() => api.call<MachinesView>('GET', '/api/machines'), [api])
  const [target, setTarget] = useState('')
  const [starting, setStarting] = useState(false)
  const [startError, setStartError] = useState<unknown>(undefined)
  const view = machines.data?.available ? machines.data : null
  // its Start <machine> behind the press guard, like every power control
  const startGuard = usePressGuard(`start:${target}`)
  const targets = (view?.machines ?? []).filter((m) => m.name !== 'native' && m.name !== s.machine && (!m.supports || m.supports.includes('sqlite')))
  const chosen = targets.find((m) => m.name === target)
  const free = chosen ? view?.live[chosen.name]?.freeDisk ?? null : null
  const sandboxes = (view?.sandboxes ?? []).filter((b) => b.project?.startsWith(`${s.name}/`) && b.state !== 'gone')
  const push = kind === 'push'
  return (
    <FormModal open onClose={onClose} t={t} title={push ? t('pushTitle', { store: s.name }) : t('moveTitle', { store: s.name })}
      description={push ? t('pushBody', { n: s.projects.length }) : t('moveBody')} action={push ? t('push') : t('move')} valid={!!target}
      submit={() => api.call('POST', `/api/stores/${enc(s.name)}/${kind}`, { machine: target }).then(done)}>
      <Field label={t('machine')} hint={machines.loading && !view ? t('readingMachines') : targets.length ? undefined : t('noTargets')}>
        <MenuSelect value={target} label={t('machine')} placeholder={t('chooseMachine')} onChange={setTarget}
          options={targets.map((m) => ({ value: m.name, label: `${m.name} · ${m.platform} · ${storeStateWords(m.state ?? 'unknown', t)}` }))} />
      </Field>
      {machines.error ? <Refusal error={machines.error} labels={refusalLabels(t)} compact /> : null}
      {chosen ? (
        <p className={css.formNote}>
          {free !== null ? t('freeDisk', { free: bytes(free), machine: chosen.name, size: bytes(s.size?.bytes ?? null) }) : t('freeUnknown', { machine: chosen.name })}
          {chosen.state !== 'ready' ? ` ${t('notRunning', { machine: chosen.name, state: chosen.state })}` : ''}
        </p>
      ) : null}
      {chosen?.state === 'stopped' ? (
        <Button variant="outline" size="sm" className={css.selfStart} disabled={starting || startGuard.settling} aria-disabled={startGuard.settling || undefined} icon={starting ? <StateDot state="ongoing" size={13} /> : undefined}
          onClick={startGuard(() => { setStarting(true); api.call('POST', `/api/machines/${enc(chosen.name)}/start`).then(machines.reload, setStartError).finally(() => setStarting(false)) })}>
          {t('startMachine', { machine: chosen.name })}
        </Button>
      ) : null}
      {startError !== undefined ? <Refusal error={startError} labels={refusalLabels(t)} compact /> : null}
      {sandboxes.length > 0 ? (
        <Notice tone="warning">
          <strong>{t('heldNow')}</strong>
          <span>{sandboxes.map((b) => t('heldSandbox', { name: b.name })).join(' · ')}</span>
        </Notice>
      ) : null}
    </FormModal>
  )
}

// ---------- Add a Store to a machine ----------

/**
 * A machine's Add Store. A SQLite Store lives on one machine, so adding one
 * here moves it from where it is (the host's place: push, pull or move, as a
 * drop on the machine does); the home also offers a new one. A PostgreSQL
 * Store is on no machine: its tile goes on to Connect a database, where one
 * is connected, or one already connected is set to run here.
 */
function ToMachineDialog({ api, t, view, machine, onClose, done, onDialog }: {
  api: LayerfsApi, t: StorageT, view: StorageView | undefined, machine: string
  onClose: () => void, done: () => void, onDialog?: ((dialog: StorageDialog) => void) | undefined
}): ReactNode {
  const m = view?.machines.find((x) => x.name === machine)
  const noSqlite = m?.supports && !m.supports.includes('sqlite') ? t('noStoresHere') : null
  const noPg = m?.supports && !m.supports.includes('postgresql_sandbox') ? t('postgresql') : null
  const [chosen, setChosen] = useState('')
  if (!view) return null
  const placeName = (name: string | null) => (name === 'native' ? t('layerfsHome') : name ?? '')
  const rows = view.stores
    .filter((s) => s.kind === 'sqlite' && s.machine !== machine)
    .map((s) => ({ s, why: noSqlite ?? (canMove(s) ? null : t('storeBusyNow')) }))
    .sort((a, b) => Number(!!a.why) - Number(!!b.why) || a.s.name.localeCompare(b.s.name))
  const pick = rows.find((r) => r.s.name === chosen && !r.why)
  const swap = (next: StorageDialog) => { onClose(); onDialog?.(next) }
  return (
    <FormModal open onClose={onClose} t={t} title={t('addStoreTitle', { machine: placeName(machine) })}
      description={t('addSqliteBody')} action={t('moveHere')} valid={!!pick}
      busyHint={pick ? t('movingFrom', { store: pick.s.name, from: placeName(pick.s.machine) }) : undefined}
      submit={async () => {
        if (!pick) return
        await api.call('POST', `/api/machines/${enc(machine)}/stores`, { store: pick.s.name })
        done()
      }}>
      {onDialog ? (
        <TileChoice label={t('kindOfStore')} value="sqlite" onChange={(v) => { if (v === 'postgresql') swap({ kind: 'connect', machine }) }}
          options={[
            { value: 'sqlite', label: t('sqlite'), body: t('addSqliteTile'), icon: <StoreGlyph kind="sqlite" />, disabled: noSqlite },
            { value: 'postgresql', label: t('postgresql'), body: t('addPgTile'), icon: <StoreGlyph kind="postgresql" />, disabled: noPg },
          ]} />
      ) : null}
      <div className={css.choiceList} role="radiogroup" aria-label={t('sqlite')}>
        {rows.length ? rows.map(({ s, why }) => (
          <button key={s.name} type="button" role="radio" aria-checked={chosen === s.name} disabled={!!why} className={css.choiceRow}
            data-checked={chosen === s.name || undefined} onClick={() => setChosen(s.name)} title={why ?? undefined}>
            <span className={css.radio} aria-hidden="true" />
            <span className={css.choiceMain}>{s.name}</span>
            <span className={css.choiceMeta}>{why ?? t('fromPlace', { place: placeName(s.machine) })}</span>
          </button>
        )) : <p className={css.choiceEmpty}>{t('addSqliteNone', { machine: placeName(machine) })}</p>}
      </div>
      {pick && m?.state && m.state !== 'ready' ? <p className={css.formNote}>{t('notRunning', { machine: placeName(machine), state: m.state })}</p> : null}
      {onDialog && machine === 'native' ? (
        <p className={css.formNote}>
          <button type="button" className={css.textLink} onClick={() => swap({ kind: 'new-store', machine })}>{t('newStore')}</button>
        </p>
      ) : null}
    </FormModal>
  )
}

// ---------- Restore ----------

/** Why backup.restore would refuse replacing `s` with the bucket's file, or null. */
function replaceRefusal(s: StoreRow, state: BackupState | undefined, bucket: string, t: StorageT): string | null {
  if (!inHome(s)) return t('replaceNotHome', { store: s.name, machine: s.machine ?? '?' })
  if (s.state !== 'ready' || s.job) return t('replaceNotIdle', { store: s.name })
  if (!s.links.some((l) => l.bucket === bucket)) return t('replaceNoLink', { store: s.name, bucket })
  if (state === 'store_changed' || state === 'both_changed') return t('replaceChanged', { store: s.name, bucket })
  return null
}

function RestoreDialog({ api, t, dialog, view, onClose, done }: { api: LayerfsApi, t: StorageT, dialog: Extract<StorageDialog, { kind: 'restore' }>, view: StorageView | undefined, onClose: () => void, done: () => void }): ReactNode {
  const linked = dialog.store ? view?.stores.find((s) => s.name === dialog.store && s.kind === 'sqlite') : undefined
  const state = backupStateOf(view?.buckets.find((b) => b.name === dialog.bucket)?.files?.find((f) => f.file === dialog.file && f.store === dialog.store)) ?? undefined
  const refusal = linked ? replaceRefusal(linked, state, dialog.bucket, t) : null
  const [mode, setMode] = useState<'new' | 'replace'>(linked && !refusal && state === 'bucket_changed' ? 'replace' : 'new')
  const [name, setName] = useState('')
  const taken = !!name.trim() && !!view?.stores.some((s) => s.name === name.trim())
  const replacing = mode === 'replace' && !!linked
  return (
    <FormModal open onClose={onClose} t={t} title={t('restoreTitle', { bucket: dialog.bucket, file: dialog.file })} description={t('restoreBody')}
      action={replacing ? t('restoreReplace', { store: linked!.name }) : t('restoreStore')}
      valid={replacing ? !refusal : NAME.test(name.trim()) && !taken}
      submit={() => api.call('POST', `/api/buckets/${enc(dialog.bucket)}/restore`, { store: replacing ? linked!.name : name.trim(), file: dialog.file }).then(done)}>
      {linked ? (
        <TileChoice label={t('restore')} value={mode} onChange={setMode} options={[
          { value: 'new', label: t('restoreAsNew'), body: t('restoreAsNewBody') },
          { value: 'replace', label: t('restoreReplace', { store: linked.name }), body: t('restoreReplaceBody', { store: linked.name }), disabled: refusal },
        ]} />
      ) : null}
      {replacing
        ? <Notice tone="warning">{t('restoreReplaceWarn', { store: linked!.name })}</Notice>
        : <TextField label={t('newStoreName')} value={name} onChange={setName} mono autoFocus placeholder={`${dialog.file}-restored`} error={taken ? t('nameTaken') : null} />}
    </FormModal>
  )
}

// ---------- Add bucket ----------

const ACCOUNT = /^[0-9a-f]{32}$/

/** The Connect dialog's first values: empty, or (Connect again…) the bucket's name and the parts of the URL LayerFS's fix names. */
export function bucketDraft(again?: Reconnect): { provider: 'minio' | 'r2', values: Record<string, string> } {
  const blank = { endpoint: 'http://localhost:9000', prefix: 'layerfs', jurisdiction: '' }
  if (!again) return { provider: 'minio', values: blank }
  const parts = bucketParts(again.url)
  const provider = parts?.provider ?? (again.provider === 'r2' ? 'r2' : 'minio')
  if (!parts) return { provider, values: { ...blank, ...(provider === 'r2' ? { endpoint: '' } : {}), name: again.bucket } }
  return {
    provider,
    values: { name: again.bucket, bucket: parts.bucket, prefix: parts.prefix, jurisdiction: parts.jurisdiction ?? '', endpoint: parts.endpoint ?? '', account: parts.account ?? '' },
  }
}

/** What a bucket connect did, from LayerFS's long form: `rotated` (the same name and URL: key replaced, links kept), else null. */
export function connectOutcome(answer: unknown): 'rotated' | null {
  return (answer as { outcome?: unknown } | null)?.outcome === 'rotated' ? 'rotated' : null
}

/** Add a bucket, or connect one again (the same URL replaces its key, spec v21; connecting again is a change, so it is listed once more). */
function BucketDialog({ api, t, again, onClose, done }: { api: LayerfsApi, t: StorageT, again?: Reconnect | undefined, onClose: () => void, done: () => void }): ReactNode {
  const [draft] = useState(() => bucketDraft(again))
  /** LayerFS answered `rotated`: the dialog stays to say so, and its action closes it. */
  const [rotated, setRotated] = useState(false)
  const [provider, setProvider] = useState<'minio' | 'r2'>(draft.provider)
  const [v, setV] = useState<Record<string, string>>(draft.values)
  const set = (k: string) => (value: string) => setV((x) => ({ ...x, [k]: value }))
  const accountError = provider === 'r2' && v.account && !ACCOUNT.test(v.account.trim().toLowerCase()) ? t('accountInvalid') : null
  const missing = [
    !v.name?.trim() && t('name'),
    provider === 'minio' && !v.endpoint?.trim() && t('endpoint'),
    provider === 'r2' && !v.account?.trim() && t('account'),
    !v.bucket?.trim() && t('bucketName'),
    !v.accessKey?.trim() && t('accessKey'),
    !v.secretKey && t('secretKey'),
  ].filter(Boolean) as string[]
  const body = () => ({
    name: v.name!.trim(),
    provider,
    ...(provider === 'minio' ? { endpoint: v.endpoint!.trim() } : { account: v.account!.trim().toLowerCase(), ...(v.jurisdiction ? { jurisdiction: v.jurisdiction } : {}) }),
    bucket: v.bucket!.trim(),
    ...(v.prefix?.trim() ? { prefix: v.prefix.trim() } : {}),
    accessKey: v.accessKey!.trim(), secretKey: v.secretKey!,
  })
  return (
    <FormModal open onClose={onClose} t={t} title={again ? t('reconnectTitle', { bucket: again.bucket }) : t('bucketTitle')} description={again ? t('reconnectBody') : t('bucketBody')}
      action={rotated ? t('close') : again ? t('reconnectAction') : t('connectBucket')} keepOpen
      valid={rotated || (!missing.length && !accountError)} hint={!rotated && missing.length ? t('stillNeeded', { what: missing.join(', ') }) : undefined}
      submit={async () => {
        if (rotated) { onClose(); return }
        const answer = await api.call<{ outcome?: unknown }>('POST', '/api/buckets', body())
        // connecting again is a change: list the bucket once more (a refusal there shows on its page)
        if (again) await api.call('GET', `/api/buckets/${enc(again.bucket)}/files`).catch(() => null)
        done()
        if (connectOutcome(answer) === 'rotated') setRotated(true)
        else onClose()
      }}>
      {rotated ? (
        <p className={css.bucketStatus} data-tone="ok" data-rotated role="status">
          <Signal state="ok" size={7} title={t('keyRotated')} detail={t('keyRotatedTip')} />
        </p>
      ) : null}
      <TileChoice label={t('provider')} value={provider} onChange={setProvider}
        options={[{ value: 'minio', label: t('minio'), icon: <BucketProviderGlyph provider="minio" size={18} /> }, { value: 'r2', label: t('r2'), icon: <BucketProviderGlyph provider="r2" size={18} /> }]} />
      <div className={css.fieldRow}>
        <TextField label={t('name')} value={v.name ?? ''} onChange={set('name')} mono autoFocus placeholder="backups" />
        <TextField label={t('bucketName')} value={v.bucket ?? ''} onChange={set('bucket')} mono placeholder="layerfs-backups" />
      </div>
      {provider === 'minio'
        ? <TextField label={t('endpoint')} value={v.endpoint ?? ''} onChange={set('endpoint')} mono />
        : (
          <div className={css.fieldRow}>
            <TextField label={t('account')} value={v.account ?? ''} onChange={set('account')} mono error={accountError} />
            <Field label={t('jurisdiction')} className={css.fieldNarrow}>
              <MenuSelect value={v.jurisdiction ?? ''} label={t('jurisdiction')} onChange={set('jurisdiction')}
                options={[{ value: '', label: t('jurisdictionNone') }, { value: 'eu', label: 'EU' }, { value: 'fedramp', label: 'FedRAMP' }]} />
            </Field>
          </div>
        )}
      <TextField label={t('prefix')} value={v.prefix ?? ''} onChange={set('prefix')} mono hint={t('objectNote', { prefix: v.prefix ? `${v.prefix}/` : '' })} />
      <div className={css.fieldRow}>
        <TextField label={t('accessKey')} value={v.accessKey ?? ''} onChange={set('accessKey')} mono />
        <TextField label={t('secretKey')} value={v.secretKey ?? ''} onChange={set('secretKey')} mono type="password" />
      </div>
      <p className={css.formNote}>{t('keysNote')}</p>
    </FormModal>
  )
}

// ---------- Confirm ----------

/** A destructive confirmation; with `name`, the name is typed to confirm. */
function ConfirmDialog({ t, title, body, action, name, force, plain, steps, onClose, run }: {
  t: StorageT, title: string, body: string, action: string, name?: string | undefined, force?: ConfirmForce | undefined, plain?: boolean | undefined, steps?: string[] | undefined, onClose: () => void, run: (force: boolean) => Promise<unknown>
}): ReactNode {
  const [typed, setTyped] = useState('')
  const [forced, setForced] = useState(false)
  const on = !!force && forced
  return (
    <FormModal open onClose={onClose} t={t} title={title} description={body} action={on ? force.action : action} danger={!plain} valid={!name || typed === name} submit={() => run(on)}>
      {steps?.length ? <ol data-steps={steps.length}>{steps.map((x, i) => <li key={i}>{x}</li>)}</ol> : null}
      {name ? <TextField label={t('typeToConfirm', { name })} value={typed} onChange={setTyped} mono autoFocus placeholder={name} /> : null}
      {force ? <Checkbox checked={forced} onChange={setForced} label={force.label} /> : null}
      {!name && !force ? <span className={clsx(css.hidden)} /> : null}
    </FormModal>
  )
}


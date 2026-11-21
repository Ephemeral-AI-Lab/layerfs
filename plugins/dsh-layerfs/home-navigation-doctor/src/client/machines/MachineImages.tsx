/**
 * A machine's cached Sandbox images (docs/specs/sandbox-images.md §5.1.3,
 * Stage 1 only, not on macOS native): the Images tab on its page (a disk
 * bar, the stored images as cards with Remove on unused ones, and this
 * computer's Docker images with Copy) and the
 * Free space dialog (one checkbox per image, the unused ones preselected),
 * which New Sandbox and a create that failed for lack of space open on top of
 * themselves. Both read `GET /api/machines/<name>/sandbox-images` and remove
 * through `prune-images`; Docker's own copies are never touched.
 */
import { useEffect, useState, type ReactNode } from 'react'
import { Button, Checkbox, IconCopyOutlineRegular, IconDownloadOutlineRegular, IconPlusOutlineRegular, IconRefreshOutlineRegular, IconTrashOutlineRegular, Input, Modal } from '@deepseek-ai/dsh-client-ui-primitives'
import clsx from 'clsx'
import { Card, Refusal, Section, usePressGuard } from '../ui/index.tsx'
import { useMachinesContext } from './context.tsx'
import { onMac, useImageName, useRestartWorker, useSandboxImages } from './ImageMenu.tsx'
import { agoWords, badgeHue, diskSegments, dockerRows, freeSpaceRows, freesWords, imageLabel, imagesFooter, imagesStatus, loadWords, phaseShare, reasonWords, removable, type ImageLabel } from './images.ts'
import { useDiskLevel } from './SpaceViews.tsx'
import { NewSandboxDialog } from './Sandboxes.tsx'
import { bytes, type DockerImage, type ImageLoad, type MachineInfo, type PruneAnswer, type ReadyImage, type SandboxImagesView } from './model.ts'
import type { Query } from '../ui/use-query.ts'
import css from './machines.module.css'
import { shortDigests } from './notices.ts'

type MachineRef = Pick<MachineInfo, 'name' | 'state'>

/** Whether LayerFS refused an image because a Sandbox here uses it. */
export const refusedInUse = (r: PruneAnswer['refused'][number]): boolean => r.code === 'InUse' || /used by a sandbox/i.test(r.message)

/**
 * LayerFS's refusals of single images, as lines under the list: the word per
 * image, why as its tip (in use by a Sandbox here, or LayerFS's words with the
 * digests cut, never a full sha256), and Open Sandboxes as the one action while
 * a Sandbox holds one (UI pass 3 F13). `onLeave` closes a dialog before it goes.
 */
function Refused({ refused, names, machine, onLeave }: { refused: PruneAnswer['refused'], names: Map<string, string>, machine: string, onLeave?: () => void }): ReactNode {
  const { t, go } = useMachinesContext()
  if (!refused.length) return null
  return (
    <div role="alert" className={css.errorText}>
      {refused.map((r) => (
        <p key={r.id} data-refused={r.code} title={refusedInUse(r) ? t('pruneWhyInUse') : shortDigests(r.message)}>
          {t('pruneRefusedWord', { image: names.get(r.id) ?? r.id })}
        </p>
      ))}
      {refused.some(refusedInUse) ? <Button size="sm" variant="outline" onClick={() => { onLeave?.(); go({ kind: 'machine', name: machine, tab: 'sandboxes' }) }}>{t('inUseOpenSandboxes')}</Button> : null}
    </div>
  )
}

/**
 * The machine page's Images tab, read while it is open: a disk bar (each
 * stored image, everything else, free), the images stored on the machine as
 * cards, and the Docker images not on it yet, each with Copy (machine.load_image in
 * the background).
 */
export function ImagesTab({ m, taken = [], canCreate = false }: { m: MachineInfo, /** The machine's Sandbox names (New Sandbox suggests a free one). */ taken?: string[], canCreate?: boolean }): ReactNode {
  const images = useSandboxImages(m)
  const answer = images.data
  // New Sandbox from a stored image's card: the dialog with that image preselected
  const [creating, setCreating] = useState<string | null>(null)
  if (answer && (answer.stage !== 1 || answer.mode === 'host')) return null
  return (
    <div className={css.tabStack}>
      <Card className={css.imgInventory}>
        {answer && <ImagesDisk m={m} answer={answer} />}
        <ImagesSection m={m} images={images} onCreate={canCreate ? setCreating : undefined} />
      </Card>
      {answer && <DockerSection m={m} images={images} />}
      {creating && <NewSandboxDialog machine={m} taken={taken} image={creating} onClose={() => setCreating(null)} />}
    </div>
  )
}

/** The disk legend names this many images, then says how many more. */
const LEGEND_MAX = 6

/** A repository's letter on its colour (the same colour every time for the same name). */
function ImageBadge({ repo, bundle = false, off = false }: { repo: string, bundle?: boolean, off?: boolean }): ReactNode {
  return <span className={css.imgBadge} data-hue={bundle ? 'bundle' : off ? 'off' : badgeHue(repo)} aria-hidden="true">{bundle ? 'L' : off ? '–' : (repo.match(/[a-z0-9]/i)?.[0] ?? '?').toUpperCase()}</span>
}

/** registry/ (quieter) repo, then one chip per tag. */
function ImageTitle({ label }: { label: ImageLabel }): ReactNode {
  return (
    <span className={css.imgTitle}>
      {label.registry && <span className={css.imgRegistry}>{label.registry}</span>}
      <span className={css.imgRepo}>{label.repo}</span>
      {label.tags.map((tag) => <span key={tag} className={css.imgTagChip}>{tag}</span>)}
    </span>
  )
}

/**
 * How much of the machine's data disk the stored images take, from
 * machine.stats (no read of its own): one segment per image, everything else
 * used, free; with what this computer's Docker still offers. Without the
 * disk's total it shows the images' size and free space alone.
 */
function ImagesDisk({ m, answer }: { m: MachineInfo, answer: SandboxImagesView }): ReactNode {
  const { t } = useMachinesContext()
  const nameOf = useImageName()
  const disk = useDiskLevel(m)
  const seg = diskSegments(answer.ready, disk.total, disk.free ?? answer.free)
  const docker = answer.docker?.available ? answer.docker.images : null
  const imagesBytes = answer.ready.reduce((sum, r) => sum + (r.size ?? 0), 0)
  const hueOf = (r: ReadyImage) => (r.source === 'bundle' ? 'bundle' : String(badgeHue(imageLabel(nameOf(r), r.refs).repo)))
  return (
    <div className={css.imgDisk} data-images-disk>
      <div className={css.imgDiskHead}>
        <span className={css.imgDiskTitle}>{t('imgSpaceTitle', { machine: m.name })}</span>
        <span className={css.imgDiskSub}>{seg ? t('imgSpaceOf', { images: bytes(imagesBytes), total: bytes(seg.total) }) : bytes(imagesBytes)}</span>
        <span className={css.columnSpacer} />
        {docker && <span className={css.imgDiskSub}>{t(onMac() ? 'imgDockerMoreMac' : 'imgDockerMoreHost', { n: String(docker.length), usable: String(docker.filter((r) => r.usable).length) })}</span>}
      </div>
      {seg && (
        <div className={css.imgDiskBar} role="img" aria-label={t('imgSpaceBar', { images: bytes(seg.imagesBytes), rest: bytes(seg.rest), free: bytes(seg.free) })}>
          {answer.ready.map((r, i) => <span key={r.id} data-hue={hueOf(r)} style={{ width: `${(seg.images[i]?.share ?? 0) * 100}%` }} title={`${nameOf(r)} ${bytes(r.size)}`} />)}
          <span data-part="rest" style={{ width: `${(seg.rest / seg.total) * 100}%` }} />
        </div>
      )}
      <div className={css.imgDiskLegend}>
        {answer.ready.slice(0, LEGEND_MAX).map((r) => <span key={r.id} className={css.imgLegendItem} title={nameOf(r)}><i data-hue={hueOf(r)} /><span className={css.imgLegendName}>{nameOf(r)}</span> {bytes(r.size)}</span>)}
        {answer.ready.length > LEGEND_MAX && <span>{t('imgLegendMore', { n: String(answer.ready.length - LEGEND_MAX) })}</span>}
        {seg && <span><i data-part="rest" />{t('imgSpaceRest', { size: bytes(seg.rest) })}</span>}
        {(seg ? seg.free : answer.free) != null && <span>{t('imgSpaceFree', { size: bytes(seg ? seg.free : answer.free) })}</span>}
      </div>
    </div>
  )
}

/**
 * The image copies to `machine` the host runs (`GET /api/image-loads`, then
 * `image-load` events), keyed by image Id; `copy` admits one, `dismiss`
 * drops a failed one. A done copy leaves the map (the image is then stored).
 */
function useImageLoads(machine: string): { loads: Record<string, ImageLoad>, copy: (image: string) => Promise<void>, dismiss: (load: ImageLoad) => Promise<void> } {
  const { api } = useMachinesContext()
  const [loads, setLoads] = useState<Record<string, ImageLoad>>({})
  const put = (l: ImageLoad) => setLoads((now) => {
    const next = { ...now }
    if (l.phase === 'done') delete next[l.image]; else next[l.image] = l
    return next
  })
  useEffect(() => {
    let live = true
    setLoads({})
    void api.call<{ loads: ImageLoad[] }>('GET', '/api/image-loads').then((r) => {
      if (live) setLoads((now) => ({ ...Object.fromEntries((r?.loads ?? []).filter((l) => l.machine === machine).map((l) => [l.image, l])), ...now }))
    }).catch(() => {})
    const off = api.onEvent('image-load', (e) => { const l = e.data?.load as ImageLoad | undefined; if (live && l?.machine === machine) put(l) })
    return () => { live = false; off() }
  }, [api, machine])
  return {
    loads,
    copy: async (image) => { put((await api.call<{ load: ImageLoad }>('POST', `/api/machines/${encodeURIComponent(machine)}/load-image`, { image })).load) },
    dismiss: async (load) => {
      await api.call('DELETE', `/api/image-loads/${encodeURIComponent(load.id)}`)
      setLoads((now) => { const next = { ...now }; if (next[load.image]?.id === load.id) delete next[load.image]; return next })
    },
  }
}

/** "Build your own with `docker build -t my-image .` then Refresh", with a button that copies the command. */
function BuildHint(): ReactNode {
  const { t } = useMachinesContext()
  const [copied, setCopied] = useState(false)
  const command = 'docker build -t my-image .'
  const copyCommand = async () => {
    try { await navigator.clipboard.writeText(command); setCopied(true); setTimeout(() => setCopied(false), 1500) } catch { setCopied(false) }
  }
  return (
    <div className={css.imgHint}>
      <span className={css.imgHintText}>{t('imgBuildLead')} <code className={css.imgCode}>{command}</code> {t('imgBuildTail')}</span>
      <Button variant="ghost" size="sm" icon={<IconCopyOutlineRegular size={12} />} onClick={() => void copyCommand()}>{copied ? t('imgCommandCopied') : t('imgCopyCommand')}</Button>
    </div>
  )
}

/**
 * The Docker images not on the machine yet: search, Refresh, one row per
 * image (its badge, repository and tags, the space a copy needs, its size
 * against the largest) and Copy (one attempt; a running copy shows its bytes
 * and share, a failure its reason with Dismiss). The ones that cannot run
 * here follow, dimmed, with why. Copying needs a machine that answers now; a
 * stopped machine or an outdated worker shows the list without Copy.
 */
function DockerSection({ m, images }: { m: MachineInfo, images: Query<SandboxImagesView> }): ReactNode {
  const { t, tw, refusal } = useMachinesContext()
  const answer = images.data
  const [query, setQuery] = useState('')
  const [error, setError] = useState<unknown>(null)
  const { loads, copy, dismiss } = useImageLoads(m.name)
  if (!answer?.docker) return null
  const title = t(onMac() ? 'imgDockerTitleMac' : 'imgDockerTitleHost', { machine: m.name })
  if (!answer.docker.available) {
    return (
      <Card className={css.block}>
        <Section title={title} className={css.imagesSection}>
          <p className={css.muted}>{t(answer.docker.reason === 'not-installed' ? 'noteDockerMissing' : 'noteDockerOff', { machine: m.name })}</p>
        </Section>
      </Card>
    )
  }
  const canCopy = imagesStatus(answer, m.state) === 'fresh'
  const { usable, unusable, total } = dockerRows(answer, query)
  const largest = Math.max(1, ...answer.docker.images.map((r) => r.size))
  const start = async (id: string) => { setError(null); try { await copy(id) } catch (e) { setError(e) } }
  const row = (r: DockerImage) => {
    const load = loads[r.id]
    const running = r.usable && load && load.phase !== 'failed'
    const share = running ? phaseShare({ phase: load.phase, done: load.done, total: load.total } as Parameters<typeof phaseShare>[0]) : null
    return (
      <li key={r.id} className={css.imgRow} data-docker-image={r.name} data-off={!r.usable || undefined} title={[...r.refs, r.id].join('\n')}>
        <ImageBadge repo={imageLabel(r.name, r.refs).repo} off={!r.usable} />
        <span className={css.imgRowMain}>
          <ImageTitle label={imageLabel(r.name, r.refs)} />
          {running
            ? (
              <span className={css.imgRowProgress}>
                <span className={css.imgBar} aria-hidden="true"><span style={{ width: `${Math.round((share ?? 0) * 100)}%` }} data-unknown={share == null || undefined} /></span>
                <span className={css.imgRowSub}>{tw(loadWords(load))}</span>
              </span>
            )
            : (
              <span className={clsx(css.imgRowSub, load?.phase === 'failed' && css.errorText, r.usable && !r.fits && !load && css.imgWarn)} role={load?.phase === 'failed' ? 'alert' : undefined}>
                {!r.usable ? t('imgCannotRun', { reason: tw(reasonWords(r.reason)) })
                  : load ? tw(loadWords(load))
                    : r.fits ? t('imgNeeds', { size: bytes(r.need) }) : t('imgNoFit', { size: bytes(r.need) })}
              </span>
            )}
        </span>
        <span className={css.imgRowSize}>
          <span>{bytes(r.size)}</span>
          <span className={css.imgSizeBar} aria-hidden="true"><span style={{ width: `${Math.max(2, (r.size / largest) * 100)}%` }} /></span>
        </span>
        <span className={css.imgRowAction}>
          {running
            ? <span className={css.imgPercent}>{share == null ? '…' : `${Math.round(share * 100)} %`}</span>
            : r.usable && load?.phase === 'failed'
              ? <Button variant="ghost" size="sm" onClick={() => void dismiss(load).catch(setError)}>{t('imgDismiss')}</Button>
              : r.usable && canCopy && (
                <Button variant="outline" size="sm" icon={<IconDownloadOutlineRegular size={12} />} title={t('imgCopyTo', { machine: m.name })}
                  aria-label={t('imgCopyTo', { machine: m.name }) + ': ' + r.name} onClick={() => void start(r.id)} data-copy-image={r.name}>
                  {t('imgCopy')}
                </Button>
              )}
        </span>
      </li>
    )
  }
  return (
    <Card className={css.block}>
      <Section title={title} count={answer.docker.images.filter((r) => r.usable).length} className={css.imagesSection}
        actions={(
          <span className={css.imgTools}>
            {total > 6 && <Input value={query} placeholder={t('imgSearchPlaceholder')} aria-label={t('imgSearch')} className={css.imgSearch} onChange={(e) => setQuery(e.target.value)} />}
            <Button variant="ghost" size="sm" icon={<IconRefreshOutlineRegular size={12} />} onClick={() => images.reload()}>{t('imgRefresh')}</Button>
          </span>
        )}>
        {total === 0
          ? <p className={css.muted}>{t('imgDockerEmpty', { machine: m.name })}</p>
          : usable.length + unusable.length === 0
            ? <p className={css.muted}>{t('imgNoMatch', { query })}</p>
            : <ul className={css.imgList} aria-label={title}>{usable.map(row)}{unusable.map(row)}</ul>}
        {!canCopy && <p className={css.muted}>{t('imgCopyNeedsRunning', { machine: m.name })}</p>}
        <BuildHint />
        {error != null && <Refusal error={error} labels={refusal} compact className={css.errorText} />}
      </Section>
    </Card>
  )
}

/**
 * The images stored on the machine, as cards (each Sandbox names its own on
 * its card): badge, name and tags, where it came from, the machine default,
 * its size, the Sandboxes using it and when one last started from it, and
 * Remove on an unused copied one. Rendered only at Stage 1 and when the
 * machine runs images (not macOS native).
 */
export function ImagesSection({ m, images, onCreate }: { m: MachineInfo, images: Query<SandboxImagesView>, /** New Sandbox from an image (its Id); absent where no Sandbox can be made here. */ onCreate?: ((image: string) => void) | undefined }): ReactNode {
  const { t, tw, refusal, pruneImages } = useMachinesContext()
  const nameOf = useImageName()
  const answer = images.data
  const [removing, setRemoving] = useState<string | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [refused, setRefused] = useState<PruneAnswer['refused']>([])
  const [freeing, setFreeing] = useState(false)
  const restarting = useRestartWorker(m, images.reload)
  const restartGuard = usePressGuard(`restart:${m.name}:${m.state}`)
  if (answer && (answer.stage !== 1 || answer.mode === 'host')) return null
  const status = imagesStatus(answer, m.state)
  // removing needs a machine that answers now: not stopped, not an outdated worker, not a failed read
  const stopped = status !== 'fresh'
  const footing = status === 'failed' ? imagesFooter(answer, m.state) : null
  const rows = answer?.ready ?? []
  const names = new Map(rows.map((r) => [r.id, nameOf(r)]))
  const now = Date.now()
  const remove = async (id: string) => {
    setRemoving(id); setError(null); setRefused([])
    try { setRefused((await pruneImages(m.name, [id])).refused) } catch (e) { setError(e) } finally { setRemoving(null); images.reload() }
  }
  return (
    <section className={css.imgStored} aria-label={t('imagesOnMachine', { machine: m.name })} data-images-stored>
      <div className={css.tabToolbar}>
        <span className={css.imgStoredTitle}>{t('imagesOnMachine', { machine: m.name })}</span>
        {answer && <span className={css.muted}>{rows.length}</span>}
        <span className={css.columnSpacer} />
        {answer && !stopped && rows.some(removable) && <Button variant="outline" size="sm" onClick={() => setFreeing(true)}>{t('freeSpaceItem')}</Button>}
      </div>
      {!answer
        ? images.error != null
          ? <Refusal error={images.error} labels={refusal} compact className={css.errorText} />
          : <p className={css.muted}>{t('imagesLoading', { machine: m.name })}</p>
        : rows.length === 0
          ? status === 'outdated' ? null : <p className={css.muted}>{t('imagesEmpty', { machine: m.name })}</p>
          : (
            <ul className={css.imgCards}>
              {rows.map((r) => {
                const bundle = r.source === 'bundle'
                const label = bundle ? { registry: '', repo: names.get(r.id) ?? '', tags: [] } : imageLabel(names.get(r.id) ?? r.name, r.refs)
                const ago = agoWords(r.lastUsed, now)
                return (
                  <li key={r.id} className={css.imgCard} data-stored-image={names.get(r.id)} title={[...r.refs, r.id].join('\n')}>
                    <div className={css.imgCardHead}>
                      <ImageBadge repo={label.repo} bundle={bundle} />
                      <span className={css.imgCardName}>
                        <ImageTitle label={label} />
                        <span className={css.imgRowSub}>{t(bundle ? 'imgBundled' : r.source === 'machine' ? 'imgFromMachine' : 'imgCopiedFromDocker')}</span>
                      </span>
                      {answer.default === r.id && <span className={css.imgDefault}>{t('imgDefault')}</span>}
                    </div>
                    {r.state && (
                      <p className={css.imgRowSub} data-image-state={r.state}
                        title={r.state === 'busy' ? t('imgStateBusyTip', { phase: r.op?.phase ?? '…' }) : t('imgStatePartialTip', { size: bytes(r.staged ?? r.size), fix: r.fix ?? '' })}>
                        <span className={css.imgWarn}>●</span> {t(r.state === 'busy' ? 'imgStateBusy' : 'imgStatePartial')}
                      </p>
                    )}
                    <dl className={css.imgFacts}>
                      <div><dt>{t('imgFactDisk')}</dt><dd>{bytes(r.size)}</dd></div>
                      <div><dt>{t('imgFactSandboxes')}</dt><dd>{String(Math.max(0, r.sandboxes))}</dd></div>
                      <div><dt>{t('imgFactLastUsed')}</dt><dd>{ago ? tw(ago) : t('imgNever')}</dd></div>
                    </dl>
                    {((onCreate && r.usable) || (!stopped && removable(r) && r.sandboxes <= 0 && r.state !== 'busy')) && (
                      <div className={css.imgCardActions}>
                        {onCreate && r.usable && (
                          <Button variant="outline" size="sm" icon={<IconPlusOutlineRegular size={12} />} onClick={() => onCreate(r.id)} data-new-sandbox-image={names.get(r.id)}>
                            {t('newSandbox')}
                          </Button>
                        )}
                        <span className={css.columnSpacer} />
                        {!stopped && removable(r) && r.sandboxes <= 0 && r.state !== 'busy' && (
                          <Button variant="ghost" size="sm" icon={<IconTrashOutlineRegular size={12} />} disabled={removing !== null} onClick={() => void remove(r.id)}
                            className={css.imgRemove} data-remove-image={names.get(r.id)}>
                            {removing === r.id ? t('imageRemoving') : t('imageRemove')}
                          </Button>
                        )}
                      </div>
                    )}
                  </li>
                )
              })}
            </ul>
          )}
      {answer && (
        <div className={css.imgStoredFoot}>
          {status === 'outdated'
            ? (
              <div className={css.imagesOutdated} role="status">
                <p>{t('imagesWorkerOutdated', { machine: m.name })}</p>
                <Button variant="outline" size="sm" disabled={restarting.pending !== null || restartGuard.settling} aria-disabled={restartGuard.settling || undefined} onClick={restartGuard(() => void restarting.restart())}>
                  {restarting.pending ? t('restartingWorker') : t('restartWorker')}
                </Button>
              </div>
            )
            : <p>{status === 'stopped' ? t('imagesStoppedNote', { machine: m.name }) : footing ? tw(footing) : t('imagesNote')}</p>}
        </div>
      )}
      {restarting.error != null && <Refusal error={restarting.error} labels={refusal} compact className={css.errorText} />}
      {error != null && <Refusal error={error} labels={refusal} compact className={css.errorText} />}
      <Refused refused={refused} names={names} machine={m.name} />
      {freeing && <FreeSpaceDialog machine={m} onClose={() => { setFreeing(false); images.reload() }} />}
    </section>
  )
}

/**
 * Free space on a machine: one checkbox per removable image, the unused ones
 * preselected and the ones in use disabled. `need` (from New Sandbox, or a
 * create refused Capacity) is said beside what the choice frees. `onClose`
 * says whether anything was removed.
 */
export function FreeSpaceDialog({ machine, need, onClose }: { machine: MachineRef, need?: number | null | undefined, onClose: (freed: boolean) => void }): ReactNode {
  const { t, tw, refusal, pruneImages } = useMachinesContext()
  const images = useSandboxImages(machine)
  const nameOf = useImageName()
  /** Images LayerFS refused to remove in this dialog, with its words: unchecked and held, whatever the listing still counts. */
  const [held, setHeld] = useState<ReadonlyMap<string, string>>(new Map())
  const { rows, preselected } = freeSpaceRows(images.data, held)
  // undefined: not touched yet, the unused images are checked
  const [picked, setPicked] = useState<string[] | undefined>(undefined)
  const checked = (picked ?? preselected).filter((id) => !held.has(id) && rows.some((r) => r.id === id && r.sandboxes <= 0))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<unknown>(null)
  const [refused, setRefused] = useState<PruneAnswer['refused']>([])
  const [freed, setFreed] = useState(false)
  const names = new Map(rows.map((r) => [r.id, nameOf(r)]))
  const close = () => { if (!busy) onClose(freed) }
  const remove = async () => {
    setBusy(true); setError(null); setRefused([])
    try {
      const answer = await pruneImages(machine.name, checked)
      const any = freed || answer.removed.length > 0
      setFreed(any)
      setPicked(undefined)
      if (!answer.refused.length) { setBusy(false); onClose(any); return }
      setRefused(answer.refused)
      setHeld((now) => new Map([...now, ...answer.refused.map((r) => [r.id, r.message] as const)]))
      images.reload()
    } catch (e) { setError(e) }
    setBusy(false)
  }
  const toggle = (id: string, on: boolean) => setPicked((now) => {
    const base = (now ?? preselected).filter((x) => x !== id)
    return on ? [...base, id] : base
  })
  return (
    <Modal open onClose={close} title={t('freeSpaceTitle', { machine: machine.name })} description={t('freeSpaceBody')} closeLabel={t('close')}
      className={css.dialog} contentClassName={css.dialogContent}
      footer={(
        <>
          <Button variant="outline" disabled={busy} onClick={close}>{t('cancel')}</Button>
          <Button variant="primary" disabled={busy || checked.length === 0} onClick={() => void remove()}>
            {busy ? t('imageRemoving') : checked.length === 1 ? t('removeImagesOne') : t('removeImages', { n: String(checked.length) })}
          </Button>
        </>
      )}>
      <div className={css.form}>
        {!images.data
          ? images.error != null
            ? <Refusal error={images.error} labels={refusal} compact className={css.errorText} />
            : <p className={css.muted}>{t('imagesLoading', { machine: machine.name })}</p>
          : rows.length === 0
            ? <p className={css.muted}>{t('freeSpaceNone', { machine: machine.name })}</p>
            : (
              <ul className={css.freeList}>
                {rows.map((r) => {
                  const used = r.sandboxes > 0
                  const no = held.get(r.id)
                  return (
                    <li key={r.id} className={css.freeRow} data-held={no !== undefined || undefined}>
                      <Checkbox checked={checked.includes(r.id)} disabled={busy || used || no !== undefined} label={names.get(r.id) ?? r.id}
                        title={r.refs.join('\n') || undefined} onChange={(on) => toggle(r.id, on)} className={css.freeCheck} />
                      <span className={css.imageTag}>{bytes(r.size)}</span>
                      {no !== undefined
                        ? <span className={css.imageTag} data-off title={no}>{t('imageNotRemoved')}</span>
                        : <span className={css.imageTag} data-off={used || undefined}>{used ? t('inUseBy', { n: String(r.sandboxes) }) : t('imagesUnused')}</span>}
                    </li>
                  )
                })}
              </ul>
            )}
        {images.data && rows.length > 0 && <p className={css.muted}>{tw(freesWords(rows, checked, need))}</p>}
        {error != null && <Refusal error={error} labels={refusal} className={css.errorText} />}
        <Refused refused={refused} names={names} machine={machine.name} onLeave={close} />
      </div>
    </Modal>
  )
}

/**
 * dsh-layerfs, browser half: mounts the `layerfs` Remote namespace, opens the
 * host event stream, and contributes the sidebar's one LayerFS row and the
 * Center page it opens (machines, Stores, projects, backups, and the Health
 * and Logs pages under Center's bar), the Projects views, the
 * conversation views and the New Session project control through DSH's slots.
 */
import { createTypertRemoteInfo } from '@deepseek-ai/dsh-typert-protocol/remote-info'
import { z } from 'zod'
import { remoteSpec } from '../shared/remote-spec.ts'
import { createApi } from './api.ts'
import { locales as doctorLocales, NS as DOCTOR_NS } from './doctor/locale.ts'
import { NS, locales } from './locale.ts'
import { registerMachines } from './machines/index.tsx'
import { PANEL_ID as STORAGE_PANEL, registerStorage } from './storage/index.tsx'
import { registerSession } from './session/index.tsx'
import { registerPresets } from './presets/index.tsx'
import { registerLogs } from './logs/index.tsx'
import { registerProjects } from './projects/index.tsx'
import { registerPicker } from './picker/index.tsx'
import { registerHero } from './hero/index.tsx'
import { registerSimulation } from './simulation/index.tsx'
import { registerViews } from './views/index.tsx'
import { registerSetup } from './setup/index.tsx'
import { syncLocale } from './ui/locale-sync.ts'

export const inject = ['slots', 'locale', 'remote', 'layout']

export function apply(ctx: any): void {
  ctx.effect(() => ctx.locale.register(NS, locales), 'dsh-layerfs: locale')
  ctx.effect(() => ctx.locale.register(DOCTOR_NS, doctorLocales), 'dsh-layerfs: doctor locale')
  // times, dates and memoized views follow the active locale (ui/locale-sync.ts)
  const followLocale = () => syncLocale(ctx.locale.getLocale?.().active)
  followLocale()
  ctx.effect(() => ctx.locale.subscribe?.(followLocale) ?? (() => {}), 'dsh-layerfs: locale clock')
  const info = createTypertRemoteInfo(remoteSpec(z) as never, z)
  ctx.effect(() => {
    let dispose: undefined | (() => unknown)
    let gone = false
    void ctx.remote.$mount(info).then((d: () => unknown) => { if (gone) void d(); else dispose = d })
    return () => { gone = true; void dispose?.() }
  }, 'dsh-layerfs: remote')
  const workspace = () => ctx.get?.('uiWorkspace')
  const api = createApi(() => ctx.get?.('remote.layerfs'), {
    createSession: (workspaceId) => ctx.get('sessions').create({ workspaceId }),
    currentSessionId: () => {
      const summaries = ctx.get?.('sessions')?.list?.getSnapshot?.()?.byId ?? {}
      return (Object.values(summaries) as any[]).find((session) => (session.retainedBy?.mainView ?? 0) > 0)?.id
    },
    openPanel: (id) => ctx.layout.selectPanel(id),
    openWorkspace: async (id) => { await workspace()?.openWorkspace(id) },
    openSession: (id) => { workspace()?.openSession(id) },
  }, workspace)
  ctx.effect(() => () => api.dispose(), 'dsh-layerfs: api')
  // Browser automation handle (tools/, test/): opt-in per browser profile.
  if (typeof window !== 'undefined' && window.localStorage?.getItem('dsh-layerfs:debug') === '1') {
    ctx.effect(() => { (window as any).__dshLayerfs = api; return () => { delete (window as any).__dshLayerfs } }, 'dsh-layerfs: debug')
  }
  const machines = registerMachines(ctx, api, [STORAGE_PANEL])
  registerStorage(ctx, api, machines)
  registerSetup(ctx, api, machines.t)
  registerPresets(ctx)
  registerLogs(ctx)
  registerProjects(ctx, api)
  registerSession(ctx, api)
  registerPicker(ctx, api)
  registerHero(ctx, api)
  registerSimulation(ctx, api)
  registerViews(ctx, api)
}

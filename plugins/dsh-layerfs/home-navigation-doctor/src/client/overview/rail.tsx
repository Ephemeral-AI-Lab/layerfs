import { useLayoutEffect, useSyncExternalStore, type ReactNode } from 'react'
import { Button, IconPanelLeftOutlineRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import type { ListT } from '../projects/list-locale.ts'

interface RailLayout {
  toggleSidebar(): void
  panelInfo: { getSnapshot(): { activePanelId: string | null }, subscribe(fn: () => void): () => void }
}

/** `sidebar.panellist` supplies an icon. The footer supplies `wide` and owns an independent button. */
export function createSidebarRail(layout: RailLayout, panelId: string) {
  let wide = false
  const active = () => layout.panelInfo.getSnapshot().activePanelId === panelId
  const subscribe = (notify: () => void) => layout.panelInfo.subscribe(notify)
  function collapse() {
    if (!wide) return
    layout.toggleSidebar()
  }
  return {
    collapse,
    Footer({ wide: shown, t }: { wide: boolean, t: ListT }): ReactNode {
      const selected = useSyncExternalStore(subscribe, active)
      useLayoutEffect(() => { wide = shown; return () => { wide = false } }, [shown])
      if (!selected || !shown) return null
      return <Button variant="ghost" size="sm" icon={<IconPanelLeftOutlineRegular size={16} />}
        aria-label={t('foldCenter')} title={t('foldCenter')} onClick={collapse} />
    },
  }
}

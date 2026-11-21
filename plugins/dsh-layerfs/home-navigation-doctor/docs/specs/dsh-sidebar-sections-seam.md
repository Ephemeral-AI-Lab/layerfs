# DSH seam request: Workspaces views (or `sidebar.sections`)

Status: proposal for DeepSeek Harness (not implemented; dsh-layerfs does not
change DSH code). Checked against `deepseek-harness-source` at `fcc0ad46`
(0.2.2-rc.1), `packages/client/ui-sidebar` and `packages/client/ui-workspace`.

Two alternative requests. **Preferred: Workspaces views**, so LayerFS projects
live inside the one Workspaces section behind a switch button in its header.
Fallback: a `sidebar.sections` list slot for a separate section.

## Why

dsh-layerfs wants its **LayerFS Projects** tree (projects, their sessions,
readiness folds) in the sidebar beside DSH's native workspace list: preferably
as a view of the **Workspaces** section behind a header switch, otherwise as a
section of its own, with DSH's code and rendered nodes left as DSH ships them.

The expanded sidebar's scroll area offers a plugin only:

| Slot | Kind | Usable for a section? |
| --- | --- | --- |
| `sidebar.panellist` | list | No: one launcher row per main panel |
| `sidebar.workspaces` | single | No: occupied by ui-workspace's browser, which is not exported to wrap |
| `sidebar.footer.action` | list | No: foot buttons beside Settings |

### Current public controls

In the DSH view, **Show LayerFS projects** is a `sidebar.footer.action`
button beside Settings. Selecting it registers the LayerFS tree in the single
`sidebar.workspaces` slot at priority -10. The LayerFS header owns **Show DSH
workspaces** and the **New session or project** menu. Switching back disposes
the occupant and restores DSH’s browser. The selection is remembered per
browser. On the rail, **LayerFS Projects** uses the slot’s `expandSidebar`.

The one `sidebar.panellist` launcher opens Center. The plugin observes the
public `layout.panelInfo` transition and folds the wide sidebar on entry.
DSH owns the launcher’s selected state. When Center is already active, use
**Fold sidebar with LayerFS Center open** in the wide footer. It is a native
button with a localized accessible name. Enter or Space calls
`layout.toggleSidebar()` and keeps the current Center page selected. The
`sidebar.panellist` icon receives `{ size, active }`. `sidebar.footer.action`
receives `{ wide }`. DSH exposes no activation callback for a repeated press.

To verify the sidebar, build the plugin and start a private dev host with
`node scripts/dev.mjs --port 3091` under Node 24. Open the URL printed by the
host. Check both Workspaces views, open Center from the rail, and verify that
folding and expanding the sidebar keeps the current Center page selected.

A switch inside DSH’s own header needs a public header action or view registry.
Folding on a repeated launcher press needs a public panel activation event that
also fires for the active panel. Both requests concern DSH. The plugin uses
the footer and the shell toggle on hosts without these hooks.

Approaches ruled out without a DSH change:

- Shadowing `sidebar.workspaces` at a lower priority with the stock entry's
  inject, store and locale and rendering the stock component inside a wrapper
  (both lists at once): the stock browser crashes (`renderSlot is not a function`). `renderSlot` is only given to the
  entry that declares the child slots (directory flow, session row actions,
  menu items), and declaring them again throws.
- Disabling the `ui-workspace` row and calling its exported `apply` with that
  registration wrapped: a disabled row's browser bundle is not shipped, so it
  would have to be bundled into dsh-layerfs, which is vendoring DSH code.

## Preferred: Workspaces views (`ui-workspace`)

The design: one **Workspaces** section. A button in its header switches the
list between DSH's workspace folders and a plugin's view (LayerFS Projects);
the header label, its position and the region's scroller stay DSH's.

```
Workspaces            [◇] 🔍 ⚙ +      ← [◇] switches views; shown only when a view is registered
📁 ~/code/site                         DSH view (default): the stock list
```
```
Workspaces            [◆]             ← LayerFS view active: search, filter, add are hidden
Ready · 2                              the active view's component fills the list area
📁 app · main-store
```

Today ui-workspace's `sidebar.workspaces` entry declares only
`directoryFlow`, `session.row.action` and the per-row `sidebar.session.row.*`
seats. The header (`WorkspaceBrowser.tsx`, `.sectionHeader`) and the list area
(`.listArea`) have no plugin seam, so a plugin can only get this layout by
inserting into and hiding DSH-rendered nodes, which dsh-layerfs does not do.

### Contract

One list slot declared by the browser entry, plus one navigation method:

```ts
// ui-workspace/src/client/contract/slots.ts — SlotMap
/**
 * Alternative views of the Workspaces list. While one is active it replaces the
 * list area (the browser keeps its header label, scroller and dialogs) and the
 * header hides search, view options and Add workspace. The header shows a
 * switch button only while this slot has registrations; the button's menu lists
 * "Workspaces" (the stock view) and every view by `label`, in `order`.
 */
'sidebar.workspaces.view': { kind: 'list'; scope: 'root'; owner: WorkspacesViewOwnerProps }

export interface WorkspacesViewOwnerProps {
  /** Shell fold state, as the browser gets it. */
  readonly wide: boolean
  /** Bind the view's vertical scroller (the browser's `useSectionScroller` hand-off). */
  readonly bindScrollerRef: RefCallback<HTMLElement>
  /** Return to the stock Workspaces view. */
  readonly showWorkspaces: () => void
}

// ui-workspace/src/client/navigation.ts — UiWorkspace
/**
 * Show a registered Workspaces view by id, or the stock view with null.
 * Unknown ids fall back to the stock view.
 */
selectView(id: string | null): void
```

The active view id lives in the browser's view store and persists per browser
like its other view options. Registrations are ordinary list entries
(`id`, `order`, `label`, optional `sidebar.panel.icon`-style icon), so a plugin
reload or removal falls back to the stock view.

### Implementation sketch (DSH side, about 60 lines)

- `index.ts`: add `'sidebar.workspaces.view': { kind: 'list', scope: 'root' }` to
  the browser entry's `children`; add `selectView` to the `uiWorkspace` service,
  writing the view store.
- `WorkspaceBrowser.tsx`: read `activeView` from the store and the slot's
  entries (`entriesOfSlot`); in `.sectionHeader`, render a switch button (menu of
  views) before the search slot when entries exist, and skip search, view
  options and Add workspace while a view is active; in place of `.listArea`'s
  rows, `renderSlot('sidebar.workspaces.view', { wide, bindScrollerRef,
  showWorkspaces })` filtered to the active id.
- Locale: the button's accessible name and the stock view's menu label.

### Plugin side once it exists

- Register `ProjectTree` (`src/client/projects/ProjectList.tsx`) as view
  `layerfs` labeled "LayerFS Projects", binding its scroller.
- Call `uiWorkspace.selectView('layerfs')` when a project session opens, and
  `selectView(null)` for a folder session, so the list follows the open session.
- Replace the footer switch and shadowing registration (`WorkspacesView.tsx`)
  with the public header switch, keeping the selected view.
- Update §7 of `docs/specs/project-sessions.md`.

## Fallback: `sidebar.sections` (`ui-sidebar`)

### Contract

A `list` slot rendered between the panel list and the workspace region, owned by
`ui-sidebar` beside its other holes.

```ts
// ui-sidebar/src/client/contract/slots.ts — SlotMap
/**
 * Additional sidebar sections, rendered between the global panel list and the
 * `sidebar.workspaces` browser. They scroll with the navigation (the sidebar
 * viewport), so the workspace browser's own scroller and the wheel hand-off are
 * unchanged. Each occupant owns its header, fold and rows; in the rail
 * (`wide: false`) it may render nothing or a single icon button.
 */
'sidebar.sections': { kind: 'list'; scope: 'root'; owner: SidebarSectionOwnerProps }
```

`SidebarSectionOwnerProps` (`wide`, `expandSidebar`) is reused as is. Entries
sort by an `order` registration field (ties keep registration order), the way
`sidebar.panellist` entries do.

### Implementation sketch (DSH side, about 15 lines)

```ts
// ui-sidebar/src/client/index.ts — 'sidebar' children
'sidebar.sections': { kind: 'list', scope: 'root' },
```

```tsx
// ui-sidebar/src/client/SidebarRoot.tsx — between </nav> and .regionArea
<div className={css.sectionsArea}>
  {renderSlot('sidebar.sections', { wide, expandSidebar: () => { if (collapsed) toggleSidebar() } })}
</div>
```

```css
/* SidebarRoot.module.css */
.scrollArea { grid-template-rows: 8px max-content 2px max-content max-content; }
.sectionsArea { grid-column: 2; grid-row: 4; min-width: 0; display: flex; flex-direction: column; }
.sectionsArea:empty { display: none; }
.regionArea { grid-row: 5; }          /* was 4 */
.railIn .sectionsArea { animation: rail-in 150ms var(--ds-ease-in-out) backwards; }
:global([data-windows-titlebar]) .collapsed .sectionsArea { display: none; }
```

Scrolling: sections are part of what the viewport scrolls away before the
workspace browser's scroller takes over (`scroll-handoff.ts` keys on the
viewport reaching its end), so a long LayerFS section scrolls away first and the
hand-off needs no change.

### Plugin side once it exists

- Register a LayerFS Projects section (the `ProjectTree` from
  `src/client/projects/ProjectList.tsx`) into `sidebar.sections`.
- Drop the section's height cap if the slot's row scrolls with the navigation.
- Update §7 of `docs/specs/project-sessions.md`.

## Open points

- On a DSH build without either slot, the plugin must not register into it;
  keep the footer switch and shadowing Workspaces occupant.
- Check where native DSH lists sessions with no workspace. Project sessions are
  detached from every workspace (`SessionService.detach`), so they must not
  show again in the native section.
- DSH's header search and session shortcuts will not cover LayerFS sessions.
  With Workspaces views, a view could later get its own search through an
  optional `search` registration field.

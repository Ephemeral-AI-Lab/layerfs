# LayerFS TUI design specification

Status: **binding Phase Two UX specification for `layerfs-tui`**.

Implementation starts only after the complete Phase One terminal pass in
`05-implementation-plan.md`. The TUI consumes the frozen CLI command, plan,
completion, event, snapshot, paging, output, and result contract. It must not
request ordinary backend redesign during Phase Two.

The TUI is an optional visual client for the complete standalone `layerfs-cli`
product. It uses `layerfs-cli` in-process for parsing, completion, execution,
queries, progress, and results. It consumes `layerfs-monitor` snapshots and
events through the CLI context. It must not depend directly on `layerfs-sdk`,
`layerfs-monitor`, `layerfs-workspace`, `layerfs-content`, `layerfs-storage`,
Store crates, or
low-level Store databases; invoke Docker; or implement a second command
language.

```text
layerfs-tui
    Ratatui/Crossterm UI only
          |
          v
layerfs-cli
    complete standalone command application and reusable Rust library
          |
          `--> layerfs-sdk
                 thin composition and Store graph/Layer/Stack/Branch semantics
                       |
                       +--> layerfs-workspace
                       |      COW/spool, session, placement, execution/output
                       |
                       `--> layerfs-monitor
                              snapshots, dedup/resource/timing receipts/events
```

The same command must mean the same thing in both products:

```text
Standalone terminal:
    layerfs workspace exec w:0195... -- npm test

TUI command line:
    > workspace exec w:0195... -- npm test
```

There is no permanent “allowed operations” panel and no button grid mirroring
the CLI. Users act through the command line. The TUI makes the system legible,
selectable, completable, previewable, observable, and safe.

## 1. Product goals

The default screen must make LayerFS's core value obvious before it asks the
user to inspect topology: repeated logical content collapses to one canonical
payload set inside each physical Store that is required to own it. The impact
must be large, honestly scoped, timestamped, and drillable.

The interface must also make five structural facts obvious without requiring
documentation:

1. One LayerStore is the authority for the active graph.
2. A LayerStore may have zero or more StackStores and zero or more direct
   BranchStores.
3. A StackStore may have zero or more BranchStores.
4. A BranchStore may have zero or more ephemeral Workspaces.
5. A Workspace may be projected into any permitted host directory or Docker
   container while every Store database remains on its configured host.

```text
LayerStore (1)
├── StackStore (0..N)
│   └── BranchStore (0..N per StackStore)
│       └── Workspace (0..N per BranchStore)
└── BranchStore (0..N direct)
    └── Workspace (0..N per BranchStore)
```

The TUI should feel like a precise filesystem control room, not a database
browser. It shows domain records, routes, state, and measured effects; it never
shows raw tables, SQL, transfer frames, or Store-internal classes.

### 1.1 Non-goals

- Reimplementing any CLI or SDK operation in the TUI.
- Embedding a terminal emulator for interactive shells in the first release.
- Displaying every command as a persistent button.
- Pretending that the same object stored once in each of three physical
  databases is one physical copy.
- Pretending that ten active Workspace spools or materialized directories have
  already collapsed to one durable Store copy.
- Storing TUI state, monitoring samples, or command logs in Store tables.
- Giving a Docker container database paths, SQLite access, or Store
  credentials.
- Introducing a `Project` UI entity or command group.

## 2. Grok Build reference study

The LayerFS design learns from Grok Build’s terminal craft but does not copy
its product hierarchy.

### 2.1 Observed upstream patterns

The following are directly observable in the primary Grok Build repository:

| Observed pattern | Primary evidence |
|---|---|
| Rust full-screen TUI with mouse interaction and a separate headless path | [Repository overview](https://github.com/xai-org/grok-build) |
| Neutral dark `GrokNight` base, restrained borders, pale primary text, magenta active accent, and distinct success/warning/error colors | [GrokNight source](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager-render/src/theme/groknight.rs) |
| Central themes, runtime theme preview, truecolor/256/16-color quantization, `NO_COLOR`, compact mode, and a terminal-native minimal mode | [Theming guide](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/06-theming.md) |
| Fullscreen, inline/minimal, and alternate-screen policies rather than assuming one terminal environment | [Configuration guide](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/05-configuration.md) |
| Dense selectable rows with state glyphs, secondary activity text, collapsible sections, contextual peek, persistent input chrome, mouse hover/click, and narrow-layout behavior | [Dashboard guide](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/23-dashboard.md), [dashboard renderer](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/views/dashboard/render.rs), and [responsive layout](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/views/dashboard/layout.rs) |
| Explicit focus ownership, keyboard-first navigation, Vim alternatives, staged `Esc` behavior, and shortcuts that remain visible for the active context | [Keyboard shortcuts](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/03-keyboard-shortcuts.md) |
| An optional, width-aware status line that can include elapsed time | [Status-line guide](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/25-status-line.md) |

### 2.2 LayerFS-specific inferences

These are design decisions for LayerFS, not claims about Grok Build:

- Replace the agent roster with an authority-first Store topology tree.
- Replace conversation scrollback with a contextual history/runtime canvas.
- Keep Grok Build’s quiet chrome and dense rows, but use LayerFS status colors
  and its supplied layer icon.
- Keep one persistent command line; completion and contextual preview replace
  operation buttons.
- Make measured Dedup Impact the dominant default dashboard; Store topology,
  histories, Workspace runtime, and timing are its drill-down dimensions.
- Use a stable inspector rather than a transient agent peek on wide terminals,
  because Store routes and Workspace placement require side-by-side comparison.
- Treat elapsed time, transfer, CPU, memory, and storage as supporting evidence
  around the primary deduplication outcome rather than optional decorations.

## 3. Visual language

### 3.1 Character

The intended character is:

```text
quiet charcoal canvas
precise monospaced hierarchy
small, meaningful color accents
thin borders
dense information with breathing rows
motion only for live activity
```

Avoid:

- rainbow state decoration;
- large banners after initial launch;
- boxed card grids for every record;
- gradients, shadows, or decorative animation;
- emoji as controls or state icons;
- low-contrast gray metadata;
- hiding critical state exclusively in color.

### 3.2 Default truecolor palette

The default theme is inspired by GrokNight’s neutral base and TokyoNight accent
family, while assigning colors to LayerFS meanings rather than agent roles.

| Token | Color | Use |
|---|---|---|
| `canvas` | `#101114` | terminal background |
| `surface` | `#15171B` | panels and command input |
| `surface_selected` | `#24262D` | keyboard selection |
| `surface_hover` | `#1C1E24` | mouse hover only |
| `border` | `#343741` | passive border/rules |
| `border_focus` | `#7AA2F7` | focused pane and active input |
| `text` | `#E7E9EE` | primary text |
| `text_secondary` | `#B7BDC9` | readable metadata |
| `text_muted` | `#7E8798` | inactive hints only |
| `layer` | `#7DCFFF` | LayerStore/Layer identity |
| `stack` | `#BB9AF7` | StackStore/Stack identity |
| `branch` | `#FF9E64` | BranchStore/Branch identity |
| `workspace` | `#73DACA` | Workspace/runtime identity |
| `success` | `#9ECE6A` | completed/clean/up-to-date |
| `warning` | `#E0AF68` | behind, waiting, partial |
| `error` | `#F7768E` | conflict, failure, integrity error |
| `running` | `#7DCFFF` | active operation/spinner |

Rules:

- Pane focus uses border plus a textual focus label; never color alone.
- State uses a glyph, word, and color together: `! CONFLICT`, not a red dot.
- Primary body text and active metadata must meet a 4.5:1 contrast target
  against their actual background.
- Theme colors must be quantized centrally for 256- and 16-color terminals.
- `NO_COLOR` removes color but preserves borders, glyphs, and words.

### 3.3 Row rhythm

Normal topology and record rows occupy one line. A selected or active row may
gain one secondary line for the most relevant live detail. This keeps the tree
dense while making current work readable.

```text
  ○ branch.db                      14 branches · 2 workspaces
▌ ● branch-build.db               active · direct to layer.db
    w:0195f632  Docker/fuse        npm test · 00:08.4
```

Use `▌` as the primary selection bar where supported and `>` as the ASCII
fallback. Use `▾`/`▸` for disclosure with `v`/`>` fallbacks.

### 3.4 Motion

Only active state moves:

- a low-frequency spinner for a running CLI/SDK operation;
- a live elapsed timer;
- a transfer/progress gauge whose underlying value changed;
- a subtle streaming-output cursor.

The default refresh ceiling is 10 frames per second during activity and
2 frames per second for passive monitoring. Idle screens redraw only on input
or changed data. A reduced-motion setting replaces spinners with `RUNNING`.

## 4. Canonical icon/avatar

The user-supplied LayerFS image is the sole canonical icon. In this documentation
set it is [layerfs.png](../tui/layerfs.png), a 512×512 RGBA PNG with a transparent
background and SHA-256:

```text
93ed656af193e3830e39741b7d224fbc79e2c3c6d6001df1453afb0e1ef9c4d6
```

It remains at the user-selected source path `docs/tui/layerfs.png`;
documentation must not relocate or alter its bytes. Do not redraw, recolor,
crop, add a background permanently, or create a competing logo.

Use it for:

- documentation and repository avatar;
- welcome screen when the terminal supports an inline-image protocol;
- future web favicon/app-icon derivatives;
- future packaged application icons.

A standard Ratatui/Crossterm application cannot guarantee PNG rendering or set
the terminal emulator’s window icon. Therefore:

| Capability | TUI behavior |
|---|---|
| inline image supported and explicitly enabled | render the exact PNG on a temporary high-contrast tile without modifying the source |
| no image protocol | render the compact text mark `LayerFS` plus a three-layer ASCII glyph |
| very narrow terminal | render `LFS` only |
| headless CLI | no logo/banner by default |

Portable fallback:

```text
     /\____/\
    /  ____  \
    \_/____\_/
      LayerFS
```

The fallback is an interface glyph, not a replacement brand asset.

## 5. Information architecture

### 5.1 Default: Dedup Impact dashboard

After a LayerStore is connected, the root/default canvas is Dedup Impact. It
answers, in order:

```text
How much repeated logical content did LayerFS avoid storing?
How complete and fresh is that measurement?
Where is the one required canonical set placed?
Which Store, Branch, Workspace, or operation produced the impact?
```

The hero may show a large percentage only when `layerfs-monitor` reports a
complete, exact byte baseline for the displayed scope. A qualifying example:

```text
╭─ DEDUP IMPACT · last 10 equivalent commits · full-closure coverage ─────╮
│                                                                         │
│       90% SAVED                       10× COLLAPSE                       │
│                                                                         │
│  10 logical payload sets   ───────▶   1 canonical payload set / DB      │
│  1.00 GiB candidates                   100 MiB stored per required DB    │
│  9 copies avoided                      900 MiB avoided inside each DB    │
│                                                                         │
│  coverage  FULL CLOSURE   fresh  2s ago   window  10 equivalent commits│
╰─────────────────────────────────────────────────────────────────────────╯
```

The statement `1 canonical payload set / DB` is mandatory. The hero must never
collapse required independent database placement into a fictitious global
single copy.

For the canonical demo, the cohort is ten equivalent `npm install` Workspace
commits. The dashboard tells the story diagrammatically:

```text
w:01 npm install -> Commit ┐
w:02 npm install -> Commit ├─ 10 equivalent logical Q candidates
w:03 npm install -> Commit │
...                        │        CAS/CDC identity collapse
w:10 npm install -> Commit ┘                    |
                                                 v
                                 Q stored once per required Store DB

result: 9 Q copies avoided per DB · 10× collapse · 90% saved
```

The demo label says `10 npm install commits`, not merely `10 installs`, so the
viewer knows the result applies to committed Store content rather than ten
simultaneously active materialized Workspaces.

`Q` denotes the equivalent canonical package payload set. Distinct Commit,
Branch, Layer, Stack, and structural fact bytes remain counted and visible in
their own byte domains; the hero must not imply that those intentionally
different records collapse to one record.

Below the hero, the pipeline makes two- versus three-database placement
explicit:

```text
DIRECT ROUTE

10 logical commits
      |
      v
BranchStore  [payload Q: 1×]
      |
      | missing-only Push
      v
LayerStore   [payload Q: 1×]

Dedup outcome:      9 repeated Q payloads avoided inside each required DB
Required placement: 2 independent durable copies of Q
Not a failure:      BranchStore 1× + LayerStore 1×
```

```text
STACKED ROUTE

10 logical commits
      |
      v
BranchStore  [payload Q: 1×]
      |
      v
StackStore   [payload Q: 1×]
      |
      v
LayerStore   [payload Q: 1×]

Dedup outcome:      9 repeated Q payloads avoided inside each required DB
Required placement: 3 independent durable copies of Q
Not a failure:      Branch 1× -> optional Stack 1× -> Layer 1×
```

If coverage is `changed set`, `negotiated frontier`, `sampled`, `stale`, or
`not measured`, the hero replaces the percentage with the measured fact:

```text
DEDUP IMPACT
5.4 MiB sent · 4,707 announced IDs already known · 112 objects inserted
byte savings rate unavailable · coverage: negotiated frontier · fresh: 1s
```

It must not turn an ObjectId count ratio into a byte-savings rate.

With no qualifying receipt yet, the connected default is:

```text
DEDUP IMPACT · NOT MEASURED

No exact candidate window is available for this scope.
Store inventory is healthy; no savings percentage has been inferred.

Run a real Commit/Push/Add, or request explicit analysis:
  monitor dedup --route branch-a --analyze
```

This is preferable to an attractive but invented `0%` or `100%`.

### 5.2 Drill-down dimensions

The hero is not a decorative summary. Every number drills down through the
same four semantic levels:

| Selection | Primary question | Main content |
|---|---|---|
| LayerStore | What canonical data and accepted Layer impact exist at authority? | required Layer placement, Layer histories, Add timings |
| StackStore | What was reused while acquiring/building/pushing Stacks? | Stack placement, boundary avoidance, Stack history |
| BranchStore/Branch | Which commits collapsed repeated payloads before Push? | local CAS reuse, Branches, Commit DAG, Push timings |
| Workspace | What candidate content did this Commit produce, and what remains transient? | committed candidate/reuse receipt plus active spool/materialization |
| Operation | Which phase cost time and where did bytes disappear? | elapsed spans, candidate/known/sent/inserted/raced, transactions/turns |

CPU, memory, storage, and timing remain visible supporting measures, but Dedup
Impact is the default explanatory frame.

Keyboard/mouse drill-down is one reversible chain:

```text
Dedup Impact hero
    |
    | Enter / click selected impact row
    v
LayerStore impact
    |
    +--> StackStore impact, when route is stacked
    |       |
    |       `--> Stack / Push operation receipt
    |
    `--> BranchStore impact
            |
            +--> Branch / Commit cohort
            |       |
            |       `--> Workspace Commit receipt
            |               |
            |               `--> operation timing phases
            |
            `--> active Workspace transient disk/resource view

Esc / breadcrumb always returns one level without changing scope data.
```

The drill-down never turns a monitor row into an implicit operation. Mutations
still require an explicit CLI command in the bottom input.

### 5.3 Topology tree

The left navigator is the canonical graph view. It is authority-first for
human comprehension even though writes flow upward.

```text
▾ LayerStore  /data/layer.db
  ├─▾ StackStore  /data/stack-a.db
  │   ├─▾ BranchStore  /data/branch-a.db
  │   │   ├─● w:0195f62e  host/materialize
  │   │   └─◌ w:0195f632  docker/fuse
  │   └─▸ BranchStore  /data/branch-b.db
  ├─▸ StackStore  tcp://builder-2:7701
  ├─▾ BranchStore  /data/direct.db       [direct]
  │   └─● w:0195f63a  host/materialize
  └─  BranchStore  tcp://branch-remote:7702 [direct]
```

Tree rules:

- Children are sorted: pinned, attention required, active, then lexical
  normalized location.
- Direct BranchStores carry a visible `[direct]` badge.
- Workspaces are runtime leaves, never shown as databases.
- A Workspace row always shows `host|docker` and `fuse|materialize`.
- Multiple Workspaces in the same container remain separate rows.
- Disconnected saved locations appear only in the database management view,
  not as live topology children.
- The tree never invents user-assigned Store names. Locations are shortened in
  the row and shown in full in the inspector.

### 5.4 History and runtime canvas

Selecting a topology node scopes the Dedup Impact header and changes the center
drill-down canvas:

```text
LayerStore selected  -> Layer histories and Layer lineage
StackStore selected  -> pulled Layers and Stack lineage
BranchStore selected -> Branch list and selected Commit DAG
Workspace selected   -> Workspace state, executions, output, diff
Operation selected   -> timing spans and exact dedup/transfer receipt
```

A breadcrumb preserves ancestry:

```text
layer.db / stack-a.db / branch-a.db / w:0195f632
```

Each segment is selectable by keyboard or mouse. The breadcrumb is navigation,
not a command surface.

### 5.5 Inspector

The inspector shows the selected record’s identity and invariants:

```text
BranchStore
location      /data/branch-a.db
route         branch-a -> stack-a -> layer
schema        Branch · valid
branches      14
workspaces    2 active
CAS bytes     284.2 MiB
last op       branch push · 83 ms · up to date
```

The inspector must never show raw columns, SQL, credentials, or opaque protocol
frames. IDs are copyable and visually shortened but full IDs are used in copied
commands.

## 6. Screen layouts

The layout responds to terminal cells, not assumed pixels.

### 6.1 Wide: at least 132 columns and 32 rows

```text
┌ LayerFS ─ DEDUP IMPACT ─ layer.db / stack-a / branch-a ────────────────────────────────────────────────────┐
│ coverage FULL CLOSURE · fresh 2s · window 10 equivalent commits       last op workspace commit · 83.4 ms │
├─────────────────────────┬──────────────────────────────────────────────────────────────────────────────────┤
│ TOPOLOGY                │                                                                              │
│                         │   90% SAVED                          10× COLLAPSE                               │
│▾ layer.db               │                                                                              │
│ ├▾ stack-a.db           │   10 logical payload sets  ─────▶   1 canonical set per required DB          │
│ │ ├▾ branch-a.db        │   1.00 GiB candidates                100 MiB per DB                            │
│ │ │ ├○ w:host           │   9 copies / 900 MiB avoided         scope: committed Store objects           │
│ │ │ └● w:docker         │                                                                              │
│ │ └▸ branch-b.db        ├──────────────────────────────────────────────────────────────────────────────┤
│ └▸ direct.db [direct]   │ REQUIRED PLACEMENT · not a dedup failure                                    │
│                         │                                                                              │
│                         │ BranchStore [Q 1×] ─▶ StackStore [Q 1×] ─▶ LayerStore [Q 1×]                │
│                         │          100 MiB               100 MiB              100 MiB                   │
│                         │ route physical CAS 300 MiB · independent durability domains 3                │
│                         ├──────────────────────────────────────────────┬───────────────────────────────┤
│                         │ IMPACT BY LEVEL                              │ TRANSIENT WORKSPACE STATE     │
│                         │ branch-a  90%  900 MiB avoided  10 commits   │ active spools        240 MiB │
│                         │ stack-a   90%  900 MiB avoided  10 pushes    │ materialized         610 MiB │
│                         │ layer     90%  900 MiB avoided  10 adds      │ not included in 90% saved    │
├─────────────────────────┴──────────────────────────────────────────────┴───────────────────────────────┤
│ > monitor dedup --route branch-a                                                                      │
├───────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Tab focus · Enter drill down · t timing · o output · Ctrl+F search · ? help            IDLE · fresh 2s │
└───────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Widths:

- topology: 24–30%; minimum 28 cells;
- canvas: remaining flexible width; minimum 70 cells;
- inspector: 24–28%; minimum 30 cells;
- command line and shortcut/status rows: full width.

On the default dashboard, the hero and required-placement pipeline occupy the
canvas and the inspector column is used for scope, coverage, freshness, and the
selected impact record. On drill-down, the same geometry renders Layer, Stack,
Branch, Workspace, operation, or output detail. This avoids a separate
dashboard navigation system.

The user may resize the first and third panes with mouse drag or keyboard. The
TUI remembers ratios in its local presentation state, never in Store schemas.

### 6.2 Medium: 90–131 columns or 24–31 rows

The inspector becomes a contextual drawer toggled with `Enter` or `i`.

```text
┌ LayerFS ─ DEDUP IMPACT · stacked route ───────────────────────────────────────────────────────────────┐
│ FULL CLOSURE · fresh 2s · 10 equivalent commits                                                      │
├───────────────────────┬───────────────────────────────────────────────────────────────────────────────┤
│ TOPOLOGY              │  90% SAVED · 10× COLLAPSE                                                    │
│▾ layer.db             │  10 payload sets -> 1 canonical set per required DB                          │
│ ├▾ stack-a            │  900 MiB avoided inside each DB                                              │
│ │ ├▾ branch-a         │                                                                              │
│ │ │ ├○ w:host        │  Branch [Q 1×] -> Stack [Q 1×] -> Layer [Q 1×]                               │
│ │ │ └● w:docker      │  placement 3× is required, not failed dedup                                  │
│ │ └▸ branch-b         │                                                                              │
│ └▸ direct [direct]    │  transient: spool 240 MiB · materialized 610 MiB · excluded                  │
├───────────────────────┴───────────────────────────────────────────────────────────────────────────────┤
│ >                                                                                                    │
├───────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Tab focus · Enter drill down · t timing · o output · ? help                                IDLE       │
└───────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 6.3 Narrow: below 90 columns or below 24 rows

Only one primary pane is visible. The breadcrumb and pane switcher preserve
context. No horizontal scrolling is required.

```text
┌ LayerFS · DEDUP IMPACT ──────────────────────────┐
│ branch-a · FULL · fresh 2s                       │
├───────────────────────────────────────────────────┤
│             90% SAVED                            │
│           10× COLLAPSE                           │
│                                                   │
│ 10 logical sets -> 1 canonical set / DB          │
│ 900 MiB avoided inside each required DB          │
│                                                   │
│ Branch 1× -> Stack 1× -> Layer 1×               │
│ required placements: 3 · not failed dedup        │
│                                                   │
│ transient Workspace disk: 850 MiB · excluded     │
├───────────────────────────────────────────────────┤
│ >                                                 │
├───────────────────────────────────────────────────┤
│ [1] tree [2] impact [3] output · Tab · ?          │
└───────────────────────────────────────────────────┘
```

Narrow behavior:

- `1`, `2`, and `3` switch topology, contextual view, and output only when the
  command line is not focused.
- Inspector opens full-screen as a dismissible view.
- Tables become key/value lists.
- Graphs degrade to vertical timelines.
- Long locations middle-elide while retaining the basename and endpoint host.
- The command line remains one row and grows to at most 35% of terminal height
  for multiline input.

### 6.4 Very small terminal

Below 60×16, show a useful refusal rather than corrupted chrome:

```text
LayerFS needs at least 60×16 cells for the TUI.
Current terminal: 52×14

The standalone CLI remains available:
  layerfs status
```

## 7. Focus and navigation model

### 7.1 Focus zones

There are four focus zones:

```text
Topology -> Canvas -> Inspector/Output -> Command line
```

`Tab` moves forward and `Shift+Tab` moves backward. In medium/narrow layouts,
hidden zones are skipped. The focused zone has both a bright border and an
uppercase title suffix such as `TOPOLOGY · FOCUS`.

Printable input while no modal is active focuses the command line and inserts
the character. It never triggers a hidden one-letter action. Single-letter
navigation shortcuts work only while a non-input pane owns focus.

### 7.2 Core keys

| Key | Non-input behavior | Command-line behavior |
|---|---|---|
| `Tab` / `Shift+Tab` | cycle focus zones | leave command line, preserving draft |
| `↑` / `↓` | previous/next row | command history when empty; cursor/history editing otherwise |
| `j` / `k` | previous/next row when Vim mode is enabled | inserts text |
| `←` / `→` | collapse/expand tree or move along lineage | move cursor |
| `Enter` | inspect/open selected record | parse and execute command |
| `Space` | toggle selected tree expansion | insert space |
| `PageUp` / `PageDown` | scroll focused pane | scroll completion/output preview if open |
| `Home` / `End` | first/last row | start/end of input |
| `Ctrl+F` | search/filter focused pane | search command history/output when applicable |
| `Ctrl+R` | refresh selected snapshot | reverse command-history search |
| `?` | shortcuts for current focus | inserts `?` when draft is non-empty; otherwise opens help |
| `Esc` | close top overlay, then inspector, then clear filter, then deselect | close completion, then clear selection, then unfocus; never executes or discards durable state |

No key both commits and ends a Workspace. No unmodified key destroys state.

### 7.3 Mouse

Mouse support is additive:

- single click selects and focuses;
- click disclosure glyph expands/collapses;
- double click or Enter opens the detail view;
- wheel scrolls the pane under the pointer;
- drag resizes wide-layout dividers;
- click breadcrumb navigates;
- click output stream toggles follow only through a labeled target;
- hover may brighten a row but may not reveal the only copy of an action or
  status.

Text selection remains possible. Mouse reporting can be disabled without losing
functionality.

## 8. Command-line interaction

### 8.1 Permanent command strip

The bottom command strip is the universal action surface:

```text
╭─ branch-a · b:91e ─────────────────────────────────────────────────╮
│ > workspace create b:91e --container agent-runtime --container-at  │
╰─────────────────────────────────────────────────────────────────────╯
```

The left border title shows the active Store/Branch context, not a fabricated
Project name. The prompt accepts the exact standalone CLI grammar without the
leading executable name.

### 8.2 Completion

Completion combines:

- static grammar from `layerfs-cli`;
- live IDs and locations queried through `layerfs-cli`;
- selected-record context supplied as a suggestion, never silently inserted;
- filesystem paths and running container IDs where relevant.

```text
> workspace create b:91e --container <Tab>
┌ completion ────────────────────────────────────────────────┐
│ agent-runtime     running · linux/arm64 · FUSE ready      │
│ build-worker      running · linux/amd64 · missing /dev/fuse│
└────────────────────────────────────────────────────────────┘
```

Invalid candidates remain visible only when explaining why they are unusable;
they are dimmed and carry a textual reason.

### 8.3 Command history

- `↑`/`↓` browse history when the input is empty.
- `Ctrl+R` opens reverse search.
- History persists in local CLI state with bounded entry/byte retention.
- Secrets and ephemeral Workspace capability tokens must never be stored.
- Selecting a prior command restores editable text; it does not execute.

### 8.4 Preview and confirmation

The TUI renders the structured plan returned by `layerfs-cli`; it must not
reconstruct semantics from command text.

Example Store creation preview:

```text
CREATE BRANCHSTORE

location       /data/branch-b.db
route          branch-b -> stack-a -> layer.db
schema         Branch
existing path  no

Enter confirm · Esc return to command
```

Confirmation is required for operations that destroy or discard local state,
replace presentation resources, or connect a Store whose identity conflicts
with saved local context. Ordinary Pull, Push, Add, Commit, and read operations
do not need a ceremonial second confirmation; their CAS/conflict rules are the
safety boundary.

## 9. Progressive Store setup UX

### 9.1 Empty landing state

With no active LayerStore, the topology pane shows one honest empty state:

```text
No LayerStore connected.

Create an empty authority:
  db create layer /path/to/layer.db

Or connect one:
  db connect layer tcp://host:7700

Type a command below. Tab completes paths and endpoints.
```

These are examples, not buttons.

### 9.2 Authority-first expansion

After connecting a LayerStore, its node becomes the selected root. The user may
then create/connect any number of StackStores or direct BranchStores by command.

```text
> db connect layer /data/layer.db
> db create stack /data/stack-a.db
> db create branch /data/branch-a.db
```

The TUI preview always displays the inferred route before creation:

```text
active layer + active stack + db create branch
    branch-a -> stack-a -> layer

active layer + no active stack + db create branch
    branch-a -> layer [direct]
```

Selecting a StackStore changes active Stack context. Selecting a BranchStore
changes active Branch context. Selection never reparents an existing Store.

### 9.3 Database management view

`db list` renders one Store-centric table including active and health status:

```text
ROLE    LOCATION                  PARENT       STATE       LAST CHECK
layer   /data/layer.db            —            connected   12 ms
stack   /data/stack-a.db          layer.db     connected    8 ms
stack   tcp://builder-2:7701      layer.db     offline      2.0 s
branch  /data/branch-a.db         stack-a.db   connected    6 ms
branch  /data/direct.db           layer.db     connected    4 ms
```

No `db delete` action is exposed. `db disconnect` removes only local connection
context and must say so in its result.

## 10. History navigation

### 10.1 Layer view

```text
LAYER HISTORY lh:main

l:0040 ─── l:0041 ─── l:0042 ◀ head
  812 MiB    +14 MiB      +3 MiB
  41 ms      63 ms        38 ms add

Selected l:0042
root obj:84b9 · 18,421 files · 1.82 GiB logical
```

The CLI command group remains `layer`; `LayerHistoryId` may appear as a record
identifier without becoming a `layer-history` command group.

### 10.2 Stack view

```text
STACK HISTORY sh:local · base l:0042

s:0005 ─── s:0006 ─── s:0007 ◀ head
                         ├ b:91e @ c:0019
                         └ b:a72 @ c:0107
```

The view differentiates:

- present locally;
- pushed to LayerStore;
- accepted into a Layer;
- behind remote copied head;
- conflict or head moved.

### 10.3 Branch and Commit view

```text
BRANCH b:main

c:0017 ── c:0018 ── c:0019 ◀ head
              └──── c:merge

Workspaces from c:0019
├── w:host-a      current · host/materialize
└── w:docker-a    current · docker/fuse · npm test
```

Subbranches are ordinary Branches whose source Commit is visible in lineage.
The view does not imply that Branch nesting controls merge eligibility; the
source/base compatibility rules do.

## 11. Workspace UX

The `layerfs-workspace` package owns Workspace COW and session lifecycle,
placement, execution, output recording, and their runtime events. The TUI sees
those values only through `layerfs-cli`; it must not recreate placement or
execution policy from fields on the screen.

### 11.1 Workspace row

Every active Workspace row exposes:

```text
UUID · branch · pinned base/head relation · placement · projection · lifecycle · active execution
```

Examples:

```text
● w:0195f62e  b:91e  c:0019=current  host:/tmp/ws  materialize  dirty 3
● w:0195f632  b:91e  c:0019<0020     docker:agent-runtime  fuse  behind
○ w:0195f63a  b:a72  c:0107=current  host:/build/ws  materialize  clean
```

### 11.2 Create view

`workspace create` preview resolves placement and projection:

```text
CREATE WORKSPACE

Branch          b:91e
Pinned head     c:0019
Placement       Docker · agent-runtime
Projection      thin FUSE
Container root  /workspaces
Session path    /workspaces/<generated-uuid>
Databases       host only
Isolation       shared container · tracked executions only

Enter create · Esc edit command
```

Host placement shows the selected arbitrary directory and resolved FUSE or
materialization strategy. Docker placement explicitly says `databases: host
only` and whether the container is FUSE-ready.

### 11.3 Lifecycle states

```text
CREATING -> ACTIVE -> COMMITTING -> COMMITTED/READ-ONLY -> ENDING -> ENDED
                |          |
                |          `-> HEAD_MOVED/CONFLICT -> ACTIVE, changes preserved
                `-> END --discard -> ENDED/DISCARDED
```

`workspace end` never captures or commits. Plain End rejects a dirty,
uncommitted Workspace. `workspace end --discard` requires explicit confirmation
and shows the dirty-path/byte count being discarded.

### 11.4 Mixed host and Docker Workspaces

The Branch detail view must make mixed placement natural:

```text
Branch b:demo · head c:0100
├── w:host-a       host / materialize  c:0100 current
├── w:host-b       host / materialize  c:0100 current
├── w:docker-a     agent-runtime / FUSE  c:0100 current
└── w:docker-b     agent-runtime / FUSE  c:0100 current
```

Several UUID FUSE projections may coexist in one trusted FUSE-ready container.
The inspector must warn that shared-container memory is not exactly attributable
and that visible mounts are not a security boundary between untrusted agents.

## 12. Command execution and output

Execution and retained output are `layerfs-workspace`
responsibilities. `layerfs-cli` exposes their commands and structured events;
the TUI renders them.

### 12.1 Noninteractive execution

```text
> workspace exec w:0195f632 -- npm test
```

The output view opens automatically for a newly started foreground execution.
The user may press `Esc` to leave it; execution and host-side recording continue.

```text
EXECUTION e:004 · npm test · Docker agent-runtime
RUNNING 00:08.4 · stdout 12.4 KiB · stderr 0 B · FOLLOW

> example@1.0.0 test
> vitest run

✓ parser.test.ts
✓ storage.test.ts
…
```

Output behavior:

- stdout and stderr remain distinguishable without relying only on color;
- observed stream order is identified by monotonically increasing sequence;
- TUI memory keeps only a bounded tail;
- older output pages from persisted host logs on demand;
- leaving the view never stops the process;
- `f` toggles follow, `Home`/`End` jump, `PageUp`/`PageDown` scroll, `/` or
  `Ctrl+F` searches, and `Ctrl+C` requests interruption of the selected
  execution after a clear confirmation hint;
- completed execution receipts and output remain accessible after Workspace
  Commit and End according to bounded retention policy.

### 12.2 Interactive shell

The first release must not embed a terminal emulator. For `workspace shell`,
the TUI:

1. saves UI state;
2. leaves raw and alternate-screen mode;
3. hands the real terminal to the host process or Docker exec PTY;
4. waits for shell exit;
5. restores terminal mode and TUI;
6. refreshes Workspace dirty/resource state.

```text
LayerFS Workspace w:0195f632
Docker agent-runtime · /workspaces/0195f632
Type `exit` to return to LayerFS.

$ npm test
$ exit
```

Shell metadata persists by default, but a full PTY transcript does not. Full
shell recording is a later explicit opt-in because it may contain secrets and
terminal-control sequences.

### 12.3 Execution history

```text
EXECUTIONS · w:0195f632

ID      COMMAND       STATE     EXIT  ELAPSED  CPU     PEAK MEM  OUTPUT
e:001   npm install   finished     0   18.2 s  12.1 s   410 MiB  4.1 MiB
e:002   npm test      failed       1    4.1 s   3.3 s   190 MiB  127 KiB
e:004   cargo test    running      —    8.4 s   6.2 s   312 MiB  1.8 MiB
```

Command logs live in bounded host observation storage owned by high-level
`layerfs-workspace`, never under a transient Workspace mount and never in
LayerStore, StackStore, or BranchStore tables.

## 13. Conflict and CAS states

### 13.1 Head moved

```text
! HEAD MOVED

Workspace       w:0195f632
Based on        c:0019
Branch head     c:0020
Changes         preserved
Commit created  no

Inspect: branch diff c:0019 c:0020
Resolve explicitly, then retry or discard.
```

This is a durable result view, not a transient toast. It remains attached to the
Workspace until acknowledged. No automatic overwrite, rebase, or retry occurs.

### 13.2 Merge/Add conflict

```text
! CONFLICT · 2 paths

PATH                 BASE       CURRENT    CANDIDATE
/config.json         obj:a102   obj:b231   obj:c884
/src/routes.rs       obj:91aa   obj:772c   obj:38ef

No Stack/Layer/Commit was published.
```

Selecting a path opens the semantic diff. Base/current/candidate labels remain
visible in no-color mode.

### 13.3 Error hierarchy

| Severity | Presentation | Examples |
|---|---|---|
| inline validation | command input message | missing argument, invalid typed ID |
| recoverable result | inspector/result panel | `HEAD_MOVED`, conflict, offline Store |
| blocking setup error | modal with command correction | wrong role/schema, incompatible parent |
| integrity failure | persistent red status plus detail view | corrupt canonical object, invalid closure |
| fatal terminal failure | restore terminal, print plain error, exit nonzero | cannot restore/render safely |

Errors always include what changed and what did not change.

## 14. Monitoring UX

Dedup Impact is the default canvas, not a secondary monitor page. Other
monitoring views are its evidence and drill-down. `layerfs-monitor` owns
measurement, aggregation, coverage, freshness, receipts, and snapshots outside
the SDK; `layerfs-cli` owns the active monitor context and passes structured
snapshots/events to the TUI. The TUI performs no measurement and does not poll
expensive database scans continuously.

### 14.1 Monitor scopes

| Selection | Primary monitor |
|---|---|
| LayerStore root | route-wide Dedup Impact hero, required placement, Store health, operation timeline |
| Store | committed local CAS reuse, physical DB/WAL/SHM, unique CAS, operation timing |
| Store boundary | Pull/Push candidate, known, sent, inserted, raced, coverage, round trips |
| BranchStore/Branch | Commit candidate/reuse receipts, active Workspaces, current/peak owned memory, CPU, CAS results |
| Workspace | committed candidate/reuse receipt separately from active spool/materialization, placement, executions, CPU, I/O, elapsed |
| Operation | exact receipt, coverage/freshness, elapsed phases, transactions, wire turns |
| Process/container | actual process/container CPU and RSS, explicitly unallocated/shared where needed |

### 14.2 Dominant Dedup Impact presentation

The dashboard is allowed—and expected—to show a large `90% SAVED` when it is
an exact byte result. The percentage must never stand alone. Its scope strip is
part of the hero, remains visible at narrow widths, and contains:

```text
scope      Store/route/Branch/Workspace Commit/operation
window     one operation or a defined equivalent-operation cohort
coverage   full closure or changed set
freshness  measured timestamp/age
domain     committed canonical Store objects
```

The hero then presents four linked facts:

```text
90% SAVED
10 logical payload sets -> 1 canonical set per required DB
9 payload copies / 900 MiB avoided inside each DB
10× collapse
```

These labels are not optional. `90% SAVED` without them is misleading.

The supporting measures are:

```text
TRANSFER AVOIDANCE
candidate bytes that did not cross this Store boundary

STORAGE AVOIDANCE
candidate bytes that did not become new rows in the receiver

ROUTE PHYSICAL COPIES
sum of physical unique CAS bytes across independently durable Stores
```

The dashboard hierarchy is:

```text
Dedup Impact hero
   |
   +-- impact by LayerStore / StackStore / BranchStore
   +-- impact by Branch and committed Workspace
   +-- required 2/3-DB placement pipeline
   +-- active transient Workspace disk (excluded)
   `-- operation receipt and timing
```

For one transfer boundary:

```text
DEDUP · BranchStore -> StackStore

Candidate       161.2 MiB   4,821 objects
Known receiver  155.8 MiB   4,707 objects
Sent              5.4 MiB     114 objects
Inserted          5.3 MiB     112 objects
Raced            96.0 KiB       2 objects

Transfer avoided  96.7%
Storage avoided   96.7%
```

Definitions displayed in help:

```text
transfer_avoidance = (candidate_bytes - sent_bytes) / candidate_bytes
storage_avoidance  = (candidate_bytes - inserted_bytes) / candidate_bytes
```

If candidate bytes are zero, show `—`, never `100%`.

For the complete route:

```text
PHYSICAL STORE COPIES

LayerStore    113.0 MiB unique CAS
StackStore    113.0 MiB unique CAS
BranchStore     5.3 MiB unique CAS
Route total    231.3 MiB

Independent durability domains: 3
Cross-DB copies are expected and are not reported as failed deduplication.
```

The direct pipeline displays:

```text
BranchStore [Q 1×] -> LayerStore [Q 1×]
```

The stacked pipeline displays:

```text
BranchStore [Q 1×] -> StackStore [Q 1×] -> LayerStore [Q 1×]
```

`1×` means one canonical payload set in that physical database, not one row or
one byte globally. A two-database `2×` or three-database `3×` placement factor
must appear under `REQUIRED PLACEMENT`, never under `WASTE` or `DEDUP FAILED`.

The TUI may show logical workload bytes beside physical bytes, but must not call
their ratio a global deduplication rate without a precisely defined root set,
candidate equivalence rule, byte domain, coverage, and time window.

### 14.3 Committed Store impact versus transient Workspace state

The main hero measures committed canonical Store objects unless its scope says
otherwise. An active Workspace may have private dirty spool, materialized
files, FUSE buffers, package-manager caches, and command outputs.
Those bytes are real but not yet a committed Store dedup result.

```text
COMMITTED STORE DEDUP                       ACTIVE WORKSPACE DISK

10 equivalent commits                      10 active Workspaces
1 canonical Q set / required DB             spool       2.4 GiB
90% saved in committed candidate scope      materialized 6.1 GiB
coverage full · fresh 2s                     output logs  0.2 GiB

                                             excluded from 90% SAVED
                                             measured separately
```

Rules:

- Never claim ten active Workspaces consume one payload copy unless an
  instrumented projection actually proves shared physical blocks for that exact
  scope.
- A Workspace Commit receipt may join the hero only after canonicalization and
  Store admission produce an exact candidate/inserted/reused byte result.
- Workspace transient storage may be larger than committed CAS savings without
  invalidating Store dedup; it is a different lifecycle and byte domain.
- Selecting the transient card drills into each Workspace, placement,
  projection, spool/materialized/output bytes, and freshness.

### 14.4 CPU and memory

Use honest labels:

```text
Process RSS             actual process metric
Container RSS           actual container/cgroup metric
Workspace-owned memory  LayerFS allocations attributable to one Workspace
Shared container memory unallocated when several Workspaces share a container
```

Current Branch memory is the sum of currently active Workspace-owned gauges.
Peak Branch memory is the peak concurrent sum, not the sum of each Workspace’s
independent historical peak.

### 14.5 Storage

```text
STORE       DB+WAL+SHM  UNIQUE CAS  OBJECTS    METADATA/PAGES  STATE
layer.db     131 MiB      113 MiB    8,592      18 MiB         healthy
stack.db     125 MiB      113 MiB    8,592      12 MiB         healthy
branch.db      9 MiB      5.3 MiB      112      3.7 MiB        healthy
```

Remote Store physical values may be `unavailable`; the TUI must not substitute
local estimates.

## 15. Operation timing

Every CLI/SDK operation gets a stable operation ID, a live elapsed timer, and a
finished timing receipt assembled by `layerfs-monitor` from the responsible
operation spans. `layerfs-cli` carries the receipt/event in its active context;
the TUI only renders it. The top status line shows the selected/running
operation:

```text
op: stack push op:8f2a  RUNNING 00:01.284
```

Finished summary:

```text
STACK PUSH · COMPLETED

total          83.4 ms
preflight       8.1 ms
closure        11.7 ms
membership     14.9 ms
transfer       31.4 ms
receiver txn   13.6 ms
finalize        3.7 ms

wire turns       4
transactions     2
sent          5.4 MiB
```

Rules:

- `total` is monotonic wall-clock elapsed time observed by the command owner.
- Phases are shown only when instrumented; missing phases display `—`, not zero.
- Parallel phases may overlap, so phase durations are not required to sum to
  total.
- Queue/wait, CPU, network/transfer, SQLite transaction, and projection time are
  distinct when measurable.
- Fast operations remain visible in history even if no progress frame rendered.
- The TUI uses duration units adaptively: µs below 1 ms, ms below 1 s, seconds
  thereafter.
- Timing receipts stay outside Store schemas and follow bounded retention.

The operation history view supports sort/filter by verb, target Store, status,
and elapsed time. It must make performance regressions discoverable without
turning the UI into a benchmark suite.

Timing is a first-class drill-down from every Dedup Impact number:

```text
90% SAVED
   `-- branch-a · 10 commits
          `-- workspace commit op:8f2a
                 |-- capture/canonicalize  21.4 ms
                 |-- local membership       8.7 ms
                 |-- SQLite admission      13.6 ms
                 `-- Branch CAS             0.8 ms
```

The user can therefore answer both “how much did we save?” and “what did the
saving cost?” without switching to an unrelated diagnostics product.

## 16. Status, progress, empty, and offline states

### 16.1 Global status line

The last line reserves the right side for state and elapsed time:

```text
Tab focus · ? help · Ctrl+F search        branch push · RUNNING 00:02.7
```

When idle:

```text
Tab focus · ? help · Ctrl+F search        IDLE · 3 DBs · 4 workspaces
```

### 16.2 Progress

Use determinate progress only with a real denominator:

```text
objects 384/512  [███████████────] 75%  5.4 MiB  00:01.2
```

Otherwise:

```text
enumerating closure  ◐  00:00.4
```

Never animate a fake percentage.

### 16.3 Empty states

Each empty state answers:

```text
what is empty
why that can be valid
one or two CLI examples
```

Example:

```text
branch-a.db has no Workspaces.
This is normal: Workspaces exist only for active filesystem transactions.

  workspace create b:91e --host-at /tmp/workspaces
```

### 16.4 Offline state

An offline Store remains in the topology with an `OFFLINE` badge and last
successful check. Its cached observation data is timestamped; it is never
presented as current.

## 17. Accessibility and terminal compatibility

### 17.1 Color and glyphs

- Truecolor, 256-color, 16-color, and no-color modes are required.
- Status always has a word/glyph alternative.
- ASCII fallbacks cover box drawing, disclosure, selection, spinner, and graph
  connectors.
- No meaning depends on cursor shape or terminal title.
- The icon is optional enhancement, never the only identity or navigation cue.

### 17.2 Keyboard accessibility

- Every mouse action has a keyboard equivalent.
- Focus is visible and announced in pane titles.
- Tab order matches visual order.
- Blocking dialogs trap focus until resolved or dismissed.
- `Esc` always moves one level back and never silently commits/discards.
- Shortcut help is contextual and usable at 60 columns.
- Commands can always be entered in full without memorizing shortcuts.

### 17.3 Text and truncation

- IDs use middle ellipsis in visual display but copy in full.
- Locations preserve basename/host and terminal suffix when truncated.
- Tables collapse to labeled lists rather than horizontal scrolling.
- Status words precede optional explanations.
- Live output preserves bytes on disk; display replacement characters do not
  mutate stored output.

### 17.4 Terminal modes

Support:

```text
fullscreen alternate-screen   default when supported
inline mode                    terminal scrollback friendly
compact mode                   reduced borders/padding
no mouse                       keyboard complete
reduced motion                 no animated spinner
NO_COLOR                       monochrome
```

The TUI must always restore raw mode, alternate screen, cursor, mouse reporting,
and terminal colors on normal exit, error exit, panic boundary, and after an
interactive Workspace shell returns.

## 18. Ratatui/Crossterm component mapping

This is an ownership map, not implementation code.

### 18.1 Product-data ownership

| Concern | Sole owner | TUI access path |
|---|---|---|
| canonical content identity/model and pure transformations | `layerfs-content` | never direct; surfaced through Store/Monitor results |
| Store persistence/admission/transfer/history records | `layerfs-storage` plus role Stores | never direct; surfaced through SDK/Monitor results |
| Workspace COW/dirty tree/spool, UUID session, host/container placement, projection, execution and retained output | `layerfs-workspace` | structured commands/events through `layerfs-cli` |
| Store graph and Layer/Stack/Branch semantic operations | `layerfs-sdk` | commands/results through `layerfs-cli` |
| CPU, memory, physical storage, Dedup Impact, transfer and timing snapshots/receipts | `layerfs-monitor`, outside the SDK | snapshots/events through active `layerfs-cli` context |
| command grammar, context, completion and event routing | `layerfs-cli` | direct in-process library dependency |
| focus, navigation, layout and rendering | `layerfs-tui` | local presentation state only |

`layerfs-tui` has no direct dependency on `layerfs-content`, `layerfs-storage`,
`layerfs-workspace`, `layerfs-monitor`, `layerfs-sdk`, or Store crates.

### 18.2 Widget/state mapping

| UI responsibility | Ratatui/Crossterm shape | TUI state owner | Source data |
|---|---|---|---|
| responsive regions | `Layout` constraints | layout mode and pane ratios | terminal size |
| topology tree | flattened stateful `List` | expansion, selection, viewport | CLI context topology snapshot |
| Dedup Impact hero | large `Paragraph`, ratios and compact bars | selected scope/window | `layerfs-monitor` snapshot through CLI context |
| required-placement pipeline | custom buffer/`Paragraph` | direct/stacked route selection | `layerfs-monitor` placement snapshot through CLI context |
| history tables | stateful `Table`/`List` | sort, selection, viewport | CLI context query snapshots |
| lineages | `Paragraph`/custom buffer rendering | selected history/head | CLI context history snapshot |
| inspector | `Paragraph` and key/value `Table` | open/closed, scroll | selected CLI context snapshot |
| command line | bordered `Paragraph` with cursor | draft, cursor, history selection | CLI parser/completion |
| completion popup | overlay `List` | candidates and selected index | CLI completion |
| output view | virtualized `Paragraph`/lines | follow, offset, search | Workspace output events/log pages through CLI context |
| metrics | `Table`, `Gauge`, `Sparkline` | scope and sample window | `layerfs-monitor` snapshots/events through CLI context |
| dialogs | centered `Clear` + `Block` | focus row and pending action | CLI plan/result/error |
| status/shortcuts | one-line `Paragraph` | active focus and operation | local UI + CLI event |
| terminal events | Crossterm poll/read | input dispatcher | keyboard/mouse/resize/paste |

Architecture rules:

- Rendering reads immutable view state and emits no LayerFS operation.
- TUI input becomes either local navigation or a command executed through
  `layerfs-cli`.
- Terminal events are read on one UI thread.
- Long operations run outside the draw loop and emit bounded events.
- `layerfs-monitor` is outside the SDK. Its updates coalesce through the CLI
  context; the TUI does not import or invoke the monitor directly.
- High-level `layerfs-workspace` owns session, placement, execution, and output;
  command output drains continuously to its persistent host logs even if the UI
  cannot render each chunk.
- There is one source of truth for command parsing and legality: `layerfs-cli`.

## 19. TUI state model

The minimal presentation state is:

| State group | Fields |
|---|---|
| layout | wide/medium/narrow, pane ratios, visible drawer |
| focus | topology/canvas/inspector-output/command/modal |
| navigation | selected topology key, breadcrumb, expanded keys, per-view viewport |
| command | draft, cursor, completion, history search, pending confirmation |
| operation | operation ID, verb, target, phase, elapsed, progress, result |
| output | execution ID, follow, viewport, search, bounded live tail |
| impact/monitor | selected scope, candidate window, coverage, freshness, last event/snapshot time |
| overlay | help, inspector, plan, conflict, error, search |
| terminal | color capability, image capability, mouse, compact, reduced motion |

Do not persist volatile selection, open overlays, output buffers, or active
operation objects into Store databases. Local presentation preferences may be
stored by `layerfs-tui`; Store connection context and command history remain
owned by `layerfs-cli`.

## 20. Required view inventory

The first complete TUI must implement these views; avoid speculative panels
beyond them:

1. Empty/connection landing.
2. Dominant Dedup Impact dashboard with coverage/freshness.
3. Required two-/three-DB placement pipeline.
4. Topology navigator.
5. Layer history/detail and impact drill-down.
6. Stack history/detail and impact drill-down.
7. Branch/Commit lineage and impact drill-down.
8. Workspace committed/transient split and detail.
9. Workspace execution output/history.
10. Database/route storage evidence.
11. Transfer-boundary deduplication detail.
12. Workspace/Branch resource evidence.
13. Operation timing/history.
14. Contextual inspector.
15. Completion/search/help overlays.
16. Plan/confirmation/conflict/error result overlays.

## 21. Acceptance criteria

### Architecture

- `layerfs-tui` depends on `layerfs-cli`, Ratatui, and Crossterm, but not
  directly on `layerfs-sdk`, `layerfs-monitor`, `layerfs-workspace`,
  `layerfs-content`, `layerfs-storage`, or Store crates.
- Every mutating TUI action is expressible as the same standalone CLI command.
- The TUI contains no Store, Docker, Commit, transfer, or dedup algorithm.
- No Store tables are added for TUI state, logs, timing, or monitoring.
- `layerfs-monitor` supplies snapshots/events through CLI context and remains
  outside the SDK.
- `layerfs-workspace` remains the sole COW/spool/session/placement/execution/
  output owner; no second Workspace/Core/Overlay subsystem exists.

### Topology and navigation

- A fixture with one LayerStore, three StackStores, two direct BranchStores,
  multiple BranchStores per StackStore, and multiple mixed-placement Workspaces
  remains understandable at wide, medium, and narrow sizes.
- Direct and stacked Branch routes cannot be confused.
- Selecting any Store or Workspace reveals full ancestry and location.
- Keyboard-only and mouse-only navigation reach the same views.

### Workspace

- Host/materialized and Docker/FUSE Workspaces may coexist under one Branch.
- Multiple Workspaces in one Docker container remain independently selectable.
- The Docker view explicitly states that databases and Commit authority remain
  host-side.
- Commit, End, and End-with-discard cannot be confused; failed Commit preserves
  Workspace state.
- Interactive shell suspension/restoration leaves the TUI usable.

### Output and monitoring

- `workspace exec` output streams without unbounded TUI memory.
- Output remains viewable after execution and Workspace End according to
  retention policy.
- CPU, memory, and storage values name their scope.
- Exact scoped deduplication is the dominant default dashboard, not a secondary
  panel; every percentage includes candidate window, byte domain, coverage, and
  freshness.
- Ten equivalent committed `npm install` results demonstrate 10× collapse to
  one canonical payload set per required physical DB and nine avoided payload
  copies per DB.
- Required Branch 1× -> optional Stack 1× -> Layer 1× placement is separate
  from dedup success/failure.
- Active Workspace spool/materialization/output bytes remain separate from
  committed Store savings; no unmeasured active-Workspace sharing claim appears.
- Every CLI/SDK operation shows live elapsed time and a finished timing receipt,
  including operations too fast to render progress.

### Compatibility and accessibility

- Render snapshots pass at representative 160×45, 110×30, 80×24, and 60×16
  terminals.
- Truecolor, 256-color, 16-color, and `NO_COLOR` snapshots remain legible.
- ASCII fallback snapshots preserve topology and state semantics.
- Terminal state is restored after normal exit, error exit, simulated panic,
  resize, and interactive shell return.

## 22. Final interaction model

```text
Dedup Impact tells the user why LayerFS matters.
Coverage and freshness tell the user whether to trust the number.
Required placement tells the user why two/three DB copies are correct.
Topology tells the user where the impact occurred.
History tells the user which Layer, Stack, Branch or Commit produced it.
Inspector tells the user what the selection means.
Command line is how the user acts.
Output shows what the action is doing.
Timing and resource evidence prove its cost and efficiency.
```

That model keeps the TUI visually sophisticated without creating a second
LayerFS product API. The standalone CLI remains complete; the TUI makes the
same system easier to understand and operate.

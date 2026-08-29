# LayerFS Phase Two TUI implementation plan

Status: **binding only after Phase One terminal pass**

Phase Two creates exactly one new production package: `layerfs-tui`. It is a
Ratatui/Crossterm presentation client over the frozen `layerfs-cli` Rust
library. It adds no Store operation, schema, Workspace lifecycle, FUSE
protocol, Monitor formula, SDK method, or backend ownership.

## 1. Entry gate

Do not start until Phase One provides current raw evidence for every B10 gate
in `05-implementation-plan.md`, including the non-Ratatui frontend fixture.

Before TUI code, verify the frozen CLI seam supplies:

```text
parse + completion
non-mutating CommandPlan
typed execute + interrupt
Started/Progress/Output/Snapshot/Finished events
topology and paged history/detail snapshots
Workspace/session/diff/execution/output snapshots
Monitor/dedup/storage/resource/timing snapshots
stable IDs/parents/generations/cursors
bounded live output and retained-output paging
versioned JSON/result/error/timing contracts
```

If any item is missing, return Phase One to `REVISE` and complete the shared
CLI contract. Do not patch around it with direct SDK/Store/Workspace access or
formatted-output parsing.

## 2. Binding dependencies and source tree

```text
layerfs-tui -> layerfs-cli
```

No other LayerFS dependency is allowed.

Implement exactly the Phase Two `layerfs-tui` subtree in
`01-topology-and-source-tree.md`. Keep `lib.rs` as declarations/re-exports and
the binary as terminal bootstrap only. Do not create a second command grammar,
client abstraction, state framework, theme engine, widget toolkit, or backend
adapter.

Use [the canonical LayerFS image](../tui/layerfs.png) for documentation and
inline-image-capable terminals when practical. Use a deterministic monochrome
text mark otherwise. Preserve the source image's geometry and transparency.

## 3. Stage T0 — shell and state loop

1. Open `CliSession` against a selected saved context.
2. Implement Crossterm enter/restore, resize, keyboard/mouse input, and panic
   restoration.
3. Implement one Ratatui app state containing focus, selected stable IDs,
   command-line state, active operation subscriptions, scroll positions, and
   current frontend-neutral snapshots.
4. Coalesce stale Progress/Snapshot events; never drop ordered stdout/stderr.
5. Render loading, empty, disconnected, stale, and error states without
   inventing backend truth.

Gate:

```text
cargo test -p layerfs-tui shell
cargo tree -p layerfs-tui
```

Prove the TUI manifest has exactly one direct LayerFS dependency:
`layerfs-cli`. Transitive CLI dependencies are expected; the TUI must have no
direct edge to SDK, Store, Workspace, Monitor, Docker, SQLite, or FUSE crates.

## 4. Stage T1 — navigation and read views

Implement the navigation model from `03-tui-design.md`:

```text
LayerStore
|-- StackStore
|     `-- BranchStore -> Branch -> Commit -> Workspace
`-- direct BranchStore -> Branch -> Commit -> Workspace
```

Selection uses stable IDs, not row numbers. Refresh preserves selection when
the selected entity still exists and otherwise moves to its nearest parent.
History views page through CLI cursors and show Layer, Stack, Branch, Commit,
and recursive Commit lineage without loading an unbounded graph.

Required panes:

```text
Store topology
Layer/Stack history
Branch/Commit lineage and diff
Workspace placement/state/diff/executions
retained/live output
operation detail and timing
```

Gate: Ratatui TestBackend snapshots at wide, medium, and narrow widths, with
empty/loading/error/stale data and stable navigation across refresh.

## 5. Stage T2 — commands, plans, and long operations

1. Reuse CLI parsing and completion; do not copy Clap grammar.
2. Make the resolved `CommandPlan` inspectable for every command. Ask for
   confirmation only when its frozen `requires_confirmation` field is true;
   never infer ceremonial confirmation merely because a command moves authority.
3. Submit through `CliSession::execute` and render the same typed results/errors
   as the standalone CLI.
4. Show Progress with elapsed time, queue/service phase, and bounded work units.
5. Support interrupt/stop through the frozen contract.
6. Implement live stdout/stderr follow, pause, scroll, retained paging, search,
   truncation markers, and stream distinction without unbounded memory.
7. Suspend/restore Ratatui around the real interactive Workspace shell; the
   Workspace subsystem owns the shell process and output, not the TUI.

Every documented CLI operation must be reachable through the universal command
line. Context-sensitive action hints may insert commands but do not implement a
second button-only operation path.

Gate: parser/completion/plan/confirmation/execution/interrupt/output tests use
the same `layerfs-cli` fixtures as the standalone product.

## 6. Stage T3 — Dedup Impact and monitoring

Dedup Impact is the default and visually dominant dashboard. It renders
Monitor snapshots without recomputing formulas.

Primary hierarchy:

```text
saved rate
saved bytes
collapse factor
N equivalent results -> 1 canonical payload set per required DB
coverage + freshness
required Branch -> optional Stack -> Layer placements
```

Committed Store deduplication and active Workspace allocation are separate:

```text
committed: canonical CAS candidate/unique/reused bytes per physical DB/route
transient: COW/spool/FUSE/materialized allocation and execution resources
```

Also expose per-DB storage, DB/WAL/SHM allocation, transfer receipts inside
operation detail, CPU/RSS, Workspace and Branch aggregation, elapsed phases,
and exact-analysis progress. Never label required independent Store placements
as failed deduplication or mix transient Workspace bytes into the saved rate.

Gate: exact fixture rendering for direct 2-DB, stacked 3-DB, partial coverage,
not-measured, active transient storage, and the `90% / 10 -> 1 / 10x` example.

## 7. Stage T4 — interaction and accessibility closure

Implement keyboard-first interaction, visible focus, scroll indicators,
discoverable help, mouse parity where useful, no-color mode, contrast-safe
status, resize behavior, Unicode-width correctness, and deterministic terminal
restoration. Color is never the only status channel.

Use the arcade visual language and information priorities in
`03-tui-design.md`; do not copy another product's assets or create decorative
animation that reduces navigation or monitoring readability.

Gate: TestBackend and event-loop tests cover all focus transitions, primary
views, command states, widths, color modes, mouse selection, terminal restore,
and panic restore.

## 8. Stage T5 — Phase Two terminal closure

```text
cargo fmt --all -- --check
cargo test -p layerfs-tui
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Terminal proof requires:

```text
exact final layerfs-tui source tree
only direct LayerFS dependency is layerfs-cli
no backend semantic/schema/protocol change made for presentation convenience
every CLI operation reachable
topology/history/Workspace navigation complete
plans and confirmations use typed CLI data
live/retained output stays bounded and ordered
Dedup Impact renders exact supplied formulas/coverage/placements
all width/color/accessibility/restore gates pass
standalone CLI behavior and Phase One performance evidence remain unchanged
```

A Phase Two `REVISE`, failed test, layout defect, or accessibility failure is
not a stopping condition. Replan and continue until the final whole-workspace
terminal pass. If Phase Two exposes a backend defect, return Phase One to
`REVISE`, repair it in its proper owner, rerun B10, refreeze the seam, and then
resume Phase Two. A parallel TUI-only backend path is forbidden.

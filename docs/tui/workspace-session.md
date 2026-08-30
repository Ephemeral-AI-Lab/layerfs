# Workspace session demo contract

This document freezes the SQLite-backed mock TUI Workspace journey. It follows
the V2 backend lifecycle while keeping execution deterministic enough for the
terminal demo.

## Lifecycle

```text
exact current Commit, or exact initial Layer for a zero-Commit Branch
    -> Create Workspace
    -> run Bash 0..n times
    -> inspect final files, content, changes, storage, output, and timing
    -> Commit Final State to the target Branch
    -> inspect the immutable Commit receipt
    -> End Workspace
    -> no Workspace, mount, COW data, file previews, or output remains
```

Only the final filesystem state enters Commit identity. Bash text, Bash order,
stdout, stderr, execution timing, and TUI activity never enter a Commit or Store
schema.

`Capture` is not a user-facing command. It is a measured phase inside Commit:

```text
stop admission -> quiesce -> capture final tree -> compare with anchor
    -> build canonical candidate -> admit missing objects -> exact Branch CAS
```

End performs cleanup only. It never captures or commits. Dirty End requires
explicit `--discard`. TUI exit, `Esc`, mount exit, or shell exit implies neither
Commit nor End.

The normal anchor is an exact Commit. One honest bootstrap exception remains:
a new local Branch forked from a Layer has no Commit, so its first Workspace is
anchored to that exact initial Layer. Every later Workspace on that Branch is
anchored to an exact current-head Commit. Historical or remote Commits require
Fork first.

## Bash-only demo execution

The TUI presents one execution action, **Run Bash**, using the existing V2 CLI
shape:

```text
workspace exec <workspace-id> -- /bin/bash -lc '<script>'
```

The mock rejects other executables. Bash runs in the real Workspace directory,
captures bounded stdout/stderr, measures elapsed time, and recaptures the final
tree even when Bash exits nonzero.

## Frontend read model

Workspace data is runtime-only and bounded. No Workspace, file, output, timing,
or operation tables are added to either Store.

```rust
WorkspaceView {
    // Identity and immutable checkout point.
    id, project_id, branch_id,
    anchor_commit, anchor_layer, anchor_root,
    expected_branch_head,

    // Publication never overwrites the anchor.
    published_commit, published_root,

    state, generation, projection, placement, mount,
    changes, files, runs, storage, timing, commit_receipt,
}
```

The final-state summary distinguishes added, modified, and removed paths. Files
contain a bounded text preview or explicit binary/truncated status. Changes are
always computed against the immutable anchor, never from Bash history.

## Storage definitions

Workspace COW savings and Commit dedup savings are separate equations.

```text
Logical full-copy baseline
    apparent bytes of the complete final file view

COW delta (model)
    allocated bytes of added/modified final files that the semantic COW owns

Base reused
    logical baseline - COW delta

Commit candidate bytes
    canonical changed file/tree objects considered by Commit

CAS inserted bytes
    missing canonical object bytes admitted to BranchStore

CAS reused bytes
    candidate bytes already present in BranchStore

BranchStore file growth
    physical SQLite file growth, shown separately from canonical bytes
```

The mock uses a real materialized directory and real Bash, but it is not the
production FUSE allocator. The TUI therefore labels private storage as
`COW DELTA (MODEL)` and separately shows the mock directory's actual allocated
bytes. It never labels modeled bytes as measured production FUSE storage.

Commit equations are exact:

```text
candidate objects = inserted objects + reused objects
candidate bytes   = inserted bytes   + reused bytes
```

## Timing definitions

Use monotonic elapsed time and show exact individual observations, never p95
without at least twenty samples.

```text
Create Workspace
Run Bash, last and cumulative
Commit: capture, candidate/dedup admission, Branch publication, total
End: cleanup time retained only in the Activity receipt
```

After End, the Workspace row disappears immediately. Activity retains only its
bounded general operation receipt and End duration.

The selected Store pair is restartable. On a later TUI process, LayerStacks,
Layers, Branches, Commit ancestry, roots, serving scopes, and canonical file
trees are reconstructed from the unchanged SQLite schemas. Workspace mounts,
Bash output, and operation activity are not Store facts and therefore do not
return after End or process exit.

## Routes and responsive layout

```rust
Route::Workspaces(Option<LayerStackId>) // inventory
Route::Workspace(WorkspaceId)           // dedicated detail

WorkspaceTab::{Overview, Files, Changes, Runs, Storage}
```

Inventory uses a 40/60 list and selected summary. `Enter` opens the dedicated
Workspace route; it no longer jumps back to Branch.

At 80x24, the detail route shows one body pane and cycles panes with `Tab`.
At 120x40 and 200x60 it uses stable Navigator / Content / Inspector roles.

```text
┌ NAVIGATOR ─────────┬ CONTENT ───────────────────────┬ INSPECTOR ───────────┐
│ tab-specific list  │ file, diff, output, lifecycle │ IDs, actions, metrics│
└────────────────────┴────────────────────────────────┴───────────────────────┘
```

Tabs:

- Overview: immutable anchor, target Branch, lifecycle, final state, actions.
- Files: canonical-path tree/list and bounded file preview.
- Changes: final-state Add/Modify/Remove list and before/after details.
- Runs: Bash receipts and bounded stdout/stderr, explicitly ephemeral.
- Storage: honest logical/COW/materialized/CAS/SQLite measurements and timing.

## Keys

```text
3             Workspace inventory
Enter         open selected Workspace or file
Tab/BackTab   pane focus
[/]           previous/next Workspace tab
j/k           move stable selection
x             open Run Bash command
c             confirm Commit Final State
e             End clean or committed Workspace
D             prefill explicit Discard & End
d             selected file -> Changes
r             refresh the current generation
:             universal command line
Esc           back; never mutates
?             contextual help
```

Commit confirmation must show the target Branch, expected head, preview
generation, changed paths/bytes, and the warning that Bash/output are excluded
from identity.

## Required demo proof

Use one npm project without network dependencies:

```text
package.json
src/index.js
test/index.test.js
```

Build five sequential local Branches with six Workspace Commits each:

```text
rollout-1: 6 Commits -> Push -> Add Layer
rollout-2: 6 Commits -> Push -> Add Layer
rollout-3: 6 Commits -> Push -> Add Layer
rollout-4: 6 Commits -> Push -> Add Layer
rollout-5: 6 Commits -> Push -> Add Layer
```

That proves exactly five Branches, thirty owned Branch Commits, and five added
Layers, or six Layers including genesis. Each cycle creates from the exact
current anchor, runs real Bash, inspects Files/Changes/Storage, commits, and
ends. The final Workspace inventory is empty, the real Store schemas remain
unchanged, and `PRAGMA foreign_key_check` returns no rows.

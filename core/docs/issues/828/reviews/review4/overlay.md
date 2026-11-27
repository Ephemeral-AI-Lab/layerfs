# Review 4: `engine/crates/layerfs-overlay`

Scope: planned layout rows with `n` under `engine/crates/layerfs-overlay/` in
`S/nav/site/layout.json`; old source read at `S/v2tree/engine/crates/layerfs-overlay/`
(archive of `layerfs_v2_0_0` at 1317ebf7ca). LOC is the planned `loc` column (production
LOC, `tools/production_loc.py --files`). Settled items (diagnostics/ dissolved, `sql/`
single-file exception) are not reopened.

## 1. Verdict

The planned layout is mostly sound. Dissolving `diagnostics/` and splitting `kernel/` out
of `lifetime/` were both right. Three real problems remain:

- `lookup.rs` was moved into `kernel/`, but nothing in it is kernel code.
- `lifetime/` mixes two roles under a vague name. Once `lookup.rs` goes back to it, it also
  goes over the 12-file limit.
- The shared test helper was placed in the `payload/` test area, but every area uses it.

The rest are short renames:

- `open/kernel.rs` holds `OpenState`, not kernel code.
- The operation-record files use two different names (`indexed_operation_record*` and
  `record_plan`).
- `namespace/captured_namespace.rs` repeats its folder's name.

On the questions in the brief:

- **`kernel/`** means the same thing as daemon `workspace/kernel/`, vfs
  `operations/kernel/` and nfs `kernel/`: the OS filesystem client (FUSE/NFS) and the
  references and handles it holds. Keep it.
- **`namespace/`** matches vfs `operations/namespace/` and `construction/namespace/`:
  inode and name rows, as opposed to file bytes. Keep it.
- **`contract/`** and **`open/`** are acceptable names. **`lifetime/`** is not (finding 2).
- **`payload/`**: the layout with 3 loose files and three subfolders of 3 is a reasonable
  grouping. Keep it (see §4).

## 2. Findings (ranked)

### F1. `lifetime/lookup.rs` is not kernel code. Do not move it to `kernel/lookup.rs`

- **What.** The plan moves `src/lifetime/lookup.rs` to `src/kernel/lookup.rs`, with the
  reason "LookupOwner: kernel lookup references".
- **Header.** `//! Exact lookup ownership, including non-file metadata after removal.`
- **The API is keyed by request, not by kernel mount.** It has `acquire_lookup(source:
  BaseSource, request: u64, base: &Inode, root: u64)`, `acquire_lookup_read`,
  `retained_lookup(request)`, `check_lookup` and `release_lookup(LookupOwner)`. No
  `KernelMount` appears anywhere in the file. The word "kernel" appears only in one comment
  (line 98).
- **Kernel lookups live elsewhere.** They are recorded by
  `add_kernel_lookup`/`drop_kernel_lookup`/`kernel_forget` in `lifetime/kernel.rs` (planned
  `kernel/references.rs`). They are stored in `OpenState.kernel` (`KernelReference`), not in
  `OpenState.lookups`.
- **`LookupOwner` sits with the other custody handles.** It is declared in
  `contract/custody.rs` next to `OpenFile`, `FileRead`, `CapturedReader` and
  `OperationOwner`. Those handles' jobs all stay in `lifetime/`.
- **No kernel file calls it.** Inside the crate, no file in `kernel/` references
  `acquire_lookup`, `release_lookup` or `LookupOwner`. Outside the crate, only the
  `layerfs-workspace` (vfs) tests `captured_namespace_holds.rs`,
  `captured_namespace_perturbed.rs`, `nonfile.rs` and `producer/mod.rs` call it.
- **Fix.** Keep it with the other custody owners: `src/custody/lookup.rs` (see F2). This
  leaves `kernel/` with 7 files, all kernel-keyed.

### F2. `lifetime/` mixes two roles under a vague name; split it into `lifecycle/` and `custody/`

- **What.** After F1, `lifetime/` holds 13 files (excluding `mod.rs`), above the 12-file
  limit (rule 1). The `pub fn` lists show two separate jobs:
  - **The Workspace's own cycle: open, capture/install, close.**
    - `workspace.rs`: `open_workspace`, `state`.
    - `generation.rs`: `capture`, `install`, `install_merged`, `install_ready`,
      `capture_ready`, `reply_attempted`, `captured_inodes`, `active_*`.
    - `composition.rs`: `resolve_failed_capture`.
    - `close.rs`: `close`, `cleanup_state`, `release_closed_capture`.
    - `frontier.rs`: `retained_capture`, `pending_publications`.
    - `tickets.rs`: `ReplyTickets`.
  - **Owner custody: mint, check and release of the handles in `contract/custody.rs`.**
    - `captured_reader.rs`: `acquire/retained/check/release_captured_reader`.
    - `file_owners.rs`: `open_file`, `close_file`, `acquire/release_file_read`,
      `mint_owner`, `file_ref`.
    - `operation.rs`: `acquire/retained/check/fence/release_operation`.
    - `source.rs`: `acquire/retained/release_base_source`.
    - `orphan.rs`: files that owners keep alive after unlink.
    - `lookup.rs` (F1).
    - `indexed_operation_record.rs`: record jobs fenced by an `OperationOwner`
      (`fence_operation` on every call; header "...jobs in the existing operation
      namespace").
- **Why the name fails.** "lifetime" fits every file in the crate (rule 12). It also
  collides with the Rust language term (rule 13). The crate's own word for the second group
  is "custody", used in `contract/custody.rs` and throughout the headers ("independent
  bounded operation_record custody", "Exact short base-source custody", "Bounded exact
  custody observations"). The daemon's `workspace/nfs/custody.rs` uses it the same way.
  For the first group, rule 19 asks for `lifecycle/` (existence and run changes: open,
  close, capture, install). Persistence `sqlite/lifecycle/` (seal, snapshot, reclamation)
  uses the word in the same sense.
- **Fix.** Split the folder into two:
  - **`src/lifecycle/` (6 files):** `workspace.rs`, `capture.rs` (renamed from
    `generation.rs`), `composition.rs`, `close.rs`, `frontier.rs`, `tickets.rs`.
  - **`src/custody/` (7 files):** `captured_reader.rs`, `file.rs` (renamed from
    `file_owners.rs`; the folder already says "owners"), `lookup.rs`, `operation.rs`,
    `operation_record.rs` (from `indexed_operation_record.rs`; see F4), `orphan.rs`,
    `source.rs`.
- **Why `capture.rs`.** `lifecycle/generation.rs` would not repeat the folder name, but
  `capture.rs` names the calls it holds (rule 15).
- **Code that must change.** Only the `crate::` paths in `lib.rs` (`lifetime::close`,
  `lifetime::tickets`), `open/kernel.rs`, `namespace/inode.rs` and
  `maintenance/orphan.rs` (`lifetime::orphan` becomes `custody::orphan`). All four `use`
  lines were checked.
- **Cost.** The crate root grows from 8 to 9 subfolders. They are still a short list of
  distinct roles: types (`contract`), the SQLite half (`database`), the memory half
  (`open`), three job domains (`kernel`, `namespace`, `payload`), the two halves above,
  and reclamation (`maintenance`).
- **Alternatives.**
  - Minimal: keep `lifetime/`, put `lookup.rs` back in it, and move
    `indexed_operation_record.rs` out to stay at 12 files. This leaves the vague name and
    the mixed roles, and there is no good home for the moved file.
  - Naming `lifecycle/` as `generation/` instead also works, if the owner prefers the
    crate's `Generation` word.
- **Outside this crate.** Store-service `workspace/owner/operations/lifetime.rs` mirrors the
  overlay's `lifetime/` calls. It is out of scope here; flag it to that reviewer.

### F3. `open/kernel.rs` defines `OpenState` (the whole in-memory state). Rename it `open/state.rs`

- **What.** The header says "Exact kernel references and the parents retained by directory
  custody". But the file declares `pub(crate) struct OpenState`, with 20 journaled tables:
  `kernel`, `parents`, `files`, `reads`, `lookups`, `custody`, `orphans`, `kept_live`,
  `digests`, `resumable`, `waits`, `mount`, `directories`, `operations`, `captured`,
  `captured_generations`, `sources`, `cookies`, `cookie_undo`, `disposals`. It also holds
  their whole-job `commit`/`rollback`.
- **Who depends on it.** `open/mod.rs` re-exports `OpenState` from it, and
  `contract/error.rs` holds `crate::open::OpenState`.
- **Fix.** Rename `src/open/kernel.rs` to `src/open/state.rs`. `KernelReference` and
  `KernelParent` are small rows of that state and can stay there. The folder name `open/`
  is fine: it matches the `Overlay.open` field, the `OpenState`, `OpenStateCounts` and
  `OpenDisposals` types, the module header "Workspace-local open state", and the AGENTS.md
  phrase "a Workspace's open state".

### F4. One word for operation records: `operation_record`, and `record_plan.rs` becomes `explain.rs`

- **What.** Three files are named `indexed_operation_record.rs`, in `contract/`,
  `lifetime/` and `maintenance/`. The plan also turns `diagnostics/indexed_operation_record_plan.rs`
  into `database/record_plan.rs`.
- **The rest of the workspace says `operation_record`:**
  - vfs `ports/operation_record.rs`;
  - store-service `workspace/owner/ports/operation_record.rs`;
  - this crate's planned test area, `tests/operation_record/`.
- **Problems with the current names.**
  - "indexed_" adds nothing a reader needs (rule 16).
  - "record_plan" is ambiguous. "record" is the control-layer `record/` concept, and
    "plan" is too generic.
  - The file's one call is `explain_indexed_operation_record`, an SQL EXPLAIN over the
    fixed statements. `database/statements.rs` says "shared by ordinary jobs and plan
    diagnostics".
- **Fix (renames only):**
  - `src/contract/indexed_operation_record.rs` → `src/contract/operation_record.rs`
  - `src/lifetime/indexed_operation_record.rs` → `src/custody/operation_record.rs` (F2)
  - `src/maintenance/indexed_operation_record.rs` → `src/maintenance/operation_record.rs`.
    This matches its sibling step files `garbage.rs`, `orphan.rs` and `pieces.rs`.
  - `src/database/record_plan.rs` → `src/database/explain.rs`
  - `examples/indexed_operation_record_profile.rs` → `examples/operation_record_profile.rs`
- **Not changed.** Public method names (`indexed_operation_record_*`) are API, not layout.

### F5. The tests' shared helper is not payload-only. Put it in `tests/support/`

- **What.** The plan's reason for `tests/payload_support/mod.rs` → `tests/payload/fixture.rs`
  is "helper used only by the payload/ area". That is false. `mod payload_support;` appears
  in 13 test files:
  - all 6 payload topics;
  - all 3 operation_record topics;
  - 4 of the 6 lifetime topics: `captured_namespace`, `kept_live`, `orphan_wake`, `orphans`.
- **Fix.** Move it to `tests/support/fixture.rs`, with a new `tests/support/mod.rs` (rule 7:
  helpers in `tests/support/`). Each area's `main.rs` declares
  `#[path = "../support/mod.rs"] mod support;`. The helper holds generic overlay fixtures:
  `Temp`, `File`, `pattern`, `noise`, `Random`, `work`, `claims`, `rows`, `committed`.

### F6. `tests/kernel_memory.rs` installs a `#[global_allocator]`. Move it to `tests/standalone/`

- **What.** At line 28 it installs a counting `GlobalAlloc`, which is process-global state.
  That is exactly rule 7's standalone case. The plan leaves it as a loose test file in no
  area.
- **Fix.** Move it to `tests/standalone/kernel_memory.rs` and add a `[[test]] name =
  "kernel_memory"` entry to `Cargo.toml`. It must not be merged into an area binary, or the
  counting allocator would cover every topic.

### F7. `tests/device_fixture.sh` and `tests/enospc_as_eio.c` are support files

- **What.** Neither is a test. `device_fixture.sh` is a manual owned-device proof script.
  It compiles `enospc_as_eio.c` into an `LD_PRELOAD` shim, then runs `cargo test`.
- **Fix.** Move both to `tests/support/` (rule 7). Line 40 of the script hard-codes
  `engine/crates/layerfs-overlay/tests/enospc_as_eio.c`; update it to the new path.
- **Already broken (record it; no fix in this layout).** The script's `--test` targets are
  stale today:
  - `device_capacity` does not exist in the overlay or the daemon.
  - The plan turns the daemon's `mounted_sqlite_full.rs` into
    `tests/mounted/sqlite_full.rs`, so `--test mounted_sqlite_full` stops resolving.

  The daemon test's `#[ignore]` text still names `device_fixture.sh`.

### F8. `namespace/captured_namespace.rs` repeats its folder. Rename it to `namespace/captured.rs`

- **What.** Rule 14. The file holds the `CapturedReader`-bound name reads:
  `reader_parent_directory_entries`, `reader_directory_entry`, `reader_symlink`. The
  rename matches `payload/captured/` and `payload/pieces/captured.rs`.

### F9. Small test-file renames (rule 14)

- `tests/payload/payload.rs` → `tests/payload/layers.rs`. Its tests are layered reads
  against a reference model, the shrink staircase and holes, which is the job of src
  `payload/layers.rs` together with `stream.rs`.
- `tests/operation_record/indexed_operation_record.rs` → `tests/operation_record/identity.rs`
  (header: "Public full-identity operation_record, atomicity, ...").
- `tests/operation_record/indexed_operation_record_cursor.rs` → `tests/operation_record/cursor.rs`
- `tests/operation_record/indexed_operation_record_entries.rs` → `tests/operation_record/entries.rs`
- Test area `tests/lifetime/` → `tests/custody/`, following F2. Four of its six topics are
  about reader and orphan custody: `captured_namespace`, `kept_live`, `orphan_wake`,
  `orphans`. The two small ones (`allocation_filesystem` at 64 lines, `error_identity` at
  34) stay as the remainder; on their own they would make thin areas.

## 3. Proposed layout of changed folders

Scope: planned tree (`S/nav/trees/tree-engine.txt`), crate `layerfs-overlay`, `src/` only,
production LOC from the plan's `loc` column. Unchanged folders (`contract/` apart from one
rename, `payload/`) are shown only where a file changes.

```diff
  engine/crates/layerfs-overlay/src/     [10,147]
    lib.rs                                    60   (paths: lifecycle::close, lifecycle::tickets)
    contract/                              [619]
      custody.rs                              82
      error.rs                                95
-     indexed_operation_record.rs             71   (renamed: operation_record.rs)
+     operation_record.rs                     71   (from indexed_operation_record.rs)
      kernel.rs                               60
      kernel_directory.rs                    104
      mod.rs                                   6
      types.rs                               201
    database/                            [1,521]
      accounting.rs                          120
      batch.rs                                54
      connection.rs                          322
-     record_plan.rs                         150   (renamed: explain.rs)
+     explain.rs                             150   (explain_indexed_operation_record; was record_plan.rs)
      metrics.rs                             231
      mod.rs                                  10
      physical.rs                            139
      profile.rs                             134
      startup.rs                             170
      statements.rs                          191
+   custody/                             [1,120]   (owner mint/check/release; from lifetime/)
+     mod.rs                                   —   new
+     captured_reader.rs                     173   (from lifetime/)
+     file.rs                                252   (from lifetime/file_owners.rs)
+     lookup.rs                              105   (from lifetime/lookup.rs; plan had kernel/lookup.rs)
+     operation.rs                            66   (from lifetime/)
+     operation_record.rs                    249   (from lifetime/indexed_operation_record.rs)
+     orphan.rs                              157   (from lifetime/)
+     source.rs                              118   (from lifetime/)
    kernel/                              [1,110]
      cookie.rs                              124
      directory.rs                           103
      directory_read.rs                      182
      file.rs                                 52
-     lookup.rs                              105   (to custody/: request-keyed, not kernel)
      mod.rs                                   —   new (plan)
      mutation.rs                            152
      references.rs                          226
      visit.rs                               271
+   lifecycle/                             [684]   (Workspace open, capture/install, close; from lifetime/)
+     mod.rs                                  21   (was lifetime/mod.rs)
+     capture.rs                             230   (from lifetime/generation.rs)
+     close.rs                                64
+     composition.rs                         204
+     frontier.rs                             16
+     tickets.rs                              85
+     workspace.rs                            64
-   lifetime/                            [1,699]   (dissolved into lifecycle/ + custody/; incl. lookup.rs)
    maintenance/                         [1,043]
      garbage.rs                             202
-     indexed_operation_record.rs             50   (renamed: operation_record.rs)
+     operation_record.rs                     50   (from indexed_operation_record.rs)
      mod.rs                                  12
      orphan.rs                              409
      pieces.rs                               71
      ready.rs                               222
      reclaim.rs                               6
      source_wait.rs                          71
    namespace/                           [1,027]
-     captured_namespace.rs                   72   (renamed: captured.rs)
+     captured.rs                             72   (from captured_namespace.rs)
      compound.rs                            303
      directory_entry.rs                      96
      inode.rs                               447
      job_rows.rs                             56
      mod.rs                                   6
      predecessor.rs                          47
    open/                                  [392]
      counts.rs                               49
      directories.rs                         121
      files.rs                                26
      journal.rs                             104
-     kernel.rs                               83   (renamed: state.rs)
+     state.rs                                83   (OpenState; was kernel.rs)
      mod.rs                                   9
    payload/                             [2,571]   (unchanged; partial: subfolders not shown)
```

Tests and examples (not production; no LOC column):

```diff
  engine/crates/layerfs-overlay/
    examples/
-     indexed_operation_record_profile.rs     —   (renamed)
+     operation_record_profile.rs             —
    tests/
-     device_fixture.sh                       —   (to support/)
-     enospc_as_eio.c                         —   (to support/)
-     kernel_memory.rs                        —   (to standalone/)
+     custody/                                    (was planned lifetime/)
+       main.rs                               —   new (plan)
+       allocation_filesystem.rs              —
+       captured_namespace.rs                 —
+       error_identity.rs                     —
+       kept_live.rs                          —
+       orphan_wake.rs                        —
+       orphans.rs                            —
-     lifetime/                                   (renamed: custody/)
      operation_record/
        main.rs                               —   new (plan)
+       identity.rs                           —   (from indexed_operation_record.rs)
+       cursor.rs                             —   (from indexed_operation_record_cursor.rs)
+       entries.rs                            —   (from indexed_operation_record_entries.rs)
      payload/
        main.rs                               —   new (plan)
        captured_runs.rs                      —
        digests.rs                            —
-       fixture.rs                            —   (to support/: used by all 3 areas)
+       layers.rs                             —   (from payload.rs)
        piece_lifetimes.rs                    —
        pieces.rs                             —
        runs.rs                               —
+     support/
+       mod.rs                                —   new
+       fixture.rs                            —   (from payload_support/mod.rs)
+       device_fixture.sh                     —   (path on line 40 updated)
+       enospc_as_eio.c                       —
+     standalone/
+       kernel_memory.rs                      —   (#[global_allocator]; [[test]] entry)
```

## 4. Checked and left alone

- **`contract/`: kept.** It is the crate's public value vocabulary (custody handles, errors,
  record and kernel types, keys and window constants). Other crates use `contract.rs` in the
  same sense: core `catalog/contract.rs`, storage `port/acquisition/contract.rs`, PG
  `diagnostic/contract.rs`. It mirrors the job folders (`contract/kernel*.rs` ↔ `kernel/`,
  `contract/custody.rs` ↔ `custody/`). Not every public type lives there (payload's
  `Captured*` types are in `payload/captured/cursor.rs`), but moving types now would be
  churn for little gain. `contract/types.rs` is the one catch-all-ish name: constants plus
  `Inode`, `DirectoryEntry`, `Cell`, `Capture`, `BaseSource` and more. Splitting it would
  invent a grouping. Left alone.
- **`kernel/`: kept, same meaning across layers.** In every layer it means the OS kernel's
  filesystem client:
  - daemon `workspace/kernel/`: attach, dispatcher, invalidate, state;
  - vfs `operations/kernel/`: kernel request operations;
  - nfs `kernel/references.rs`: a synthetic kernel's lookup references;
  - overlay `kernel/`: the overlay jobs and custody of kernel requests.

  File names line up with vfs: `directory`, `mutation`, `visit`. Overlay
  `references.rs` matches nfs `kernel/references.rs`. `references.rs` also holds kernel
  mount attach and revoke; splitting a 226-line file is not worth it.
  `directory_read.rs` (the READDIR visit) beside `directory.rs` (directory owners) and
  `cookie.rs` is clear enough.
- **`namespace/`: kept, consistent.** Overlay inode and name rows, vfs namespace calls
  (create, list, remove, rename) and construction's captured namespace all use "namespace"
  for the name/inode tree as opposed to bytes. vfs calls the byte side `file/` and the
  overlay calls it `payload/`. That is the overlay's established storage word
  (`PayloadWrite`, `PayloadWork`, payload tables). A `file/` here would also blur with the
  `OpenFile` custody. `compound.rs` (the `apply` job, `COMPOUND_*` constants) and
  `directory_entry.rs` are crate terms and fine.
- **`payload/`: kept as planned.** It has 3 loose files (`layers`, `stream`, `work`) and
  three folders of 3 files.
  - `cells/` (`codec`, `access`, `runs`) and `pieces/` (`rows`, `moves`, `captured`) are
    the two row kinds of a layer. They have parallel inner designs.
  - `captured/` (`cursor`, `runs`, `digest`) is the captured-run read side.
  - **`digest.rs` is arguable.** Four of its five functions follow appends on the write
    path (`resume_digest`, `carried_digests`, `follow_digests`, `digest_closed`, called from
    `namespace/compound.rs` and `lifetime/file_owners.rs`). Only `captured_digests` and
    `CapturedDigest` are captured-side. Moving it back loose would leave `captured/` with 2
    files, a thin folder. Moving `pieces/captured.rs` in to compensate would thin out
    `pieces/` in turn. Left alone.
  - `stream.rs` would read better as "window", but `cells::Window` is already a type, so it
    stays.
- **`database/`: fine.** 9 files, all SQLite-level. `batch.rs` and `metrics.rs` (from
  `diagnostics/`) fit there.
- **`maintenance/`: fine.** It holds one reclaim step per domain. `reclaim.rs` (6 LOC,
  `ReclaimStep`) is small but public. `fold_namespace` sits in `lifecycle/composition.rs`
  but is run by `maintenance/ready.rs` (`FOLD`). It is left there beside
  `resolve_failed_capture`, which enqueues it.
- **Shape and naming.** Depth is at most 3 below `src/`. After the changes, every leaf
  holds 3 to 12 files and `payload/` has at most 4 loose files. No folder repeats the crate
  name. `sql/schema.sql` is a granted exception.

## 5. Machine-readable changes

```json
[
  {"from": "engine/crates/layerfs-overlay/src/kernel/lookup.rs", "to": "engine/crates/layerfs-overlay/src/custody/lookup.rs", "op": "move", "why": "request-keyed LookupOwner custody; no KernelMount, no kernel/ caller"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/captured_reader.rs", "to": "engine/crates/layerfs-overlay/src/custody/captured_reader.rs", "op": "move", "why": "lifetime/ split: owner custody"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/file_owners.rs", "to": "engine/crates/layerfs-overlay/src/custody/file.rs", "op": "rename", "why": "lifetime/ split: OpenFile/FileRead custody; folder says owners"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/operation.rs", "to": "engine/crates/layerfs-overlay/src/custody/operation.rs", "op": "move", "why": "lifetime/ split: OperationOwner custody"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/indexed_operation_record.rs", "to": "engine/crates/layerfs-overlay/src/custody/operation_record.rs", "op": "rename", "why": "record jobs fenced by OperationOwner; one word operation_record"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/orphan.rs", "to": "engine/crates/layerfs-overlay/src/custody/orphan.rs", "op": "move", "why": "lifetime/ split: files kept by owners after unlink"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/source.rs", "to": "engine/crates/layerfs-overlay/src/custody/source.rs", "op": "move", "why": "lifetime/ split: base-source custody"},
  {"from": "engine/crates/layerfs-overlay/src/custody/mod.rs", "to": "engine/crates/layerfs-overlay/src/custody/mod.rs", "op": "new", "why": "module root of custody/"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/mod.rs", "to": "engine/crates/layerfs-overlay/src/lifecycle/mod.rs", "op": "rename", "why": "lifetime/ vague (rule 12) and Rust-term collision; lifecycle per rule 19"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/workspace.rs", "to": "engine/crates/layerfs-overlay/src/lifecycle/workspace.rs", "op": "move", "why": "open_workspace/state: Workspace lifecycle"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/generation.rs", "to": "engine/crates/layerfs-overlay/src/lifecycle/capture.rs", "op": "rename", "why": "capture/install calls"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/composition.rs", "to": "engine/crates/layerfs-overlay/src/lifecycle/composition.rs", "op": "move", "why": "resolve_failed_capture"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/close.rs", "to": "engine/crates/layerfs-overlay/src/lifecycle/close.rs", "op": "move", "why": "terminal close"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/frontier.rs", "to": "engine/crates/layerfs-overlay/src/lifecycle/frontier.rs", "op": "move", "why": "retained capture / pending publications"},
  {"from": "engine/crates/layerfs-overlay/src/lifetime/tickets.rs", "to": "engine/crates/layerfs-overlay/src/lifecycle/tickets.rs", "op": "move", "why": "reply tickets awaited by capture and close"},
  {"from": "engine/crates/layerfs-overlay/src/open/kernel.rs", "to": "engine/crates/layerfs-overlay/src/open/state.rs", "op": "rename", "why": "file defines OpenState, the whole in-memory open state"},
  {"from": "engine/crates/layerfs-overlay/src/contract/indexed_operation_record.rs", "to": "engine/crates/layerfs-overlay/src/contract/operation_record.rs", "op": "rename", "why": "same word as vfs/store-service ports and the test area"},
  {"from": "engine/crates/layerfs-overlay/src/maintenance/indexed_operation_record.rs", "to": "engine/crates/layerfs-overlay/src/maintenance/operation_record.rs", "op": "rename", "why": "same word; matches sibling step nouns"},
  {"from": "engine/crates/layerfs-overlay/src/database/record_plan.rs", "to": "engine/crates/layerfs-overlay/src/database/explain.rs", "op": "rename", "why": "holds explain_indexed_operation_record; record_plan is ambiguous"},
  {"from": "engine/crates/layerfs-overlay/src/namespace/captured_namespace.rs", "to": "engine/crates/layerfs-overlay/src/namespace/captured.rs", "op": "rename", "why": "file repeats folder name (rule 14)"},
  {"from": "engine/crates/layerfs-overlay/examples/indexed_operation_record_profile.rs", "to": "engine/crates/layerfs-overlay/examples/operation_record_profile.rs", "op": "rename", "why": "one word operation_record"},
  {"from": "engine/crates/layerfs-overlay/tests/payload/fixture.rs", "to": "engine/crates/layerfs-overlay/tests/support/fixture.rs", "op": "move", "why": "used by all three test areas (13 files), not payload only"},
  {"from": "engine/crates/layerfs-overlay/tests/support/mod.rs", "to": "engine/crates/layerfs-overlay/tests/support/mod.rs", "op": "new", "why": "support module root included by each area main.rs"},
  {"from": "engine/crates/layerfs-overlay/tests/device_fixture.sh", "to": "engine/crates/layerfs-overlay/tests/support/device_fixture.sh", "op": "move", "why": "manual proof script, a support file; update its line-40 path"},
  {"from": "engine/crates/layerfs-overlay/tests/enospc_as_eio.c", "to": "engine/crates/layerfs-overlay/tests/support/enospc_as_eio.c", "op": "move", "why": "LD_PRELOAD shim compiled by device_fixture.sh"},
  {"from": "engine/crates/layerfs-overlay/tests/kernel_memory.rs", "to": "engine/crates/layerfs-overlay/tests/standalone/kernel_memory.rs", "op": "move", "why": "installs #[global_allocator]; needs its own process and [[test]] entry"},
  {"from": "engine/crates/layerfs-overlay/tests/payload/payload.rs", "to": "engine/crates/layerfs-overlay/tests/payload/layers.rs", "op": "rename", "why": "file repeats area name; tests layered reads and staircase"},
  {"from": "engine/crates/layerfs-overlay/tests/operation_record/indexed_operation_record.rs", "to": "engine/crates/layerfs-overlay/tests/operation_record/identity.rs", "op": "rename", "why": "file repeats area name"},
  {"from": "engine/crates/layerfs-overlay/tests/operation_record/indexed_operation_record_cursor.rs", "to": "engine/crates/layerfs-overlay/tests/operation_record/cursor.rs", "op": "rename", "why": "drop repeated area prefix"},
  {"from": "engine/crates/layerfs-overlay/tests/operation_record/indexed_operation_record_entries.rs", "to": "engine/crates/layerfs-overlay/tests/operation_record/entries.rs", "op": "rename", "why": "drop repeated area prefix"},
  {"from": "engine/crates/layerfs-overlay/tests/lifetime/main.rs", "to": "engine/crates/layerfs-overlay/tests/custody/main.rs", "op": "rename", "why": "test area follows src custody/ (4 of 6 topics are custody)"},
  {"from": "engine/crates/layerfs-overlay/tests/lifetime/allocation_filesystem.rs", "to": "engine/crates/layerfs-overlay/tests/custody/allocation_filesystem.rs", "op": "move", "why": "area rename"},
  {"from": "engine/crates/layerfs-overlay/tests/lifetime/captured_namespace.rs", "to": "engine/crates/layerfs-overlay/tests/custody/captured_namespace.rs", "op": "move", "why": "area rename"},
  {"from": "engine/crates/layerfs-overlay/tests/lifetime/error_identity.rs", "to": "engine/crates/layerfs-overlay/tests/custody/error_identity.rs", "op": "move", "why": "area rename"},
  {"from": "engine/crates/layerfs-overlay/tests/lifetime/kept_live.rs", "to": "engine/crates/layerfs-overlay/tests/custody/kept_live.rs", "op": "move", "why": "area rename"},
  {"from": "engine/crates/layerfs-overlay/tests/lifetime/orphan_wake.rs", "to": "engine/crates/layerfs-overlay/tests/custody/orphan_wake.rs", "op": "move", "why": "area rename"},
  {"from": "engine/crates/layerfs-overlay/tests/lifetime/orphans.rs", "to": "engine/crates/layerfs-overlay/tests/custody/orphans.rs", "op": "move", "why": "area rename"}
]
```

# Handoff: R4 captured namespace construction and incremental topology

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` on local `main`. R2 and R3 are
implemented and functionally verified; both are "IMPLEMENTED — owner acceptance
pending" in the [rollout ledger](ROLLOUT-LEDGER-20261008.md). This handoff
assigns **R4 only**: construct a complete canonical filesystem root from one
exact Capture, with bounded validation and incremental topology. R5 (mounted
Commit), R6, timing qualification and retirement of the root reference tree are
not authorized. Do not push, open a pull request, start another worktree or
publish anything remotely.

The lead agent is **explicitly asked to work with subagents**, under the rules
in [Working with subagents](#working-with-subagents).

## Checkpoint

R3 commit `37dcf5405731566e39dba42bcdec2aaaa1ffde89`, first parent
`fde14d848240d53a7d977b4c35034e06ecc211be`. Inspect HEAD and status before
editing. This handoff and an edit to the root `AGENTS.md` (the "File and folder
layout listings" rule) may still be uncommitted; carry both in the first R4
commit, as a documentation-only commit with delta 0.

| Product | Git tree |
| --- | --- |
| `core/crates` | `87343cf7b2dc151a6be87538571d8f08e26ca7bb` |
| root reference `crates` | `498dd1917812ae90efb8841f57e22bfc284e96fb` |
| preserved `core/crates/layerfs-fuse-legacy` | `be11d96ec9db31b897109323379fec9f3e009dae` |

Read first, completely:

- Root [AGENTS.md](../../../../AGENTS.md) and [core/AGENTS.md](../../../AGENTS.md),
  including [shared construction boundaries](../../../AGENTS.md#shared-construction-and-completion-boundaries)
  and the mandatory [optimization guide](../../../../docs/general/optimization-guide.md).
- The R4 section of the [R2–R5 implementation handoff](HANDOFF-R2-R5-IMPLEMENTATION-20261008.md)
  and the R4 row of the [rollout](CLUSTER-TWO-LOC-AND-ROLLOUT-20261008.md).
  They are the requirement; this handoff orders the work and does not relax it.
- The [R4 component checkpoint](checks/r4-captured-namespace-port-20261008/19-results.md):
  what the page port does and what it states is still missing.
- The [R3 completion record](R3-COMPLETION-20261008.md) and
  [native mutation and kernel coherence](../../architecture/77-native-mutation-coherence.md):
  the states a mounted Workspace can now be in.
- The [reviewed layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md). It predates
  R2 and R3; treat its file names as candidates.

[R4-UPSTREAM-CACHE-20261007.md](R4-UPSTREAM-CACHE-20261007.md) is an older,
unrelated checkpoint that happens to share the label.

## Binding instructions

Everything in the two guides applies. The points most often at stake in R4:

- **Disposable/WAL/synchronous=OFF is the only permitted global Store profile.**
  Select it explicitly. Durable is `NOT_RUN — disabled by owner until explicit
  reauthorization`. The daemon overlay stays MEMORY/OFF/EXCLUSIVE.
- **One construction producer.** Export `LAYERFS_CONSTRUCTION_WORKERS=1`. No
  helper lane, second encoder, second validator or host-Init detour. Content owns
  canonical filesystem, attribute, value, directory and inode algorithms; Storage
  owns encoding, deduplication and packs; Persistence owns writes.
- **Construct from the Capture, never from live active state.** Every pass and
  point lookup retains the same captured owner, root and frontier. Do not filter
  captured changes by Exec registration or completion.
- **No artificial total cap** on edits, files, names, Workspace or Commit size,
  and no resident whole-namespace container. Bounded windows, indexed or backed
  state, streaming and backpressure.
- **No quadratic or worse work.** A small change must not walk the whole base.
  Use SQLite-backed mutable state rather than custom duplicate trees or graphs.
  A SQLite optimization claim needs both `EXPLAIN` and correlated runtime
  profiling; wall time alone is not evidence.
- **One attempted operation.** No retry, replay, refresh or error-driven
  alternate source. Keep the first original failure with its owners; release
  only after the actual consumers end.
- **Every test command has an explicit wall limit of at most 120 s.** Build with
  `--no-run` first. Never loop, background or output-filter a test. A test that
  reaches its limit has failed as a hang: diagnose from source before any rerun.
  Keep every failed attempt and its receipt; never relabel a failure as a pass.
- **Host cargo is `cargo +1.85.1 --locked`.** Linux builds use the pinned image
  in [Environment](#environment).
- **No new dependency; no third-party edits** beyond the authorized fuser
  patches. R4 should not need a native fuser build at all.
- **Product source only under `crates/<package>/src/`**, no test hooks or
  test-only APIs. New production files at most 999 lines; `lib.rs`/`mod.rs` at
  most 200 and declarations only. No `fsync`-family call on Disposable backing.
- **Every commit** records `Production LOC: <before> -> <after> (delta ...)`
  with subtotals from the pinned counter, and ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Commit locally on
  `main` only.
- **No cold, timing, storage or resident-memory claim.** R4 evidence is
  functional results and counted work. A timing sample, if one is ever taken,
  follows the measurement workflow and is labelled diagnostic.

## What exists

- **The Commit driver.** `BoundWorkspace::commit` in
  [store/commit.rs](../../../crates/layerfs-daemon/src/store/commit.rs) captures
  once and calls one caller-supplied producer
  `FnOnce(&Save, Capture, &BranchSnapshot) -> Result<FilesystemRootId, CommitError>`.
  R4 builds what goes in that closure. Changing the driver is R5.
- **The canonical update.** `update_filesystem_streamed_backed` in
  [filesystem/update.rs](../../../crates/layerfs-content/src/filesystem/update.rs)
  takes `PreparedDirectoryStreams`, `IndexedConstructionBacking` and an optional
  `OrderingBacking`. The input contract is `StreamedRowSource` in
  [rows/stream.rs](../../../crates/layerfs-content/src/filesystem/rows/stream.rs):
  ordered header, change, inode and fresh-serial cursors, each with a matching
  point lookup that must answer the same sealed rows.
- **The changed-file constructor.** `CapturedFileEdits::prepare`/`construct` in
  [construction/captured/owner.rs](../../../crates/layerfs-workspace/src/construction/captured/owner.rs).
  Existing files take the retained-view incremental edit route; new files are
  constructed from runs. Reuse it; do not write a second one.
- **The captured page port.** `OverlayCapturedNamespace` in
  [ports/captured_namespace.rs](../../../crates/layerfs-workspace/src/ports/captured_namespace.rs)
  and its Owner adapter in
  [captured_namespace_port.rs](../../../crates/layerfs-daemon/src/overlay/captured_namespace_port.rs):
  captured inode and directory-entry pages over the retained captured reader,
  64-row windows, whiteouts included. The adapter is synchronous and for
  constructor threads only.
- **Resident state that R4 must bound.** In
  [filesystem/validate.rs](../../../crates/layerfs-content/src/filesystem/validate.rs):
  the `additions` and `by_parent` maps, the per-directory `demanded` vector, the
  `retained`/`candidates`/`bound`/`seen` containers of the alias pass and the
  `records`/`absent` inode cache. In
  [validate/cycles.rs](../../../crates/layerfs-content/src/filesystem/validate/cycles.rs):
  the `declared`, `edges` and `seen` containers. Confirm each from source; this
  list is where to look, not a finished audit.

What is missing: the sealed indexed adapter that implements `StreamedRowSource`
over a Capture; canonical metadata patches and changed file roots; the complete
namespace producer; bounded Content validation and incremental topology; exact
producer failure custody.

## Target layout

Production LOC at `37dcf5405` from the pinned `tools/production_loc.py --files`.
Directories marked "part" show only the files R4 is expected to touch, and their
bracket sums only those. Planned names come from the reviewed layout and are
candidates: add or split only for real behaviour or the line limits.

```
layerfs-workspace/src/
  ports/                              [390]
    captured_namespace.rs               36   (extend: points the producer needs)
    captured_runs.rs                    19
    files.rs                            37
    lengths.rs                           4
    mod.rs                              15
    operation_record.rs                197
    overlay.rs                          82
  construction/                      [1263]
    mod.rs                               4
    records.rs                         247
    context.rs                           —   new
    driver.rs                            —   new
    outcome.rs                           —   new
    captured/                        [1012]  (reused as is)
      context.rs                        84
      mod.rs                             7
      normalize.rs                     116
      owner.rs                         256
      scan.rs                          246
      source.rs                        185
      state.rs                         118
    namespace/                           —   new
      mod.rs                             —
      cursor.rs                          —
      directories.rs                     —
      inodes.rs                          —
      normalize.rs                       —
      assemble.rs                        —
    scratch/                             —   cond.
      mod.rs                             —
      records.rs                         —
      release.rs                         —
```

```
layerfs-daemon/src/
  overlay/                           [1105]  (part)
    captured_namespace_port.rs          45   (extend)
    captured_run_port.rs                49
    commands.rs                        749   (extend only for a missing job)
    indexed_operation_record.rs        109
    operation_record_port.rs           153
  store/                              [583]  (part)
    commit.rs                          167   (unchanged in R4)
    commit_types.rs                    161
    ports.rs                           220   (extend)
    settle.rs                           35
```

```
layerfs-overlay/
  sql/                                [566]
    accounting.sql                     281   (extend only if needed)
    schema.sql                         285   (extend only if needed)
  src/namespace/                      [810]
    compound.rs                        347
    directory_entry.rs                 129
    inode.rs                           331
    mod.rs                               3
    captured_namespace.rs                —   cond.
  src/lifetime/                       [549]  (part)
    captured_reader.rs                 190
    composition.rs                     286
    frontier.rs                         73
```

```
layerfs-content/src/filesystem/     [6791]  (part of 11266)
  input.rs                            146
  objects.rs                          120
  update.rs                           684   (resident state to bound)
  validate.rs                         634   (resident state to bound; keeps its owner)
  validate/                           [252]
    cycles.rs                         141   (resident state to bound)
    entries.rs                        111
    backed.rs                           —   new
    incremental.rs                      —   new
  directory/                          [493]
    codec.rs                          156
    mod.rs                              6
    read.rs                           292
    update.rs                          39
    changes.rs                          —   cond.
  references/                        [2675]
    backing.rs                        328
    indexed.rs                        175
    indexed_release.rs                360
    indexed_rows.rs                   145
    indexed_wire.rs                   153
    meaning.rs                        107
    merge.rs                          173
    mod.rs                             23
    operation.rs                      146
    record.rs                         120
    reduce.rs                         378
    release.rs                        164
    runs.rs                           403
    indexed_validation.rs               —   cond.
  rows/                              [1141]
    check.rs                           72
    mod.rs                             21
    source.rs                          86
    spool.rs                          441
    stream.rs                          47
    stream_update.rs                   67
    update.rs                          56
    view.rs                           351
  state/                              [646]
    codec.rs                           76
    initial.rs                        107
    mod.rs                             10
    roots.rs                          115
    store.rs                          328
    types.rs                           10
```

Tests stay outside `src/`: `layerfs-content/tests/filesystem_*.rs`,
`layerfs-workspace/tests/` and `layerfs-daemon/tests/{captured_namespace,captured_runs,store_commit}.rs`
are the existing homes. Fuse, Bridge, the SDK and Sandbox are not part of R4;
say why before changing any of them.

## Required behaviour

- **Complete final namespace.** Names, links, metadata, changed file roots and
  fresh serial declarations for everything the Capture publishes. Metadata
  patches preserve untouched keys. Stable existing input is kept without a bulk
  Overlay copy.
- **Cursors and points agree.** Every ordered cursor of `StreamedRowSource` and
  its point lookup answer the same sealed rows across all passes. Full names
  live in values, not truncated identity keys. Reuse the existing 64-row and
  64 KiB job windows and `IndexedConstructionRecords`.
- **Policy from the Store.** Use the actual Store-derived construction policy
  and capacities, `Save::sink` and same-Save authenticated reads
  (`FilesystemObjects::new_with_accepted`).
- **Bounded validation.** Replace the resident state listed above with backed
  or incremental state in its owning Content algorithm. A streamed front end
  around unbounded validation does not meet the contract.
- **Incremental topology.** Work is proportional to the change and the depth it
  touches, not to the base. No whole-base non-file alias scan for a small
  change.
- **Exact failure custody.** Preserve the first original file, namespace,
  backing or captured-reader failure with its owners. No root-only callback
  error that drops retained owners.

States R3 made reachable, which the producer must get right and the proofs must
include:

| Captured state | Required result |
| --- | --- |
| A file unlinked while open, or held only by a kernel lookup (link count 0) | Absent from the final namespace; its content is not constructed or referenced |
| A name replaced by rename while the old inode is still held | The name binds the new inode only |
| Hard links created and removed | Exact final link count and one inode for all remaining names |
| A directory moved across parents, possibly more than once | One final binding; no cycle; descendants intact without being rewritten |
| An inherited name removed, or removed and recreated | Whiteout where the base still binds it; the recreated inode is a fresh serial |
| Shrink then regrow, `O_TRUNC`, mapped stores | Final bytes exactly the published frontier, no discarded tail |
| A directory removed while a process still has it as working directory | Absent from the final namespace |

Later active mutations made after the Capture must not appear in the result.

## Working with subagents

The lead agent owns the plan, every decision, every commit and the final
report. Subagents exist to read widely in parallel, to implement disjoint
pieces, and to review with a fresh context. A subagent's report is a claim: the
lead reads the diff and the receipt before recording anything as done.

Rules that hold for every subagent, and that the lead must write into each
brief because a subagent starts with no context:

1. **One checkout, no worktrees.** Never pass a worktree isolation option. All
   agents share this working tree, so file ownership is by assignment.
2. **Only the lead stages, commits, counts LOC, edits the rollout ledger or the
   completion record, and removes anything.** Subagents never run `git add`,
   `git commit`, `git stash`, `git checkout` or `git reset`.
3. **Disjoint write sets.** Each implementing subagent gets an explicit list of
   files it may create or edit. Two agents never hold the same file. A needed
   change outside the list is reported to the lead, not made.
4. **One Cargo, Docker or test command at a time in this checkout**, under the
   nonblocking lock `core/target/r4.lock`. A busy lock is not a failure and not
   a test attempt; wait and take it again. Read-only subagents run no Cargo,
   Docker or test command at all.
5. **Every binding instruction above is repeated in the brief**: Disposable
   only, the 120 s test ceiling, build with `--no-run` first, one attempt, keep
   every receipt, no test hooks, no new dependency, no third-party edits, the
   file-size limits, and the preservation list.
6. **Receipts are append-only.** A subagent that runs a test writes its full
   output to `checks/r4-captured-namespace-20261008/<track>-attempt<N>-<binary>.txt`
   and reports every attempt, including failures. It never deletes or
   overwrites a receipt.
7. **A subagent stops and reports** when it meets an ambiguity in the
   requirement, a conflict with current source, or a result it cannot explain.
   It does not choose a contract point on its own.

### Phase 1: parallel read-only audits

Launch these together. Each returns findings with file and line references and
runs no build.

| Audit | Question to answer |
| --- | --- |
| Content resident state | Every container in `filesystem/update.rs`, `validate.rs` and `validate/` whose size grows with the base or the total change: what it holds, who reads it, and which existing backed facility (`state/`, `sorted/`, `references/indexed*`) could hold it instead |
| Content cost | For each pass of the canonical update and validation, what it visits per changed entry and per base entry; where a small change walks the whole base |
| Captured reader | Which ordered cursors and point lookups `StreamedRowSource` needs that the existing reader jobs and SQL indexes already serve, and which are missing. Include the query plan for each |
| Changed-file constructor | Exactly how `CapturedFileEdits` is prepared, constructed and released, what custody it holds, and what the namespace producer must supply to and take from it |
| Commit driver and custody | What `BoundWorkspace::commit` hands the producer, which failures it distinguishes, and what the producer must retain on each failure path |
| R3 states | How each row of the table above appears in captured inode and directory-entry rows: link counts, orphan rows, whiteouts, moved-directory changes |

### Phase 2: the lead writes the plan

From the audits and its own reading, the lead writes
`checks/r4-captured-namespace-20261008/01-deepest-file-plan.md`: every file to
add or change, what it owns, what it reuses, and why it cannot live in an
existing file, in the layout format the root guide requires. The plan also
**freezes the interfaces between tracks**: the Content traits or functions the
producer will call, the port methods the producer needs, and the producer's own
entry point. Open questions for the owner are listed there, not decided
silently. Commit it as documentation before any product edit.

### Phase 3: implementation tracks

| Track | Write set | Depends on |
| --- | --- | --- |
| C — Content bounded validation and incremental topology | `layerfs-content/src/filesystem/**` and `layerfs-content/tests/**` | Phase 2 |
| P — captured cursors and points | `layerfs-overlay/src/namespace/**`, `layerfs-overlay/sql/**`, `layerfs-overlay/tests/**`, `layerfs-daemon/src/overlay/**`, `layerfs-daemon/src/store/ports.rs`, `layerfs-workspace/src/ports/**` | Phase 2 |
| W — the namespace producer | `layerfs-workspace/src/construction/**`, `layerfs-workspace/tests/**` | The frozen interfaces; lands after C and P |
| I — integration, lead only | `layerfs-daemon/tests/**`, documentation | C, P and W |

C and P touch different crates and may be written at the same time; their
builds and tests still take the lock in turn. W may be drafted against the
frozen interfaces while C and P are in progress, and is built only after both
have landed. If a track needs an interface change, it stops and the lead
decides; the other tracks are told before they continue.

Each track is committed by the lead as its own reviewable increment with its
own receipts and LOC comparison, and each commit says honestly what is and is
not wired yet.

### Phase 4: independent reviews

After the tracks land, launch fresh read-only reviewers together. They have not
seen the implementation discussion and are asked to find what is wrong.

| Reviewer | Asked to find |
| --- | --- |
| Cost and bounds | Any path whose work or resident state grows with the base rather than the change; any container without a bound; any query without an index. Must point at code, and at the counted evidence that would show it |
| Failure custody | Any error path that drops an owner, replaces the original failure, retries, or releases before its consumers end |
| Oracle adversary | Input classes the proofs do not cover, and assertions that would still pass if the producer were wrong |
| Rules | A second encoder or validator, live state read during construction, a test hook, a file over its limit, anything in the binding instructions |

The lead verifies each finding against source, fixes what is real as a new
source identity, and records findings it rejects with the reason.

## Order of work

1. Documentation-only commit carrying this handoff and the `AGENTS.md` edit.
2. Phase 1 audits, then the Phase 2 plan, committed.
3. Track C and track P.
4. Track W.
5. Track I: drive the existing Commit driver with the producer over a real
   Disposable Store in a daemon test, without a mount and without changing the
   driver.
6. Phase 4 reviews and their corrections.
7. Final suites, architecture documentation, the rollout ledger and an R4
   completion record. Then stop. Do not continue into R5.

## Proofs R4 owns

The proof plan has no numbered rows for R4; these are derived from the R4
requirement and are the acceptance set unless the owner amends them.

| Row | Requirement |
| --- | --- |
| R4-1 | Full final namespace, bytes, metadata and link identity equal an independent oracle, for new, deleted, changed, renamed and moved entries |
| R4-2 | Wide, deep, sparse, alias and cycle cases; a cycle is refused with its exact failure and nothing is published |
| R4-3 | Every row of the R3 state table |
| R4-4 | Arbitrary captured fragmentation of one file gives the same root as the unfragmented edit |
| R4-5 | Cursors and points agree across all passes; names sharing a long prefix and names of 255 bytes |
| R4-6 | Incremental cost: counted construction, SQL, I/O, copy and release work for a fixed small change does not grow when the base grows; counted, not timed |
| R4-7 | Bounded state: no resident container proportional to the base or to the total change; shown by counted windows, not by a heap sample |
| R4-8 | Later active mutations after the Capture do not appear; install and Close leave the reader's original state |
| R4-9 | Failure custody: the first original failure from each of file, namespace, backing and captured reader is preserved with its owners, and release follows the consumers |
| R4-10 | The producer through `BoundWorkspace::commit` over a real Disposable Store: the published root read back from a fresh bind equals the oracle |

A resident `FilesystemInput` demonstration or an existing-file-only case does
not satisfy any row.

## Standing risks

- **Three host test binaries fail on a timing race**: `captured_file_edits`
  (workspace), `root_qualification` and `edit_backing` (daemon). They read the
  owner's `credited_bytes`/`outstanding` immediately after a completion, while
  the credit can still be held by the engine thread. They pass on Linux. R4
  works in exactly this area, so settle them first: bound the wait as R2 did
  for `captured_runs`, as a separate test-only commit, keeping the first
  failing receipt. Do not change `credits.rs` for this.
- **R2 and R3 decisions are still unreviewed.** None gates R4. The one that
  shapes captured state is R3's: a removed file stays openable under a live
  kernel reference, so removed-but-held inodes with open handles are common.
- **The layout predates R2 and R3.** Do not create a planned file because it is
  listed.
- **The Owner adapter of the page port is synchronous.** It must not be called
  from native receive or service workers.
- **Bounding validation is the deep part.** If it cannot be finished, say so:
  a producer over unbounded validation is a partial R4, not a complete one.

## Environment

- Host: macOS ARM64, `cargo +1.85.1 ... --locked`. The machine default 1.96.0
  is not the pinned toolchain. macOS has no `timeout`; use a bounded runner.
- Linux: image
  `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`,
  repository bound at `/work`, `CARGO_HOME=/work/core/target/cluster2-linux-cargo`,
  `CARGO_TARGET_DIR=/work/core/target/cluster2-linux`.
- Store, Overlay and scratch files on the container's native filesystem or the
  host temporary directory, never under the repository bind mount.
- Local, untracked conveniences from R3 under `core/target/`: `r3-locked.pl`
  (nonblocking lock, wall limit, group kill), `r3-suite.py` (one run per test
  binary with receipts) and `r3-count.py` (LOC comparison over git archives).
  Copy and rename them for R4 if useful; the pinned counter is the authority.
- Three daemon cases need explicit preconditions and stay unrun unless
  supplied: `complete_installed_roots::huge_native_namespace_is_complete_after_install`,
  `shared_processes` on Linux and `host_handoff` on the host.

## Preservation

Do not touch these unrelated running containers:

- `9cf2fe345496b45d79ff272e6c698552948f1e92ea106926826a770592623b24`
- `ce75ac504df9d0e58cedcdd0df93fe7ef39c3b66ef3c13b0bb6b580def64516c`
- `d2433851ea5990e273b0673fdbd5c180917f1216f3c81bdfb4517da0207e2c2a`
- `d2550144998b4b71de98cb9ecf6448f87c7b99a3c2f69ea778a29413fffa5ffb`

Leave the exited `layerfs-e04-*` containers alone. Remove only containers and
volumes you create.

Preserve these untracked files and do not stage them:
`HANDOFF-PRE-S8-SERVERLESS-20261007.md`, `HANDOFF-S7-S9-RESUME-20261006.md` and
`S7-S9-SPEED-TEST-PLAN.md` in this directory, and
`checks/multi-workspace-model-confirmation-20261008/04-commit-confirmation.json`.
Do not rewrite raw evidence. Do not touch
`/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff`.

## Source accounting

| Checkpoint | Combined production LOC | Migration |
| --- | --- | --- |
| `37dcf5405731566e39dba42bcdec2aaaa1ffde89` | 180867 → 180871, delta +4 | None; all R3 growth in active core members |

Current totals: core 115454, active 72580, excluded predecessors 38878, excluded
integration 3996, root reference 65417. Use `tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, over exact
first-parent and final staged archives with exact active Cargo-member paths. A
documentation-only or test-only commit reports the unchanged total and delta 0.

## Done means

R4 is finished when the producer constructs a complete root from an exact
Capture, the resident state named above is bounded in its owning Content
algorithm, rows R4-1 to R4-10 have receipts with every failed attempt retained,
each review finding is fixed or answered, the architecture documents and the
rollout ledger describe the implemented state, each commit carries its LOC
comparison, and a completion record states plainly what is partial or unrun.
Then stop and report: what is wired, which rows passed, which are partial or
unrun and why, every failed attempt, every contract decision that needs owner
review, how subagents were used and which of their findings were rejected, and
the LOC comparison for each commit.

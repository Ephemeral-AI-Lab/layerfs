# Serverless Store: deepest-file plan

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-07 against `main` `af963a718`. No product source is changed
> by this plan. It implements the owner direction recorded as
> [K28–K33](../303/08-decisions-provenance.md#3-decisions-of-this-design) and
> the [integration contract](../303/06-cluster-one-integration.md).

## Owner direction

1. Every Linux daemon opens the global SQLite Store directly from a volume the
   daemons share. No host or server adapter sits in the data path.
2. A mounted SQLite file is the raw shape of the serverless design. Another
   database may later replace it behind the same ports.
3. No retry. Keep it simple.
4. Both Store profiles are supported; verification focuses on Disposable.
5. The LOC rules are enforced, and a large cut is expected.
6. Not built: #307 S9 R1 application assembly, R3 restart custody, R4 remote
   Save. R2 (Content root qualifier) and Q1 (complete roots) stay valid.

## Shape

```text
 macOS host                                Docker Linux VM (one kernel)
 --------------------------------          ---------------------------------------
 layerfs-sdk (thin)                        named volume, mounted in daemons only
   init:    create -> import -> seal         store.sqlite
   install: stream the sealed file  ------>      ^          ^          ^
   control: mount / Exec / Commit /          daemon A   daemon B   daemon C
            status / unmount  <---------->   layerfs-daemon, one process each
                                               control   five commands
                                               store/    Handles + Storage + History
                                               overlay/  local mutable state
                                               Workspace + FUSE -> Bash
```

## Folder structure and production LOC

"Now" is nonblank non-comment production LOC of committed `af963a718`, from
`tools/production_loc.py` (SHA-256 `c0fe7f36…624adb`). "After" is an estimate
from source reading by three read-only reviews; nothing was built. Every new
file is far below the 999-line ceiling and every `mod.rs`/`lib.rs` stays
declaration-only under 200 lines.

```text
core/crates/                                            now    after   change
  layerfs-api/sdk/src/                                 6,985    ~330   -6,655
    client/                         RETIRED            1,865       0
    runtime/                        RETIRED            5,111       0
    lib.rs                                                 9     ~10
    init.rs        NEW  create, import, first Branch, seal, manifest    ~90
    install.rs     NEW  stream the sealed file to a daemon install      ~60
    control.rs     NEW  connect, five commands, Exec output stream     ~170

  layerfs-api/core/src/             RETIRED (excluded    291       0     -291
                                    today; vocabulary
                                    moves to Bridge)

  layerfs-bridge/src/                                  1,259    ~690     -569
    native/        handshake, channel, io, profile,      746    ~540
                   types, error (framing.rs RETIRED)
    codec/                          RETIRED              421       0
    contract/                       RETIRED               86       0
    control.rs     NEW  request/reply records for the five commands    ~145
    lib.rs                                                 6      ~5

  layerfs-daemon/src/                                  2,835  ~3,220     +385
    overlay/  service/              unchanged          2,328   2,328
    upstream/                       RETIRED              491       0
    store/         NEW, replaces upstream/                      ~400
      mod.rs       declarations                                   ~8
      open.rs      one Handles + Storage + History at startup    ~85
      ports.rs     objects, lengths, serials over the Store      ~75
      bind.rs      mount: snapshot, root checks, Workspace bind ~110
      commit.rs    Save scope, stage, publish, discard, install ~120
    control.rs     NEW  serve the five commands                 ~200
    exec.rs        NEW  Bash launch, confinement (S8)           ~230
    install.rs     NEW  write-then-rename the sealed Store       ~40
    lib.rs                                                16     ~22

  layerfs-persistence/src/                             6,706  ~6,365     -341
    store/open.rs                   macOS gates removed  109    ~109
    store/seal.rs  NEW  checkpoint, close, verify one file       ~35
    store/handles.rs                checkpoint -> seal     46     ~43
    backend/sqlite/allocation.rs        DELETED (O-22)    162       0
    backend/sqlite/allocation_owner.rs  DELETED (O-22)    111       0
    backend/sqlite/connection.rs    gate, allocation out  308    ~238
    backend/sqlite/transaction.rs   before_pack out       279    ~251
    backend/sqlite/profile.rs       WAL for both (O-21)   109     ~94
    backend/sqlite/rows.rs          lock code -> Busy      54     ~55
    backend/records.rs              typed Busy            125    ~122
    backend/sqlite/{publish,units_publish,mod}.rs         182    ~177
    history/{catalog,staging,commit}.rs  stage+publish    590    ~610
                                         in one tx (O-23)

  layerfs-storage/src/                                 9,336  ~9,350      +14
    port/persistence.rs             Busy variant           97     ~99
    store/error.rs                  Busy variant          140    ~152

  layerfs-history/src/contract/catalog.rs  one method      71     ~76      +5
  layerfs-workspace/src/            scoped client helpers 3,861 ~3,830     -31
                                    removable after store/
  layerfs-project/src/              unchanged           1,562   1,562       0
  layerfs-content/ layerfs-overlay/ layerfs-telemetry/  unchanged by this plan
```

| Scope | Now | After (est.) | Change |
| --- | ---: | ---: | ---: |
| Data path only: SDK, API core, Bridge, daemon `upstream/` → `store/` | 9,026 | ~1,420 | about −7,600 (−84%) |
| All touched crates, including the control, Exec and install slices that do not exist today | 33,652 | ~26,170 | about −7,480 |
| Active core workspace | 61,711 | ~54,520 | about −7,190 |
| Core scope (active, excluded predecessors and excluded integration) | 104,876 | ~97,390 | about −7,480 |

Under the repository LOC rule this is reported as **retirement of the
host-mediated transport**, a scope change, not an algorithmic simplification.
Each commit carries its own exact first-parent comparison from the pinned
counter; the estimates above are never copied into a commit message.

Slices in the "after" column that are new product, not relocation: daemon
`control.rs`, `exec.rs`, `install.rs` (about 470) and SDK `init.rs`,
`install.rs`, `control.rs` (about 320). The daemon binary, control and Exec
were unbuilt S8 work under the previous design as well.

Avoided and never written: R4 remote Save and consumer, R1 application
assembly, R3 restart custody.

## What moves, what goes

| Today | After | Lines kept (est.) |
| --- | --- | ---: |
| SDK `runtime/sessions.rs` bind, lengths, finish-before-stage order | daemon `store/bind.rs`, `store/commit.rs` | ~35 of 357 |
| SDK `runtime/root_binding.rs` bounded root checks | `store/bind.rs`, reused before stage | ~40 of 82 |
| SDK `runtime/handlers/history.rs` stage, publish, discard requests | `store/commit.rs` | ~55 of 187 |
| SDK `runtime/ports/{serials,lengths}.rs` window and reply checks | `store/ports.rs` | ~28 of 84 |
| SDK `runtime/{owner,error,binding}.rs` | `store/open.rs` | ~57 of 147 |
| SDK service, supervisor, custody, wake, reply/wire/writer/failure handlers, `authorized_objects.rs`, all of `client/` | nothing | 0 of 6,048 |
| Daemon `upstream/operation.rs` per-operation remote failure custody | nothing | 0 of 125 |
| `FinalizedObject::admit` at the wire | nothing: objects come from in-process Content constructors, as in Project Init | 0 |

Peer authorization is not replaced in the data path. Authority becomes: which
containers get the volume, the authenticated control command, and hiding the
volume from Bash. Every daemon holds whole-Store authority.

## Storage and History under several writers

Reviewed from source, not run. The data model is already correct for several
writer processes under WAL: every id, counter and head is decided inside one
`BEGIN IMMEDIATE` transaction, and every process cache holds immutable rows or
advisory hints.

| State | Under another writer | Action |
| --- | --- | --- |
| Policy, capacities, layout, catalog id, incarnation, cursor key | Fixed at create | None |
| Pack-id and ordinal cursors, metadata window, stage token, inode high-water mark, Branch and stack heads | Read and advanced in one write transaction | None |
| Object locators | First writer wins; the loser re-reads and compares full bytes | None |
| Positive caches (locators, packs, value groups) | Rows are never updated or deleted | None |
| Negative caches, signature ring, pooled-value index, serial remainder | Stale but safe; costs reuse only | None |
| Storage `Busy` is a formatted string | Caller cannot type the refusal | Typed variant, about +15 |
| Disposable is a memory journal with rollback locking | A killed writer tears the file for all; readers and writer block each other | WAL with `synchronous=OFF` (O-21) |
| `SQLITE_PROTOCOL` falls to unknown and quarantines | A lock-layer refusal poisons the session | Map to `Busy`, +1 |
| Checkpoint TRUNCATE | Holds the writer lock for the whole WAL | Host seal only |
| Acquisition treats other epochs as abandoned | Would delete a live import | Host Init only; daemons get no acquisition handle |
| Stage row left after a `Busy` publish | Partial history state | One stage+publish transaction (O-23) |
| macOS preallocation and extent release | Sizes from main-file length; one-writer custody | Delete (O-22) |

Write-lock hold times are bounded per transaction: a reservation is one read and
one or two writes; a publication batch is at most 8,191 rows and 4 MiB, or one
singleton up to 16 MiB + 4 KiB; a history transition is about 12 point
statements. No lock spans a file, a Save or a Commit. The one unbounded class,
checkpoint TRUNCATE, is not available to a daemon.

## Consequences of no retry

Stated so they are accepted knowingly, not discovered later.

- A Save is many short write transactions. It succeeds only if none meets
  another writer. Several daemons committing large changes at once will see
  Commits fail with `Busy`.
- A failed Commit changes no history and no Workspace state. Immutable objects
  from its earlier waves stay stored and are reused by a later explicit Commit.
  Nothing collects them.
- A create that needs a new serial block (1,024 per refill) can be refused
  with `Busy`.
- Durable holds the write lock across its sync, so `Busy` is more frequent
  there than on Disposable.
- SQLite's own bounded internal waits when a reader begins under WAL cannot be
  removed without patching it. They are not product retries.
- The daemon serializes its own threads' Store jobs with one mutex. That is
  mutual exclusion inside a process, not a retry of an attempted operation.
- WAL size is unbounded while readers overlap; passive checkpoints can starve.

## Checkpoints

Each is one reviewable local commit with focused Disposable proofs, at most
120 s per test invocation, and an exact LOC comparison.

| # | Checkpoint | Proof on Disposable | Needs |
| --- | --- | --- | --- |
| 1 | Persistence opens on Linux: gates out, WAL profiles, typed `Busy`, lock-code mapping, `seal`, allocation owner deleted | Linux create/open/publish/read; a second connection holding the write lock yields `Busy` with no effect and a healthy session; sealed file has no sidecar and reopens | O-21, O-22 |
| 2 | Daemon `store/open.rs`, `ports.rs`, `bind.rs`; `upstream/` retired | Ported binding, length, serial and demand cases over a real sealed Store in the Linux image | 1 |
| 3 | Daemon `store/commit.rs`; stage+publish in one transaction | Committed, UpToDate, conflict with exact discard, missing dependency, `Busy` leaves no stage and no Workspace change | 2, O-23 |
| 4 | Retire SDK `client/`, `runtime/`; Bridge `codec/`, `contract/`, `framing.rs`; API core; their tests and examples | Remaining packages build, Clippy clean; the cut is reported as retirement | 2, 3 |
| 5 | Host `init.rs`, `install.rs`, `control.rs`; daemon `control.rs`, `install.rs`; Bridge `control.rs` | macOS Init → seal → install → Linux daemon mounts the installed root | 1, 4, O-18–O-20 |
| 6 | Exec and confinement (S8) | Bash cannot see or open the Store or overlay path, by name or through `/proc` | 5, O-24 |
| 7 | Two daemons on one volume | Concurrent Saves both land; a same-Branch race gives one `Committed` and one `HeadMoved`; a killed daemon does not damage the Store | 3, 5 |

Tests retired with checkpoint 4, none of which count as production LOC: every
file under `layerfs-api/sdk/tests`, Bridge `logical_frames.rs` and
`native_framing.rs`, daemon examples `upstream_consumer*` and the host half of
`e2_writes`. Ported: the bind, length, serial, UpToDate, conflict/discard,
pending-read and missing-dependency cases; daemon `tests/upstream.rs` becomes
`store.rs`; `upstream_docker.rs` becomes init → seal → install → direct open.
E04 stays closed on its recorded host-mediated topology and is not rerun.

Durable is implemented and builds at every checkpoint. Its execution stays
`NOT_RUN — deferred by owner for Disposable-only development`.

## Engineering notes

- **No new dependency.** Linux gets `rusqlite` `bundled` exactly as
  `layerfs-overlay` already declares it. The macOS-only `nix` edge leaves
  Persistence with the allocation owner. Exec confinement needs a private
  mount namespace and a user change; whether that is the locked `nix` crate on
  a new edge or existing container tools is decided at checkpoint 6 and
  reported before use.
- **Deployment invariant.** The Store volume is a named in-VM volume, never a
  host bind mount or network filesystem. The product cannot verify this.
- **Mixed profiles** on one file are undetectable. The profile is a
  provisioning fact in the install manifest.
- **Root qualification.** The Content whole-root qualifier runs in the daemon
  over overlay scratch records. It is an explicit paid operation, for example
  once after install, and is never part of mount.
- **Benchmark hosting rule** still routes the global Store to the host. It
  needs alignment before any new measurement is admitted.

## Decisions needed before code

| # | Question | Recommendation |
| --- | --- | --- |
| O-21 | Disposable for a shared Store | WAL with `synchronous=OFF` |
| O-22 | Delete the macOS allocation owner | Yes; one new Init speed measurement later |
| O-23 | One stage+publish history transaction | Yes |
| O-18 | Host fork and history reads after install | Two daemon-served control verbs |
| O-19 | Second import into an installed volume | Refused in the first slice |
| O-20 | Control transport | Keep the authenticated native channel |
| O-24 | Bash user | One unprivileged user per Workspace |

Checkpoint 1 is blocked only by O-21 and O-22. The others block later
checkpoints and can be answered in order.

## Working-tree state at this plan

The S9 R2 Content qualifier is implemented and uncommitted: module
`layerfs-content/src/filesystem/qualify/`, a sequential inode reader, 10 Content
tests and one daemon test over real overlay scratch. All pass on the host with
Clippy, formatting and the boundary guard clean. Its Linux run was interrupted
before a build completed and is **NOT_RUN**. Receipts, including the retained
failures, are under
[`checks/s9-root-qualification-20261007/`](checks/s9-root-qualification-20261007/).
The SDK proof-required bind that was to follow is dropped with the SDK runtime.

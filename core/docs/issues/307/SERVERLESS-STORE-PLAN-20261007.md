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
 macOS host                                 Docker Linux VM (one kernel)
 ---------------------------------          ----------------------------------------------
 layerfs-sdk (thin)                         named volume, mounted in daemons only
   init:    create -> import -> seal          store.sqlite  (WAL; one writer at a time,
   install: stream the sealed file  ------>                  readers never blocked)
   control: mount / Exec / Commit /               ^   ^            ^   ^
            status / unmount                    write read        write read
                    ^                             |   |             |   |
                    |  authenticated         +----+---+-----+  +----+---+-----+
                    +--- control channel --> | daemon A     |  | daemon B     |  ...
                                             |              |
                                             | control      five commands (+ fork, history)
                                             | store/       1 write handle: Save, history, serials
                                             |              N read handles: objects, lengths
                                             | object cache immutable objects by id, shared
                                             | overlay/     overlay.sqlite, local mutable state
                                             | Workspace    one mount per Workspace
                                             | FUSE  -->    any number of ordinary Bash processes
                                             +--------------+
```

Read path: FUSE → Workspace → object cache → read handle → SQLite snapshot.
Commit path, one thread: capture → Content construction → Save batches on the
write handle → stage+publish in one transaction → local base install.

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
    init.rs        NEW  create, import, first Branch, seal,             ~90
                        manifest with a Store locator
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

  layerfs-daemon/src/                                  2,835  ~3,250     +415
    overlay/  service/              unchanged          2,328   2,328
    upstream/                       RETIRED              491       0
    store/         NEW, replaces upstream/                      ~430
      mod.rs       declarations                                   ~8
      open.rs      one write handle + fixed read handles        ~105
      ports.rs     objects, lengths, serials over read handles   ~85
      bind.rs      mount: snapshot, root checks, Workspace bind ~110
      commit.rs    Save scope, stage+publish, discard, install  ~122
    control.rs     NEW  serve the five commands                 ~200
    exec.rs        NEW  Bash launch, confinement (S8)           ~230
    install.rs     NEW  write-then-rename the sealed Store       ~40
    lib.rs                                                16     ~22

  layerfs-persistence/src/                             6,706  ~6,365     -341
    store/open.rs                   macOS gates removed  109    ~109
    store/seal.rs  NEW  checkpoint, close, verify one file       ~35
    store/handles.rs                public checkpoint out  46     ~43
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

  layerfs-storage/src/                                 9,336  ~9,356      +20
    port/persistence.rs             Busy variant           97     ~99
    store/error.rs                  Busy variant          140    ~152
    save/{wave,reservation}.rs      one reservation per Save      ~+6

  layerfs-history/src/contract/catalog.rs  one method      71     ~76      +5
  layerfs-workspace/src/            scoped client helpers 3,861 ~3,830     -31
                                    removable after store/
  layerfs-project/src/              unchanged           1,562   1,562       0
  layerfs-content/ layerfs-overlay/ layerfs-telemetry/  unchanged by this plan
```

| Scope | Now | After (est.) | Change |
| --- | ---: | ---: | ---: |
| Data path only: SDK, API core, Bridge, daemon `upstream/` → `store/` | 9,026 | ~1,450 | about −7,580 (−84%) |
| All touched crates, including the control, Exec and install slices that do not exist today | 33,652 | ~26,205 | about −7,450 |
| Active core workspace | 61,711 | ~54,555 | about −7,155 |
| Core scope (active, excluded predecessors and excluded integration) | 104,876 | ~97,430 | about −7,450 |

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

## Performance design

From source reading; nothing here is measured. Each item is checked by the
proofs of its checkpoint and by a later registered measurement.

| # | Design | Why |
| --- | --- | --- |
| 1 | One write handle and a fixed set of read handles per daemon, each behind its own mutex, opened once at startup | WAL readers run while a writer commits. A single connection would stall every base read in a daemon during its own publication |
| 2 | The shared immutable object cache stays above the read handles | An object never changes, so a cached object is valid forever; most FUSE reads never reach SQLite |
| 3 | Pack ids and ordinals are reserved once per Save, in one larger block; unused ordinals are released at finish as today | Every write transaction is a lock acquisition, a WAL commit and a chance of `Busy` |
| 4 | Publication batches keep their current bound (8,191 rows, 4 MiB) | Fewer, larger write transactions |
| 5 | Stage and publish are one history transaction (O-23) | One write transaction fewer per Commit and no leftover stage |
| 6 | The per-daemon inode serial block is larger than today's 1,024 | Creating files almost never touches the shared Store |
| 7 | Init imports under WAL first. If the Init measurement after checkpoint 1 shows a loss, Init builds under a memory journal and converts to WAL at seal | Init is one host process and does not need WAL while importing; WAL writes each page twice. One measurement decides, not a guess |
| 8 | Daemons never checkpoint explicitly; SQLite's passive auto-checkpoint runs. Only seal truncates | TRUNCATE holds the write lock for the whole WAL |

The size of the read set and of the two reservation blocks are startup
settings with defaults chosen at their checkpoints. They are resource
settings, not caps on file, Commit or Workspace size.

Known risk, to measure at checkpoint 7: the WAL can grow while readers
overlap, and a large WAL slows readers.

## Rules that keep a later database swap cheap

A mounted SQLite file is the first provider. These rules keep the daemon
unchanged when another database replaces it.

1. The daemon's `store/` module uses only the Storage and History ports and
   the opened handles. No SQLite type, file path, WAL or lock concept appears
   in it. Seal, install and locking belong to Persistence and the install step.
2. Every Store call has one of two shapes: an idempotent put or get of
   immutable content by id, or one conditional transition with exact expected
   state. No call is an interactive multi-step transaction.
3. `Busy` is a typed outcome of every write. It maps to a failed conditional
   write or throttling elsewhere.
4. Reads are batched by id, as the demand path already is.
5. The publication rule is "bytes are stored before anything references
   them". SQLite meets it with one transaction; a two-store provider meets it
   with two ordered steps.
6. The install manifest carries a Store locator and the provider kind. Today
   the locator is a path on the volume.
7. Nothing is collected or rewritten in place.

Known coupling, not changed now: pack ids, ordinals and inode serials come from
central counters. Block reservation (items 3 and 6 above) is the mitigation and
the reason the blocks grow.

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
- Each Store handle in a daemon is used by one thread at a time under its own
  mutex. That is mutual exclusion inside a process, not a retry of an
  attempted operation.
- WAL size is unbounded while readers overlap; passive checkpoints can starve.

## Checkpoints

Each is one reviewable local commit with focused Disposable proofs, at most
120 s per test invocation, and an exact LOC comparison.

| # | Checkpoint | Proof on Disposable | Needs |
| --- | --- | --- | --- |
| 0 | Commit the finished S9 R2 Content qualifier on its own | Its two test binaries in the Linux image | — |
| 1 | Persistence opens on Linux: gates out, WAL profiles, typed `Busy`, lock-code mapping, `seal`, allocation owner and public checkpoint removed | Linux create/open/publish/read; a second **process** holding the write lock yields `Busy` with no effect and a healthy session; a reader proceeds during that write; sealed file has no sidecar and reopens | 0 |
| 1m | One Init measurement on the changed source | Decides performance item 7; the eight historical Init failures stay as recorded | 1 |
| 2 | Daemon `store/open.rs`, `ports.rs`, `bind.rs` with the write handle and read set; `upstream/` retired | Ported binding, length, serial and demand cases over a real sealed Store in the Linux image; a base read completes while the write handle is held | 1 |
| 3 | Daemon `store/commit.rs`; stage+publish in one transaction; one reservation per Save | Committed, UpToDate, conflict with exact discard, missing dependency, `Busy` leaves no stage and no Workspace change; counted write transactions per Commit | 2 |
| 4 | Retire SDK `client/`, `runtime/`; Bridge `codec/`, `contract/`, `framing.rs`; API core; their tests and examples | Remaining packages build, Clippy clean; the cut is reported as retirement | 2, 3 |
| 5 | Host `init.rs`, `install.rs`, `control.rs`; daemon `control.rs`, `install.rs`; Bridge `control.rs` | macOS Init → seal → install → Linux daemon mounts the installed root | 1, 4, O-18–O-20 |
| 6 | Exec and confinement (S8) | Several concurrent Bash processes on one mount; none can see or open the Store or overlay path, by name or through `/proc` | 5 |
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

## Checkpoint 1 decisions

| Point | Decision |
| --- | --- |
| Existing Disposable Stores | Regenerated. They are refused at open, not converted; no convert tool. Historical receipts are unchanged |
| Allocation and preallocation fields in the checkpoint result and SQL work counters | Removed with the mechanism. The Persistence allocation-release test is deleted; four Persistence tests, five Project examples, one Project test and the harness readers of those fields are updated or marked `NOT_RUN — mechanism removed` |
| Public checkpoint call | Removed. Seal is the only caller of TRUNCATE |
| Seal ownership | Seal consumes the handles and refuses while anything else holds the session |
| `Busy` proof | A real second process holds the write lock |
| SDK during the transition | Two temporary match arms for the new variant keep the workspace building until checkpoint 4 retires the SDK runtime |
| SQLite versions | Host and daemon versions are both recorded in the install manifest; the schema identity is checked at open |

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

## Owner rulings

All seven were ruled on 2026-10-07 and are recorded in
[08 §7](../303/08-decisions-provenance.md#7-questions-only-the-owner-can-answer).

| # | Ruling |
| --- | --- |
| O-21 | Disposable for the shared Store is WAL with `synchronous=OFF` |
| O-22 | The macOS allocation owner is deleted |
| O-23 | Stage and publish are one history transaction |
| O-18 | Host fork and history reads are daemon-served control verbs |
| O-19 | A second import into an installed volume is refused |
| O-20 | Control stays on the authenticated native channel |
| O-24 | One unprivileged Bash user per daemon; a Workspace is one mount serving many concurrent Bash processes |

## Working-tree state at this plan

The S9 R2 Content qualifier is implemented and uncommitted: module
`layerfs-content/src/filesystem/qualify/`, a sequential inode reader, 10 Content
tests and one daemon test over real overlay scratch. All pass on the host with
Clippy, formatting and the boundary guard clean. Its Linux run was interrupted
before a build completed and is **NOT_RUN**. Receipts, including the retained
failures, are under
[`checks/s9-root-qualification-20261007/`](checks/s9-root-qualification-20261007/).
The SDK proof-required bind that was to follow is dropped with the SDK runtime.

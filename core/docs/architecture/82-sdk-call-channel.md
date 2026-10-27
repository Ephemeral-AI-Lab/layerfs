# SDK call channel and operator boundary

> **Status:** Implemented #327/#328 source description, 2026-10-15, with the
> #352 and #373 source updates below. Correctness receipts and the component
> counter screen are distinct from this architecture.

The JavaScript SDK has 27 positional methods in four groups (29 before #373
replaced the `sandbox` group with `workspace`; see the #373 update below). The
[SDK guide](../../../api/docs/agent.md) lists signatures, environment configuration,
read/page limits and compatibility. [SPEC](../issues/327/SPEC.md) describes the
UI/server integration; [VERIFICATION](../issues/327/VERIFICATION.md) owns acceptance.
No harness, agent/call metadata, remove alias or public inspect is retained.

JS wrappers form SdkValue maps internally. Workspace Mount/Attach/Commit/Status/
Unmount retain typed control records. Other filesystem/history methods use
`Request::Sdk(SdkCall { method, args })`, answered once as `Reply::Sdk` or
`SdkRefused { code, message, detail }`. The stable refusal code decides behavior.
SdkValue supports null, bool, integer, text, bytes, list and unique-key map;
nesting is at most 16 levels. One authenticated record fits `u16::MAX - 16` bytes.
Methods therefore use bounded windows rather than whole-history/tree replies.

Issue #340 admission correction: an SDK call that cannot encode within this
record is refused locally as `TooLarge`, with null detail, only when encoding
stopped before any send, without a received answer or failed fence. Other
encoding, transport and uncertain failures retain their original custody.
No request is split or replayed. The complete encoded call includes paths,
metadata and observation fields as well as content; there is no fixed content
byte allowance. Larger atomic changes use an ordinary Workspace step.
This is control-call admission, not a file, namespace or Workspace size limit.

The root public table refuses `operator.*`. `@layerfs/sdk/operator` uses a
separate native entrypoint and Rust API/daemon admission table. Store/Commit
statistics, file statistics/history, Workspace overlay/changes and snapshot are
internal diagnostics. Process start/list/cancel stays in the native Sandbox
executor; no command process record or supervision belongs to the filesystem
daemon. HTTP exists only in the demo server, which maps GET reads and positional
POST `{args:[...]}` writes to the two tables. No server filesystem data path.

Project is the named stack; main is its line of Layers. Branches have unique
names and named IDs; immutable Commit/Layer IDs use lowercase tagged hex. Public
pageSize/pagePos defaults are 50/0 with maximum size 1,000 and safe integers.
Names sort by bytes; recursive paths use component-by-component byte order;
Commit history is first-parent newest-first. Numeric deep positions skip bounded
windows linearly. Byte bounds may shorten a page and `hasMore` discloses more.
A live ref resolves once per call, so independent calls can observe new heads.

Issue #342 source update, 2026-10-16: the recursive immutable reader used by
Branch/Commit diffs, grep and glob acquires at most 64 directory bindings and
8,192 encoded binding bytes per frame, then resolves that window's inode values
in one grouped Content demand. A depth-first frame retains only that window and
its name continuation; exhausted frames require no final empty listing. This
avoids one directory and inode-table descent per name. Equal filesystem roots
return an empty diff after validating pagination. Equal directory binding roots
cannot skip descendants: their inode values live in the filesystem's separate
inode table. Numeric diff page positions still scan earlier paths; no cursor
state or namespace-sized changed-path map is retained between calls.

Issue #354 source update, 2026-10-18 (#352 F-035): `fs.read` and `fs.grep`
scan a file's text in whole parts (one read pass per chunked file, a
whole-file payload in place) with the standard library's substring search and
a buffer-wide line counter; the NUL / invalid UTF-8 window boundary is
unchanged. `fs.grep` scans each content id once per call (results kept within
the reply budget) and reads a window of up to 64 files' roots in demands of
at most 4 MiB, sized from their file lengths, so chain levels are resolved
together. `fs.glob` starts at an anchored pattern's literal leading folders
and skips folders it cannot match. Every read call keeps one set of storage
read caches (then `sdk/cached.rs`, now store-service `read/cached.rs`) and resolves its path through a memo over them;
the walker memoizes directory pages and the inode table's upper pages. Results
are byte-identical to the earlier calls; paging still counts positions.

Diff omits a same-identity directory only when its metadata is also unchanged.
Directory metadata changes occupy a returned item and therefore exactly one
pagination position. This corrects the earlier mismatch where the scan counted
a directory that the reply omitted, causing a later page to repeat a file.
The [#342 investigation](../issues/342/README.md) records the original demand
counts, bounded regression proofs and separate diagnostic observations.

Direct Branch mutations bind a private indexed Workspace, drive ordinary
Workspace operations and publish one captured Content construction through the
shared Save and History ports. Apply uses ordered `{op,...}` changes, one
publication, and detail.index on refusal. Recursive operations use bounded name
windows and a backed copied-hardlink map. Large edits stream chunks. Definite
validation failure closes the private overlay; uncertain construction/publication
retains the original failure and custody. There is no resident paths draft or
separate canonical encoder. Main is read-only except through publish.

Immutable reads use 1-based inclusive lines, at most 2,000 lines/48 KiB reply
payload. Huge-line UTF-8 prefixes, omitted remainder and nextLine are explicit;
there is no byte-read method. Grep streams literal matching. Blame follows
immutable first parents without retaining the entire chain, skips unchanged
content IDs and uses bounded LCS with exact common edges. In coarse comparisons,
a line is inherited from the common suffix only when it starts at a
corresponding line boundary in both versions, and from the common prefix only
when it ends at one in both. Oversized changed middles are
removed/added blocks with comparisonCoarse. Diff hunks may be omitted
explicitly under comparison/reply bounds; equality uses immutable identity.

Commit-v3 includes the message and operation/origin/displacement metadata in
identity, excluding time/counters/LWW details. History persists fork origins,
second parents, merge origins, displacement, LWW facts and overwritten Layers.
The UI consumes those facts. Store accounting is maintained in original atomic
publication/insertion transactions and read from one fixed row plus page PRAGMAs;
per-file physical attribution stays null. Definitions and the retained v5
speed/storage screen are in the linked SDK guide and issue checkpoint.

Private LAYERFS_HOME ownership/mode authorizes persistent project/Sandbox records.
A fresh process validates full saved deployment/container/Noise/daemon identity
and can use a Workspace (`{ sandbox, path }`) without Init, reinstall or attach replay.
Nonblocking OS locks and persisted create intent prevent guessing after an
uncertain create. Restart recovery and failed-operation replay are excluded.

`operator.inspect.snapshot` reads history with `operator.inspect.history`, the
same bounded answer with no Workspaces, from the Store in the calling process,
and joins the Workspaces of every running Sandbox of the project, including
each mounted Workspace's real overlay data. It finds them with
`operator.sandbox.list`, which reads the project's records and inspects each
container once, so a snapshot starts no Sandbox; a listed Sandbox refused as
`NoSuchSandbox` is listed in `gone` with its detail. The bounded answer discloses
moreBranches/more/moreLayers. Its version includes local publication revision as
well as history/binding facts; JS combines the history and daemon versions. The demo
may poll; it has no event-stream or reconstructed marker authority. Individual
operator Workspace reads are observations and are not an atomic global snapshot.

Typed Status remains a local observation with no Store SQL. The binding separately
reads one newest Commit to add head/stale information. Unmount without discard
refuses Dirty and preserves ownership; discard closes the Workspace explicitly.
The Dirty detail is bounded. Busy still represents held runtime/kernel/operation
ownership. Reaping and requests on replaced inodes retain the owner's deferral.

Disposable/WAL/OFF only; Durable is NOT_RUN pending explicit owner reauthorization.
New Store schema/identity records reject old Disposable Stores without migration.

Issue #352 source update, 2026-10-19 (api shape recovery). Since F-020 the
`fs`, `branch` and `project.list_branches` methods are not daemon calls: they
run in the SDK's own process against the Store it opened (Linux only,
`Unsupported` elsewhere; [direct Store access](../issues/352/DIRECT_STORE.md)).
The `Request::Sdk` record above now carries only `operator.*` calls to a
daemon. The source is split in two crates:

- `layerfs-api` is the public surface: one file per JavaScript method, under
  `project/`, `fs/`, `branch/` and `sandbox/` (#328 standard). Each Store
  method is `fn(&StoreService, &SdkValue) -> Result<SdkValue, SdkRefusal>`:
  it reads its arguments (`base/args.rs`) and runs the engine.
  `layerfs_api::call` (`base/dispatch.rs`) is the method table the native
  binding calls; it refuses unknown and `operator.*` names `UnknownMethod`.
- `layerfs-store-service` is the engine, organised by mechanism and free of
  method names and argument maps: `read/` (file, listing, search, compare,
  blame, windows and call caches), `write/` (a typed change list applied to
  one private Workspace and committed once), `history/` (fork, merge,
  publish, listings, lookups and refusals). The operator tables keep
  name-keyed arguments; their readers are `operator/args.rs`, which
  layerfs-api re-exports. layerfs-api depends on layerfs-store-service, never
  the reverse.

A method reads each argument just before the engine step that first uses it,
and an argument used after a lookup reaches the engine as `Arg<T>` (read but
not yet refused), so a call with several faults is refused by the first, as
before. Answers stay `SdkValue` maps built by the engine. Streamed write
bodies are not read on this route; a daemon connection drops a call's declared
bytes before answering (`layerfs-daemon` `application/bodies.rs`).

Layout at `04f2576961` (`tools/production_loc.py --files`; `src/` only; tests
live in each crate's `tests/`):

```
layerfs-api/src/                  [2257]
  lib.rs                             25
  base/                            [958]
    args.rs                         121
    control.rs                      289
    dispatch.rs                      45
    failure.rs                       61
    install.rs                      147
    install_types.rs                 44
    internal.rs                     130
    mod.rs                           10
    recovery.rs                      52
    sdk.rs                           59
  branch/                           [97]
    diff_branches.rs                 21
    diff_commits.rs                  13
    fork.rs                           8
    list_commits.rs                  12
    merge.rs                         19
    mod.rs                           12
    publish.rs                       12
  fs/                              [152]
    apply.rs                          9
    blame.rs                         14
    cp.rs                             6
    diff_files.rs                    13
    edit.rs                           6
    glob.rs                          10
    grep.rs                          13
    list.rs                           8
    mkdir.rs                          6
    mod.rs                           28
    mv.rs                             6
    read.rs                          14
    rm.rs                             6
    stat.rs                           7
    write.rs                          6
  operator/                         [25]  (unchanged; 4 files not listed)
  project/                         [206]
    init.rs                         187
    list_branches.rs                 13
    mod.rs                            6
  sandbox/                         [794]  (unchanged; 9 files not listed)
```

```
layerfs-store-service/src/        [3706 shown]  (partial: the engine)
  history/                         [719]
    branch.rs                       577
    lookup.rs                       139
    mod.rs                            3
  operator/                        [340]  (partial: inspect.rs not listed)
    args.rs                          44
    files.rs                        174
    mod.rs                           21
    store.rs                        101
  read/                           [1840]
    blame.rs                        251
    cached.rs                        94
    compare.rs                      391
    mod.rs                            9
    plans.rs                         26
    reads.rs                        295
    search.rs                       143
    snapshot.rs                     165
    walk.rs                         183
    window.rs                       283
  write/                           [807]
    edit.rs                         111
    files.rs                        193
    mod.rs                            4
    mutation.rs                     224
    recursive.rs                    275
```

## Issue #373 source update (2026-10-21)

The public `sandbox` group (`create`, `mount`, `exec`, `status`, `commit`,
`unmount`, `close`) is replaced by `workspace`: `mount(branch)`,
`exec(ws, command, message?)`, `status(ws)`, `commit(ws, message?, force?)`,
`unmount(ws, discard?)`. A Workspace is `{ sandbox, path }`. The binding
places each mount in the project's numbered Sandboxes under the project's
placement lock (the lowest-numbered running one of the mount's image,
`deploy.image` or the mount's `{ image }`, with fewer than 16 Workspaces, else
the next number on that image; one project's Sandboxes may run different
images, and the mount result and `operator.sandbox.list` give each `image`), and a Sandbox's daemon ends itself after
two idle minutes; the next mount or `operator.sandbox.list` reclaims it.
There is no service Sandbox and no `operator.sandbox.service`; Import starts
no Sandbox. Operator `process.start(ws, command)`, project-wide
`process.list()` and `process.cancel(id)` replace the per-Sandbox forms, and
a process id names its Sandbox. The native ops are `workspace.*`,
`operator.sandbox.list` and the operator process ops; the daemon's typed
control records and its timing names (`sandbox.mount.bind`,
`sandbox.mount.attach`) are unchanged. Design and owner decisions:
[#373](../issues/373/README.md); placement and idle close are described in the
[SDK guide](../../../api/docs/agent.md#sandboxes-and-workspaces).


## Issue #428 source update (2026-10-23): backend names and packages

The `operator.*` names are now `backend.store.*` (answered by the Store in
the calling process) and `backend.daemon.*` (the Sandbox must be running).
The native addon's second entry point is `backendCall`; the daemon admits
`backend.*` and `sandbox.list_workspaces` (was `operator.inspect.snapshot`)
through `operator::admits`, and public dispatch still refuses them
`UnknownMethod`. Runtime-only native calls are `runtime.project.get_init`
(was `operator.project.progress`) and `runtime.sandbox.reclaim`, which took
over the reclaim `operator.sandbox.list` did; `project.list_sandboxes` now
only lists. `@layerfs/agent` exports `RENAMED_METHODS` for reading older
traces. The Rust module paths (`operator/`) keep their names.

The JavaScript is split into `api/agent` (`LayerFS`),
`api/backend` (`LayerFSBackend.of(shop)`, one file per method)
and `api/runtime` (`LayerFSRuntime.open`, on a LayerFS home).
`@layerfs/sdk/operator` and its `inspect.snapshot` are gone; the snapshot is
the runtime's `project.get_state`. On macOS and Windows the runtime's
`worker/server.js` makes the Store calls with the Linux addon and
`@layerfs/agent`'s `transport/worker_client.js` reaches it. The route was
first allowed for the dsh adapter only; since 2026-10-24 (owner) it is open to
any integration that names the endpoint (`LAYERFS_WORKER_ENDPOINT`, or the
runtime's `useWorker(endpoint)`), and the SDK reads only `LAYERFS_*` names.
New calls: `sandbox.create`, `sandbox.close` and
mounting into a chosen Sandbox (agent), and
`backend.daemon.sandbox.get_log`, which reads a Sandbox's daemon log once
through the Engine where the binding runs. See the
[backend guide](../../../api/docs/backend.md#a-sandboxs-log).

## Project Store composition (#463)

Store Init, sealing and bounded authenticated installation live in
[`layerfs-project::store`](../../../engine/crates/layerfs-project/src/store/mod.rs).
The existing namespace constructor remains `layerfs_project::init_observed`;
[`store/init.rs`](../../../engine/crates/layerfs-project/src/store/init.rs) composes it
with Persistence creation, the first Branch, Init statistics and sole-owner
seal. It requires Disposable explicitly and preserves original failure custody.
Project depends on Bridge for the existing provisioning records and channel
install protocol; it defines no second format or protocol.

[`layerfs-api::project`](../../../engine/crates/layerfs-api/src/project/mod.rs) and its
root retain the existing public names as re-exports. Its Control owner calls
Project's `install_file` without changing correlation or quarantine. The
[Node binding](../../../engine/crates/layerfs-node/src/project.rs) calls Project's Init once
directly. This is ownership relocation only; no canonical identity, Store
format, behavior or performance claim changes.

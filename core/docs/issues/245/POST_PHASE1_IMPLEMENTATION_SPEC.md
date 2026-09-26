# #245 post-Phase-1 implementation contract

> **Status:** Current planning checklist; no release candidate exists.
>
> Source pin: `f74dbe77da12fa533587be8a578375bce3f19373` (2026-09-26).
> This coordinates open issues and the [mini benchmark](SHELL_BRAINSTORM_MINI_V1.md).
> It is not a claim that the target implementation or performance has shipped.

## One product route, several issue owners

The public route is always `WorkspaceApi::mount` → one opaque
`WorkspaceApi::exec(command)` → `/bin/sh -c` → ordinary mounted POSIX/FUSE
operations → optional explicit `WorkspaceApi::commit`. Product mutation follows
the FUSE callbacks. No benchmark or product logic parses a command to recognize
`dd`, `cp`, `mv`, `grep`, package names, or benchmark IDs; no private Workspace,
Bridge, service, or C1 mutation API is called by the benchmark driver. A
read-only or printed-output command deliberately has no Commit. A shell command
is not an atomic Workspace transaction: its syscalls can cross a Commit capture.

| Issue | Owned implementation and public proof | Dependency / closure boundary |
| --- | --- | --- |
| [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245) | Parent integration, exact old/new heads, real mounted shell cases and one combined file-plus-namespace generation. | Phase 1 A–F and #252 are prerequisites, not repeated work. Parent closes after its required children and registered public proofs. |
| [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) | Per-file final-run scale, private payload/page maintenance, monotone Commit cursors, bounded C1 construction and writer progress. | A public 4,097 separated-write Exec/Commit gate; exact old/new bytes and canonical roots. |
| [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) | Path-local keyed namespace updates, charged live caches, replayable prepared rows, C1 ordering and the 4,096 visited-binding cap. | Public 129, 257 and 1,025 changed-file/name gates. Shares page ownership with #248, implemented once. |
| [#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258) | Stable canonical origin for base-resident directory moves, descendant lookup and atomic path validation. | Distinct semantic gate; #256's count work does not make inherited descendants movable. |
| [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249) | Daemon registry for several mounted Workspaces; independent overlapping Exec calls; shared lightweight, count-free command supervision and no whole-Exec runtime timer. | One Commit/Stage submission at a time **per Workspace**. Different Workspaces may commit concurrently subject to Store admission and head conflicts. |
| [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219) | Operator `max_workspaces_per_sandbox` setting at sandbox creation, enforced by daemon-internal Workspace creation. | Sibling of #249. It counts live/attaching/retained/closing Workspaces, never Exec calls or generations. |

Sequence: keep the already sealed public mini contract as a later verification
input; implement shared backing and #248 and #256 tree work in coordinated
slices; prove #258's move semantics; complete
#249's multi-Workspace/Exec transport; add #219's operator policy over the same
daemon registry. This is a dependency order for proofs, not a global source lock.
The mini runner belongs to the benchmark/test lane after the relevant product
work, outside production LOC. The full issue acceptance cases retain their
larger registered counts and separate performance budgets.

## Target architecture

```text
host SDK:  mount ── exec(opaque command) ── optional Commit ── Status
                         │                         │
sandbox daemon:  Workspace-ID registry       one submission slot / Workspace
                 shared Exec supervisor      (no daemon-wide Commit slot)
                         │                         │ freeze G1
Linux FUSE:       normal POSIX callbacks      G2 remains writable
                         │                         │
private backing:  Local aligned payloads      frozen Base/Local/Zero cursors
                  length-indexed extent tree  ordered keyed namespace cursors
                  keyed namespace tree            │
                  shared page/ledger custody ─────┤
                                                 ▼
host service:             bounded SaveFile + prepared-namespace spools
                         C1 canonical roots → C2 CDC/CAS/encoding → Store
                         one expected-head publication; reconcile G2 to C1
```

One accepted file or namespace mutation publishes one coherent private root.
It does not create a per-file checkpoint of every earlier edit. A Commit pins
G1, derives its **final** file runs and namespace rows, and publishes one
canonical head. After a known success, G2 reanchors to that head; its next
Commit compares against the immediately preceding saved version. Unknown
outcomes retain custody. Private `Base` extents share canonical data; `Zero`
extents store no payload. Private `Local` payloads are aligned write segments,
not CDC/CAS objects. During Commit, C1 can retain unchanged canonical
chunks/subtrees and C2 can reuse CAS objects or select compressed/PREFIX
records. Those Store savings do not lower pre-Commit private quota.

## Cost model and constraints

Let `P` be final file extents, `Hf` and `Hn` private file/keyed tree heights,
`N` retained Local acquisitions, `R` final changed runs, `S` final replacement
bytes, `F` changed inodes, `K` changed names, `L` distinct index leaves reached
by a cursor, `M` canonical nodes visited/emitted and `U` canonical base bytes
actually demanded. The following are algorithm targets to prove with counters,
not measured bounds:

| Work | Current source | Target |
| --- | --- | --- |
| One narrow WRITE | Path copying plus `O(N)` routine payload scan and linear payload-ID lookup; physical page/ledger operations are separately counted. | `O(write bytes + Hf)` touched file pages, indexed Local ownership and work-triggered reclaim; no scan of previous acquisitions. |
| `N` tiny writes | Routine retained-record scans can total `O(N²)` even though payload bytes are small. | `O(Σ submitted bytes + Σ touched tree paths + charged physical I/O)`; no history-sized maintenance term. |
| One directory delete/rebind | Materialize/rebuild its E/T names, `O(K)` per update and potentially `O(K²)` across updates. | `O(Hn)` point mutation plus bounded sibling work; an ordered cursor traverses each reached leaf once. |
| Frozen file/namespace lowering | Reopened cursors can reread ancestors; arrays grow with `R`, `F` and `K`. | Fixed-number ordered passes, `O(P+F+K+R+S)` logical work, `O(H+L)` page visits per cursor pass and fixed buffers plus charged active pins. |
| Canonical file/namespace build | C1 repeats file split/join per run and materializes several namespace collections. | Reuse existing exact-root route if measured counts fit; otherwise bounded replay/frontier work following `R+M+S+U`, with canonical identity proof or an explicit format ruling. Namespace ordering uses charged spool/merge, not an unbounded resident vector. |

Lower bounds remain `Ω(R+S+F+K)` for the final input; a fresh incompressible
file also requires `Ω(file bytes)` write, scan and Store space. Live private
disk is aligned reachable Local payloads plus unique retained 4 KiB pages and
ownership ledgers, including pinned G1/G2 generations. Commit spool and Store
pack/index/WAL are separate disk domains; CAS/CDC/delta savings depend on
actual content and cannot be assigned a universal file-size ratio. The
[joint tree study](JOINT_248_256_TREE_RESEARCH.md) gives formulas and concrete
page/file-count illustrations.

Remove fixed *aggregate* refusals for 128 names/dirty inodes, 4,096 visited
bindings, 32 KiB prepared metadata, 65,536 page
slots, fixed temporary-page/ledger tables, 256 live nodes, 128 handles, 1,024
cookies, 32 roots and 11 arenas where resources remain available. Fixed page
fanout, transport frames, 128 KiB FUSE callbacks and bounded resident windows
remain batch sizes with continuation. #252 removed both the 4,096 retained
payload admission and the explicit 4,096-final-run refusal; #248 still needs
indexed payload lookup, nonquadratic cleanup and proof that no downstream
replacement cap or unacceptable work remains. #249 removes the whole-Exec 30 s product
timer and fixed concurrent/lifetime Exec counts; its command lease still owns
real PIDs, FDs, output buffers and cleanup. #219 selects the live Workspace
count separately. The current 4 GiB logical file format, path/name grammar,
finite reference widths, per-callback/Commit deadlines and configured memory,
disk/PID/FD budgets remain explicit representability or resource constraints.
See the [constraint inventory](RESOURCE_CONSTRAINT_LIFT_PLAN.md) for each
boundary and its lift path.

## Source ownership and provisional LOC

```text
core/crates/layerfs-workspace/src/
  backing/page_store/          arena, root, reservation, ledger, reclaim
  backing/payload/             existing aligned I/O, indexed IDs, reclaim
  backing/binary_plus_tree/
    extent/                    length-indexed splice, balance, cursor
    keyed/                     key insert/delete, cursor, ordered builder
  commit/                      frozen SaveFile and namespace streams
  runtime/                     charged pins, handles, cookies, ownership
core/crates/layerfs-bridge/src/  versioned prepared stream and Exec lease protocol
core/crates/layerfs-server/src/  bounded namespace receive spool and C1 handoff
core/crates/layerfs-content/src/ canonical file/namespace replay and validation
core/crates/layerfs-daemon/src/  Workspace registry and shared Exec supervisor
core/crates/layerfs-sandbox/src/ selected max_workspaces_per_sandbox policy
```

| Slice | Provisional **net production** LOC | Main existing/new files |
| --- | ---: | --- |
| Shared page/payload ownership, counted once | +500–700 | `backing/metadata.rs`, `ownership.rs`, `payload.rs`; extracted `page_store/`, `payload/index.rs`. |
| #248 file paths | −100 to +700, plus conditional +150–350 C1 | Existing `metadata_pieces.rs`, `metadata_cursor.rs`, `commit/{lower,upload,source}.rs`; `binary_plus_tree/extent/`. |
| #256 namespace and streaming | No reliable range yet; the detailed line-item sketch sums to about +2,990 | Existing `metadata_index.rs`, `overlay/directories.rs`, `runtime/state.rs`, Commit/Bridge/server/C1; add keyed or stream modules only where current code cannot be reused. |
| #258 inherited move | +250–600 | Existing `filesystem/{rename,namespace_view}.rs`; focused move/origin module as needed. |
| #249 daemon/Exec | +600–1,500 | Existing daemon control/execution/lifecycle and SDK/Bridge route; shared supervisor module. |
| #219 operator setting | +100–250 | Sandbox config/launcher and daemon admission. |

These are planning ranges, **not** commit LOC counts; file moves are counted
once, tests/docs/benchmark code excluded. The #248/#256 detailed file-by-file
physical-line and LOC forecast is in [§6 of the joint study](JOINT_248_256_TREE_RESEARCH.md#6-implementation-slices-and-fileloc-forecast).
Each production file stays below 1,000 physical lines; `lib.rs` and `mod.rs`
stay below 200. Every actual commit records exact first-parent before/after
production LOC, including migration subtotals.

## Product implementation slices and stop conditions

| Order | Slice | Finish before the next dependent claim |
| ---: | --- | --- |
| 1 | Implement **one shared** page/payload ownership substrate for #248 and #256: charged page/ledger capacity, payload-ID lookup, work-triggered reclaim, progressive reserves, grouped ledger I/O only where custody stays exact. | Narrow writes and many tiny writes preserve atomic local roots and pinned G1/G2; count traces show no scan of earlier acquisitions per write. Quota/unknown failures retain their owners. |
| 2 | Complete #248's file path: balanced extent splice at height transitions, persistent frozen cursor across lower/upload pulls, short writer-gate holds and bounded C1 replay. Measure C1 node visits before replacing its current exact-root split/join algorithm. | Public 4,097 separated writes and Commit preserve exact bytes/root/head; no historical-write scan or count refusal; G2 can mutate during every declared Commit phase. A new C1 builder requires an explicit canonical-identity proof or ruling. |
| 3 | Complete #256's keyed namespace path and prepared stream: point delete/rebind, ordered cursors, charged live pins, wide counts, validated replay spool and C1 bounded ordering. | Public 129, 257 and 1,025 changed-name/file Commit cases pass full-tree and old-head oracles. A single generation publishes one head; read, readdir and cleanup remain bounded by resources. |
| 4 | Complete #258's inherited-directory move on the same keyed namespace substrate. | A base-resident directory with descendants moves without a full subtree copy; old paths disappear, new inherited paths resolve, open handles survive and invalid deep paths fail before publication. |
| 5 | Complete #249's per-sandbox Workspace registry and shared event-driven Exec leases; add #219's selected positive Workspace-count policy over that registry. | Multiple mounts and overlapping Exec on one or several Workspaces work with no fixed Exec count or whole-command timer; one Commit per Workspace, independent Store admission across Workspaces, exact lease cleanup and count=1/2/3 policy. |
| 6 | Integrate the implemented product paths under #245, including combined file-plus-namespace generations and immediate-predecessor reconciliation. | Focused correctness gates preserve G2 writes, old-head readability and exact custody; public load-bearing qualification is in the separate verification lane below. |

Code verification is per changed Core component with locked Cargo tests,
clippy, fmt and the product-boundary guard. Public performance rows are taken
once only after the relevant source, workload, cache contract and budget are
frozen. A cache-ineligible mini timing can motivate a count-driven diagnostic;
it cannot authorize a performance PASS or a shorter workload.

The [13-cell mini runner](SHELL_BRAINSTORM_MINI_V1.md) is a separate
**verification/benchmark artifact**, implemented after the relevant product
paths. It is excluded from production source and LOC. Run its public shell
cases and the full-size issue gates as verification work, with sealed release
binaries, independent oracles, append-only receipts and the declared cache
status. A failed case returns to its owning product slice for a focused fix;
the runner itself is not a prerequisite product phase.

## Proof gates before claiming this architecture

Public SDK/FUSE acceptance must include #248's 4,097 separated final runs;
#256's 129/257/1,025 changed-name/file cases; #258's base-directory move;
#249's overlapping Exec calls in one and several Workspaces; #219's configured
Workspace counts 1/2/3; two incremental Commits with a G2 write accepted during
G1 construction; old-head exact bytes; and precise cleanup/resource refusals.
Record actual callback and page/ledger counts, private and Store bytes, spool,
RSS/page cache, Exec/Commit/cleanup wall, and every nonpassing row. The
[proposed mini benchmark](SHELL_BRAINSTORM_MINI_V1.md) is a verification-only API smoke and cost
diagnostic for all ten brainstorm categories, not a substitute for these
full-size gates or a cold-cache performance admission.

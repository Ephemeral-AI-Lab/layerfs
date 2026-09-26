# #245 architecture and complexity research

> **Status:** Research; informative and not a product contract.
> Source inspected: `74ac30e28bcfaef180f405c9bc9045970f1678eb` on
> `codex/issue245-range-cow-plan` (2026-09-26). The final E/F functional and
> comparative evidence is at product source `b2cd0df23`; see the
> [E/F report](evidence/phase1-e-f-final/REPORT.md). Complexity rows marked
> **target** describe [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248)
> or [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249), not code
> already delivered or a measured speedup.

This is the review map for the [private backing and page-tree study](PRIVATE_BACKING_AND_PAGE_TREE.md)
and the [final-delta Commit study](FINAL_DELTA_COMMIT_COMPLEXITY.md). It separates
four different things that are easy to conflate: the mounted file's private
range tree, the private payload store, C1's canonical file tree, and the daemon
that schedules commands. The current [#245 architecture](ARCHITECTURE.md) is the
design origin; the tables here make its algorithmic obligations falsifiable.

## 1. Layers and ownership

```text
                    CURRENT at 74ac30e28
  SDK exec(command) -- one daemon control session, one selected mount
          |                           (a shell can spawn many processes)
          v
  Linux FUSE syscalls --> Workspace active generation G
                         |  atomic local root publication for accepted calls
                         +--> 4 KiB private metadata pages
                         |       length-indexed Base / Local / Zero sequence
                         +--> private Local payload segments
                                  (base bytes remain in immutable Store)

  SDK Commit --> freeze G --> enumerate final extents --> EditFile/ConstructFile
                       |            descriptor/body stream, 4,096-run ceiling
                       v
                Bridge --> service --> C1 canonical content tree
                                       --> one Branch-head publication
                       |
                       +--> live G+1 accepts later mounted writes;
                            reconcile onto the saved head after known success
```

```text
                  TARGET after #248 and #249
  sandbox daemon: bounded session + Workspace-ID/incarnation registry
      |                       |                      |
      W1 mount                 W2 mount               shared Store budget
      |  exec 1, exec 2        | exec 3               + Branch-head checks
      |  concurrent syscalls  |                     |
      |  G[n+1] live          |                     |
      +-- capture frozen G[n+1]                     |
      |      final-state cursor, not WRITE history  |
      |      count/validate --> framed descriptors  |
      |      --> bounded replay --> one C1 result ---+
      |  G[n+2] accepts writes while Commit runs
      +-- one Commit/Stage slot per Workspace
```

The *private page tree* is a mutable Workspace view implemented with immutable
path-copied pages. The *canonical content tree* is the result that C1 builds in
the Store. One byte write requires a local publication so a following read sees
it. It does **not** require a canonical file checkpoint or Branch commit for
that byte. The current file tree already represents the final live state; #248
must derive the final delta from a frozen version and carry it through the
entire Commit path without a fixed run-count allocation. C1 already emits one
final file root after applying its accepted edit stream; a replacement builder
must also preserve the existing canonical root and mapping partition for those
inputs, or obtain an explicit identity-version decision. The
[Commit study](FINAL_DELTA_COMMIT_COMPLEXITY.md#canonical-root-compatibility-is-a-design-gate)
details that constraint.

### What is already established

| Statement | Status at source pin | Evidence / scope |
| --- | --- | --- |
| Length-indexed Base/Local/Zero private pages replace the old absolute-offset, whole-vector index. | Implemented; high-tree splice correctness and ideal path-local cost still need proof. | [Extent-sequence source description](../../architecture/proposal/fuse-workspace-snapshot-overlay/59-length-indexed-extent-sequence.md), [`metadata_pieces::replace`](../../../crates/layerfs-workspace/src/backing/metadata_pieces.rs) |
| D streams descriptors and replacement bytes instead of the earlier 256-edit/8 MiB request. | Implemented; 4,096 final-run and other resource ceilings remain. | [`MAX_EDITS_PER_OPERATION`](../../../crates/layerfs-bridge/src/contract/request.rs), [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) |
| E captures a generation and reconciles an accepted mounted write during an actual successor-builder page read. | Functional PASS at `b2cd0df23`, including a second sequential Commit. | [E/F final report](evidence/phase1-e-f-final/REPORT.md) |
| F's frozen four-case comparison meets its registered complete-command wall envelope. | Comparative PASS at `b2cd0df23`; **all latency cells INELIGIBLE** because their cache state was uncontrolled. | [E/F final report](evidence/phase1-e-f-final/REPORT.md) |
| More than 4,096 final runs use bounded resident memory and exact canonical identity. | Target of #248; no qualifying proof here. | [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) |
| Two independent SDK Exec calls overlap on one Workspace; one daemon hosts multiple simultaneous mounts. | Target of #249; current control/session/lifecycle route serializes them. | [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249), [`Lifecycle`](../../../crates/layerfs-daemon/src/lifecycle.rs) |

## 2. Cost notation and lower bounds

All complexity claims use **one logical file version** and one explicitly named
operation. A constant-sized page or transport frame is a constant in the
algorithm, though its real bytes, page-cache residency, and I/O still count.

| Symbol | Meaning |
| --- | --- |
| `P` | Number of final Base/Local/Zero extents in the relevant private file version. |
| `H` | Its private extent-tree height; bounded by the on-disk format, roughly logarithmic in `P` when valid and occupied. |
| `K` | Extents intersected or split by one local mutation, including boundary extents. |
| `W` | Input bytes accepted by one local WRITE; `S` is the final Local/Zero replacement bytes sent for one Commit. |
| `R` | Maximal final changed runs against that Commit's immediate canonical base. `R` is independent of the number of prior WRITE calls. |
| `M` | Canonical mapping nodes that actually must be read or constructed for the final result. |
| `D` | Dirty file/namespace identities in the captured generation, distinct from total Workspace entries `N`. |
| `A` | Simultaneously retained private generations or roots; `U` is unique reachable private pages and payloads across them. |
| `Q` | Simultaneously admitted SDK commands, and `V` is mounted Workspaces in one daemon. |

Any correct Commit with `R` distinct final runs must at least validate/emit
`Ω(R)` descriptors, and must consume `Ω(S)` replacement bytes. A `P`-extent
private version costs `Ω(P)` storage for its description. An actual insertion
that changes or creates `K` extents costs `Ω(K)` work. “Limit-free” therefore
means **no arbitrary 4,096-run refusal and no resident allocation proportional
to `R`**; it does not mean unbounded file size, zero work, or unlimited disk.

## 3. Comparison of time, space, and proof obligations

These are per-operation costs, not elapsed-time promises. `B` denotes a fixed
page/frame/window byte bound, and `C` denotes fixed content-builder frontier
state. “Target” requires both an implementation and a count-driven check.

| Path | Before #245 | At source pin | #248/#249 target and check |
| --- | --- | --- | --- |
| One small WRITE into a fragmented file | Whole `P`-piece vector load/splice and fixed two-level index rebuild: at least `Θ(P)` piece work and index rewrite. | Length-indexed path copy shares untouched subtrees, but replacement extents are materialized and branch processing needs a high-tree audit. | Seek plus affected range: `O(H + K + W)` logical work and path-local new pages, with no work proportional to an untouched suffix; count visited/read/written pages as `P` grows. |
| Private file storage | Absolute-offset pages plus local payloads; old fixed 1,024-piece admission. | 4 KiB length-indexed pages, shared across roots, plus charged Local payloads; 4 GiB logical file cap and remaining count budgets. | `Θ(P)` extent description plus actual unique reachable payload and path-copy pages `U`; bound **retained** generations and charge both transient and pinned roots. |
| Derive one Commit's final delta | Full file materialization in earlier path. | Cursor exists, but lowering retains `O(R)` edits and packed descriptors; stepping to a new leaf rereads ancestors, so index I/O can be `O(H × L)` for `L` leaves. | One or a fixed number of monotone `O(P)` logical passes, amortized `O(H+L)` index-page visits per pass, and `O(H+B)` cursor/transport resident state; measure actual visits and reseeks. |
| Bridge/service input | Earlier 256-edit/8 MiB limits. | Framed body, but declared count capped at 4,096 and server retains descriptor arrays. | Transfer `Θ(24R+S)` bytes through fixed buffers; bounded resident state plus **quota-charged** replay storage `Θ(24R+S)`. Validate exact count/length/order before publication. |
| Canonical file result | Existing C1 accepts an edit vector; its comparison/construction uses several materialized structures. | Same C1 4,096-run ceiling; current `EditStream`, planning segments and mapping cache can grow with input. | One final canonical root with resident `O(C+B)` frontier **if** C1 is redesigned; count mapping visits/reused subtrees, drafts and peak memory; require old accepted inputs to produce identical canonical roots. |
| Sequential Commits | Each successful Commit installs a new base; later generations need correct reconcile. | E strict overlap and a second sequential Commit have functional proof at `b2cd0df23`. | `C[n+1]` compares frozen final state only with saved `C[n]` roots; no cumulative replay of edits from `C[n-1]`. Verify base root, expected head and descriptor stream. |
| SDK command concurrency | One selected daemon mount and Q=0 control route. | Still one selected mount/session; one shell can have parallel child processes. | Bounded `V` mounts and `Q` admitted commands: owner storage `O(V+Q)` plus process/FUSE resource cost; per-Workspace Commit serialization, with deterministic overlap/custody proof. |

The claimed `O(H+K)` local mutation is an **algorithm target**, not a result
established by the current line count or E/F wall comparison. The
[private-tree study](PRIVATE_BACKING_AND_PAGE_TREE.md) examines the current
branch walk, replacement vector, payload registry and a potential mixed-level
shape at height 2. The [Commit study](FINAL_DELTA_COMMIT_COMPLEXITY.md) traces
every proportional allocation downstream. Both must pass before treating the
table's target column as implemented.

### A final-state example

```text
canonical C[n]:       a b c d e f g h
WRITE history in G:     b=X ; b=Y ; c=Z ; g=Q
frozen final G:        a Y Z d e f Q h
Commit input:           [b,c -> Y,Z] [g -> Q]
                       R = 2, regardless of four WRITE calls

after C[n+1]:         a Y Z d e f Q h
later G+1 changes e:  a Y Z d Y f Q h
Commit n+2 input:                  [e -> Y] against C[n+1]
```

The example describes the **semantic delta**. It does not by itself prove
bounded C1 memory, canonical identity, or that unchanged canonical subtrees
are reused. Those require the cross-layer gates below.

## 4. Commit lineage and concurrency

```text
 time -------------------------------------------------------------->

 saved head:  C[n] -------------------- C[n+1] ----------- C[n+2]
                 \                         ^                  ^
 live upper:       G[n+1] --freeze--------|                  |
                     \   G[n+2] accepts WRITE while C[n+1] builds
                      \____ rebase captured base to C[n+1] ____|
                              (only after known success)

 per Workspace: one Commit/Stage submission slot, ordered capture and install
 across Workspaces: independent owners, shared Store admission and head conflicts
```

For a linear history, the next Commit's `expected_head` and file base IDs must
be the **immediately preceding successful Commit**. Its final delta is computed
against those roots, not the Workspace's original attachment root. If an
external writer moved the Branch head, that is a conflict/rebase decision; a
successful local publish cannot silently use a stale ancestor. Unknown remote
outcome preserves the frozen generation, stage identity and dependent successor
until ownership is resolved. The current [CommitStaged and reconcile source
description](../../architecture/proposal/fuse-workspace-snapshot-overlay/17-commit-staged.md)
and [final E/F evidence](evidence/phase1-e-f-final/REPORT.md) cover the known
scope; #248 must keep those rules while changing the delta path.

The proposed concurrent daemon needs **command leases**, separate from native
`active_operations`: a shell may exist between two syscalls while that native
counter reads zero. Each lease begins before process spawn and ends after the
process group and its output are cleaned up. Unmount, close and shutdown must
consult leases and retained Commit owners; a registry mutex must not stay held
through a shell or Store call. The current source has a bounded WorkspaceHost
registry, but the daemon has one selected mount and one control session. The
[#249 issue](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249) owns that
transition and coordinates its Workspace-count policy with
[#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219).

### What “writes continue during Commit” currently proves

“Non-pausing” must name **which route and phase**. The E gate proves an
accepted mounted FUSE write while a real successor-builder page **read** was
held, followed by a correct first and second Commit. It does not prove that
every write can complete immediately during every Commit phase. The current
gate ownership is:

| Commit phase | Current interaction with mounted writes |
| --- | --- |
| Capture G | A short state ordering point pins the old root and advances the live generation. A caller may wait for this point. |
| Prepare/lower a frozen file | [`lower_file`](../../../crates/layerfs-workspace/src/commit/lower.rs) holds the shared metadata `writer()` gate **through the full extent cursor walk**. A mounted mutation uses deadline-bounded `writer_until`; it can wait for that walk and reach its deadline. |
| Supply replacement bytes | [`ReplacementSource::pull`](../../../crates/layerfs-workspace/src/commit/source.rs) reacquires `writer()` and a backing window for each pull, then reads the frozen sequence and payloads. A pull can contend with mounted work; the entire transfer is not covered by the E overlap proof. |
| Remote canonical construction and Branch publication | The local writer gate is not held continuously through the remote call, but its `Source::pull` callbacks take it as above. Shared transport/Store admission can also contend. |
| Reconcile saved head onto live G+1 | [`reconcile_commit`](../../../crates/layerfs-workspace/src/commit/reconcile.rs) takes two deadline-bounded ordering points. Its successor build is ungated; if a write advances the live revision during the build, it rebuilds from the newer root before install. The strict E test exercised this read-side window. A blocked page write with an unfinished metadata allocation is a different schedule and can still refuse a competing write. |
| Separate SDK Exec plus Commit | Today's daemon admits one control session and holds one lifecycle lock through both calls, so independent `WorkspaceApi::exec` and Commit calls cannot overlap through that control route. #249 owns this interface-level concurrency. |

The target is **no gate held for work proportional to `P`, `R`, replacement
bytes, remote I/O or canonical construction**. An immutable frozen-root reader
should use bounded read leases/windows; mutations should take only bounded
publication/ordering holds. Instrument maximum gate-hold time, write wait time,
deadline outcomes, window ownership and accepted revisions at each Commit
phase, with deterministic barriers. A deadline, genuine quota exhaustion or
unresolved ownership may still prevent progress. Continuous writers may force
reconcile to rebuild until its original deadline; “non-pausing writes” is not
a guarantee that Commit itself is wait-free or will always succeed.

## 5. “Bounded” means three different budgets

| Budget | What must be charged or measured | False inference to avoid |
| --- | --- | --- |
| Resident process memory | Cursor path, input/frame buffers, C1 frontier, live per-command state, drafts and all retained vectors. | A 64 KiB read buffer does not make an `O(R)` descriptor `Vec` or C1 plan bounded. |
| Container memory including page cache | Anonymous memory **and** file-backed pages retained by private backing, descriptor spool, Store reads and output. | Bounded heap does not establish bounded cgroup memory; cache state must be controlled before a latency claim. |
| Private/Store disk and object lifetime | Unique payload segments, 4 KiB metadata pages, replay spools, old heads, frozen and failed roots, temporary canonical objects. | Path copying does not free a page still reachable from an older root; an unknown outcome keeps its custody. |

CPU is bounded by admitted input and quotas, not by a constant: at minimum it
grows with final changed runs and bytes. Deadline failure must identify an
unfinished operation accurately and preserve the previous published head. A
fixed per-file count hidden in a payload registry, a protocol length calculation
that charges descriptor bytes against the 4 GiB *logical file* cap, or a 16-bit
inode count would simply replace the old 4,096 cap. The detailed studies name
the current sites and corresponding acceptance conditions. `MAX_FILE = 4 GiB`
is a genuine logical file bound; disk, memory and process quotas remain necessary.

## 6. Proof map for implementation review

| Gate | Falsifiable observation | First owner |
| --- | --- | --- |
| Tall private tree | Probe the 249-leaf transition (`30,753`–`30,876` nonmergeable records), then build 250 leaves (`30,877` records) and make first/middle/last-subtree narrow edits; prove child levels, exact bytes, old-root readback and path-local page work. | #248 prerequisite within #245 |
| No hidden 4,096 ceiling | More than 4,096 separated *final* runs reach Workspace, Bridge, service and C1; payload ownership and protocol length do not refuse by run count. | #248 |
| Final aggregation | Repeated overwrites and adjacent changes collapse to final state; insertion, deletion, truncation, Zero and no-op cases yield exact descriptors and old accepted canonical roots. | #248 |
| Bounded materialization | Instrument peak resident bytes, descriptor spool bytes, cursor page reads/reseeks, C1 segment/draft high-water, payload high-water and untouched base bytes read as `R` grows. | #248 |
| Incremental lineage | Two sequential Commits name `C[n]` then `C[n+1]` as their respective bases; write during frozen Commit is present only in the later head; prior head remains readable. | #248, retaining E proof |
| Concurrent commands and mounts | Two same-Workspace SDK Exec calls and one other-Workspace call overlap at barriers, with independent output/status, bounded owners and no premature unmount; same-Workspace second Commit cannot publish. | #249 |

Use count-driven diagnostics and the repository's one-attempt measurement rules
for any later timed arm. A complexity expectation is rejected when structural
counts grow with an untouched suffix or `R`-sized resident buffers survive,
even if one warm wall-clock row looks fast. The F latency cells remain
`INELIGIBLE`; this research does not resample them or create a performance PASS.

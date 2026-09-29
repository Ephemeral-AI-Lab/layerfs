# Architecture refinement for Phase 4.5 integration

> **Status:** Research; informative and not a product contract.
> Source review dated 2026-09-29. Documentation basis
> `8e8408ad3ac27e29407ed7e0ec643b8e6ae49cc7`; current product
> `11a864fc133844cae7a4247b1f243d84d5763b10`.
> Source and retained-evidence reads only; no new build or benchmark run.

This report describes the main-lane namespace refinement, the side-lane active
backing and Commit refinement, and their actual combined implementation. Three
read-only investigations traced namespace, Workspace ownership, and content/
transport separately. The comparisons use the code at each named pin; an
ancestor's algorithm is not automatically the current algorithm.

Reading order: [comparison scope](#1-comparison-scope-and-how-to-read-the-costs),
[component workflow](#2-component-ownership-and-the-complete-public-workflow),
[namespace](#3-main-lane-namespace-workflows),
[private mutations](#4-side-lane-private-backing-and-ordinary-mutation-workflows),
[Base reuse and lowering](#5-generic-base-reuse-and-authenticated-savefile-lowering),
[capture and completion](#6-capture-commit-reconciliation-and-funded-completion),
[grouped reads](#7-bounded-grouped-source-reads),
[held views](#8-public-old-view-reads-and-checked-release),
[canonical history](#9-canonical-storage-and-retained-history),
and [combined limits/proofs](#10-combined-complexity-limits-and-proof-boundaries).

## 1. Comparison scope and how to read the costs

| Slice | Before | After used here |
| --- | --- | --- |
| Main-lane mounted namespace | Phase 4, `ef3a310480254774d6e6966004fb5e0a4fa3b94d` | Standalone Phase 4.5 algorithm, `6eb7553671d3000160ad023a57b335e52dd81a26`, and current combined product, separately |
| Side-lane active backing | Frozen pre-#273 control, `48b51e874a41b3e1e6c6661e145316df8b408f07` | Current product `11a864fc133844cae7a4247b1f243d84d5763b10` |
| Generic Base reuse / internal SaveFile v2 | Implemented active backing before v2, `f00644479a9b7dfe0b74e02438eed6d60e30dc0a` | Current product |

Intermediate source-reader changes have their own pins below. These are source
comparisons, not matched timing arms. The [baseline inventory](BASELINE-20260929.md)
retains the complete 3x3 cohort, the NOT_RUN frozen control, and separate Init/
history profiles. No speedup is inferred from complexity or historical rows.

`O(...)` describes source-visible work as a dimension grows within the supported
profile. It does not remove a quota, hard bound, admission check or I/O deadline.
CPU, page updates, bytes transferred and physical allocation are separate costs.
Small fixed caches can bound one term while other terms remain proportional to
entries, extents, owners or retained generations. `Theta(x)` means proportional
to `x`; `Omega(x)` means at least that much required work.

Memory comparisons separate:

```text
consumer Budget                 charged logical resident reservations
Workspace private quota         allocated backing + reserved completion credit
Service input spool             metadata and uploaded bytes on Service disk
canonical C2 Store              persistent objects, packs and representations
actual process RSS              allocator, stacks, libraries and resident pages
kernel / VM / device / host      caches outside a process's logical Budget
```

A reservation is not a measured RSS peak. A bounded userspace window is not a
proof of bounded kernel cache. Additional memory qualification is deferred to
#283 under the owner's direction; numeric rows keep their recorded status.

## 2. Component ownership and the complete public workflow

The public execution route remains ordinary mounted POSIX. The namespace and
backing changes live behind that route. Internal SaveFile v2 extends an
authenticated Service boundary; it does not create a public range-edit API.

```text
macOS caller / SDK                         Docker Linux execution machine
------------------                        -------------------------------
WorkspaceApi.mount / exec / commit
           |
           | authenticated control; Workspace + incarnation + authority
           v
                               +---------------------------------------+
                               | daemon control slot                    |
                               | validate selector; refuse Busy/stopped |
                               +-------------------+-------------------+
                                                   |
                 exec(command) --------------------+--> /bin/sh -c command
                                                   |      cwd = FUSE mount
                                                   |             |
                                                   |       POSIX syscalls
                                                   |             v
                                                   |    Linux VFS / FUSE
                                                   |             |
                                                   |    layerfs-fuse replies
                                                   |             v
                 commit() -------------------------+--> Workspace owner
                                                          |
                                      +-------------------+-------------+
                                      | live G2 / captured G1             |
                                      | namespace, extents, handles, pins |
                                      | private indexes + payload backing |
                                      +-------------------+-------------+
                                                          |
                                          authenticated logical Bridge RPC
                                                          |
macOS Service machine                                     v
---------------------              +-------------------------------------+
                                   | Service authorization + admission   |
                                   | SaveFile / filesystem / History     |
                                   +----------------+--------------------+
                                                    |
                              +---------------------+------------------+
                              |                                        |
                              v                                        v
                    C1 canonical algorithms                    C5 History catalog
                    file / inode / directory                    stage / Branch /
                              |                                exact Commit outcome
                    authenticated provider + consumer                  |
                              v                                        v
                    C2 content Store                           History database
                    SQLite / packs / CAS                       separate ownership
```

The Service owns C1/C2 execution and the History catalog. Workspace backing and
kernel handles stay on the execution machine. The two databases are separately
acknowledged; their existence is not an atomic cross-database transaction.
Library separation between FUSE and Workspace adds no process or RPC boundary.

| Component | Refinement in this slice | Preserved responsibility |
| --- | --- | --- |
| SDK / sandbox / daemon control | Public held-view binding and checked custody | Execute caller commands; manage mounted Workspace lifecycle |
| FUSE | Bind ordinary callbacks to the current semantic owner | Kernel handles, visibility, replies and errno |
| Workspace namespace | Identity-relative reads and main-lane rename changes; active union qualifications below | POSIX names, inode identity, handles and live namespace |
| Workspace private backing | Pooled indexes, hot references, shared payload packs, capture and funded reconciliation | Exact private ownership, quota and old-reader custody |
| Bridge / Service | Capability-gated internal v2 and Service-local Base resolution | Authentication, one operation attempt, bounded framing |
| C1 file construction | Existing canonical algorithm reused by v2 | CDC, mapping and exact canonical construction |
| C1 filesystem construction | Identity reads and ordering/cycle-validation repairs | Canonical inode/directory maps and topology checks |
| C2 storage | No new storage format or CDC/delta algorithm introduced by #273 | Physical encoding, CAS, packs, publication and authenticated reads |
| History | Consumes the selected prepared change set once | Exact stages, expected-head checks and known/unknown outcomes |

Source: [SDK execution/Commit](../../../../crates/layerfs-api/sdk/src/workspace.rs#L75),
[daemon shell execution](../../../../crates/layerfs-daemon/src/execution.rs#L55),
[control admission](../../../../crates/layerfs-daemon/src/control.rs#L307), and the
[existing ownership diagram](../../../architecture/proposal/fuse-workspace-snapshot-overlay/README.md).
The older proposal's SDK editing description is historical; current Exec follows
the shell route shown here, with the direct range-edit entrypoint retired.

## 3. Main-lane namespace workflows

Let `P` be resident Nodes, `P_m` moved resident Nodes, `d` directory depth,
`L` aggregate locator bytes, `n` component-name bytes, `N` moved effective
subtree entries, and `B_ns` canonical base bindings. `h_i`, `h_d` and `h_p`
are the inode, directory and private index heights. `c` is the constant number
of parent/inode keys changed by an ordinary directory move.

### 3.1 Stable identity removes dependence on the old canonical pathname

Before Phase 4, at `6af2c5c59a48d0b6c85d656e55aecc353e346728`, an unrecorded
inherited directory move was refused. Canonical fallback addressed a pathname
that the old immutable root could not have at the new location. Phase 4 added
identity-keyed child/inode/list operations. Both later lanes retain that change.

```text
BEFORE IDENTITY READS                         AFTER IDENTITY READS
---------------------                        --------------------
parent Node.full_path                        parent stable serial + one name
       |                                                  |
join entire child pathname                                v
       |                                     select immutable Base root B0
       v                                                  |
Inspect::Attributes(B0, full path)                         v
       |                                     private binding / tombstone?
start at canonical root                           |              |
       |                                         yes             no
walk all d components                             |              |
       |                                          v              v
child inode                                  private I     C1 ChildAttributes
                                                          (B0, parent, name)
moved inherited directory:                                      |
new path absent from B0                                          v
       |                                                 inode[parent serial]
       v                                                        |
Unsupported before publication                          directory tree[name]
                                                                |
                                                                v
                                                        inode[child serial]
                                                                |
                                                                v
                                                        attributes + roots
```

The C1 child read costs `O(h_i + h_d + n)` rather than resolving the entire
path, `O(L + d(h_i+h_d))`. Inode serials and unchanged file-content roots
survive a rename. A committed namespace change still produces a different
filesystem root because its bindings change.

Current source: [C1 identity reads](../../../../crates/layerfs-content/src/filesystem/read.rs#L138),
[Service identity dispatch](../../../../crates/layerfs-server/src/service/read/content.rs#L77),
and [active binding resolution](../../../../crates/layerfs-workspace/src/filesystem/active_view.rs#L111).
Historical [inherited-move refusal](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6af2c5c59a48d0b6c85d656e55aecc353e346728/core/crates/layerfs-workspace/src/filesystem/rename.rs#L712).

### 3.2 Rename has three distinct implementations

```text
PHASE 4: ef3a31048
-----------------
resolve old/destination bindings in one selected view
       |
type / permissions / replacement emptiness / cycle checks
       |
build old and new full paths
       |
scan resident Nodes: projected descendant path valid?
       |
destination prefix longer OR depth greater?
       +--- yes ---> walk inherited effective descendants
       |             prove <=4096 bytes / <=256 components
       +------------------------------+
                                      |
edit old/new parent deltas: tombstone + same moved serial
                                      |
COW candidate -> seal -> recheck expected revision/root
                                      |
rewrite affected resident full paths -> publish -> checked result
```

```text
STANDALONE PHASE 4.5 ALGORITHM: c51b5c982 / 6eb755367
--------------------------------------------------
resident directory = {serial, parent serial, component name, attached}
       |
resolve two bindings by parent serial + component
       |
type / replacement / permissions
       |
walk destination's attached ancestor chain through node_index
       |                   refuse cycle / missing / detached edge
       v
old parent[name] = tombstone; new parent[name] = same moved serial
       |
prepare only the moved Node's charged component name
       |
COW candidate + final revision/live-chain checks
       |
update moved Node's parent/name; detach replaced directory
       |
publish private root -> checked result

unchanged descendants keep their serials and canonical roots
no descendant enumeration; no resident descendant path rewrites

separate lifecycle repair, 05eb5c148:
  two adjacent fallible seal calls -> one attempted seal
  same parent/name algorithm; corrected failure/reservation custody
```

The standalone algorithm removes the descendant work, but the pins
`c51b5c982` and `6eb755367` still contain the duplicate seal. The later
`05eb5c14849f1b874383fd1600ba9288f103ad93` repair must be credited separately.

```text
ACTUAL COMBINED ACTIVE PRODUCT: 11a864fc1
--------------------------------------
resident Node = {serial, parent, attached,
                 path[4096], optional charged extended_path}
       |
copy parent full locators -> validate old/new aggregate paths
       |
active N(parent,name), then C1 identity fallback
       |
type / replacement / prefix-cycle / resident-path checks
       |
destination DEPTH increases?
       +--- yes ---> walk inherited effective descendants
       |             prove <=256 components
       +------------------------------+
                                      |
under state gate: reserve P-entry path-tuple vector
scan ALL resident Nodes; prepare paths for P_m moved Nodes
                                      |
constant-count N / I / D updates through active pooled index
                                      |
publish matching index selection/revision
                                      |
install new resident paths / parent; checked result

same-depth length growth avoids the inherited subtree walk
resident full-path rewriting and depth-increasing walks remain
active locator ceiling: 65536 bytes / 256 components
```

| Cost | Phase 4 | Standalone Phase 4.5 algorithm | Current active union |
| --- | --- | --- | --- |
| Local same-depth move | `O(P L + P d + c h_p)` | `O(c h_p + d log P + n)` | `O(P L + c h_p)` plus actual active publication/owner work |
| Growing/deeper move | Above plus `N`-dependent traversal on longer or deeper prefix | Same parent/name bound; no subtree scan | Above plus `O(N(h_p+h_i+h_d+L))` on depth increase |
| Resident locator storage | Inline 4096 bytes per Node capacity | `O(P + sum(component-name bytes))` | Inline 4096 bytes per Node capacity plus extended locator capacities |
| Rename scratch | Recursive growth scan, conservative `O(d L)` | Candidate path/name scratch; no descendant path vector | `O(P + sum(new moved locator capacities))`, plus depth-scan frames |
| Changed private namespace | Constant-key COW paths; no inherited copy-up | Same sharing principle | Constant logical N/I/D keys; reached active index/owner work is separate |

These are local rename costs. They exclude C1 complete-Commit validation and
physical retirement. The standalone `node_index` attached-chain check also
runs during child lookup. A full mounted lookup includes `O(d log P)`;
repeating it from root through all depths can accumulate `O(d^2 log P)`.
Identity-only Service child lookup is the smaller, depth-independent slice.
The current lookup retains `O(L)` aggregate locator construction, private
index probes and optional inherited-name memo lookup.

Source: [current rename dispatch](../../../../crates/layerfs-workspace/src/filesystem/rename.rs#L48),
[active rename](../../../../crates/layerfs-workspace/src/filesystem/active_rename.rs#L239),
[depth scan](../../../../crates/layerfs-workspace/src/filesystem/rename_paths.rs#L33),
[resident representation](../../../../crates/layerfs-workspace/src/runtime/state.rs#L118),
[standalone ancestry](https://github.com/Ephemeral-AI-Lab/layerfs/blob/c51b5c982f186c7ab078def81cea0401f3f6d5f2/core/crates/layerfs-workspace/src/runtime/ancestry.rs#L9),
[standalone rename/seals](https://github.com/Ephemeral-AI-Lab/layerfs/blob/c51b5c982f186c7ab078def81cea0401f3f6d5f2/core/crates/layerfs-workspace/src/filesystem/rename.rs#L517),
and [main-lane implementation record](../../245/PHASE4_5_IMPLEMENTATION.md).

### 3.3 Listing, collection and canonical Commit still pay their own work

```text
opendir(serial) -> selected View + Node/path/parent pin
       |
live rename / WRITE / Commit advances another selection
       |
readdir(handle, cookie, <=128 requested entries)
       |
merge selected active N bindings/tombstones + canonical InodeList(serial)
       |
resolve emitted child identities -> charged ordered cookie indexes
       |
return bounded page -> continue on later call
       |
releasedir -> release view/node -> remove owned cookies -> collect
```

Enumeration is at least linear in returned entries and also pays for examined
rows, tombstones, seeks and emitted-child resolution. The earlier cookie/index
repair replaced a fixed 1024-cookie vector and linear searches with charged
ordered maps: seek/reuse becomes `O(log J)` for `J` cookies. This predates #273.
Current Node collection marks retained ancestors, rebuilds `node_index`, and
filters inherited-name records; it remains population work. Marking breaks
at already marked ancestors, so the current pass need not visit every ancestor
`P*d` times. Current rebuild/filter work has `O(P log P + C log P)` terms for
`C` inherited-name records, and its ancestry loop still has a 256-step bound.

```text
NAMESPACE COMMIT: separate from local rename
-------------------------------------------
captured changed N/I/D rows + immutable Base B0
       |
Service validates/spools prepared ordered changes
       |
C1 filesystem update
       +--> check retained directory/symlink aliases in canonical BASE tree
       +--> walk rebound EFFECTIVE subtree to reject final cycles
       +--> update changed directory trees and inode reference counts
       +--> retain unchanged content/subtree roots
       |
clean ordering scratch -> encode v1 filesystem root
       |
C2 save -> exact History publication -> local G1/G2 reconciliation
```

The v1 filesystem root remains 116 bytes with one inode-table reference; it
has no authenticated parent index. Applicable stored-directory moves still
require at least `Omega(B_ns + N)` alias/cycle binding examinations, with
visited/memo/candidate structures and effective-directory entries in memory.
The proposed [C1 parent-index follow-up](../../245/PHASE4_5_C1_COMMIT_FOLLOWUP.md)
is not implemented by the internal SaveFile v2 capability.

A separate ordering repair, `fbda0f0f1dbb6bb3ddd375694e3ea7edbab62ebd`, changes
unreachable-parent membership from all `U` positively bound children to only
`F_new` declared-new directory parents. It retains `O(F_new)` membership and
examines `R_ns` directory rows and their `K_bindings` individual bindings in
`O((R_ns+K_bindings) log(1+F_new))` work, plus subject queries. One row may
contain many bindings. It also charges serials using the existing
per-serial allowance instead of the decoded-walk allowance. The ordering
Budget is unchanged; these allowances are not total Workspace file limits.

Source: [paged directory/cookies](../../../../crates/layerfs-workspace/src/filesystem/directory.rs),
[C1 filesystem root](../../../../crates/layerfs-content/src/filesystem/root.rs#L12),
[C1 topology validation](../../../../crates/layerfs-content/src/filesystem/validate.rs#L331),
[effective cycle checks](../../../../crates/layerfs-content/src/filesystem/validate/cycles.rs#L28),
and [targeted parent membership](../../../../crates/layerfs-content/src/filesystem/update.rs#L535).


## 4. Side-lane private backing and ordinary mutation workflows

The frozen control already has a length-indexed per-file COW extent tree:
untouched suffix pages are shared. It also already grows resident Node storage
in charged 256-node chunks. Neither full-file copying on every WRITE nor a
newly removed 256-file cap is an honest baseline for this comparison.

### 4.1 Owned payloads and roots become pooled active records

```text
FROZEN CONTROL: 48b51e874              CURRENT ACTIVE PRODUCT
-----------------------              ----------------------
ordinary FUSE WRITE                  ordinary FUSE WRITE
       |                                    |
OwnedPayload, even for 1 byte         handle/root/rights/frontier admission
       |                                    |
per-file extent COW splice            <=128-byte input? ---- no --> OwnedPayload
       |                                    |                         |
pooled legacy I + generation D               yes                       v
       |                                    |                    Payload extent
RootOwner candidate + custody               v                         |
       |                             shared tiny pack record          |
seal candidate + expected revision          |                         |
       |                                    +-----------+-------------+
publish overlay root                                    |
       |                                                v
scan ALL resident Nodes for inode        eligible hot update OR generic splice
       |                                                |
notification / ACK                       coherent I/N/D/E/P/R/L publication
                                                        |
                                         node_index -> update one Node
                                                        |
                                         selecting-owner retirement
                                                        |
                                         notification / ACK
```

The Workspace-wide private index contains:

```text
I | inode                       current inode (416 bytes, <=4 inline extents)
N | parent | name               binding/tombstone (16 bytes)
D | generation | inode          dirty membership
E | inode | logical start       Base / Zero / Packed / Payload final interval
P | logical pack page           selected physical pack PageRef
R | page | ordinal | inode | start   inverse tiny-slot reference
L | payload | inode | start          inverse large-payload reference
```

```text
BEFORE: three live one-byte payloads          AFTER: shared logical tail
----------------------------------          --------------------------
Local A -> separate p-* file, >=4096 B        E(A) ---+
Local B -> separate p-* file, >=4096 B        E(B) ---+--> logical page 42
Local C -> separate p-* file, >=4096 B        E(C) ---+          |
                                                              P|42
                                                                |
                                                                v
                                                   physical authenticated pack
                                                   [128-byte header]
                                                   [48-byte record + bytes]...

append another record:
  build/verify complete new physical page p8
  publish P|42 -> p8 with E/R/I/D in one selected revision
  current view uses p8; old selected view keeps P|42 -> p7
  unrelated E records keep their logical page/ordinal coordinates
```

A 4096-byte pack has 3968 body bytes. It fits 80 one-byte records or 22 records
with 128 payload bytes. For `L_tiny` distinct live one-byte records, the
payload component changes from `4096*L_tiny` to an ideal
`4096*ceil(L_tiny/80)`. Both are linear. This is not an 80-fold reduction in
whole backing: indexes, dead slots, old pins, candidates, failures and larger
payloads must be added. Repeated replacement could already reclaim old payloads;
all historical write bytes were not necessarily retained before.

Source: [ordinary input ownership](../../../../crates/layerfs-workspace/src/filesystem/write.rs#L151),
[active mutation dispatch](../../../../crates/layerfs-workspace/src/filesystem/active_file.rs#L35),
[pack framing](../../../../crates/layerfs-workspace/src/backing/active/pack.rs#L6),
[index record framing](../../../../crates/layerfs-workspace/src/backing/active/records.rs),
[extent/inverse keys](../../../../crates/layerfs-workspace/src/backing/active/extents.rs#L19),
and [old payload floor](https://github.com/Ephemeral-AI-Lab/layerfs/blob/48b51e874a41b3e1e6c6661e145316df8b408f07/core/crates/layerfs-workspace/src/backing/segments.rs#L5).

### 4.2 Stable hot references avoid unchanged ancestor publication

Intermediate active v1 at `79eca5ddd1b8d6eb1f84a53f5e2486a83bac0d27` already
has pooling. Its branch records carry physical child pointers and exact maxima.
Replacing a cached leaf still changes its ancestors. Private-index v2 first
lands at `91c9c4938ec56c8550a8998a10dbf05c17c917c3`.

```text
INTERMEDIATE ACTIVE v1                    CURRENT PRIVATE INDEX v2
----------------------                    ------------------------
root R1                                   selected root + directory D10
  [exact max, physical B1]                             |
              |                                       v
branch B1                                  slot7/epoch3 -> physical leaf L1
  [exact max, physical L1]                  slot8/epoch2 -> physical branch B1
              |
leaf L1                                    branch B1
                                           [fixed fence, HotRef(7,3)]
replace L1 -> L2                                      |
  -> replace B1 -> B2                                 v
  -> replace R1 -> R2                                L1

                                           eligible replacement:
                                             verify complete physical L2
                                             D11: slot7/epoch3 -> L2
                                             publish matching selection
                                             branch B1 stays byte-identical
                                             frozen D10 still resolves L1
```

```text
hot attempt: selected inode/generation/dirty facts agree?
       |      recorded ancestor bindings/fences still match?
       |      EOF or advancing Base/Zero coverage eligible?
       |
       +-- yes --> merge affected leaf cells once
       |           verified page + directory selection publication
       |
       +-- no ---> generic indexed splice

overflow / eviction:
  byte-balanced groups -> split/carry through actual ancestors
  normalize unused reachable hot closure, preserving physical birth facts
  changed connection pages -> matching root/directory/revision

slot reuse increments epoch: old frozen HotRef cannot select a new occupant
```

The hot cache holds at most 8 frontier cursors and 64 decoded nodes under a
1 MiB reservation. Maximum active branch level is 7. These bound the hot term,
not the entire Workspace. Arbitrary repeated/overlapping writes can take the
generic route. Candidate, old-page, registry and operation allocations coexist.

Source: [tagged targets/fences](../../../../crates/layerfs-workspace/src/backing/active/keyed.rs),
[hot caps and bindings](../../../../crates/layerfs-workspace/src/backing/active/hot_cursor.rs#L14),
[hot eligibility](../../../../crates/layerfs-workspace/src/backing/active/hot_path.rs#L190),
[split/carry](../../../../crates/layerfs-workspace/src/backing/active/splice.rs),
and [selected publication](../../../../crates/layerfs-workspace/src/backing/active/index.rs#L415).

### 4.3 General writes, truncate and holes retain affected-range costs

```text
selected intervals:
[ Base 0..100 ][ Packed 100..101 ][ Base 101..500 ][ Payload 500..900 ]

WRITE [80,120)
  floor(80) -> scan affected E rows in pages of 128
       |
  retain left Base [0,80); remove covered E and only their R/L inverse keys
       |
  replacement [80,120); retain right Base [120,500)
       |
  publish I + D + changed E/inverse records together
       |
  check remaining inverse refs only for touched pack/payload prefixes

WRITE beyond EOF          TRUNCATE shorter          EXTEND afterward
  gap -> Zero extent        cut crossing extent       new range -> Zero
  data -> replacement       remove suffix E/R/L        no stale-byte resurrection

old pinned selection retains its own intervals, EOF and physical versions
```

For a WRITE touching `K_overlap` final extents, planning and charged scratch
remain proportional to affected records. The 128-record scan size is paging,
not a refusal after 128 overlaps. A whole-file overlap can touch all `E_f`
extents. Genuine new input still costs at least its byte length.

Let `W` be accepted writes, `u_i` input bytes, `N_i` resident Nodes, `h_e/h_k`
the old extent/keyed heights, `v_i/a_i/r_i` actual active node visits/new page
versions/replaced owners, `A_i` retained physical owners, `G_i` selecting
revisions, and `q_i/z_i` changed keys/examined inverse references. `S_i` counts
retained payload owners in the shared Host; `b_i` counts quota-status calls
reached by this operation.

| Work | Before | Current |
| --- | --- | --- |
| Update resident inode attributes | `O(N_i)` scan | `O(log N_i)` indexed lookup |
| Metadata publication | Extent/keyed COW paths, `O(h_e+h_k+K_overlap)` plus custody work | Actual affected visits/pages/inverse prefixes; eligible hot leaf avoids unchanged ancestor COW |
| Tiny eligible point sequence | `O(W*h_e)` structural path work as the extent tree grows | Conditional `O(W)` non-registry structural/page work under admitted fixed hot/tree bounds |
| Repeated one-location sequence | Already bounded final spans and `O(W)` structural work | Still `O(W)` structural work; packing/publication constants differ |
| General overlap | Affected-record and byte work | Still affected-record and byte work |
| Real CPU beyond structural paths | Payload/ledger/owner and notification work | `log N_i`, changed-map ordering, owner-registry `log A_i`, pin-cohort `log G_i`, conditional shared-Host payload scan, physical I/O/syscalls and notification |

A useful current accounting expression is
`O(u_i + log(N_i+1) + q_i log(q_i+1) + v_i C_page
+ (v_i+a_i+r_i)log(A_i+1) + r_i log(G_i+1)
+ z_i(H*C_page+log(q_i+1)) + 4096*a_i + b_i*S_i)`, plus actual payload and syscall work.
`C_page` is fixed-page cell work and `H<=7` the active level. With
`A_i=Theta(W)`, registry CPU alone can total `O(W log W)`. The source does not
justify universal old quadratic WRITE, current constant CPU, or a timing ratio.
Generic boundary admission can call quota status, which scans and locks every
retained payload-owner record in the shared Host. Direct eligible hot publication
does not itself reach that scan. This is separate from the conditional linear
structural/page term.

Source: [generic extent planning](../../../../crates/layerfs-workspace/src/backing/active/extents.rs#L367),
[inverse-prefix liveness](../../../../crates/layerfs-workspace/src/backing/active/reclaim.rs),
[physical registry](../../../../crates/layerfs-workspace/src/backing/active/pages.rs#L50),
[shared-Host payload status](../../../../crates/layerfs-workspace/src/backing/payload.rs#L314),
and [old all-Node scan](https://github.com/Ephemeral-AI-Lab/layerfs/blob/48b51e874a41b3e1e6c6661e145316df8b408f07/core/crates/layerfs-workspace/src/filesystem/write.rs#L737).


## 5. Generic Base reuse and authenticated SaveFile lowering

### 5.1 Ordinary byte proof avoids retaining a copied Base tail as new payload

At pre-v2 product `f00644479`, generic reads return Base bytes and generic
writes retain copied bytes as Local/Packed payload. Current code can remember
one successful contiguous canonical read and prove a following WRITE's bytes.

```text
ordinary FUSE READ -> selected view -> actual returned bytes
       |
all bytes from one immutable root + contiguous source interval?
       +-- no --> clear memo
       +-- yes -> one charged record in this Workspace incarnation:
                  {inode, root, source offset, actual bytes <=128 KiB}
                                  |
ordinary FUSE WRITE -> acquire normal owned input -> validate selected identity
                                  |
                         compare EVERY supplied byte
                                  |
                      +-----------+---------------------+
                      | exact root/inode/length/bytes    | mismatch/stale/mixed
                      v                                 v
              Base-origin destination extent      ordinary Local/Packed route
                      +------------------+--------------+
                                         |
                               normal index publication / ACK
```

The memo is not a general dedup finder, pin or refund permit. It is optional,
charged and replaces the preceding record. Larger owned-input comparison uses
an 8 KiB scratch window. Failure to reserve the memo leaves ordinary semantics.

For `B_copy` actually copied bytes, command work remains `Omega(B_copy)`:
reads, caller copying, WRITE acquisition and equality comparison all occur.
Matching bytes avoid a `Theta(B_copy)` retained Local payload contribution;
private final payload depends on genuinely new Local bytes plus extent/index/
ownership metadata. An eligible block adds `O(block_bytes)` comparison and
at most one 128 KiB memo. This is a space improvement, not constant-time copying.

Source: [read memo](../../../../crates/layerfs-workspace/src/filesystem/read_origin.rs#L6),
[actual read provenance](../../../../crates/layerfs-workspace/src/filesystem/read.rs),
[ordinary WRITE selection](../../../../crates/layerfs-workspace/src/filesystem/active_file.rs#L134),
and [Base-origin publication](../../../../crates/layerfs-workspace/src/backing/active/generation/read_origin.rs#L5).

### 5.2 v1 forward anchors and v2 Service-local replacement reads

```text
SAVEFILE v1: opcode20                          INTERNAL v2: opcode29
--------------------                          ---------------------
final Base/Local/Zero descriptors              LocalEdit attachment FIRST
       |                                               |
parser tracks prior retained Base end          authenticated capability opcode28
       |                                      existing SaveFile grant
Base source offset >= prior end?                       |
       +-- yes -> keep anchor                 typed version EXACTLY2?
       +-- no --> InvalidInput                         +-- no --> refuse attachment
                                                        +-- yes -> admit mutations
promoted reordered Base descriptors                     |
can violate that grammar                               v
                                              version2 + 24E descriptors
recursive daemon ReadFile is not viable:        + exact L Local/Zero input bytes
upload owns that client's mutex                         |
       -> recursive call needs same mutex               v
       -> upload cannot advance               validate counts/source arithmetic/
                                              framing/end input / Zero bytes
                                                        |
                                 +----------------------+----------------------+
                                 |                      |                      |
                                 v                      v                      v
                         forward Base anchor    backward/duplicate Base    Local/Zero
                         retained unchanged     32-byte origin record     exact input
                                 |                      |                      |
                                 +----------------------+----------------------+
                                                        |
                                              ordered 32-byte edit spool
                                                        |
                                              Store::begin_save
                                               /                 \
                                              v                   v
                                        StoreProvider        SaveHandoff
                                              |                   ^
                                        ResolvedSource            |
                                        binary search fixed       |
                                        origin record             |
                                               /          \       |
                                              v            v      |
                                    Base: local C1    Local/Zero   |
                                    read_range on     retained    |
                                    same root         bytes/spool |
                                    <=64 KiB window        |      |
                                              +------------+      |
                                                      |           |
                                              existing apply_edits+
                                                      |
                                              cleanup / original deadline
                                                      |
                                              save.finish -> checked result
```

The Service resolves Base using its own authenticated provider; it never asks
the daemon to make another RPC during upload. v1 opcode and forward grammar
remain unchanged. Current LocalEdit admission requires exactly typed version 2;
it does not treat any higher advertised version as automatically compatible.
The actual persisted Base length is checked when C1 opens FileView, after input
validation and after `begin_save`; those are distinct ordering points.

Pre-v2 ordinary copies retained literal Local bytes and remained compatible
with v1. The non-forward Base refusal and recursive-RPC timeout occurred in
the [reverted dirty provenance experiment](../LOWERING-GENERIC-EXEC-RESEARCH-20260929.md).
They are retained diagnostic failures, not a clean product's SaveFile result.

For `E` descriptors and `L` declared Local **plus Zero input bytes**:

| Quantity | v1 | v2 |
| --- | --- | --- |
| Wire input | `24E + L` | `1 + 24E + L` |
| Conservative derived spool admission | `48E + 32 + L <= 8 GiB` | `80E + 32 + L <= 8 GiB` |
| Base order | Forward, non-overlapping retained anchors | Forward anchors retained; backward/duplicated ranges become replacement sources |
| Replacement Base byte spool | No such accepted reordered Base route | None; Service reads actual Base bytes in bounded windows |
| Input spool space | Descriptor/edit metadata and declared bytes | `O(E+L)` metadata and declared bytes; not result-sized Base spool |

`MAX_FILE=4 GiB` and the 8 GiB input budget remain. Origin/edit/temporary-Zero
records are 32/32/16 bytes. Up to 64 KiB declared bytes stay resident; larger
input uses a Service spool and 64 KiB transfer window. These are input-owner
bounds, not whole-Service RSS bounds.

Let `R_b` be backward/duplicated Base replacement bytes, `Q` actual source
window/run lookups, and `k` derived C1 edits. Input parsing costs `O(E+L)`;
origin navigation costs `O(Q log E)` plus actual Base reads. Total lowering is
`O(E+L+Q log E) + T_C1(k,L+R_b,affected paths,comparisons) + T_C2`.
Anchored chunked edits can share untouched subtrees, but `R_b` can be
`Theta(final result size)`. Non-no-op chunked replacements pay actual reads,
CDC, hash and C2 work; a whole-file result pays assembly/hash instead.
No-op comparisons and representation transitions retain their own paid work.

Source: [capability admission](../../../../crates/layerfs-workspace/src/runtime/host.rs#L366),
[wire contract](../../../../crates/layerfs-bridge/src/contract/request.rs),
[Service dispatch](../../../../crates/layerfs-server/src/service/handler.rs#L275),
[stream parser/spools](../../../../crates/layerfs-server/src/service/save/file_stream.rs#L279),
[fixed-record Base resolution](../../../../crates/layerfs-server/src/service/save/file_stream/origin_runs.rs#L83),
[save owner ordering](../../../../crates/layerfs-server/src/service/save/content.rs#L23),
and [approved prospective decision](../SAVEFILE-V2-PROSPECTIVE-20260929.md).

### 5.3 Local progress and upstream admission are different

```text
Commit thread                                  concurrent FUSE thread
-------------                                  ----------------------
Workspace remote permit                        admitted/cached local operation?
       |                                                |
daemon authenticated-client mutex              +--------+------------------+
       |                                       |                           |
held through complete SaveFile call             v                           v
       |                                 local metadata/WRITE        needs Service
Service StoreProvider -> local Base reads       |                     Inspect/ReadFile
       |                                  local gate/publication           |
Service SaveHandoff -> C2 work                    |                   remote slot occupied
       |                                        v                           |
checked result                             G2 can progress                  v
       |                                                                   Busy
release mutex / permit
```

Processes are not frozen or drained. Local progress is real, but operations
requiring the occupied upstream slot can return Busy. State/publication gates
also have actual work and contention; no diagram promises every callback
progresses during every Commit.

Source: [daemon transport mutex](../../../../crates/layerfs-daemon/src/run.rs#L98),
[Workspace remote admission](../../../../crates/layerfs-workspace/src/runtime/state.rs#L752),
and [retained local successor proofs](../PREMERGE-FUNCTIONAL-COMPLETION-20260929.md).


## 6. Capture, Commit, reconciliation and funded completion

### 6.1 G1 remains selected while G2 changes the same backing

```text
BEFORE, 48b51e874                            CURRENT ACTIVE PRODUCT
-----------------                           ----------------------
pin RootOwner G1                            capture selected root/directory
advance generation -> live G2                 seal fixed tail; register pin
       |                                                |
G2 CapturedBase points at exact G1            +-----------+------------------+
       |                                     |                              |
canonical G1 result                          v                              v
       |                              immutable G1                  live G2 selection
freeze current G2 frontier             Stage captured D keys         new versions/fund
       |                              save selected facts            no process pause
stream/rebuild LIVE dirty frontier            |                              |
       |                              ONE canonical Commit outcome          |
current frontier changed?                     |                              |
       +-- yes -> rebuild newer frontier      v                              |
       +-- no --> seal/install         prepare captured rows/deletions      |
                                      outside state gate                    |
                                             +------------------------------+
                                                                            |
                                                  under gate: recheck identities/revision
                                                                            |
                                                  each CAPTURED dirty inode:
                                                    unchanged since G1 -> saved Base
                                                    G2 changed -> preserve its extents
                                                                  and old Base coordinates
                                                                            |
                                                  delete captured D keys only
                                                  publish affected index closure
                                                                            |
                                                  advance context; release G1;
                                                  checked maintenance; finish fund
```

The old rebuild covered the live changed frontier, not the whole Workspace.
The current route removes that rebuild/restart loop in favor of a captured-set
patch and keeps later G2 changes. It does not reset backing, replay Commit,
freeze processes, drain Exec or remount.

Low-level active capture does not enumerate extents: it seals a fixed tail,
shares the selected directory and adds `O(log G)` pin bookkeeping for `G`
selecting revisions. Public capture also performs maintenance and clones pending
new-directory declarations; total capture includes their count. There is one
public Submission, while lower active backing has 32 captures and at most
160 distinct selecting revision keys.

For `D` captured dirty identities, final extent counts `E_f`, and `K_C5`
changed/deletion/inverse/compaction keys, preparation includes
`O(D*lookup_work + sum E_f)` and scratch `O(D+sum E_f+K_C5+name_bytes)`.
Patch ordering/installation includes `O(K_C5 log K_C5)` and actual reached page/
owner work. Under pressure, C5 may scan `J` pack locators and sort partial-page
candidates in `O(J log J)` plus relocation reads and the shared-Host
`O(b*S_payload)` quota-status scan. Ordinary WRITE's restricted
`max_sources=1` cannot enter that full pressure scan. Actual affected installation
and compaction still run under the state/publication gate.

Source: [old successor rebuild](https://github.com/Ephemeral-AI-Lab/layerfs/blob/48b51e874a41b3e1e6c6661e145316df8b408f07/core/crates/layerfs-workspace/src/commit/reconcile.rs#L271),
[current capture](../../../../crates/layerfs-workspace/src/overlay/snapshot.rs#L436),
[active reconcile](../../../../crates/layerfs-workspace/src/commit/active_reconcile.rs#L26),
[map transfer/publication](../../../../crates/layerfs-workspace/src/backing/active/lifetime.rs#L112),
and [pressure compaction](../../../../crates/layerfs-workspace/src/backing/active/compaction.rs#L257).

### 6.2 Retirement visits selecting cohorts, not unrelated pinned prefixes

Intermediate active v1 maintenance repeatedly searched a retired-owner vector.
A pinned prefix could be inspected again at later publications. Current active
retirement landed at `0513f8a1a12be6221574e893485fb8b67c091d6e`.

```text
replaced physical page: birth b, retired at revision r
       |
latest selecting pin s in [b,r)?
       +-- no --> verify identity / st_blocks -> unlink -> refund
       +-- yes -> cohort[s], retain physical charge
                          |
                    final release of s
                          |
                    visit cohort[s] only
                          +-- another selector -> migrate owner to its cohort
                          +-- none -> checked unlink / refund
                          +-- failure -> charged failed custody / stop admission
```

Final release costs its actual `T_pin` owners:
`O(T_pin*(log A + log G + identity/unlink work))`. Owners may migrate between
cohorts. Large-payload maintenance has its own retired-map scan; not every
payload route gains this exact cohort algorithm. Known failures stay charged.

Source: [retirement cohorts](../../../../crates/layerfs-workspace/src/backing/active/retirement.rs#L70),
[checked page release](../../../../crates/layerfs-workspace/src/backing/active/pages.rs#L749),
and [large-payload maintenance](../../../../crates/layerfs-workspace/src/backing/active/lifetime.rs#L27).

### 6.3 Completion credit is owned before dirty publication

The escrow repair compares with active source
`3318e5cd09a91b2e9a3bf62f03bc1ccafb2125dd`. The earlier legacy RootOwner
already had a different completion mechanism and is not the right "no escrow"
baseline. The broken active path acquired its completion quota at capture and
allocated active pages from ordinary headroom.

```text
BEFORE ACTIVE REPAIR                         CURRENT FUNDED COMPLETION
--------------------                         -------------------------
ordinary owners consume quota                prospective first dirty publication
       |                                                |
capture asks for 208-page credit              pre-admit frontier RAM
       |                                     precharge 208*4096 disk credit
StorageFull under occupied headroom                       |
                                             refusal -> no dirty publication
                                                         |
                                             live G1 owns same ProgressFund Arc
                                                         |
                                             capture transfers Arc to Submission
                                             G2 first dirty change owns a NEW fund
                                                         |
                                             active C5 allocation:
                                               take reserved credit
                                               create exclusive identity-known page
                                               observe actual st_blocks*512
                                               reserved -> allocated same owner
                                               unused partial credit -> SAME fund
                                                         |
                                             write/readback/authenticate
                                               publish OR retain failed page
                                                         |
                                             checked unlink:
                                               unfinished fund -> same reserved credit
                                               finished fund -> ordinary refund
                                                         |
                                             successful install + cleanup
                                             finish fund -> refund unused reserve
                                             pinned/published pages stay allocated
```

The reserve is **851,968 bytes of private disk quota**, not a heap buffer.
The ProgressFund metadata object has a separate 256-byte resident charge.
Zero/partial allocations conserve allocated plus reserved charge; unrelated
owners and old pins are never sacrificed. Clean Commit also requires its
explicit capture/completion admission. A bounded reserve is not a promise that
an arbitrary frontier will always complete under fixed quotas.

```text
one canonical command attempt
       +-- unknown terminal -> keep Submission/pins/charge; no resend
       +-- known result ----> local C5 install
                                  |
                             local failure, no installed revision
                                  |
                             preserve saved G1 / G2 / old views
                                  |
                             explicit SAME-selector local resume
                                  |
                             eligible never-ready candidate?
                               identity/accounting complete
                               unpinned, same unfinished fund
                               reserved=0, allocated<=4096
                                  +-- no --> retain Busy custody
                                  +-- yes -> checked release to SAME fund
                                            local install only
                                            canonical command not reissued
```

Unknown identity/accounting, foreign funding, a pin, another stopped condition,
or an already installed revision does not authorize repair. The SDK physical
failure proof retains its own known-result/held-lease custody; it is not promoted
into the separate native clean-refund result.

Source: [frontier admission](../../../../crates/layerfs-workspace/src/runtime/state.rs#L350),
[completion constants](../../../../crates/layerfs-workspace/src/backing/metadata.rs),
[same-owner fund](../../../../crates/layerfs-workspace/src/backing/metadata/progress.rs#L10),
[allocation/repair/refund](../../../../crates/layerfs-workspace/src/backing/active/pages.rs#L370),
and [known/unknown completion](../../../../crates/layerfs-workspace/src/commit/completion.rs#L147).


## 7. Bounded grouped source reads

These are Workspace-private 4096-byte packs, distinct from C2 persistent CAS
packs. The correct before is `6c283fb3b2d272f8615ff61187787f734cab2633`.
`cc9ce918c077e319d5848f3e67ff64d56a60f5bd` adds 256-reference grouping;
`686d002fa889f016cac3eb491b5712229e511360` widens it to 1024 references.

```text
BEFORE: final-output order, one decoded pack cache
------------------------------------------------
r0(A) -> r1(B) -> r2(A) -> r3(C) -> r4(B)
  |        |        |        |        |
load A   load B   load A   load C   load B
5 references, 3 distinct packs, potentially 5 loads

AFTER: required-reference window, original output order preserved
----------------------------------------------------------------
selected G1 extents in final logical order
       |
gather <=1024 REQUIRED references AND <=32768 payload bytes
       |
assign original scatter positions: r0=0 r1=1 r2=2 r3=3 r4=4
       |
sort INDEXES by logical pack: [r0(A),r2(A),r1(B),r4(B),r3(C)]
       |
identity-checked reader:
  load A once -> fill positions0,2
  load B once -> fill positions1,4
  load C once -> fill position3
       |
32 KiB scatter buffer: [r0][r1][r2][r3][r4]
       |
emit ORIGINAL logical byte order -> ordinary SaveFile stream
```

All selected slot identity, generation, revision, ordinal, offset and length
checks remain. This is not a whole-file cache, warming pass or multi-pack LRU.
On a 64-bit host, additional fixed capacity is
`32768 + 1024*(32-byte Reference + 8-byte index) = 73,728 bytes`, excluding
vector headers, the existing decoded pack and other upload allocations.

For `N_ref` packed references, `r_i` references and `d_i` distinct required
packs in window `i`, loads fall from up to `N_ref` under interleaving to at most
`sum d_i <= N_ref`. Worst case remains `Theta(N_ref)` across windows. CPU adds
`sum O(r_i log r_i)` sorting and actual byte scatter/decoding. With fixed window
caps that term is linear in reference count, but locator/owner and byte work
remain. A recurring pack may be loaded in every window; there is no global
"one read per pack" guarantee.

### 7.1 Descriptor collection improves growth cost but remains extent-sized

```text
FROZEN CONTROL, 48b51e874             CURRENT ACTIVE UPLOAD
------------------------             ---------------------
pinned extent root                   scan selected E rows in pages of128
       |                                        |
persistent extent cursor             need more vector capacity?
       |                                        +-- no --> append
one 24-byte descriptor                          +-- yes -> max(required,2*capacity)
       |                                                  precharge OLD+NEW
stream descriptor body                                    move charged elements
       |                                        |
fixed replacement cursor/window                 v
                                      Vec<Extent> + 24E descriptor vector
                                                |
                                      fixed source window + normal upload
```

Intermediate active collection used repeated exact 128-entry growth, with
worst-case prefix relocation `O(E_f^2/128)`. Geometric growth makes aggregate
element movement amortized `O(E_f)`. Current extent/descriptor memory remains
`O(E_f)` with old/new capacity overlap. The frozen control's cursor had
`O(h_e + fixed window)` upload scratch. This is an explicit memory-scaling
regression in this part of the active implementation, despite the collection
and source-locality improvements.

Source: [grouped window](../../../../crates/layerfs-workspace/src/commit/active_source.rs#L11),
[one decoded pack and authentication](../../../../crates/layerfs-workspace/src/backing/active/reader.rs#L60),
[extent/descriptor collection](../../../../crates/layerfs-workspace/src/commit/active.rs#L296),
and [frozen streaming upload](https://github.com/Ephemeral-AI-Lab/layerfs/blob/48b51e874a41b3e1e6c6661e145316df8b408f07/core/crates/layerfs-workspace/src/commit/upload.rs#L22).
Private index v2, internal SaveFile v2, and `LFS_ACTIVE_SOURCE v=2` diagnostic
schema are three different version domains.


## 8. Public old-view reads and checked release

The native selected-view machinery existed before the public SDK lease binding.
The refinement adds a real authenticated way to hold and read that selection
across a finished Commit. It does not change a read-only lease into an editor.

```text
BEFORE: native View / snapshot owner       AFTER: authenticated public lease
-----------------------------------       ---------------------------------
local selected View                       pin_view(Workspace, incarnation)
       |                                             |
internal snapshot pin                                v
       |                                  charged registry, maximum 32 leases
native reads + internal release                       |
                                          token(tag + 32 random bytes)
                                          generation / revision / root entry
                                                     |
                                  +------------------+------------------+
                                  |                                     |
                                  v                                     v
                          lookup(parent entry, name)             finished Commit
                                  |                              live G2 advances
                          bind serial to this lease                     |
                          retain pinned selected path                   |
                                  |                                     |
                                  +----> read / list / readlink <--------+
                                         through the same old selection
                                                     |
                                            release_view(token)
                                                     |
                                      checked active retirement selector
                                      +--------------+--------------+
                                      |                             |
                                      v                             v
                              Completed: refund            failed/unknown terminal
                              known released charge        retain explicit custody
```

Tokens bind to one Workspace/incarnation, and issued entry serials bind to one
lease. An entry from another lease does not authorize a read. Control operations
use a `try_lock`: calls during an in-flight Commit fail Busy; sequential pin,
Commit, old-view read is the supported SDK order. Local FUSE progress has a
different lock boundary and is described with Commit below.

If `v` leases hold a total of `y` issued entries, registry navigation adds
`O(log v + log y)` work before the underlying selected-view lookup/read cost.
Its resident records are `O(v + y + sum(path_bytes))`; cloning an entry also
copies its path. Source-level accounting reserves 256 bytes per lease and
512 bytes per issued entry, while actual path allocation and response buffers
remain distinct memory terms. These charges do not establish exact RSS.
Pinned backing adds the still-live pages/payload of each selected revision;
releasing a token is not permission to refund another pin's data.

Listing is paged at 128 entries and the declared read maximum is 128 KiB.
The retained live SDK full-byte proofs use 16 KiB reads. The separate 32 KiB
`Io` observation remains unresolved; declared 128 KiB support is not accepted
128 KiB proof. A response Budget reservation happens before allocation and
response publication; the fixed-16-MiB Budget refusal proof stays scoped.
Dropping the plain client lease value does not issue a checked remote release.
Lost release acknowledgement is retained as Unknown and is not automatically
retried or reported as a successful refund.

Source: [SDK lease calls](../../../../crates/layerfs-api/sdk/src/workspace_view.rs),
[client lease value](../../../../crates/layerfs-api/core/src/workspace_view.rs),
[daemon dispatch](../../../../crates/layerfs-daemon/src/control_view.rs),
[charged registry](../../../../crates/layerfs-workspace/src/runtime/view_leases.rs#L18),
[response admission and pinned reads](../../../../crates/layerfs-workspace/src/filesystem/view_reads.rs#L223),
and [wire bounds](../../../../crates/layerfs-bridge/src/contract/workspace_view.rs#L13).

## 9. Canonical storage and retained history

C1 file algorithms and C2 storage are existing foundations. C2 production source
has no changes between the frozen pre-#273 control and current product. The C1
file edit change in that interval is diagnostic wrapping; the server's new v2
source adapter calls the existing constructor. Namespace validation repairs are
separate from file construction and are described in section 3.

```text
authenticated Base root + ordered edits + replacement source
       |
C1 FileView::open -> actual Base length / edit validation
       |
bounded no-op byte comparison
       +-- equal --> same Base root (comparison work still paid)
       +-- differs -> choose representation by FINAL length
                          |
              +-----------+---------------------------+
              |           |                           |
              v           v                           v
            Empty      WholeFile                   Chunked
              |           |                           |
        defined root  assemble/hash          split affected boundaries
                      full final object       retain untouched subtree IDs
                                              CDC replacement bytes
                                              join/coalesce boundaries
                                              emit children before parents
              +-----------+---------------------------+
                          |
                  finalized canonical objects
                          |
                  C2 SaveHandoff
                          |
                  exact CAS reuse / authentication
                  FULL or eligible PREFIX / compression / pack placement
                          |
                  save.finish -> known root result
                          |
                  independent History publication
```

| Case | Required time | Resident/space qualification |
| --- | --- | --- |
| Whole-file result | `Theta(final bytes)` assembly/hash | Whole object allocation, bounded by configured cutoff; default 128 KiB |
| Whole-to-chunked transition | Complete builder processes retained/replacement stream | Bounded construction owners; not an `O(delta)` shortcut |
| Anchored chunked edit | Affected mapping paths/joins plus replacement bytes and comparison work | Deferred drafts/maps and explicit limits remain; untouched subtrees shared |
| Reordered/duplicated v2 replacement | Actual `L+R_b` construction/read/hash work | No daemon Base spool; input metadata scales with descriptors |
| Exact no-op | Reads/comparisons needed to establish equality | Returns existing root after proving equality |
| C2 save/read | Authentication, CAS, dependencies, encoding, physical I/O and finish | Provider session/caches/decoded owners are separate Service terms |

C1 mapping fanout is at most 128, maximum level 31, mapping object at most 8192
bytes. Its deferred edit charge cap is `8 MiB-1`, not Service RSS. Read waves
include up to 32 raw chunks of at most 32 KiB each, up to 32 navigation pages and
64 memo pages; traversal frontier and other owners coexist. C2 permits up to
4096 requested objects/32 MiB canonical response bytes and separately owns its
4 MiB dependency-pack and 512 KiB decoded-group caches. None is the Workspace
hot-cache budget or a new per-Workspace file-count limit.

```text
HISTORY SHARING
---------------
state 0 root A -> mappingA -> payload objects
       |
state 1 root B -> changed boundary/new objects
                 +------> retained old subtree/object IDs
       |
state 2 root C -> next changed boundary/new objects
                 +------> retained IDs from earlier states

C2: retained union of unique canonical objects + physical representations
    + dependencies + pack slack
C5: immutable roots/parents and catalog rows
Workspace private pack/escrow: local execution ownership, not History storage
```

For `H_states` retained states, space is the union of their retained unique
objects/representations plus catalog and slack, not inherently
`H_states*final_file_bytes`. It is also not universally just literal edit bytes:
representation changes, mapping boundaries, encoding and dependencies matter.
Time is the sum of actual per-state C1/C2 construction. Stride10/3/1 selects
17/53/157 states; it changes workload membership, not one Commit's semantics.
The current matched history cohort is NOT_RUN. Historical profiles retain their
own routes/policies in the [baseline inventory](BASELINE-20260929.md).

Native Init likewise still pays for source entries and bytes. Prepared entry/
inode/directory vectors scale with imported entries. Mounted rename and active
packing do not remove Init's total-input work or change its worker exception.

Source: [C1 edit dispatch](../../../../crates/layerfs-content/src/file/edit/apply.rs#L46),
[no-op comparison](../../../../crates/layerfs-content/src/file/edit/compare.rs#L74),
[deferred ownership](../../../../crates/layerfs-content/src/file/edit/tree.rs#L31),
[mapping bounds](../../../../crates/layerfs-content/src/file/mapping/types.rs#L12),
[read waves/frontier](../../../../crates/layerfs-content/src/file/mapping/read.rs#L30),
[C2 operation provider](../../../../crates/layerfs-storage/src/cas/provider.rs#L58),
[C2 resource policy](../../../../crates/layerfs-storage/src/policy.rs#L157),
[History ownership](../../../architecture/16-history.md),
and [native import](../../../../crates/layerfs-server/src/service/save/import/namespace.rs#L76).


## 10. Combined complexity, limits and proof boundaries

### 10.1 Current aggregate memory and backing are not one fixed window

```text
CURRENT RESIDENT / SCRATCH TERMS
-------------------------------
O(resident Nodes * inline4096 + extended locator capacities)
 + node / inherited-name / cookie indexes
 + sum(directory-handle pinned locator capacities)
 + O(physical PageStore owners + retired cohorts + large-payload owners)
 + prepared dirty/name frontier
 + <=1 MiB selected hot reservation
 + <=128 KiB optional provenance bytes
 + 73,728-byte grouped source capacity + one decoded pack
 + O(E_f) upload extent/descriptors, including growth overlap
 + O(D + sum E_f + K_C5 + name bytes) reconciliation scratch
 + held-lease entries and their paths / response buffers
 + C1/C2 Service owners and caches in THEIR process
 + allocator / stack / library / kernel cache effects (not source Budget)
```

PageStore has a 128-byte reservation per physical owner, retirement records
have their own charges, and resident Nodes still store inline full paths.
The fixed hot budget does not cover those terms. Reconcile map transfer removes
duplicate complete patch clones and moves charged key/value ownership; it
reduces overlap but leaves `O(K_C5)` memory.

Private physical backing consists of current index/pack pages plus uniquely
retained pin/candidate/failed pages, large payload blocks and surviving legacy
result/ledger backing. Add unused completion credit to **reserved quota**, not
physical allocation. Service input spools and C2 persistent Store space are
separate. Checked `st_blocks*512` proves actual private allocation for a given
receipt, not a phase RSS peak or all caches.

### 10.2 Limits removed, raised, or retained

| Boundary | Actual disposition |
| --- | --- |
| Unrecorded inherited-directory move refusal | Removed by Phase4 stable-identity/origin work |
| Standalone descendant/path rewrite cost | Removed by parent/component representation; current active union retains some of this work |
| Current active aggregate locator |4096-byte refusal raised to65,536 bytes;256 components retained |
| Resident Node count256 |Already a charged allocation chunk at frozen control; not a newly removed file cap |
| Fixed1024 readdir cookies |Removed earlier by charged ordered-cookie repair |
| Small-write payload physical floor |Amortized across shared packs; metadata/pins/failures still add space |
| Unchanged hot ancestors rewritten |Avoided for verified eligible v2 targets; splits/general updates still pay changed paths |
| Generic matching tail retained as Local bytes |Avoided through exact byte/identity proof; actual copying/comparison still paid |
| Reordered immutable Base SaveFile refusal |Internal authenticated v2 handles replacement ranges; v1 grammar preserved |
| Completion depends on spare ordinary headroom |Bounded precharged same-owner escrow; no promise for arbitrary frontier |
| Default Workspace memory Budget |8 MiB retained |
| Hot selected profile |8 cursors,64 nodes,1 MiB retained; not total memory |
| Native owned mutation / ordinary FUSE callback / tiny input |8 MiB /128 KiB /128 bytes retained, respectively |
| Open file/directory handles |128 retained |
| Public held-view leases |32 retained; checked release required |
| Listing/read declared maxima |128 entries /128 KiB; accepted large pinned reads still qualified |
| Canonical direct LogicalPath |4096 bytes /256 components retained; component identity route is distinct |
| File and stream maximum |4 GiB logical /8 GiB save-stream admission retained |
| C1 alias/cycle complete-Commit scans |Remain; parent-index follow-up not implemented |
| Worker / quota / deadline / ARMv8 profile |Unchanged; no extra workers, fsync, third-party patch or timer extension |

No table entry claims an unlimited Workspace file count. The 8192 proof is
**8192 writes/references in its scoped file workload**, not 8192 files. Current
state grows under charged admission; broader many-file/package scale is #256
NOT_PROVED. Pure-move path-local C1 Commit is #270 NOT_PROVED.

### 10.3 Proof map and next integrated verification

| Workflow | Retained proof / proposed benchmark ownership |
| --- | --- |
| Point WRITE/Commit |Complete 3x3 functional/count cohort; `workspace_write` |
| 8192 extension / 10,240 refusal |Separate native final-byte and default Budget custody proofs; not substituted SDK speed rows |
| Generic Base copy / SaveFile v2 |Original 64 MiB lowering and real SDK reordered/duplicated copy; nonmatch/root/pin/G2/auth/framing proofs |
| Occupied completion headroom |Original 2 MiB Stage / 4 MiB Commit, exact funded ownership and failure custody |
| Namespace |Standalone 3-versus-67 local counters; later combined inherited suite; `workspace_namespace` needs its actual integrated source |
| Mixed mutation/old G1/new G2/retirement |Existing scoped external selections; prospective `workspace_mutations` timing family |
| Clean/one-edit retained Commit |Exact registered benchmark controls still NOT_RUN; `workspace_commit` |
| Package/many-file |Existing shell registry; larger scale not proved; `workspace_shell_package` |
| History stride / Init |Separate C1/C2 history and release SDK Init routes; preserve their oracle/profile boundaries |

The [final functional report](../PREMERGE-FUNCTIONAL-COMPLETION-20260929.md)
is pinned to product 11a864fc1 and test/harness a8528b975. It proves selected
functional routes, exact bytes and custody. The standalone 3-versus-67 counts
prove that source's local rename slice, not complete C1 Commit or current
component-only resident paths.

Later combined integration work has a retained targeted inherited-suite pass,
a pending readable-fixture capability correction and broader live SDK routes
that were not executed when the image variable was absent. Those pending edits
remain outside this architecture commit. See the [integration checkpoint](../PHASE45-INTEGRATION-CHECK-20260929.md)
and [merge-planning entry](README.md); this source review supplies no new
all routes functional PASS or merge approval.

Numeric rows remain INELIGIBLE, frozen control NOT_RUN, and additional memory
qualification deferred. Follow the [benchmark plan](BENCHMARKS.md) to freeze
future identities/workloads/methods before executing one selected sample per
case/arm. No physical benchmark, new Cargo build/test or source change was
performed to create this report.

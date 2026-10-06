# Workspace architecture and efficiency analysis

> **Status:** Research; informative and not a product contract.
> Design analysis dated 2026-10-06. Reviewed in the primary checkout on local
> `main` at `1775fdf98`, with the independent-custody S6 working checkpoint still
> uncommitted. Source descriptions, selected requirements and optimization
> hypotheses are distinguished below. This document does not select a new
> algorithm, complete a milestone or establish a measured speedup.

LayerFS needs a complete mutable filesystem over an immutable committed root.
The Workspace must accommodate large files, many small files, sparse ranges,
concurrent commands and repeated incremental Commits. Its efficiency depends on
keeping ordinary operation work related to the requested bytes and affected keys,
while preserving exact ownership of captured and live state.

The selected architecture combines demand-loaded immutable content with indexed
mutable SQLite state, bounded payload cells, short owner jobs and automatic
maintenance. This is a suitable foundation for those workloads. Remaining
construction, import, physical-admission and native-integration restrictions
prevent a claim that the complete system already supports every intended workload.

This document explains the architecture and its costs. It contains design
analysis and optimization opportunities rather than test procedures, benchmark
campaigns or an execution checklist. The primary [design index](../303/README.md)
and operation contracts retain authority; the [implementation progress](PROGRESS.md)
records milestone state.

## Workload and lifecycle

A Workspace includes source, `.git` and its index, ignored files, dependencies,
symlinks, caches and outputs. Git ignore rules do not filter its filesystem state.
Initial full-root acquisition is explicit. Once a complete root exists, repeated
binding must avoid a whole-root scan, copy, dependency reinstall or new database.
Demanded metadata and content still pay their actual acquisition costs.

Per-tool-call and per-task orchestration are both supported by the target
contract. Workspace lifetime, command duration and Commit cadence are independent.
A command can be long-lived in either mode; several commands can share one live
Workspace and observe each other's published changes. Command exit does not
implicitly Commit, unmount or release descendants and descriptors.

```text
One-time root acquisition and runtime readiness
                         |
                         v
                 Bind complete root R0
                         |
          +--------------+--------------+
          |                             |
          v                             v
   Call A / process A            Call B / process B
          |                             |
          +-------- shared live view ---+
                         |
                         v
             Explicit capture and Commit
                         |
                         v
           Install R1, preserve later edits
                         |
               Further calls and Commits
                         |
                         v
               Explicit terminal unmount
                         |
           Logical close and owner release
                         |
           Eligible physical cleanup follows
```

The important scale dimensions are independent. Large dense files primarily
stress content bandwidth and construction. Many small files stress metadata,
directory access and ownership. Sparse files separate logical length from actual
data. Repeated overwrites stress replacement amplification; long lifetimes stress
version composition and reclamation. An efficient design must address each
dimension without treating a small processing window as a total workload ceiling.

## Architecture and responsibility boundaries

```text
             Application / ordinary Bash / FUSE
                              |
                              v
                   Workspace filesystem rules
                  identity, names, attributes,
                   reads, writes and truncation
                              |
                  short typed owner operations
                              |
                              v
                   Fair daemon SQLite owner
            +-----------------------------------+
            | One initialized overlay database  |
            | Workspace-scoped rows and indexes |
            | Names / inodes / payload cells    |
            | Generations / owners / scratch    |
            | Ready maintenance and retirement  |
            +-----------------------------------+
                 |                        |
      stable source/capture          bounded maintenance
                 |                   compose / reclaim
                 v
       Immutable demand and canonical construction
                 |
       authenticated host runtime adapters
                 |
                 v
       Embedded content / storage / history libraries
                 |
       known publication and prepared base install
```

The daemon initializes one local overlay SQLite database before readiness.
Workspace-prefixed keys separate logical state inside that database. SQLite owns
mutable indexes, rows, membership, scratch and cleanup eligibility. Cluster one's
immutable canonical trees remain the content and filesystem format; their public
readers and constructors are reused.

The Workspace layer decides filesystem semantics. It obtains missing immutable
facts outside the SQL owner, then evaluates current local state when publishing.
The daemon owner schedules short typed jobs and retains exact completion custody.
It does not hold SQL ownership across provider I/O, hashing, content construction,
a whole command or a whole Commit. The host embeds storage and history libraries
and supplies the runtime boundary; the retired `layerfs-server` is not revived.

Source folders reflect these responsibilities: Workspace `base/`, `workspace/`,
`mutation/`, `operations/` and `ports/`; overlay `database/`, `namespace/`,
`payload/`, `lifetime/`, `maintenance/`, `diagnostics/` and `contract/`. This
organization makes ownership and dependencies easier to review. Relocation alone
does not reduce runtime work. See [source organization](SOURCE-ORGANIZATION.md).

One shared database also creates a shared writer, pager and failure domain.
Namespace isolation does not provide physical corruption isolation. Fair short
jobs limit how much work one operation contributes to a service turn, but cannot
eliminate device stalls or create unlimited writer throughput.

## Variables for efficiency analysis

The following expressions describe mechanisms and design requirements. They are
not latency estimates or measured process-memory bounds. Indexed costs assume
bounded keys and the actual supporting access path; an index name alone does not
establish bounded filtering or visited-row work.

| Variable | Meaning |
| --- | --- |
| N | Relevant indexed population; specify Workspace-local or shared-database scope |
| M | Requested filesystem operations across a workload |
| K | Affected or returned rows and keys |
| B | Actual bytes supplied, acquired, copied or emitted, distinguished by operation |
| L | Logical file length, including holes |
| F | Prior fragmentation intersecting the operation |
| H | Prior Commit or capture history |
| D | Actual namespace, canonical-tree or dependency depth traversed |
| P | Payload cell size, currently 4,096 bytes |
| C | Cells intersecting one processing window |
| S | Live shrink-staircase height |
| V | Effective payload layers consulted by a read |
| W and Q | Outstanding service lanes and admitted queued work |
| G and R | Eligible cleanup debt and independently retained state |

Point indexed work generally follows `O(log N)`. A suitable covering keyset range
returning K rows follows `O(log N + K)`; independent row fetches can add
`O(K log N)`. Actual payload processing includes B. A full directory listing
necessarily returns its entries, but should not revisit every previous page.

Analyze cumulative work as well as one operation. Scanning N owners on each of
M releases costs `O(MN)` and becomes quadratic when both dimensions grow
together. Likewise, rewriting a growing file prefix on every append is quadratic
copying. Bounded individual buffers do not prevent either pattern.

## Namespace operations and immutable facts

Create, link, unlink, rmdir, rename and attribute changes publish related rows in
one compound owner transaction. A rename changes its source and destination
bindings with the necessary inode and parent effects. Maintained directory counts
avoid discovering emptiness through a whole-directory scan. Stable serials keep
identity independent of path changes and hard-link bindings.

Required immutable facts can be demand-loaded before publication. Current overlay
state is then checked again inside the owner job; a provider wait does not freeze
local mutation or authorize publishing stale assumptions. A directory move still
pays for necessary verified ancestry. Atomicity does not make that topology work
constant or authorize skipping cycle and alias checks.

Serial allocation reserves authority-owned ranges and consumes them locally.
The current refill of 1,024 IDs amortizes reservation calls across new inodes.
It is not a maximum file count or edit count. Refill waits and failures remain
real, and consumed or abandoned identities are not recycled.

Captured enumeration uses a fixed generation/domain and keyset cursor. It must
terminate on that captured input even while active state receives disjoint new
keys. Two versions per key would not alone bound a cursor that scans the growing
active population. See [namespace operations](../../architecture/30-namespace-operations.md)
and [effective views](../../architecture/29-effective-base-view.md).

## Payload writes reads and truncation

### Bounded cells and replacement

One inode has a payload layer in each relevant live generation. A layer records
its inherited cutoff, shrink epoch and staircase. Cells store bytes only through
the last valid byte. A missing validity mask means all stored bytes are valid;
a partial mask distinguishes local bytes, including written zero bytes, from
inherited or hole bytes.

```text
File address space
  0               4096             8192            12288
  |---- cell 0 ----|---- cell 1 ----|---- cell 2 ----|

Write window             [ supplied replacement bytes ]
                          |                         |
                     partial edge              partial edge
                          +-- complete cells -------+

Complete cell: direct upsert
Partial edge:  point read / bounded merge / upsert
Other cells:  no replacement work for this request
```

A fully covered cell needs one upsert without reading its prior payload. Each
partial edge needs a bounded cell read and merge. A 128 KiB unaligned window
intersects at most 33 cells. Its work follows request bytes and intersecting
indexed cells, with logarithmic staircase checks where partial stale cells need
them. Earlier one-byte fragmentation does not require enumerating a historical
extent list for the later overwrite.

Across M writes, all supplied bytes still cost ingestion; repeatedly overwriting
the same range cannot erase the work of earlier accepted requests. Within the
same active generation, replacement updates current cells rather than retaining
one permanent version per write. Captured input retains the older version it
actually needs. The cell size imposes no total edit-count ceiling.

The layout has tradeoffs. Partial writes may read and replace a bounded existing
cell; masks, keys and SQLite pages add overhead. Cell size affects this copy work,
row count, dense allocation and tiny-file representation together. Four KiB is
the selected bounded unit, not an established universally optimal size.

### Composed reads and base demand

```text
One requested byte window
             |
             v
Compose effective local cells and inherited-byte mask
             |
       Any inherited bytes?
         /           \
       no             yes
       |               |
       |       acquire one covering canonical range
       |       outside SQL, under exact source custody
       |               |
       +------- combine selected bytes -------+
                                               |
                                               v
                                          caller output
```

Grouping inherited gaps into one covering range avoids a demand operation for
every invalid byte or small gap. The covering range may acquire bytes that the
final mask does not use. Its canonical traversal can require multiple objects;
one range is not a promise of one object, one network call or zero copy work.

Read work includes the requested window, effective layer composition, indexed
cell/staircase access and actual canonical demand. Root-qualified immutable caches
can reuse objects across base changes without treating mutable paths as cache
identity. Cache retention is optional for correctness; misses pay real I/O.
Request, cache, provider and output custody are separate costs.

### Logical shrink and sparse ranges

Shrink trims its boundary cell, lowers the inherited cutoff and updates an
indexed staircase. It does not enumerate and delete every discarded cell before
replying. Regrow exposes zeros where shrink removed visibility; it must not
resurrect older local or inherited data.

```text
Before shrink       [ visible bytes .................... ]
New EOF                           |
After shrink        [ visible ... ]  logically discarded
                                  |----------------------|
                                           |
                          stale physical cells remain eligible
                                           |
                              bounded maintenance removes them
```

Foreground work follows the boundary and staircase lookup rather than discarded
file length. Eligible physical cleanup still costs the discarded rows and bytes.
Moving that work to maintenance changes scheduling and reply latency, not its
cumulative existence or retained allocation.

Overlay holes have no payload rows. Canonical construction also needs a compact
zero-run path: repeatedly streaming L zero bytes would defeat sparse efficiency.
The existing all-zero CDC period is 32 KiB. Uniform canonical mapping subtrees can
be reused while ordinary boundary alignment and final partition rules preserve
the streamed root. For unfinished-page capacity U and mapping height D, one zero
run uses `O(chunk bytes + U D)` work and `O(D)` object emissions; ordinary data
still costs its actual bytes. See [payload architecture](../../architecture/31-payload-streams.md)
and [zero-run derivation](S5-HOLE-CONTRACT.md).

## Capture Commit and installation

Capture retains existing rows as stable input and routes later mutation to active
state. It does not copy the entire overlay or preconstruct the candidate root.
The capture publication fence orders already admitted mutations and their reply
attempts. This fence has real queue and ownership wait; a small metadata transition
is not a claim of zero capture latency.

```text
Before capture             Live = active A over base R0

After capture              Commit input = captured C over R0
                           Live         = active A over C over R0

During construction        C remains stable
                           A accepts later published mutations

Objects saved             R1's objects are in the global Store
                           Live = active A over C over R0

Known history publication Branch selects R1
                           Live = active A over C over R0

Known local installation  R1 replaces the lower view C over R0
                           Live = active A over R1
                           Later active changes remain effective
```

Commit constructs the captured final filesystem state rather than replaying every
historical syscall. Its work should follow affected content, canonical tree paths
and necessary verified topology/reference changes. Full replacement still pays
for actual input bytes; final-state selection is not permission to omit required
construction, validation, storage or release work.

Prepared installation acquires and checks the next root before the short binding
transition. Known SQL installation and the selected BaseView change are paired,
with no fallible provider work after known SQL success. The constructed/saved/
published root and exact outcome are caller obligations, not inferred from a
pointer swap. See [prepared installation](../../architecture/27-prepared-base-install.md).

### Different storage formats and the same filesystem view

R0 and R1 are immutable canonical filesystem roots. C is the captured local
overlay state; A contains mutations published after capture. The word "over"
describes filesystem precedence and inheritance, including names, metadata,
EOF and truncate rules. It is not a union of SQLite tables or a requirement that
both sides use the same payload format.

The daemon's local SQLite stores cells, validity masks, cutoffs and generation
state. The host's global SQLite Store holds canonical object packs, object
locations and history. Their readers interpret those different representations
and expose bytes at logical file offsets to Workspace composition.

```text
                  Application reads a file window
                               |
                               v
                     Workspace read composition
                       /                  \
                      v                    v
             Local overlay reader   Immutable base reader
             cells / masks /        canonical objects /
             cutoffs / generations  file mappings
                      |                    |
             daemon local SQLite    host global SQLite
                      |                    |
                      +-- logical bytes ---+
                               |
                               v
                    Effective application bytes
```

After installation, valid active bytes in A take precedence. Its size and cutoff
rules also decide EOF and ranges that must read as zeros. Remaining inherited
bytes come from R1 through the canonical reader, using exact immutable cache
entries or actual provider demand. Canonical chunk boundaries need not match
the overlay's 4 KiB cells.

The construction requirement is that R1 faithfully represents C over R0:
captured names, stable inode serials, represented attributes, file lengths and
bytes must agree. Under that requirement, installation preserves this equality
of effective filesystem views:

```text
R1 represents C over R0

Before local installation:  A over C over R0
After local installation:   A over R1

Effective live filesystem:  unchanged by the installation transition
```

For example, all offsets below are zero-based:

| Event | Captured or committed bytes | Effective live bytes |
| --- | --- | --- |
| Initial base R0 | `abcdefghij` | `abcdefghij` |
| Write `XY` at offsets 2 and 3, then capture C | `abXYefghij` | `abXYefghij` |
| During construction, write `ZZ` at offsets 7 and 8 into A | C remains `abXYefghij` | `abXYefgZZj` |
| Save and publish R1 | R1 contains `abXYefghij` | A over C over R0 remains `abXYefgZZj` |
| Install R1 as the base | R1 contains `abXYefghij` | A over R1 remains `abXYefgZZj` |

Before installation, XY is supplied by C and ZZ by A. Afterwards, XY is read
through the canonical R1 base and ZZ still comes from local A. A stays in overlay
format. There is no bulk download of R1 into local cells or conversion of every
active cell. First inherited reads can nevertheless incur provider, decode and
copy work; switching representation does not imply a warm cache.

The inherited cutoff is relative to the lower view. If C shrinks a 1,000-byte
file to 400 bytes and A later grows it to 800 bytes, A preserves a 400-byte
fall-through cutoff. R1 represents the captured 400-byte file. Replacing C over
R0 with R1 keeps bytes 400 through 799 zero unless A writes them; it cannot expose
the discarded bytes from the old 1,000-byte R0 file.

Known local installation advances the base root and installed-generation floor
and pairs that engine transition with the Workspace BaseView change. Ordinary
reads then exclude the installed captured layer. C's physical rows become
eligible for bounded retirement only when their exact owners permit it; retained
captured readers and independent file sources can delay deletion. Short current-
base read windows hold the install fence until release. Inode identity remains
stable across the base change, while open-unlinked files retain their separate
orphan view. See [source windows](../../architecture/28-base-source-windows.md),
[independent custody](../../architecture/33-independent-custody.md) and the
[composed read](../../../crates/layerfs-workspace/src/operations/file/read.rs).

Save completion and Branch publication do not themselves switch the running
Workspace. If publication is known but local installation fails, the operation
retains and reports that exact disposition. It cannot delete C or describe the
live view as A over R1 without the successful paired installation. The complete
Save/history/native composition remains S9/S10 integration work; this explanation
states its invariant rather than claiming complete pipeline qualification.

One pending or unresolved Commit per Workspace bounds capture ownership. Different
Workspaces retain separate operations under shared service. A head conflict or
unknown outcome does not authorize automatic replay or guessed deletion. These
are coordination and correctness restrictions, not lifetime Commit-count limits.

Localized construction still has limitations: deferred draft state, directory
change vectors, new-parent membership and validation/release collections remain
resident in current cluster-one paths. Some checks convert an ordering budget into
population refusals; stored non-file rebinds can require broad topology traversal.
Indexed overlay state does not remove those downstream costs. Their backed,
incremental corrections belong to the [integration prerequisites](../303/06-cluster-one-integration.md).

## Independent ownership and bounded maintenance

Long-lived open files must not retain a namespace layer for every later Commit.
The independent-custody working checkpoint gives an open-unlinked file its own
orphan domain and exact immutable base identity. Later descriptor writes use that
domain. Short file-read sources and sealed captured readers have separate owners.
Copying a capability does not acquire another reference; every use and release
checks its exact identity.

```text
Namespace generation ----- referenced lower input ----+
                                                      |
Last unlink --> independent orphan domain <--- open descriptor
                         |                            |
                    bounded transfer                  |
                         |                            |
             fixed lower inputs are incorporated      |
                         |                            |
             orphan becomes its own current layer     |
                                                      |
Last exact owner release -----------------------------+
                         |
                  targeted eligibility
                         |
                  bounded physical deletion
```

The reviewed checkpoint bounds live namespace payload composition to two layers,
and an orphan to at most three while migrating and one after migration. These are
view-depth bounds, not bounds on total retained bytes: a sealed reader can retain
old input until release. Definite failure composition can wait for that reader;
unknown history disposition retains the original capture instead of entering
definite-failure cleanup.

After a definite fenced failure, a short ownership transition schedules bounded
consolidation. It preserves newer active bytes and names without a foreground
payload-sized merge. Another capture waits for readiness while ordinary mutations
remain runnable. Repeated failures must not add an unbounded read-depth dimension H.

Maintenance processes eligible cells, steps, retired rows and released scratch in
bounded weighted turns, including idle periods. Targeted release updates avoid
collecting every owner on every close. For K transferred or reclaimed records,
cumulative work includes their indexed access, bytes and required stale checks.
It does not become constant merely because each turn is bounded.

If eligible garbage arrives faster than cleanup can process it, debt G grows.
If readers or captures retain data, that belongs to R rather than eligible G.
Admission, protected cleanup capacity and physical allocation accounting must
address both. Freeing rows does not imply the shared database file shrinks. Global
history and immutable-object retention are separate from local overlay cleanup.
The reviewed S6 checkpoint still leaves physical reservation/headroom, exact
whiteout simplification and complete resource/debt obligations unfinished.
See [live composition](../../architecture/32-live-composition.md) and
[independent custody](../../architecture/33-independent-custody.md).

## Whole-system costs and design tradeoffs

End-to-end operation cost includes admission and queue wait, semantic evaluation,
SQL work, base acquisition, authentication/decoding, payload copies and reply
delivery. Commit adds construction, Save completion, history publication and
installation. Logical close and physical reclamation have separate lifetimes.
Optimizing one component can move work into another, so each remains visible in
the architecture.

| Choice | Efficiency rationale | Tradeoff or boundary |
| --- | --- | --- |
| One initialized SQLite database per daemon | Amortizes setup and supplies mutable indexed backing | One writer, shared pager and corruption/failure domain |
| Short atomic owner jobs | Publish consistent metadata/payload while sharing service fairly | Transaction, statement and admission overhead still exists |
| Fixed bounded cells with trimmed masks | Replacement follows intersecting cells; holes stay implicit | Partial-cell copy work, masks, indexes and page overhead |
| Immutable root and object identities | Stable capture input and safe cache identity | Demand acquisition, authentication and canonical traversal remain |
| Capture existing rows plus successor state | Avoids a bulk snapshot copy while later writes continue | Captured owners retain necessary old data and need exact release |
| Logical cutoff plus shrink staircase | Removes discarded-length work from the reply path | Stale data/steps require maintenance and occupy backing meanwhile |
| Independent orphan and reader domains | Prevents descriptor lifetime from growing live generation depth | More explicit capabilities, source references and readiness rules |
| Keyset and generation-selective cursors | Avoids repeated prefix scans and active-population chasing | Appropriate indexes add mutation and storage cost |
| Disposable MEMORY/OFF overlay profile | Avoids durable-store synchronization on local working backing | No crash-survival guarantee; pager/journal and device costs remain |
| Bounded automatic maintenance | Shares foreground/idle service without a whole-payload pause | Progress, debt and protected headroom require explicit accounting |

The single writer makes maximum job work and fair admission especially important.
Scheduler service turns can prevent logical starvation; they cannot guarantee a
device-latency bound. Provider and transport waits need independent capacity and
ownership so a stalled demand does not occupy unrelated SQL service.

Memory analysis includes request buffers, result copies, caches, SQL pager and
rollback journal, transport, native receive/reply state and kernel pages. Logical
credit accounting or a configured pager cache does not establish an aggregate
resident-memory bound. Provider I/O outside SQL still consumes host resources;
maintenance outside the reply still consumes device work.

## Constraints and compatibility

| Constraint | Interpretation |
| --- | --- |
| 4 KiB cells, 128 KiB read/write windows and 1,024-ID refills | Processing/allocation units; no total file, edit or creation count is implied. Oversized direct write operations must be split by their caller |
| One pending/unresolved Commit per Workspace | Coordination rule that bounds captured ownership; successive known Commits remain possible |
| 255-byte names, 4,096-byte canonical paths and 256 path components | Current canonical path-profile boundaries, distinct from indexed parent/serial access |
| UTF-8 names without backslash | Current representation restriction; raw Unix-name compatibility remains an explicit design gap |
| 4,096-byte symlink target without NUL | Existing canonical target grammar |
| File length and positive inode serial through i64::MAX | Active overlay and identity representational bounds, rather than a practical small-file policy |
| 16 MiB canonical object and 8 MiB field | Per-object/field limits; larger files use multiple objects and mapping pages |
| Portable mode/mtime; ctime reported from mtime | Explicit metadata semantics; arbitrary ownership/atime and full POSIX fidelity are not implied |
| EDIT_DEFERRED_LIMIT of 8 MiB minus 1 | Current resident content-edit construction refusal; unrelated to overlay cell size and assigned to backed S10 editing |
| Native Init 4 GiB file limit and symlink refusal | Current import restrictions that prevent faithful acquisition of some intended roots |
| Current global persistence opens on macOS | Host runtime integration boundary; the Linux daemon cannot directly open that provider |
| Native FUSE and logical runtime assembly unfinished | Active library behavior does not establish complete mounted filesystem capability |

Processing bounds and conservative ownership rules are reasonable design choices.
Artificial total-construction refusals and incomplete import fidelity remain
correction work. Merely assigning the sandbox more resources does not change
hardcoded bounds, unsupported names or missing platform integration.

Sources for these boundaries are [filesystem limits](../../../crates/layerfs-content/src/filesystem/limits.rs),
[name grammar](../../../crates/layerfs-content/src/filesystem/path.rs),
[canonical limits](../../../crates/layerfs-content/src/contract/policy.rs),
[portable attributes](../../../crates/layerfs-content/src/filesystem/attributes/portable.rs),
[deferred editing](../../../crates/layerfs-content/src/file/edit/tree.rs),
[native import](../../../crates/layerfs-project/src/import/scan.rs),
[import file ceiling](../../../crates/layerfs-project/src/import/error.rs),
[Workspace writes](../../../crates/layerfs-workspace/src/operations/file/write.rs)
and [provider open](../../../crates/layerfs-persistence/src/store/open.rs).

## Optimization opportunities

The following entries are analysis hypotheses, not selected replacements or
established gains. An implementation change must preserve canonical compatibility,
authority, definite refusal, exact uncertainty and source/consumer custody.

| Opportunity | Mechanism to examine | Correctness and cost implications |
| --- | --- | --- |
| Back remaining construction state | Resident draft/reference maps, directory changes and topology membership | Remove total-state refusals through bounded backed inputs; keep alias/cycle/release semantics |
| Reduce repeated immutable-fact acquisition | Repeated attribute/root decoding and provider demand across semantic rounds | Reuse only exact immutable identities; mutable decisions still use current owner state |
| Reduce partial-cell and output copies | Edge-cell merge, byte binding, local-read clone and downstream output custody | Preserve captured bytes, validity and torn-read prevention; account each owner's buffers |
| Improve hot SQL access | Projection, prepared statements, suitable covering and generation-selective indexes | Added indexes cost writes/storage; consistency and actual visited rows remain load-bearing |
| Refine service shares | Foreground, source, lifecycle and maintenance scheduling | Reduce avoidable waits without starving Saves, releases or cleanup; one writer remains |
| Improve native cache and deferred reply behavior | Repeated FUSE calls, worker occupancy, mmap and teardown collection | Preserve permissions, EOF/truncate coherence, exact lookup/open/reply custody and the selected writeback policy |
| Remove broad topology work for localized changes | Whole-base alias checks and resident touched/parent collections | Supply incremental checked evidence instead of omitting validation or relocating a full scan to bind |
| Complete faithful root acquisition | Import scan state, large-file admission, symlink and raw-name handling | Keep explicit initial acquisition complete; repeated bind must not conceal import work |

Prioritization follows dependency and architectural impact. Backed construction,
faithful import, physical admission and native ownership determine the supported
workload. Copy, query and scheduling refinements then improve that supported path.
Changing P alone cannot repair downstream Commit materialization or a lifetime leak.

The [optimization guide](../../../../docs/general/optimization-guide.md) governs
implementation choices. This document adds no new profile, permission to batch
independent acknowledged mutations into one transaction, competing mutable index
or error-driven algorithm fallback.

## Evolution from earlier phases

| Earlier direction | Principle retained | Current architectural development |
| --- | --- | --- |
| Phase 4.5 active head and packed journal | Workspace-scoped mutation and stable captured generations | Indexed SQL state replaces much custom mutable backing/index machinery; payload and metadata have one short publication boundary |
| Phase 5 bounded streaming Commit and concurrent Workspaces | Final-state construction, bounded processing and preservation of later changes | Embedded public cluster-one libraries and explicit host adapters replace the earlier Server-centered integration proposal; remaining construction limits stay visible |
| Phase 6 shared SQLite metadata and reusable active/frozen overlays | One initialized daemon database, bounded successor state and source lifetime independent of pathname | Concrete payload cells/cutoffs and bounded failure/orphan composition make operation and lifetime costs explicit |

These phases contain mixtures of proposals and scoped implementations. The
comparison explains architectural lineage, not a qualified before/after speed
or storage improvement. Current folder organization improves reviewability;
it does not itself change the algorithms.

Historical references: [Phase 4.5](https://github.com/Ephemeral-AI-Lab/layerfs/issues/273),
[Phase 5](https://github.com/Ephemeral-AI-Lab/layerfs/issues/287),
[Phase 6 metadata](https://github.com/Ephemeral-AI-Lab/layerfs/issues/296) and
[Phase 6 overlay reuse](https://github.com/Ephemeral-AI-Lab/layerfs/issues/298).
Current design decisions and supersessions are recorded in
[decision provenance](../303/08-decisions-provenance.md).

## Relationship to current work

S4 supplies atomic namespace semantics and serial allocation. S5 supplies bounded
payload windows, append, overwrite, truncate/regrow, composed reads and canonical
zero-run construction. S6 has a committed failure-composition foundation plus
the reviewed uncommitted independent-custody checkpoint; it is not complete.
Physical reservation/headroom, exact whiteout simplification and complete
resource/debt obligations remain. Native ownership and full runtime/Commit
integration retain their later milestone scope.

The architecture is designed to support substantial mutable Workspaces without
letting total file size, file count or prior edit history become the cost of every
ordinary request. Its complete support depends on finishing the explicit
construction, import, ownership and integration gaps above. No blanket
constraint-free or end-to-end performance claim follows from this design analysis.

Related documents: [Workspace live maintenance issue](https://github.com/Ephemeral-AI-Lab/layerfs/issues/311),
[current tracker](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307),
[cluster-one handbook](../../../../cluster_one_handbook.md),
[canonical construction handbook](../../../../cas_cdc_deltaencoding_handbook.md)
and [runtime integration](../303/06-cluster-one-integration.md).

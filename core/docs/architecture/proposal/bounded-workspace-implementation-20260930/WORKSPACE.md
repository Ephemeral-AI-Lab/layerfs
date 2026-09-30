# Workspace implementation specification: bounded live backing and Commit

> **Status: Research; informative and not a product contract.**
> Implementation design, 2026-09-30. Current-source authority:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. This packet selects proposed algorithms
> and interfaces; it authorizes no production edits, profile widening, tests,
> measurements, commits, merges or release claims. Only this document was written.

## 1. Decision and scope

Select a generation-native private representation with a paged inode/version
catalog, per-file interval roots, paged namespace/parent facts and paged exact
payload/page custody. Retain small-write packing and immutable Base sharing.
Use borrowed page cursors and bounded split/join construction for live edits,
ordered cursors for Commit and a pre-admitted fixed installation ticket. An
unrelated inode may publish while a fragmented edit is prepared; capture does
not wait for that whole edit. A source version stays pinned until acceptance or
explicit retained failure. Canonical content and History remain their separate
owners; this packet does not move C1/C2 into Workspace.

This is a private-format change, labelled v3, because a complete update slice
against one pooled I/N/D/E/P/R/L index cannot both hide a million-key operation
behind a small publication point and retain its current update interface.
Unchanged page authentication, 4 KiB pages, selected hot/cold fencing and exact
unlink rules should be reused where their grammar fits. Existing v2 owners are explicitly quiesced under their original provider/custody
at the deployment boundary; release or complete them before enabling the concurrent
v3 target for that owner. No new v3 operation traverses old mutable authority or
selects v2 after an error. If retained read-only inspection is required, isolate
a bounded compatibility reader with an explicit removal condition and LOC budget;
it is not a second new-write backend. Never reinterpret v2 backing as v3. New v3
attachment requires an explicit supported capability. No automatic retry or full
payload-copy migration is introduced.

The target covers [SC-01–08](../../../../../scenarios.md). The public route stays
`WorkspaceApi::exec(command)` through ordinary tools and FUSE callbacks. Command
text, filenames, fixture IDs, benchmark shape and application type never select
an alternate publisher. This design applies equally to ordinary SDK/native file
operations, an application copying bytes and a shell doing many writes.

Read with the parent [architecture proposal](../bounded-memory-commit-20260930.md),
[Server packet](SERVER.md) and [concurrency packet](CONCURRENCY.md). The existing
component [content I/O contract](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/content-io.md),
[canonical content boundary](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/canonical-content.md)
and [current active description](../../17-workspace-active-components.md) provide
ownership constraints, not permission to promote historical limits. The cleanup
skill was read as requested; its explicit documentation-only exclusion means its
code-changing/testing workflow was not invoked.

## 2. Exact baseline and replacement points

| Current source / symbol | Actual mechanism | Replacement responsibility |
| --- | --- | --- |
| `filesystem/active_file.rs::publish_active_file_mutation` | Holds State while range publication and private work complete | Admit and pin source, prepare per-inode candidate outside State, then short current-catalog publication |
| `backing/active/extents.rs::ExtentPlan::replace/resize` | Offset-range scan accumulates all affected extents, map and vector | Borrowed range cursor plus immutable split/join candidate; removed subtree custody |
| `backing/active/generation.rs::publish_no_pack/write_with` | Full per-operation updates and dead/compaction lists | Fixed catalog edits and persisted owner/cleanup jobs |
| `backing/active/index.rs::Index::prepare_file/IndexCandidate::publish` | Whole update slice, created/replaced lists, selected root | Bounded node builder and paged candidate custody; O(H) fixed-catalog transaction |
| `backing/active/hot_path.rs::hot_plan/try_hot_file` | Selected EOF/Base/Zero fast path | Preserve admitted restricted fast path against per-file roots; generic route remains exact |
| `backing/active/generation.rs::read_file` | Offset-key lookup; deferred Base/Payload span vector | One selected interval/span at a time, caller-sized data window |
| `runtime/state.rs::frontier_bytes/reserve_nodes` | Future encoded bytes charged to RAM; growing resident identity state | Actual resident leases; paged identity/reference facts and bounded cache |
| `overlay/snapshot.rs::capture_active_submission` | Captures roots/counters but clones declarations; host frozen flag | Fixed capture marker and per-Workspace pending permit; paged declarations |
| `commit/active.rs::scan_dirty/scan_extents/directory_rows/prepare` | Full dirty, file, name, saved-root and prepared arrays | Reusable ordered cursors, persisted result join and framed pull sources |
| `commit/active_reconcile.rs::prepare/reconcile` | Proportional preparation and updates after known publication | Fixed parent/result binding install; cursor retirement |
| `backing/active/lifetime.rs::publish_reconcile_map` | Whole patch conversion and unlimited-source compaction under writer gate | No aggregate reconciliation publication; quota-owned bounded maintenance |
| `backing/active/pages.rs::PageStore`, `retirement.rs::Retirement` | 128 bytes per page, 256 bytes per retired page resident charges | Exact paged ownership, reference edges and failed/retirement continuations |
| `backing/payload.rs::PayloadHost/Record`, `metadata.rs::MetadataHost` | Shared registries/window/writer/count limits | Resource-admitted tagged leases and paged active ownership |
| `filesystem/active_view.rs`, `namespace_view.rs::View` | Local binding plus direct canonical fallback | Immutable full base-view resolver; own orphan and parent facts |
| `commit/completion.rs::CommitAttempt::reserve/installed/resume` | Known/unknown custody with later large local work | READY ticket, known outcome installation, same-selector local completion |

These paths are under `core/crates/layerfs-workspace/src/`. The existing
reference root `crates/` is neither dependency nor runtime fallback.

Current storage = 2 is **offset-keyed E(serial,start)**. The older length-indexed
`metadata_pieces` sequence is not the active baseline. Current `MAX_AFFECTED=128`
is a scan batch, not a total edit cap. Tiny packing applies at <=128 bytes;
larger replacements use owned payloads. Current large acquisitions use aligned
128 KiB windows and 1 MiB segments. File length is capped at 4 GiB; one native
replacement at 8 MiB; FUSE request/read buffers at 128 KiB. These remain unchanged
until a separate arithmetic/profile review qualifies new limits.

## 3. Before and after

```text
BASELINE
  many ordinary callbacks
           |
  State / active writer exclusion
           |
  overlapping E records -> affected Vec -> map -> complete update Vec
           |
  pooled selected I/N/D/E/P/R/L index + resident page/payload/cohort maps
           |
  capture G1; G2 extends shared active authority
           |
  dirty Vec + extent Vec + descriptor bytes + name rows + saved map
           |
  full prepared bytes --> canonical operation
           |
  known Branch result
           |
  PreparedRow/deletion/map/vector/compaction/index work under writer gate
           |
  local successor + checked cleanup
```

```text
TARGET (one construction producer for each admitted operation)
  ordinary callbacks                   independent commands / Workspaces
         |
  byte/quota leases, selected source and conflicting-inode/namespace leases
         |
  per-file split/join or fixed namespace edit candidate
         |     borrowed pages + quota-owned candidate/edge records
         v
  short CURRENT catalog publisher: O(H) page work, one selected revision
         |
  RevisionContext: inode catalog / names / parents / dirty / locations
         |
  fixed capture --> immutable G1          separate live G2 delta
         |                               over exact captured ViewBase
  dirty / extent / name / result cursors   unrelated inode writes continue
         |
  count then replay; fixed descriptor/replacement/namespace frames
         |
  READY: sealed inputs + outcome/install/cleanup custody + protected credits
         |
  exactly one final History Branch operation
         |
  known reply --> fixed resolver/baseline install preserving CURRENT G2 root
         |
  old selections + removed subtree roots --> bounded owner/GC cursors
```

A short logical selection change is O(1) in selected roots. Creating/verifying
a fixed number of catalog paths still costs O(H) pages and may encounter device
latency. A large same-inode operation has a longer conflict lease while it is
prepared. This design guarantees reader/unrelated-inode eligibility, not zero
wait for conflicting writers or constant wall time for a wide syscall.

## 4. Selected private v3 representation

### 4.1 Selected revision and page grammar

A fixed `RevisionContext` contains incarnation, logical revision, current live
generation, canonical Branch/base context, immutable resolver-root ID and roots
for inode/version catalog, namespace bindings, parent facts, current dirty facts,
payload locations and reference owners. It has a bounded pre-admitted root pin;
cloning a selected view never enumerates its descendants.

All private pages remain 4096 bytes. Reuse checked framing/hash/incarnation and
page identity/epoch rules. Add a v3 record discriminator; unsupported kinds,
stale epoch, malformed fence, wrong scope/serial, unexpected parent or bad totals
fail explicitly. Branch/cursor parsing borrows authenticated page bytes instead
of cloning each cell into a Vec. Mutable caches are fixed-capacity performance
state; selected pages/resolvers are authority. Two candidate boundary pages per
level permit deterministic byte-balanced split/join without unlimited child lists.

Proposed records, with final field widths frozen before implementation:

| Record | Key and bounded value | Meaning |
| --- | --- | --- |
| InodeVersion | serial -> immutable version ID, kind/length/mode/mtime/link facts, interval root, provenance/profile | Selected inode semantics, separate from stable serial identity |
| FileInterval | destination start -> end, source kind, source token/offset, checked source length | Final non-overlapping coverage; byte-ordered per-file tree |
| NameBinding | parent serial + component -> serial/kind or tombstone | One selected namespace effect, no aggregate path |
| ParentFact | directory serial -> selected parent/attached state | Paged ancestry/cycle and `..` authority |
| DirtyFact | generation + serial -> selected version ID and roles | Final captured population, not a historical WRITE journal |
| SavedFact | generation + serial + version -> known content/metadata roots, length, completion bits | Exact save observation; persisted before a subsequent fallible call |
| Declaration/OrphanFact | generation + serial + selected version -> declared/live-name/held-owner state | Fresh/unbound/open-unlinked facts absent from a canonical successor |
| PayloadLocation | stable payload/slot ID -> exact pack page/ordinal or segment owner | Versioned location lets compaction avoid editing every file range |
| OwnerFact | owner ID -> kind/epoch/physical identity/allocated/reserved/ref state, edge progress | Exact custody; physically resident data is not inferred from logical length |
| CleanupJob | root/owner + phase + next edge/range + outcome | Fixed descriptor selecting arbitrarily large unfinished work on disk |

The OwnerFact and its containing ledger file both consume quota. Bootstrap custody
retains the exact ledger-file identity and block count in a fixed root descriptor;
ledger growth is pre-admitted and recorded before another owner is exposed.
Do not create recursive in-memory owner tables for the ledger's own pages.

### 4.2 Concrete encoding defaults

Freeze the following v3 encoding as the initial implementation specification,
subject to independent codec/authority review rather than discretionary tuning:

- Keep the current 128-byte page header, 3968-byte body and SHA-256 authenticated
  byte framing. New private kinds/version 3 use bytes 112..128 for checked owner
  scope (catalog kind or file serial plus scope epoch), formerly required zero in
  v2. Magic/version and scope prevent old-provider adoption. The checksum covers
  the same complete page with its checksum field zeroed. Use no new dependency.
- All integer fields are big-endian, checked before arithmetic; flags/reserved
  bytes and unused record padding are zero unless explicitly defined. Private
  PageRef remains 16 bytes (id64, epoch64), scoped by incarnation/header authority.
- FileInterval key is destination_start64. Value is64 bytes: end64; kind8 plus
  seven reserved bytes; source-token32; source_offset64; source_limit64. Canonical
  token is a 32-byte root. Captured token is generation64/serial64/version64/
  selected-context-id64, within the page incarnation. Packed/payload token names
  stable owner-id64/owner-epoch64/slot-or-segment selector64/declared-length64.
  Zero requires zero token/offset/limit. Validate coverage and source bounds.
- InodeVersion values are 224 bytes: version64/generation64/length64; portable
  kind/flags/profile/mode8; seconds64/nanos32/links32; interval PageRef16;
  metadata-root32; construction-Base32; live-parent-token32; edit-policy-digest32;
  construction-fact PageRef16; reference-owner PageRef16. Serial is the catalog key. Layout exposes live source authority separately from canonical construction
  Base and the admitted edit-policy digest.
- NameBinding is 16 bytes; ParentFact 32; DirtyFact 24; SavedFact 104;
  Declaration/OrphanFact 96. Each bounded grammar carries its applicable selected
  version/roles/roots and exact completion/reference flags. A file-set result
  produces SavedFact; grouping/result delivery itself does not change canonical
  identity. The explicitly selected edit policy/profile determines any changed
  FileState or namespace identity.
- OwnerFact 128 contains owner/epoch/kind/phase/flags, birth/retire stamps,
  actual allocated/reserved bytes, device/inode, reference count, outgoing-edge
  PageRef/progress, failure class and fixed reserved space. CleanupJob 96 selects
  a root/owner, phase/next edge, paged path continuation, failure and fund origin.
  Ledger pages own checked fixed-width records. These widths are storage design
  choices, not Rust sizeof/RSS claims.

File-tree child summaries additionally carry checked destination lower/upper
bounds, final interval count and selected provenance summary with the child
PageRef. These are fixed fields, not a list. They permit split/join to carry an
unchanged covered subtree or identify it as a range of captured V without decoding
all its leaves. Validate summaries against child structure during construction
and independently in proof; an unverified cached count is not authority.

The byte widths define maxima for cursor copies, page fanout and physical metadata
admission. Parser tests must reject every inconsistent reserved field, source
scope, record width and edge transition. A format review that changes a width
updates the version/layout specification before dependent product code; no hidden
unversioned layout or codec compression optimization is permitted.

### 4.3 File sources and no recursive WRITE chain

A source is one of canonical file range, exact captured parent-file range, stable
owned payload/pack-slot range, or Zero. Captured sources carry incarnation,
generation, serial, selected file-version identity, selected length and destination
coordinate scope. Canonical roots alone do not replace those authority checks.

FIRST TOUCH in a successor generation starts its file delta with one opaque
`CapturedParentSpan(0, captured_length, exact captured version)` or the equivalent
terminal canonical parent span. A metadata-only capture/drop never expands that
span. Lowering stops at the certified span and emits one Base-range fact rather
than walking its captured private interval leaves.

`SourceCoverage` is a constructor-checked fixed summary: logical length/current
version and generation; current-generation final descriptor count; replacement/
Zero bytes; opaque inherited span count; and checked source bounds. Its counts
compose from the current delta tree plus opaque span records, not the physical
interval counts inside a captured predecessor. Subtree cut/join updates those
summaries by fixed child facts. `physical_inherited_extent_count`, if separately
reported for storage diagnostics, can never enter FileSet extent declaration or
control admission. The file-set lowerer consumes this same authority and verifies
its observed current delta totals at exact EOF. Therefore a predecessor with
millions of final fragments does not force the next one-byte edit to enumerate
those fragments again.

Ordinary successive writes within one live generation splice the existing
persistent interval tree and share untouched immutable subtrees. They do **not**
create an old-file-version edge for every WRITE. At the capture boundary, inherited
unchanged portions may be represented as ranges of the exact frozen file version.
Those ranges are resolved through at most one pending G1 context. Known saved
roots turn ordinary inherited ranges into terminal canonical sources through the
selected immutable binding context. Orphan exceptions hold terminal ranges or
private interval subtrees; an exception cannot reference a chain of older exception
resolvers. Tree descent remains bounded by the declared structural height.

Payload identity is stable while physical location changes. A selected view holds
its original location root; new compaction publishes a new root. An old view never
uses a later live location to authorize a slot. Retained slots cannot be refunded
because newer copies contain equal bytes.

### 4.4 Namespace and selected orphan resolution

`ViewBase = Canonical(root) | CapturedView(frozen context)` covers names,
attributes, link/parent facts and content. G2 starts as an empty delta over G1's
complete effective view. Falling directly back to canonical R0 would lose G1
creates/renames/unlinks. Ordered listing merges a fixed number of binding cursors;
a tombstone overrides inherited names, and a fresh directory has an empty origin.

Names carry stable serials. Lookup/handle/view owners select exact versions.
Fresh open-unlinked files and removed inherited files with selected handles remain
in paged orphan facts. A canonical filesystem root does not locate every such
identity. Normal alias/reference validation stays with C1 before Branch publication.
A live directory move changes only fixed component/parent/catalog facts. An
admitted verified namespace v2 successor checks changed effective ancestors and
matched forward/parent effects; initial/import or unproven-root certification
still requires full correspondence/graph work.

## 5. Live mutation algorithm and syscall ordering

### 5.1 Admission and conflict scopes

Acquire the byte/quota/source leases before publication locks. A file write,
size change or metadata change takes that inode's mutation lease. Namespace
mutations take the Workspace namespace-ordering lease and the affected inode
leases in stable serial order. Read selection takes no mutation lease. A directory
cycle/ancestry walk is paged under the namespace-ordering lease; independent file
content writers may progress while it runs. There is no promise that a deep move
has constant conflict duration.

An already admitted operation waits for a conflicting short/long mutation only
inside its own FUSE/native callback deadline. It gets a precise Deadline/Busy or
resource refusal when required ownership cannot be acquired. There is no hidden
operation replay, root refresh or automatic catch-up rebuild. Cancellation before
publication retains/aborts the candidate explicitly; cancellation after accepted
publication reports the accepted revision and cleanup disposition.

### 5.2 Per-file split/join candidate

1. Under the inode lease, pin the exact selected InodeVersion V and source owner.
   Copy only fixed facts; release State/root locks before acquisition or traversal.
2. Split V's interval tree at write start and end by descent. Carry untouched
   subtrees. Cut at most the two boundary intervals, changing their source offsets
   without copying payload bytes. Append beyond EOF inserts a Zero gap. Shrink
   retains the left tree and records the removed right subtree root for cleanup.
3. Emit replacement interval(s) into a bounded builder and join left/replacement/
   right. Reuse terminal provenance and exact parent span selections. A wide
   delete does not load every deleted extent; cleanup later visits the removed
   subtree through owned continuation.
4. Stage/verify new pages and payload facts; store created/edge/abort custody
   incrementally. Admit root-catalog publisher scratch and required physical
   credits before publication. No created/replaced-page Vec is retained.
5. Enter the short publisher. Check current incarnation, lifecycle and inode
   version == V; the inode lease excludes another update of V. Capture/install
   may have changed the global generation or parent resolver, but cannot change
   V's bytes or discard its source owner. Use the CURRENT catalog/dirty generation,
   apply fixed inode/dirty/location edits and publish a new revision.
6. Release the old root to an owned cleanup job. Projection invalidation is an
   existing ordinary post-publication operation; an error never rolls back bytes.

The global catalog transaction operates on the latest root while holding its
publisher lease, performs only a statically bounded number of key/path changes,
and selects the resulting root once. It is not a compare-and-retry of a detached
whole-Workspace snapshot. Path-copy page I/O O(H) remains inside that bounded
publisher operation. An inode version mismatch under a valid exclusive lease
is corruption/coherence failure, not permission to rebase automatically.

A capture while this candidate is building freezes the last **published** V;
the unaccepted candidate is outside G1. When it later publishes into G2, inherited
shared subtrees are identified against the frozen V using exact root/range
identity. They can become parent ranges without enumerating their descendants.
Boundary pages remain explicit. If this is the first accepted write after a
capture that occurred during preparation, encode its unchanged prefix/suffix as
at most two ranges of the exact frozen V, plus its replacement/gap, rather than
publishing a complete expanded predecessor tree. This conversion uses selected
V/range identity and fixed split/join boundaries; it does not require scanning
all old E records. A changed-size operation uses the same retained-range rule.
Within the same already-mutated generation, retain its actual new delta intervals.
The immutable saved-result binding preserves the
logical selected version V as an alias of its proved equivalent canonical result;
installation must not make an already leased unchanged V appear stale merely by
changing its physical representation. Selected older contexts keep the private
representation, and new live contexts use the result binding. Real accepted inode
mutations still allocate a different logical version. Never substitute R1 offsets
into an R0 span;
if a span was not selected from captured V, preserve its terminal source or use
a paged exception fact. There is no payload-sized normalization pass at install.

### 5.3 Small writes and namespace transactions

Keep the current <=128-byte pack route and restricted selected EOF/Base/Zero
frontier optimization where it proves all affected leaves/fences. The generic
split/join route implements the same semantics; route choice follows actual
selected shape before staging, never failure followed by alternate execution.
Many small files may share a pack tail. Larger small files use owned payloads;
packing does not make per-file metadata or data I/O free.

Create/link/unlink/rename publish fixed name, inode/link, parent and dirty effects
as one selected revision. A replacement detaches the old namespace identity while
its held handles keep their selected contents. Directory moves validate the exact
current ancestor relation under the namespace lease and change no descendant
paths. Two shells may race on one name: each syscall has an explicit ordering and
error; their entire commands are not transactions. A failed/cancelled command
cannot rollback accepted shared G2 state.

Ordinary create consumes an admitted monotone identity-range lease under a
tiny range/accounting lock coordinated with its candidate, not a whole-Workspace
wide-edit sequencer. An initial block of 4,096 with low-water at 1,024 follows SERVER.md as a
proposed generic resource policy; a range is a fixed record, not 4,096 resident IDs.
IDs once
possibly exposed are burned, never reused on a failed local publication. Range
refill is an ordinary permitted C5 operation outside State/publication locks.
Unknown allocator delivery retains its exact reservation custody and is not
reissued. Range refill uses the selected independent protected `C5CatalogPermit`,
not a `C2SavePermit` or a third C2 writer. The Server supplies one short catalog
transaction/control execution class with byte-admitted pending work and qualified
engine headroom. Saturating both C2 Saves therefore need not block ordinary G2
range refill. A finite local range still provides progress during catalog/channel
contention; exhausted credits or a full/expired catalog queue gives precise
allocator resource/deadline refusal before publication. No infinite progress
promise follows from separate connection/counter admission. SERVER.md owns the
64 KiB control ring, 1 MiB first-party catalog work and 4 MiB protected SQLite
headroom within the selected process-wide guard, their qualification and exact
allocation grammar; these Server bytes are not charged twice as Workspace RAM.

## 6. Capture, lowering and pull upload

### 6.1 Fixed capture marker

Before capture, acquire per-Workspace SubmissionLease, fixed capture/attempt/
outcome/failure/installation resources and required physical credits. With the
publisher held, pin the current RevisionContext, freeze its dirty root/counters,
select a new live generation whose base is that complete view and whose dirty
root is empty, and store the pending submission identity. No dirty/name/file
scan or declaration clone occurs. A concurrently prepared inode candidate can
publish afterward under the new generation as specified above.

Only this Workspace's submission slot is held. Other Workspaces use independent
slots and the shared byte/Store admission contracts. Unknown/known-local-failure
keeps the pending slot; local G2 progress remains admitted against its own resources.

### 6.2 Replay contracts

Use concrete private cursors internally; keep the existing Bridge `Source` at
transport boundaries. No public factory/trait for every record or a new scratch
service is needed. A cursor owns immutable selection, declared bounds, selected
page path, current leaf position and a charged output slot. A successful `next`
returns one borrowed record valid until the next pull. Explicit finish checks
ordering, complete interval coverage, counts and exact EOF. It cannot substitute
newer input when a page fails or a deadline expires.

- Dirty traversal is one monotone cursor ordered by serial; inode facts and saved
  results use ordered joins where possible. No full saved-root map is constructed.
- File pass1 validates final coverage, measures extent/replacement counts and
  recognizes a whole unchanged Base. Pass2 emits v2 descriptors into a fixed
  frame; pass3 pulls non-Base bytes from the same immutable intervals. Descriptor
  and replacement phases can use independent cursor positions. This is a fixed
  number of metadata passes, not a one-pass claim.
- Pack-local lookahead remains byte/ref bounded (current 32 KiB/1024 refs is the
  initial preserved profile). It fetches only demanded selected pack pages.
- File-set result observations enter provisional paged SavedFact custody before
  accepting the next result window. One bounded pending observation preserves an
  authenticated received row on local failure without claiming final Save
  completion. A matching unit terminal seals a paged UnitCompletion fact; all rows
  reference that unit instead of being individually rewritten. Workspace joins
  only sealed result units, never a Vec of returned roots.
  Partial/lost-terminal file-set outcomes retain exact request/result-token owners
  under the Server contract; no automatic file replay or inferred completeness.
- Namespace pass1 measures exact directory/name/identity totals. Pass2 emits a
  directory header then each binding and the identity sections into fixed frames.
  A wide directory never becomes one Vec. Fresh/declaration/orphan facts are joined
  from the captured selection rather than resident lists.
- Replay/sort backing is selected from the first record where different key order
  requires it. It is not an error-driven overflow fallback. Fixed-fan-in external
  merge is used for genuinely different orders; existing ordered inputs stay
  cursor joins. Actual input/output runs remain charged until checked release.

The standalone SaveFile v2 API remains an independent supported compatibility
boundary. The Workspace target uses the generic admitted file-set construction
stream for one or many files; it does not loop per-file SaveFile RPCs or select a
different route by file count. Existing v2 descriptor semantics can be nested
inside a file-set file record where exact construction compatibility permits. Zero is
metadata-only in live backing but still transmitted as replacement bytes by
current v2; implicit-zero transport is a separate versioned optimization.
No recursive daemon ReadFile is issued while upload holds a transport session.

### 6.3 Generic one/many file-set boundary

Prepare one logical `FileSetSource` and owner over the captured dirty
non-directory population. It advances deterministic natural construction units
under the same FileSetConstruct v1 grammar/assembler for one or many members.
Each unit request identifies Workspace/incarnation, generation, captured
selection, exact member count/totals and admitted object/byte policy. Each file header
carries serial, selected version, length/kind/portable metadata, construction
profile/Base/recipe facts, and descriptor/replacement totals. Symlink target and
portable metadata are ordinary typed file-set units; directory attribute roles
remain in the ordered namespace section. No shell text or scenario ID enters
this boundary.

The producer advances one file/header/descriptor/replacement unit at a time.
The Server admits each natural unit through the one generic construction
operation and returns bounded provisional result observations plus a sealed
paged unit-result capability. Normal coalescing targets <=512 members and
<=4 MiB total encoded member body; a larger singleton remains the same assembler
with ordinary bounded byte pulls and its supported larger body envelope. A one-file set has identical semantics and resource policy; a unit byte target
is distinct from a transport-window bound and a total file-size ceiling. It is not an alternate bulk loader. C1/C2 output units fit both
object count and byte capacity; no unbounded object/root queue crosses the boundary.
Source count/replay passes and actual payload reads remain charged to Commit.

A proposed 96-byte result row carries member token8, serial8, content/metadata
roots32 each, length8 and flags8; the member token joins to the captured selected
version. Workspace persists it provisionally or retains its single bounded
pending observation on local failure. UnitCompletion records terminal selector/
count/schema/bytes/digest and finished disposition. Server partial construction,
object visibility, unit completion and full file-set coverage are distinct.
Final namespace/Branch preparation joins only exact sealed unit tokens under
the captured full file-set identity and verifies complete member coverage. Large
files continue through ordinary byte windows of the same assembler; they do not
switch to a standalone SaveFile RPC because one file exceeds the coalescing
target. Unit size is backpressure/coalescing policy, distinct from total file
length. Unit count grows with real selected work; no lifetime counter or whole
selection result array is introduced.
Missing/forged/mismatched results fail before final Branch send. A lost file-set
terminal keeps uncertain request/token ownership; it does not authorize resending
individual files or synthesizing an expected result from the candidate trace.

The independent existing SaveFile v2 operation remains available at its API
compatibility boundary; it neither defines a special one-file Workspace route
nor supplies a fallback when file-set admission fails. Namespace encoding and
final composite History Commit stay one final Branch attempt with no hidden
Stage+CommitStaged pair. SERVER.md owns the exact file-set/frame/result grammar
and partial-result custody; CONCURRENCY.md owns simultaneous channel leases.

## 7. Generation provenance, canonical compatibility and pins

Live ParentFileVersion coordinates are the frozen file's destination offsets.
They are not today's I.base offsets. Example: R0=`abcdef`; captured G1 may select
`abXXcdef` through a supported edit/copy route. R0 offset2 selects`cd`; captured
G1 offset2 selects`XX`. Replacing an old I.base with R1 is therefore unsafe.
Our source token identifies which coordinate system every span uses before Save.

After known publication, saved file roots bind exactly the frozen file versions
in an immutable paged resolver. Selected old views retain their earlier private
resolver/location roots. Installation changes live resolver/baseline ownership,
not the meaning of an old pin. G2's then-current orphan facts remain in its own
catalog; READY never freezes an exhaustive future G2 exception population.

Canonical construction provenance is a separate concern from live byte provenance.
Select **file-edit-policy v2: immediate-selected-parent** for the replacement
end-state. A successor file's ordinary construction Base is the known canonical
result of its exact captured predecessor version. While G1 is pending, G2 retains
that predecessor's destination coordinates; known-result binding resolves the
one pending edge. Dirty captured orphan/exception versions needed as later range
sources also receive known canonical roots through the generic file-set pipeline.
Their exception ledger owns any extra space. The target never retains R0 merely
to replay old deltas or reproduce a legacy construction recipe, and does not copy
the whole file to collapse lineage.

Reuse the C1 split/join machinery and frozen replacement-only CDC over immediate
R1. Untouched R1 subtrees retain IDs. Changed chunk boundaries/mapping roots can
differ from a legacy v1 operation based on R0. The v2 namespace profile binds
this edit-policy digest. Changed chunked results use **FileState grammar v2**,
carrying the existing CDC digest plus a 32-byte edit-policy digest; mapping/chunk
byte grammars remain unchanged. Unmodified/no-op legacy file roots and
scope-independent symlink/attribute roots remain reusable/readable where their
formats permit. Whole-file objects retain their existing byte-defined identity.

The selected target guarantees exact semantic bytes and **independently specified
v2 roots/partitions**, not equality with the legacy v1 expected-root ledger.
Freeze the FileState v2 layout/policy digest and independent reference ledger
before enabling it. A v2 implementation is compared to that ledger; it cannot
relabel prior receipts, use a candidate as its own oracle, downgrade after an
error or interpret a serial as source-version authority.

Exact legacy v1 split/join/CDC partitions with paged recipes are an explicit
bounded-memory compatibility milestone/surface. They are not the concurrent
end-state and cannot satisfy the final-delta gate through cumulative old-delta
replay. V1 remains v1 under its declared scope; admitted new v2 Stacks always
select the v2 policy. Existing independent SaveFile v2 wire naming is unrelated
to these canonical policy versions; its compatibility semantics remain explicitly
negotiated rather than inferred from the numeric suffix.

### 7.1 Namespace v2 trust and explicit migration

**Store precondition:** complete the explicitly quiesced C2 schema10-to11
metadata-only migration specified in SERVER.md before advertising v2 parent-role
support. Copy/compare locator rows in bounded indexed batches, keep pack BLOBs,
ordinals and content identities unchanged, switch verified tables, retire old
locator rows in bounded transactions and publish the version only after exact
cleanup. All legacy uncooperative users must already be stopped; a new maintenance
lock cannot retroactively exclude them. Unknown maintenance acknowledgement keeps
admission closed and exact custody. No automatic open-time upgrade, v10 fallback,
payload recompression or whole-model copy is introduced. Locator-table temporary
space and SQLite freelist/physical high-water remain measured costs, not a guessed
block refund. This Store metadata transition is separate from the v1-snapshot to
new-v2-Stack/new-scope semantic import below.

Admit the target against **namespace profile v2**, whose filesystem root includes
an authenticated parent-tree root and binds parent grammar plus edit-policy v2.
Workspace's paged ParentFacts derive from that exact selected relation and its
own accepted changes; a loose reverse cache or caller-provided validated flag
cannot authorize locality-sensitive mutation. Complete batch correspondence,
parent/alias rules, scope/type/root facts and effective-final ancestry belong to
Server/C1's private-constructor `VerifiedNamespace` capability.

An accepted verified Base plus checked changes yields an inductively verified
successor. The semantic proof lease names exact filesystem/inode/parent roots,
profile, scope, trusted validation origin and Server trust epoch. Its live bytes
are admitted; a reconstructible proof cache is bounded/evictable and does not
retain every historical root. The transported capability is the Server's
principal-bound `NamespaceValidityLease`, with opaque ID, Store/scope/root/profile
and issuer epoch; it is not a serialized private C1 VerifiedNamespace object. Workspace acquires required proof authority before
mutation/resource locks. Authentication of object bytes or an opaque C5 root
alone does not create this semantic capability.

After Server process restart/trust-epoch reset, an opaque opened root requires
explicit bounded full correspondence/graph certification **before mutation**.
Retain that admitted proof for the open root; subsequent accepted successors
carry its inductive origin. Do not silently recertify/retry after an incremental
operation has begun. Read-only canonical acquisition need not hold a semantic
mutation capability. Recertification is real O(namespace) work and is not free
measurement setup, crash recovery or adoption of an unknown Commit outcome.

Select migration by explicit import of a selected v1 snapshot into a **new v2
Stack/new scope**. Server/C5 authorizes source/destination, counts selected
identities, reserves new disjoint serial ranges and builds a paged old-to-new
serial map. Stream remapped bindings/inodes/parent relation; reuse scope-independent
file/symlink/attribute roots. Forward directories, inode table, filesystem root
and new Commit IDs change. Initialize the new Stack only after certification and
C2 completion with exact known/unknown initialization custody. Original v1 history
stays intact. No in-place mixed-profile lineage, automatic owner conversion or
whole-payload migration is introduced. Private backing v3 and canonical namespace/
file-policy v2 are separate negotiated dimensions.

One pending unresolved generation is allowed per Workspace. Ordinary resolved
sources terminate at canonical roots; orphan exceptions terminate at owned ranges/
subtrees. Before a dependent capture, the selected resolver must certify this
bounded depth. Certification is a fixed manifest property plus paged exact facts,
not a whole-live-frontier normalization under the installation gate. Old detached
pinned trees may remain on disk; they do not extend the current resolver chain.

## 8. READY and the exact outcome state machine

```text
LIVE -> CAPTURED -> PREPARING -> READY -> BRANCH_SENT
           |           |                 |       |
      pre-command local/save failure   Unknown    known checked outcome
           |                             |       |
   retained exact saved facts            |       v
                                  retained G1   INSTALLABLE
                                  + live G2      |
                                          fixed result/context slot write
                                                 |
                              fixed current-parent/baseline selection install
                                                 |
                                          INSTALLED
                                                 |
                           checked bounded cleanup -> COMPLETE / RETAINED_CLEANUP
```

READY requires sealed captured inputs, saved/exception roots, reply validation
context, exact declared shape and the following pre-admitted resources:

- next Branch/resolver context and fixed outcome/install/failure slots, including
  a fixed slot for the returned epoch-bound successor NamespaceValidityLease;
- publication scratch and protected fixed physical installation credits;
- custody selecting all unfinished captured/candidate/cleanup roots;
- sufficient fixed cursor/ledger space to stop on failure while preserving that
  root and continuation, without an expanding failure vector;
- any new completion/retirement backing not covered by those roots, including its
  physical quota, before Branch send. This reservation is disk, not fictitious
  resident encoded bytes.

Server also pre-admits the candidate's successor NamespaceValidityLease record,
terminal/capability bytes and response owner before its final Branch CAS. C1's
verified candidate becomes a live principal-bound lease without a post-CAS
registry growth attempt. Failed/unknown reply keeps its exact proof/result owner;
an issuer-epoch reset invalidates the transport token and requires explicit
certification before a new mutation, never guessed token restoration. This is
the Server side of READY while the filesystem root is unknown to Workspace.

G2 may not consume these protected resources. No F/E/name/retirement-sized RAM
or predictable required physical allocation remains after Branch send. Post-send
maintenance can reuse pre-admitted fixed workspace and stop with exact root
custody; it cannot require new unbounded completion records to report failure.

A checked known outcome is recorded before local installation. The fixed install
slot is written/verified and selects submission, known head/root, sealed result/
exception roots, current live G2 catalog root, baseline epoch and cleanup job. Under
the publisher, validate exact pending submission and Branch lineage, retain the
CURRENT G2 root and change parent/baseline selection. Invalidate cache by epoch;
never iterate all cached/dirty nodes under that gate. Old contexts become owned
cleanup roots without recursively destroying graphs in a lock-held Drop.

| Outcome | Allowed continuation |
| --- | --- |
| Before Branch operation | Retain known Save observations and candidate owner; no assumed Branch result |
| Sent command with lost/invalid terminal | Unknown; retain G1/G2 dependencies and pending slot; no resend/query-based guessed adoption |
| Known canonical/local failure before selection | Same-selector explicit completion of local slot/install only; no second Branch publication |
| Selection accepted then cleanup fails | Report installed revision plus separate retained cleanup; no rollback fiction |
| Complete checked cleanup | Release exact owners and credits; pending slot and command leases follow their own terminal policy |

Preparation uncertainty and Branch uncertainty are separate facts. A lost
file-set terminal before Branch send retains unknown construction/result custody
and a known fact that this attempt sent no Branch command. It is not a guessed
head result; another authorized Workspace may still change a shared Branch.
A lost final Branch terminal retains genuinely unknown publication outcome.
Status/failure records include operation phase, exact sent selector and each known
save/install fact rather than one ambiguous boolean.

I/O, locking or authentication can still fail after canonical publication. READY
removes predictable proportional admission failures; it does not promise infallible
storage. Keep ordinary composite Commit as one public operation/one final Branch
attempt; do not insert hidden Stage+CommitStaged calls. C2 candidate visibility,
known saved objects and Branch-head publication remain distinct boundaries.

Default public Commit completion retains its existing required cleanup scope.
If cleanup cannot finish within its admitted policy, expose installed result plus
typed retained cleanup through the existing failure model. A future logical-only
success with pending maintenance would need an explicit API/profile change; do
not move current required work outside a benchmark timer.

## 9. Exact owners, retirement and bounded reclaim

### 9.1 Paged owner authority

Use the existing `backing/ownership.rs` checked ledger mechanics as a foundation,
not its 64-byte grammar unchanged. Internal active catalogs store owner keys, not
an Arc/Record for every payload/page. A public OwnedPayload/View/handle owns a
charged small capability referring to its exact owner; user-held capability count
is independently admitted. Authority remains paged even when a cache entry is
evicted. Fixed allocator metadata and any ledger-file identity lists are paged too.

Creation phases are reserved -> exact native identity -> actual allocation ->
verified bytes -> reference edges admitted -> selectable. At each phase a fixed
candidate descriptor and paged owner state retain exact progress. Child references
are increased before a new parent becomes selectable; partial edge setup records
its next edge. Failure never exposes an incomplete page as an ordinary view.

Only immutable page/reference edges and selected root owners control reachability.
Pinning adds one selected root owner, not a per-descendant resident counter array.
After replacing a root, decrement its root owner and walk newly unreachable child
edges one page at a time. A shared subtree remains referenced. Unknown physical
identity/blocks or a failed unlink keeps its exact record and stops/refuses the
applicable authority; no adoption or refund on a guess. Shared-accounting integrity
failure may quarantine the shared host; ordinary per-Workspace capacity does not.

### 9.2 Maintenance algorithm

A CleanupJob pins a removed subtree/generation root and stores phase, next edge/
owner position and failure disposition. Use a paged traversal stack; namespace
depth and number of removed owners never become a resident stack/list. Decrement
one bounded page's outgoing edges, verify last-owner status, perform identity and
allocated-block checks, unlink, then transfer credit to the exact reservation
origin. Record completed progress before changing the job cursor. Stop on first
unknown/failure and retain the remaining root; do not collect every failure in RAM.

Pack compaction uses one bounded pack source/window and paged live-slot references.
Stable slot/location indirection avoids rewriting every referencing file interval.
Publish the new exact location root once; old pins keep old locations. Useful live
bytes, actual read/write/refund work and temporary overlap are accounted. Pressure
does not trigger an unlimited full-page/population plan. Compaction is a selected
maintenance policy with bounded steps; it is not a codec/storage gate campaign.

No extra helper construction worker is needed. The owning operation or admitted
maintenance operation performs bounded steps. A backlog consumes physical quota
and counted continuation state. Admission can refuse new growth when declared
maintenance headroom is exhausted. Whole-operation cleanup work remains O(owners
visited plus actual relocation/index/I/O work), even with a small install gate.

## 10. Resource and memory contracts

Admission is hierarchical: global host resources -> tagged Workspace accounts ->
operation/handle/view leases. Count private memory, physical quota and declared
shape separately. Current `frontier_bytes`' future encoded-input reservation is
removed; exact encoded totals remain checked shape/wire policy, not RAM.

| Owner | Resident capacity/lifetime | Physical accounting | Refusal |
| --- | --- | --- | --- |
| Attached Workspace | Fixed base/lifecycle/reference controls plus bounded cache; all live idle owners counted | Backing identity/root metadata | Precise Workspace-count/global-base capacity |
| Live mutation | Borrowed input window, bounded split/join paths, node builder, fixed publisher state | Actual payload, new pages, edge/abort facts, old selected roots | Callback-deadline/conflict/Workspace/global quota |
| Submission | Fixed capture/result/failure/READY owner; exclusive pull working lease | Captured data, saved facts, necessary ordering/replay and protected completion | Pre-Branch shape/resource or retained exact disposition |
| View/handle | Small charged capability and selected context; paged issued/reference/cookie facts | Actual unique pinned versions | Explicit admitted owner/resource ceiling |
| Cleanup | Fixed worker window and paged continuation | Unreleased owners, runs, failed/unknown facts | Exact retained failure/backlog admission |

For page width p = 4096, declared maximum height H, fixed maximum simultaneously
held paths c, fixed merge fan-in k and admitted data windows W:

```text
M_mutation <= c*(H+1)*p + two-builder-pages-per-level
            + bounded record slots + payload window + publisher scratch
M_commit   <= dirty/result/name/extent cursor paths
            + descriptor frame + replacement/pack windows
            + fixed-fan-in ordering buffers + fixed READY/outcome state
M_cleanup  <= bounded owner/edge/pack windows + fixed cursor/job controls
M_host     <= shared caches/control + SUM(all live Workspace base/reference state)
            + SUM(admitted actual phase working leases)
            + SUM(protected retained completion allowances)
```

Different phase terms are maxed or summed according to actual overlap, not added
as if every scratch buffer exists forever. Borrowed node parsing uses fixed page
buffers and bounded current-record copies; encoded and decoded/allocator overhead
is admitted before growth. The final constant envelope is derived from compiled
layouts and exact coexisting windows before implementation qualification; no
invented 1 MiB post-implementation PASS is claimed here. Preserve the default 8 MiB
host accounting target initially. Server/engine/codec and child RSS are separate
resource domains composed in sibling packets.

```text
D_private = live accepted payload + index/owner/reference pages
          + captured/pinned unique old data
          + candidate/replay/sort input-and-output overlap
          + saved/orphan facts + unfinished cleanup custody
```

Disk can grow with actual changes and retained versions. Every new ledger/run/
index page is counted, including the metadata needed to free it. Fixed-fan-in
merges release checked input runs only after their output/continuation is owned.
The same unchanged payload is not recopied merely to bound metadata memory.

Use qualified private direct-I/O backing/window enforcement where required.
Buffered spools whose file cache grows with input size do not satisfy this
contract. Host-independent logical behavior and explicit platform capabilities
remain separate; unsupported required enforcement fails explicitly. No APFS-specific
format, fsync/fdatasync/sync_all/sync_data, WAL, crash-recovery promise or third-party
patch is introduced. Memory advice alone is not a complete residency proof.

## 11. Time, I/O and space comparison

These are proposed algorithmic costs, not measured times. F is captured dirty
identities; E surviving extents; K changed bindings; A affected live intervals;
S actual replacement bytes (including current Zero wire bytes); H index height;
U owners requiring retirement; L compact facts requiring another key order.

| Operation | Baseline cost/peak mechanism | Target work and peak |
| --- | --- | --- |
| Tiny overwrite/append | Restricted hot path or range scan, complete small map; physical page staging | Preserve fixed hot shape; generic O(H + changed boundary pages) with fixed windows and real I/O |
| Wide overwrite/shrink | O(A) affected Vec/map/vector plus index/retirement work | Split/join carries removed subtree roots; bounded boundary paths, then actual O(U) cleanup with paged state |
| Capture | Fixed roots plus declaration clone and future frontier accounting | Fixed selected context/generation marker; actual pin/slot I/O counted |
| File lowering | O(E) extent storage plus24E descriptors | Fixed metadata passes O(E) plus demanded replacement reads; fixed windows |
| Namespace lowering | Dirty/saved/names/prepared population resident | Count/replay/join O(F+K) plus actual page seeks; fixed windows and necessary facts on disk |
| Known install | Full reconcile patch/map/vector/compaction/index; writer hold | Fixed marker/parent/baseline selection; O(H) bounded catalog I/O, then actual cleanup |
| Owner bookkeeping |128 bytes per active page +256 per retired page source charges | Paged authority O(owner records) disk; bounded owner cache and exact root/reference work |
| Differently keyed facts | RAM maps/sets or current limited fallback | Selected external ordering O(L log L) comparison work / fixed-fan-in passes; O(L) successful run backing |

The split/join bound depends on a correct range tree that can carry subtrees and
reclaim their edges later. It is not obtained by streaming a million update keys
through the current slice-based index interface. Canonical construction and semantic proof costs remain in SERVER.md. Verified
namespace v2 sparse moves pay changed effective ancestors/index paths and matched
effects; initial/import/restart certification of an unproven root pays the whole
namespace, and recursive release pays actual removed descendants. No unconditional linear/constant Commit claim follows.

A fresh/bulk payload is read from its source and written into private backing by
ordinary acquisition, then read/transmitted for construction: those are real
passes. A sparse inherited overwrite reads the affected/boundary canonical data
needed by the chosen C1 algorithm, not automatically the entire model. A shell
prepending through cat/temp/mv still writes the full new file. File count alone
does not determine data passes; surviving extents, ownership and graph proof do.

Metadata externalization adds ledger/run I/O; borrowed cursors reduce duplicate
arrays/reseeks; selected small installation reduces long writer holds. Their actual
speed/storage trade must be measured later. Keep the accepted up-to10% retained
history storage tolerance and reject approximately 50% speed loss for 5% storage
benefit. Do not retune codecs, page size, workers or quotas to conceal a miss.

### 11.1 Quadratic exclusion and accumulated-work proof

The latest [#276 memory checkpoint](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5905916098),
[10,240 deferral](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5902583820)
and [270-component diagnosis](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903016828)
are evidence authorities. The first two distinguish final fragments/files/names
from raw WRITE count and Budget charges from RSS. The third derives
270+269+...+1=36,585 repeated directory visits for that specific old validator
shape; it is not an instrumented universal timing law. Current post-page-credit
10,240 success remains unknown/owner-deferred. This specification runs no new
measurement and makes no blanket claim that all current paths are quadratic.

Let n accepted mutations have structural heights H_i, changed syscall facts k_i,
actual source/replacement byte work s_i and actual charged ownership cleanup u_i.
Let E_j/F_j/K_j denote the final generation-delta intervals/identities/bindings
processed by Commit j, not all historic file runs, and M_j canonical index/mapping
page work. Full validation/import/restart certification and actual released
namespace descendants have their own G_j/U_j lower bounds.

```text
T_live(n) = O(SUM_i(H_i + k_i + s_i/p + u_i))
            + actual admitted namespace-ancestor/order work

T_commits = SUM_j O(E_j + F_j + K_j + required payload/boundary bytes + M_j)
            + actual external ordering/validated-ancestor/certification costs
            + actual cleanup/relocation work
```

These are selected algorithmic obligations, not new measured bounds. Constant
page/record widths and declared height make the resident frontier bounded.
O(nH) does not mean universal O(n): H, bytes, required affected/releases,
namespace ancestors, sorting and physical latency must remain visible.

| Schedule | Target accumulated work | Excluded amplification |
| --- | --- | --- |
| n small appends in one generation | O(SUM H_i + bytes + final E + actual cleanup) before/through one Commit; final E may be n | Reading/rebuilding the entire growing prefix or reseeking once for every emitted byte |
| n disjoint small overwrites | O(SUM H_i + actual bytes/boundaries/cleanup), then one fixed-pass O(E_n) lowering | Whole-file/all-extents/full owner-registry walk after each accepted write |
| n rewrites of the same small region | Final delta can stay small; O(SUM H_i + bytes + replaced-owner work) | Retaining and replaying every historical syscall or growing version-resolution chain |
| One edit followed by Commit, repeated n times | Each new delta starts from an opaque immediate parent; O(SUM(E_j+F_j+K_j+M_j+actual bytes/cleanup)) | E_j includes all previous Commit deltas, producing1+2+...+n work |
| One wide overwrite/shrink after heavy fragmentation | Boundary split/join plus actual eventual removed-owner traversal | Full affected Vec/map at admission or repeated scanning of unrelated owner populations |
| Repeated status/control during growth | Fixed aggregate counters/selected fixed summaries; paged enumeration only when explicitly requested | Status walking every page/payload after each write, then being mistaken for harmless observation |

An operation that truly consumes/deletes/revalidates a growing population can
legitimately have SUM A_i or SUM U_i work. For example, repeatedly requesting
full output of sizes1..n necessarily outputs Theta(n^2) bytes. Distinguish that
requested lower bound from gratuitous full-population control/rebuild work.

**Required mechanisms that prevent the gratuitous sum:**

1. `nodes::cursor` keeps selected branch stack, leaf position and exclusive next
   key. A completed leaf is not root-reseeked per record/output byte. A monotone
   complete pass costs opening O(H) plus visited pages/records. Deliberate second/
   third passes reopen once each; independent inode lookup may cost O(H) unless
   an ordered join shares it. Baseline128-row `scan` restarts a Resolver, giving
   O(ceil(E/128)*H + visited-page/record work), not automatically Theta(E^2).
2. Split/join carries untouched and removed subtrees. Node created/replaced facts
   are written incrementally. No all-file normalization, locator sort or global
   ownership census runs on each mutation. Hot-slot normalization visits only
   admitted current paths and its fixed hot closure; page-cause/visit proof must
   reject representation-only sweeps of untouched forests.
3. On FIRST TOUCH after each capture, predecessor data is an opaque ParentSpan.
   Fixed-pass lowering counts current delta only. Known-result binding terminates
   at the immediate selected file root; orphan origins are terminal canonical or
   private subtrees. Current resolver chain cannot grow with completed Commits.
4. Quota availability reads maintained allocated/reserved/uncertain counters,
   not status enumeration. Baseline `PageStore::remaining_quota` calls
   `PayloadHost::status`, whose record loop is O(payload population). If repeatedly
   invoked as that population grows it can create the forbidden growing-sum
   pattern; this is a source-supported conditional risk, not a measured universal
   WRITE curve. Target admission/statistics update counters on exact owner-phase
   transitions. A deliberate full ownership audit is a paged, separately labelled
   operation and is not hidden in the hot quota path.
5. Release/collect updates keyed reference facts and only the changed ancestor
   retention path, with paged continuation. Baseline `State::collect` scans all
   resident nodes, walks retained directory ancestry and rebuilds its index; its
   cost depends on the actually retained population/depth. The target does not
   invoke a full resident-table rebuild for every handle close. Shared ancestor
   retention uses exact local reference counts; no repeated suffix walk over all
   held directories is needed.
6. Cleanup resumes from a persisted owner/root/edge position. It never restarts
   the oldest registry or all locator keys whenever one byte of quota is needed.
   A failed/unknown owner stops its exact job with charged custody; it cannot
   trigger an automatic global rescan/retry. Unique actual release/relocation
   work remains accounted, and pins can cause a deliberately charged later visit.
7. Namespace v2 validated-publication provenance permits incremental changed-
   ancestor checks and matching parent/forward effects. Distinct effective
   ancestors use exact operation-local completed state, paged when needed. It
   does not rewalk each overlapping descendant suffix per changed binding.
   Complete initial/import/epoch certification stays O(actual graph plus external
   costs); recursively deleting real descendants still pays them.

**Prospective count diagnostics, never additional speed samples:** record accepted
mutations n, final E/F/K per generation, live affected/cut/carried subtree counts,
root seeks, cursor leaf advances/reopens, borrowed page loads, created/replaced/
representation-only pages, hot-closure nodes, exact owner transitions/audit visits,
retired/refunded/failed-job steps, current and maximum resolver depth, canonical
predecessor root per successive Commit, descriptor counts and replacement bytes.
Also record status/quota aggregate reads versus explicit enumeration, retained
reference/ancestor visits, normalization pages and completed-prefix bytes reread.
Counters are bounded aggregate telemetry, not one heap trace node per mutation.

Freeze append/disjoint/same-region/repeated-Commit axes separately, then diagnose
cause from the one registered performance receipt or a labelled count instrument.
Reject an unexplained prefix sum or a count proportional to all older owners on
one new operation; do not rerun unchanged arms or invent an asymptotic claim from
a few wall times. All cleanup/proof counts retain their actual lower-bound scope.

## 12. Modules, responsibility and physical-line budgets

Retain the `layerfs-workspace` package. Internal types are concrete; use existing
Bridge Source/Delivery and the genuine private provider/ordering boundary. Avoid
service locators, a new manager crate, trait-per-field factories or a second runtime.
Each listed budget is a planning ceiling, **physical lines**, not production LOC.
All must be <=999; entry `lib.rs`/`mod.rs` <=200 and declaration/delegation only.

```text
layerfs-workspace/src/
  lib.rs                         <=120  public routing/reexports only
  runtime/
    leases.rs                    <=420  Workspace/operation byte ownership
    references.rs                <=420  paged held-reference/cache admission
    state.rs                     <=500  fixed current context + lifecycle state
    host.rs                      <=650  global admission + exact registry
  backing/active/
    mod.rs                       <=120  declarations/reexports
    format.rs                    <=420  v3 records, checked bounds/discriminants
    page.rs                      <=420  authenticated4 KiB physical page framing
    nodes/
      mod.rs                     <=80   declarations
      parse.rs                   <=420  borrowed node checks, no full-cell clone
      cursor.rs                  <=500  selected monotone cursor/path
      split_join.rs              <=700  bounded structural edits/subtree carry
      builder.rs                 <=480  ordered leaf/branch emission
    files/
      mod.rs                     <=80   declarations
      sources.rs                 <=480  terminal/captured range authority
      intervals.rs               <=420  file range coverage/cuts/coalescing
      mutation.rs                <=650  per-inode candidate and publication facts
      reader.rs                  <=480  selected span cursor/payload demands
      hot.rs                     <=600  proved tiny selected-shape optimization
    catalog/
      mod.rs                     <=80   declarations
      inodes.rs                  <=420  version/dirty/result lookup and join
      names.rs                   <=450  binding/parent/alias/orphan facts
      publication.rs             <=550  fixed current-root transaction
    generations/
      mod.rs                     <=80   declarations
      context.rs                 <=420  immutable RevisionContext/ViewBase
      capture.rs                 <=450  fixed G1 cut/new live generation
      resolver.rs                <=650  saved bindings/terminal exceptions/depth
      pins.rs                    <=420  exact selected owner acquisition/release
    payloads/
      mod.rs                     <=80   declarations
      pack.rs                    <=650  small shared tail/stable slot facts
      location.rs                <=420  immutable selected location index
      owners.rs                  <=420  owned segments/capabilities, no global Arc map
    custody/
      mod.rs                     <=80   declarations
      ledger.rs                  <=650  exact owner phases/physical identities
      edges.rs                   <=550  admitted root/child ref transitions
      cleanup.rs                 <=650  paged cursor/failure/refund progression
      compact.rs                 <=600  bounded pack relocation/location install
      quota.rs                   <=420  global/per-W reservation transfer
  commit/
    mod.rs                       <=120  public delegation
    operation.rs                 <=300  one composite attempt orchestration
    prepare.rs                   <=550  dirty/file/metadata save sequence
    files.rs                     <=480  measure/descriptors/replacement Source
    namespace.rs                 <=550  count/header/binding/identity encoder
    results.rs                   <=420  exact paged saved observation/result join
    ready.rs                     <=500  pre-admitted ticket/known outcome slots
    install.rs                   <=550  current G2-safe selection change
    completion.rs                <=600  explicit known/unknown/same-selector custody
```

Reuse/split existing implementations into these responsibilities; the tree is a
map, not an instruction to scaffold empty files. Keep generic paged node grammar
under nodes; file byte sources under files; custody never invokes a command parser
or C1 construction. A serializer parses its own bounded records and cannot also
own mount lifecycle or Branch publication. Public integration tests stay outside
src and exercise real product paths. Supporting runtime SQL, if ever introduced,
would count as product and receive the same file ceilings; no SQLite dependency
is introduced to private backing by this design.

## 13. Reproducible baseline LOC and estimates

Measured source-size facts only: counter blob
`c7dd2b9c6aa9db63393a4ff3ebca9327529d146e` from baseline
`tools/production_loc.py`, exact Git blobs, nonblank/noncomment Rust lines after
its inline-test exclusion. Core product src has no inline tests. No benchmark,
fixture, tests, example, docs, manifest or generated artifact is included.

| Non-overlapping Workspace scope | Files | Production LOC | Physical lines |
| --- | ---: | ---: | ---: |
| backing/active/ |20 |7,224 |7,693 |
| commit/active* |3 |1,306 |1,368 |
| runtime/ |8 |2,485 |2,741 |
| overlay/snapshot.rs |1 |568 |574 |
| backing/metadata.rs, payload.rs, ownership.rs and ownership/ |4 |2,662 |2,761 |
| filesystem/active* |7 |1,901 |1,937 |
| These change-touching groups combined |43 |16,146 |17,074 |
| Entire Workspace product | all classified src |26,835 | reported separately from LOC |

Selected baseline files: active/extents443 LOC/468 physical, generation914/976,
index728/779, pages871/916, retirement124/139, lifetime256/280;
commit/active865/898, active_reconcile264/272; runtime/state644/753.
The complete baseline product is reference 65,417 + Core 70,279 = 135,696. This packet
has production delta 0. Those totals are not post-redesign estimates.

Reproduction method used: load the counter with `git show <baseline>:tools/production_loc.py`;
list `git ls-tree -r --name-only <baseline> core/crates/layerfs-workspace/src`;
read each .rs blob with `git show <baseline>:<path>`; apply counter
`blank_inline_tests(blank_rust(text))`; count nonblank lines; group exact paths
above. The normal full counter on an exact Git archive is the commit method.

**ESTIMATED implementation planning range, not measured after totals:** add
7,000–11,000 production LOC for new/reworked paged intervals, selected contexts,
custody and stream/completion orchestration; delete 5,000–8,000 production LOC of
superseded materializing/gate/registry paths after migration. The mathematical
net envelope is **-1,000 to +6,000 LOC**. These endpoints are independent uncertainty
bounds, not a prediction that maximum additions and deletions coincide. Existing
reused code moved between folders has zero net production LOC; temporary dual-format
readers and migration coexistence are separately labelled. No reference retirement
or relocation is counted as algorithmic simplification. Exact per-commit parent/
staged counts remain mandatory during any future implementation.

## 14. Scenario coverage and later evidence

| Scenario | Mandatory implemented mechanism | Future scope-specific proof |
| --- | --- | --- |
| SC-01 frequent/disjoint/append | Selected fast path + bounded generic splice, final dirty/extent cursors | Record historical calls vs final intervals; same-location/disjoint/append; actual plan capacities and work counts |
| SC-02 sparse/dense/shrink/hole | Shared canonical/parent subtrees; streamed range edit/GC | Full bytes/EOF/old pins, large affected range, first/middle/last, no whole-base acquisition for sparse edit |
| SC-03 many tiny/wide/deep files | Paged identities/bindings/references/cookies/results | Complete tree, alias/parent/cookie facts,32/128/1KiB payload boundary, per-file/graph work |
| SC-04 fresh/append/temp replacement/logs | Payload windows and exact selected owners | Ordinary Exec callback/data counts, full actual command bytes, selected open-handle replacement and cleanup |
| SC-05 moves/remove/copy | Atomic fixed namespace transaction and paged ancestry | File vs directory move, cycles rejected, cross-parent/alias/symlink/orphan exactness and untouched-root reuse |
| SC-06 G1/G2 successive/selected views | Per-inode current publication, immutable full resolver, fixed install | Candidate started before capture and completed after; writes during each phase; immediate predecessor and independent v2 partition/root oracle; old contexts |
| SC-07 admission/failure/custody | READY protection, typed known/unknown/install/cleanup states | Pre/post Branch refusal, lost response, ledger/unlink failure, same-selector local completion, precise refunds |
| SC-08 many Workspaces/Execs | Per-W leases and shared byte/private/Store hierarchy | Concurrent arbitrary commands, cross-W progress under bulk work, retained slots, idle/active resources, no stale-incarnation teardown |

Later tests must include deterministic barriers from real external providers/process
control, not sleeps or production test hooks. In particular: hold a wide file
candidate off the publisher while an unrelated inode writer and reader complete;
capture while that candidate is prepared; install while G2 metadata/names change;
verify the publisher uses CURRENT catalog/generation and preserves every accepted
revision. Same-inode conflicts must be exact, and parent/name multi-key transactions
must not expose partial link counts or moves. Exercise final-pin release and
removed-subtree GC against retained unknown owners.

Canonical proof independently checks full new/old trees and bytes, selected view
identity/context, immediate selected predecessor, saved-result associations and
exact roots/partitions for the admitted profile. V2 compares to its independently
specified v2 oracle; the separate v1 compatibility surface compares to original
v1 roots. Byte equality alone is insufficient, and cross-profile root inequality
is not relabelled as a failure or success of an old receipt. Failure proofs inspect actual
allocated/reserved/uncertain space, next-edge/job progress, canonical call count,
known outcome and installed revision. A capacity/status final counter is not a
resident phase peak.

No tests or benchmarks run now. At a frozen implementation identity, use external
owning tests plus locked Core fmt/test/clippy and boundary checks required by AGENTS;
no CI or retired aggregate preflight. Read benchmark rules/report template before
any future selection. Freeze manifests/cache/shape/deadline/worker policy before
one sample; performance and independent verification stay separate. Reuse unaffected
finite family evidence explicitly, especially Family2. Current 10240 remains
UNKNOWN/OWNER-DEFERRED/SKIPPED, and numeric cache verdicts remain INELIGIBLE.

## 15. Structural exclusion of the previous failure causes

This is a replacement of enabling representations and ownership, not a patch
campaign over each observed symptom. Existing canonical/physical algorithms are
reused where correct; their materializing caller representation and host-scoped
semantic ownership are replaced. The following API/type boundaries are mandatory
review constraints. They make the named old causes structurally unavailable on
the new path; they do not claim that arbitrary new software cannot contain bugs.

| Previous enabling cause | New authority/type/invariant | Adversarial later check |
| --- | --- | --- |
| A dirty/file/name population is returned as a Vec | CapturedCursor yields one borrowed checked record; no population-sized return type in capture/lowering | Million-record shape with fixed resource lease; inspect simultaneous capacities and precise refusal |
| A fragmented overwrite builds an all-affected patch | PreparedFileCandidate owns split/join root + paged edge/candidate custody; publisher accepts only fixed catalog facts | Large shrink/overwrite while unrelated inode publishes; no update-vector allocation |
| Detached whole-Workspace state overwrites later accepted writes | PublisherGuard applies candidate inode V to CURRENT catalog/generation; no detached Workspace root install API | Capture and known install between candidate preparation/publication; verify all other G2 writes |
| Every WRITE adds another old-version resolver | TerminalRange or one authenticated CapturedParentRange; subtree sharing is structural, not recursive version lookup | Many sequential writes/Commits and orphan copies; bounded current resolver depth |
| Old Base offset is silently rebound to a different file | CheckedRange includes exact file-version/coordinate authority; resolver binding certifies same selected bytes/length | Insert-like shift, truncate, holes, repeated/backward ranges absent from new root |
| Pinned view consults mutable latest-by-serial state | SelectedView owns immutable resolver/location roots; read methods receive only selected authority | Hold old view during mutation, location compaction and installation; reject stale/forged identities |
| Encoded future work consumes the resident Budget | WorkingLease admits actual coexisting windows; ShapeTotals and PhysicalReservation are separate types | Large encoded metadata within wire profile under unchanged resident budget |
| Known Branch success triggers large local admission | ReadyTicket owns all fixed InstallableTicket/failure/root-continuation resources before send | Deny ordinary scratch/quota growth after send; exact known result still installable or retained on genuine IO failure |
| READY names a stale G2 root or incomplete future orphan set | InstallableTicket owns sealed G1 facts; install chooses CURRENT G2 root and paged G2 exceptions atomically | Create/unlink/open orphan after READY, before reply; ensure exact later state survives |
| Owner/population memory grows with stored pages/pins | OwnerKey + PagedLedger + bounded cache; root/ref facts are authority, not Arc-map population | Hold old versions with many pages; owner/refund proof with constant cache and explicit disk growth |
| Cleanup loses failed owners or frees before visibility | CleanupJob/root custody, exact phased edge progress and physical checks | Partial unlink/ledger errors, selected last-pin release, unknown block accounting |
| One Workspace excludes all other submissions | PerWorkspaceSubmissionLease + global resource permits; no host semantic frozen bool | Two Workspaces each retain independent known/unknown attempts and progress/refusal scopes |
| Shared buffer is overwritten by simultaneous pulls | Exclusive WindowLease carries byte charge and owner identity; transfer consumes/relinquishes lease | Two concurrent sources with distinct deterministic bytes and forced slow consumer |
| Shell cancellation rolls back shared private state | ExecLease owns process/output lifecycle only; accepted MutationReceipt is immutable | Two commands straddle capture; one fails/cancels after writes; exact surviving G2 bytes |

Keep typestate concrete and small: `SelectedView`, `CheckedRange`,
`SourceCoverage`, `PreparedFileCandidate`, `CurrentCatalogGuard`,
`SubmissionLease`, `ReadyTicket`,
`InstallableTicket`, `KnownOutcome`, `CleanupJob` and `WindowLease`. These are
real ownership boundaries, not generic trait factories. No `install(Vec<Update>)`
or `install(detached WorkspaceSnapshot)` escape hatch remains on the v3 route.
A genuine storage/authorization uncertainty transitions to retained custody; the
new representation must not reconstruct an old error-driven fallback path.

## 16. Implementation sequence and release-blocking gates

1. Freeze private v3 records, selected FileState/edit-policy v2 and namespace
   v2/profile/parent-tree grammars, page ownership/reference transitions, static
   simultaneous window accounting and exact supported physical capability. Build no generic
   manager. Prove one paged cursor and one split/join against current bytes first.
2. Introduce per-file roots/current catalog publisher and selected source contexts.
   Replace wide live planning; verify generic atomic syscalls and unrelated-inode
   progress. Keep any quiesced legacy inspection as isolated read-only custody with an
   explicit budget/removal condition; v3 target operations never use old mutable
   authority or fallback.
3. Move mandatory identity, payload, location and owner authority to paged facts;
   remove full resident registries before claiming bounded live backing.
4. Add fixed capture/pending lease, ordered saved facts and pull encoders. Compose
   Server binding cursor, paged C1 construction and selected file-policy/namespace
   v2 capability. Reuse unchanged mapping/chunk grammars and reject unproved
   profile/semantic authority before dependent work.
5. Implement READY, known/unknown/current-G2 install and bounded cleanup. Remove
   active_reconcile's aggregate patch only once replacement custody is proved.
6. Freeze independent FileState/edit-policy and namespace v2 expected-root
   ledgers; qualify immediate-selected-parent deltas and authenticated parent
   locality/certification. Import a selected v1 snapshot into a new v2 Stack/scope
   through the paged remap workflow. Enable jointly designed #249/#219 after
   #248's memory/profile-specific identity/failure-custody proof; do not widen
   file/count limits merely to pass a case.
7. Qualify larger logical/write/namespace profiles and child execution capabilities
   separately. Database crash durability/mmap/locking and an S3 adapter remain
   independent product capabilities.

Defaults are explicit: new attachments use v3 after capability handshake; existing
legacy private v2 owners stay in quiesced custody; admitted canonical v2 Stacks
use immediate-selected-parent edit-policy and authenticated parent-tree profile
with independent exact v2 identities. Same-inode writers serialize; unrelated
inodes do not; public Commit keeps required cleanup; no automatic replay/
refresh/catch-up; one producer per operation; no new dependency/sync/WAL; supported
private residency enforcement is required. Genuine gates are exact admitted-v2
root/partition qualification, separate v1 compatibility, semantic namespace proof
provenance/epoch recertification, authenticated page/custody atomics, compiled
capacity bounds, physical cache qualification and real concurrent route/descendant enforcement.
They are proof obligations, not unanswered owner preferences or permission gates.

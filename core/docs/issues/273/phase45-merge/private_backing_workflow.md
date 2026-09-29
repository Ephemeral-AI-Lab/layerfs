# Private backing, live Commit and canonical publication

> **Status:** Research; informative and not a product contract.
> Reviewed 2026-09-29 at product source
> `11a864fc133844cae7a4247b1f243d84d5763b10`, with documentation basis
> `5b23b3753b5a2621dd67dd14b40f07b7a924f536`.
> This is a source trace. No new runtime, benchmark or merge result is claimed.

An ordinary Workspace mutation publishes a verified private revision. Commit
captures that revision, saves its file and metadata objects through the Service,
constructs a canonical filesystem, advances History conditionally, and installs
the known outcome into the still-live Workspace. Later writes have their own
generation and remain private until their own Commit.

This document follows the actual combined Phase 4.5/#273 product. Use
[the FUSE workflow](fuse_workflow.md) for the kernel callbacks and mount lifecycle,
[architecture refinement](architecture_refinement.md) for historical before/after
comparisons, and [the benchmark plan](BENCHMARKS.md) for future selections.

## Reading map

- [Owners and roots](#1-owners-and-four-different-identities)
- [Attachment and physical files](#2-attachment-establishes-private-custody)
- [Metadata and extent records](#3-the-selected-records-describe-metadata-and-payload-together)
- [Mutation publication](#6-a-mutation-publishes-one-complete-verified-selection)
- [Operation coverage](#7-operation-to-private-record-work)
- [Read and pin ownership](#8-reads-ordinary-handles-and-explicit-old-views)
- [Completion escrow](#10-completion-credit-is-owned-before-dirty-publication)
- [G1 capture and live G2](#11-capture-separates-the-commit-input-from-live-writes)
- [File lowering and Service construction](#12-file-preparation-and-authenticated-savefile)
- [Full Commit into C2 and History](#13-full-commit-constructs-a-filesystem-and-publishes-history)
- [Local installation and exact bytes](#14-c5-installs-the-captured-set-without-overwriting-g2)
- [Concurrency and failure custody](#15-live-progress-has-specific-admission-and-lock-boundaries)
- [Retirement, close and retained limits](#17-retirement-compaction-and-checked-refund)

## 1. Owners and four different identities

In the selected macOS-host/Docker-Linux topology, execution-side temporary
backing and the host-side canonical stores have different owners.

```text
MACOS CALLER / HOST                         DOCKER LINUX EXECUTION MACHINE
------------------                         ------------------------------
public SDK                                 daemon control + FUSE projection
  WorkspaceApi.mount/exec/commit                       |
        |                                             v
        +------ authenticated control ------> attached Workspace incarnation X
                                                      |
Service <------ authenticated Bridge calls -----------+
  |                                                   |
  +-- C1: file/metadata/filesystem algorithms           +-- live private selection
  |       uses provider/consumer interfaces            +-- selected old views
  |                                                   +-- completion/payload owners
  +-- C2: canonical content Store                      +-- temporary backing files
  |       SQLite objects and persistent packs
  |
  +-- History: branch/commit/stage catalog
          separate SQLite owner

Private 4 KiB pack page != persistent C2 object pack.
Private revision       != canonical History Commit.
```

The file-content root, filesystem root, active index selection and Workspace
incarnation are also different identities.

```text
canonical filesystem root R0
        |
        v
canonical inode table
        +-- root directory serial -> names: "data.bin" -> inode42
        +-- inode42 -> file-content root F0 + portable metadata root M0
        +-- other canonical identities

Workspace incarnation X
        +-- selected branch/Base context -> filesystem R0
        +-- current active index selection
        |      {root Target, height, selected HotDirectory}
        |                         |
        |                         v
        |                   I/N/D/E/P/R/L records
        +-- live generation/revision, handles, views and owned physical files

I|42.base -> file-content F0 used by this inode's Base extents
State.base -> filesystem R0 used by namespace/branch context
```

After Commit, `State.base` can advance to filesystem R1 while a later-written
inode retains file-content F0 and its original source coordinates. A private
PageRef or HotRef never becomes a canonical C1 identity. An all-zero private
`I.base` is an absence sentinel, not the canonical hash of an empty file.

Sources: [Workspace state](../../../../crates/layerfs-workspace/src/runtime/state.rs#L47),
[active inode](../../../../crates/layerfs-workspace/src/backing/active/records.rs#L9),
[index selection](../../../../crates/layerfs-workspace/src/backing/active/index.rs#L26),
[C5 root handling](../../../../crates/layerfs-workspace/src/commit/active_reconcile.rs#L208).

## 2. Attachment establishes private custody

```text
attach(id, incarnation, LocalEdit, selected Root/Branch/Commit)
        |
validate ID, incarnation, access, owner, Host capacity and unique registry entry
        |
reserve directory ownership; registry = Attaching
        |
authenticated FileSaveCapabilities (existing SaveFile authority)
        |
typed version exactly 2?
        +-- no --> retain original attachment failure + checked cleanup outcome
        +-- yes
              |
select immutable filesystem Base
  Root: supplied root
  Branch/BranchAt: History GetBranch and checked historical selection as required
              |
Inspect::Attributes(root, empty path); validate root kind/serial/metadata
              |
precharge State + initial 256-Node capacity + 128-handle capacity
only the root Node is present; children and file bytes are still lazy
              |
create incarnation-owned private directory and empty mount leaf
verify directory identity/permissions and supported aligned-I/O profile
              |
LocalEdit: metadata arena + ActiveBacking owner
              |
publish registry = Attached
```

The Host's RAM Budget and PayloadHost quota are shared configured resources
across its managed Workspaces. Files and accounting records still have exact
incarnation ownership. The first 256 Nodes are allocation capacity, not a
256-file namespace limit.

```text
WorkspaceConfig.root
        +-- workspace/
        |      +-- <workspace-id>/             FUSE mount location
        |
        +-- private-backing/                   shared Host backing parent
               +-- <workspace-id>/             fresh incarnation-owned leaf
                      +-- a-pack-v1-<id>-<epoch>
                      +-- a-index-v2-<id>-<epoch>
                      +-- a-hot-v2-<id>-<epoch>
                      +-- p-<payload-id>-<segment-index>
                      +-- m-page-<slot>-<epoch>
                      +-- m-ledger-<index>
```

The `a-*` files carry active packs, indexes and selected hot directories.
`p-*` files own larger inputs and symlink targets. The existing `m-*` arena
retains saved observations and completion bookkeeping; it is not another live
namespace authority.

Backing admission verifies a canonical owner-only directory and the selected
Linux filesystem profile. Creation uses the verified directory descriptor,
exclusive names, `O_NOFOLLOW`, `O_CLOEXEC` and aligned direct I/O. The supported
deployment establishes ext4 and 4096-byte blocks; filesystem magic by itself
does not prove a general portable backend. A collision with an earlier or
foreign directory/file is a refusal, not permission to adopt or delete it.

ReadOnly attachment does not create the same mutable active owner or require
the LocalEdit SaveFile capability. Attachment and FUSE mounting are separate
native operations; the public SDK mount route composes them.

Sources: [Host attachment](../../../../crates/layerfs-workspace/src/runtime/host.rs#L275),
[capability admission](../../../../crates/layerfs-workspace/src/runtime/host.rs#L366),
[directory verification](../../../../crates/layerfs-workspace/src/backing/directory.rs#L37),
[payload ownership](../../../../crates/layerfs-workspace/src/backing/payload.rs#L179),
[segment creation](../../../../crates/layerfs-workspace/src/backing/segments.rs#L183).

## 3. The selected records describe metadata and payload together

Private integer fields are big endian. The selected index orders unique keys.

| Family | Key | Value and purpose |
| --- | --- | --- |
| `I` | `I\|inode:u64`, 9 bytes | 416-byte current inode facts and content representation |
| `N` | `N\|parent:u64\|name` | 16-byte binding or tombstone for one component |
| `D` | `D\|generation:u64\|inode:u64`, 17 bytes | `[1]`; captured/live dirty membership |
| `E` | `E\|inode:u64\|logical_start:u64`, 17 bytes | 56-byte final logical extent |
| `P` | `P\|logical_pack_page:u64`, 9 bytes | 16-byte selected physical PageRef |
| `R` | `R\|logical_page:u64\|ordinal:u16\|inode:u64\|start:u64`, 27 bytes | `[1]`; tiny-slot inverse reference |
| `L` | `L\|payload_id:u64\|inode:u64\|start:u64`, 25 bytes | `[1]`; larger-payload inverse reference |

The `R` record family is unrelated to the R0/R1 symbols for canonical
filesystem roots. Its key identifies which selected extents need a packed slot.

```text
I value: 416 bytes
+-------------+------------------------------------------------------------+
| 0..8        | inode revision                                             |
| 8..16       | generation                                                 |
| 16..24      | selected length                                            |
| 24          | kind: regular file / directory / symlink                    |
| 25          | fresh/unintroduced identity flag                           |
| 26          | storage: 0 empty, 1 inline, 2 ordered E records              |
| 27          | inline extent count                                        |
| 28..32      | portable mode                                              |
| 32..40      | mtime seconds                                              |
| 40..44      | mtime nanoseconds                                          |
| 44..48      | regular-file link count                                    |
| 48..80      | immutable file-content Base root                            |
| 80..112     | portable metadata root                                     |
| 112..160    | required zeros                                             |
| 160..416    | four 64-byte inline slots                                  |
+-------------+------------------------------------------------------------+
inline slot = logical_start:u64 + ordinary 56-byte E value
```

Up to four inline extents must cover the selected length contiguously. Ordered
storage uses `E` records instead. Metadata-only first touch of an inherited
regular file can use one inline Base span. A matching saved revision collapses
to one inline Base span after Commit. The current generic WRITE path generally
publishes ordered extents; the grammar does not promise automatic inline
storage for every small edited file.

```text
Base    -> {I.base immutable file root, source_offset, length}
Zero    -> {length}; private read synthesizes zeros
Packed  -> {logical page, ordinal, original inode/G/revision/offset/length}
            P resolves physical page; each fragment has its own R key
Payload -> {owned payload ID, source_offset, declared payload length}
            each fragment has its own L key
```

For a Payload extent the shared codec fields named `logical_page` and
`generation` mean payload ID and declared payload length. Cutting a Packed
extent retains the original authenticated record identity; it does not invent
a new packed record for each fragment.

`N` stores one name, while resident Nodes currently retain full locators too.
Public names are 1–255 UTF-8 bytes, exclude `.`, `..`, NUL, slash and backslash,
and must fit the aggregate locator bound. An `N` tombstone prevents canonical
fallback for that selected name. `D|G1|inode` and `D|G2|inode` can coexist;
removing captured dirty membership must preserve later membership.

A Zero extent avoids a private payload allocation for a gap. During canonical
construction its bytes are still synthesized and uploaded. This is not a
public hole-punch or `SEEK_HOLE` implementation.

Sources: [record codec](../../../../crates/layerfs-workspace/src/backing/active/records.rs#L49),
[extent kinds](../../../../crates/layerfs-workspace/src/backing/active/extents.rs#L9),
[public name checks](../../../../crates/layerfs-workspace/src/filesystem/namespace.rs#L52),
[dirty generation](../../../../crates/layerfs-workspace/src/backing/active/generation.rs#L519).

## 4. Tiny regular writes share verified physical packs

```text
borrowed FUSE WRITE bytes
        |
validate flags, rights, inode/handle, <=128 KiB callback input
        |
input <=128 bytes?                         larger input?
        |                                      |
bounded charged copy                      acquire OwnedPayload
        |                                      |
        +------------------+-------------------+
                           |
                 ordinary active publication

native owned-input WRITE -> same ownership/publication checks, input <=8 MiB
```

Tiny regular data uses one Workspace-shared tail, so different files share its
packing density. Symlink targets use the Payload route even when short.

```text
4096-byte physical pack
+-------------------------------+------------------------------------------+
| authenticated header: 128 B   | body: 3968 B                             |
+-------------------------------+------------------------------------------+
record = 48-byte identity/length header + 1..128 data bytes
ideal body capacity: 80 one-byte records or 22 full 128-byte records

selected logical tail 42
  P|42 -> physical page p7
    record0: inode A, generation/revision, original offset/length, bytes
    record1: inode B, generation/revision, original offset/length, bytes

next tiny WRITE
  preview logical page/ordinal
       |
  copy bounded selected tail body into candidate p8; append record
       |
  write complete p8 -> cleared readback -> authenticate exact page
       |
  publish P|42 -> p8 + matching E/R/I/D in ONE selected revision

old view's P|42 -> p7                    current P|42 -> p8
```

When full or sealed, the next record starts a new logical pack page. The
logical-to-physical indirection avoids rewriting unrelated extents just because
the shared tail acquires a new physical version.

Active headers bind kind, format, Workspace incarnation, physical PageRef,
generation/revision, used length and count. The whole-page SHA-256 calculation
zeros the digest field; unused bytes must be zero. Pack framing remains
`LFSAPAK1` version 1. Index and hot-directory pages use `LFSAIDX2`/`LFSAHOT2`
version 2. These versions are separate from Bridge SaveFile v2.

Current physical PageRef IDs increase monotonically with epoch 1. Reusable
HotRef slot epochs are a different selection mechanism.

Sources: [pack layout and staging](../../../../crates/layerfs-workspace/src/backing/active/pack.rs#L80),
[active page framing](../../../../crates/layerfs-workspace/src/backing/active/page.rs#L51),
[WRITE acquisition](../../../../crates/layerfs-workspace/src/filesystem/write.rs#L151),
[FUSE WRITE](../../../../crates/layerfs-fuse/src/adapter.rs#L496).

## 5. Larger payloads and exact read provenance

Larger input is owned in immutable segmented `p-*` files. Acquisition reserves
planned aligned physical charge, records exclusive-file identity before later
fallible work, observes actual blocks, fills the declared input and retains
partial/failure custody. A small payload uses one 4096-byte inline segment;
larger payloads use 1 MiB data segments with aligned headers and 128 KiB
transfer windows. `LargeOwner` retains the verified OwnedPayload; selected `L`
references and old pins control its retirement.

The optional read-origin optimization is behind generic reads and writes.

```text
ordinary READ returns actual contiguous immutable Base bytes
        |
retain ONE optional charged ReadOrigin, <=128 KiB
  {inode, actual file root, source offset, exact returned bytes}
        |
later ordinary WRITE
        |
same selected inode/root + valid range + EVERY supplied byte matches?
        +-- yes --> publish a Base-source extent
        |           no retained Local/Packed/Payload extent for those bytes
        +-- no ---> ordinary pack/Payload publication

mixed/local/Zero read, changed identity or nonmatch -> generic path
```

Large owned-input comparison uses an 8 KiB comparison window. Retaining
provenance neither transfers a canonical owner nor refunds a private owner.
The larger/owned-input path may already have acquired a temporary Payload;
its actual allocation and charge remain owned until checked release even
when the final selected extent is Base.
Command text, benchmark ID and fixture identity do not choose this path.
Backward or duplicated source offsets are valid ordinary copy semantics; the
internal Service SaveFile route described below accepts them without daemon
recursive RPC.

Sources: [segment layout](../../../../crates/layerfs-workspace/src/backing/segments.rs#L5),
[payload acquisition](../../../../crates/layerfs-workspace/src/backing/payload.rs#L412),
[actual read-origin memo](../../../../crates/layerfs-workspace/src/filesystem/read_origin.rs#L15),
[Base-origin publication](../../../../crates/layerfs-workspace/src/backing/active/generation/read_origin.rs#L5).

## 6. A mutation publishes one complete verified selection

The mutation boundary binds metadata, payload selection and physical custody.
It never acknowledges a locator whose candidate bytes have not been verified.

```text
REQUEST
   |
validate access/kind/handle/append offset/length/deadline
   |
select and recheck branch baseline, inode, generation and revision
   |
precharge frontier RAM, resident growth, candidate scratch
precharge completion ownership before first dirty publication
   |
choose eligible optional hot route BEFORE physical allocation
prepare changed keys and affected inverse/physical ownership
   |
reserve candidate physical page credit
   |
exclusive create; retain exact identity before fallible allocation
   |
allocate -> independently observe st_blocks * 512
transfer reserved charge into actual allocated charge
   |
write complete aligned bytes -> read back into cleared window
verify identity/header/hash/zero tail/exact bytes
   |
prepare matching root + height + selected directory
recheck expected generation/revision
   |
PUBLISH ONE SELECTED INDEX REVISION
  I/N/D/E/P/R/L agree on effective metadata and bytes
   |
install matching resident attributes/namespace bookkeeping
retire replaced owners; release publication locks
   |
checked projection notification where selected
typed native result / FUSE reply attempt
```

Before publication, the previous acknowledged revision remains selected.
Checked abort releases only created, identity-proven candidates. A filename
collision is foreign ownership and must not be unlinked. Uncertain identity,
accounting or unlink retains charged custody.

After publication, notification or cleanup failure can report an error while
the mutation and its receipt remain selected. A published create also retains
its handle. FUSE errno cannot carry the full native receipt, and fuser's reply
send is not a checked later kernel acknowledgement. An error therefore grants
neither semantic rollback nor automatic replay/refund.

Projection WRITE selects checked inode notification. The separate projected
`set_len` entrypoint and projected creation suppress that delivery. Current
FUSE SETATTR uses `ProjectionAttributes`, including a size-only request, and
the regular-file route selects a notifier despite broader suppression comments
elsewhere. Directory SETATTR suppresses projected delivery. This is a
source/comment discrepancy, not a newly tested runtime failure. See
[the callback qualification](fuse_workflow.md#10-namespace-and-portable-attribute-mutations).

Sources: [physical creation/accounting](../../../../crates/layerfs-workspace/src/backing/active/pages.rs#L447),
[index publication](../../../../crates/layerfs-workspace/src/backing/active/index.rs#L533),
[file publication/notification choice](../../../../crates/layerfs-workspace/src/filesystem/active_file.rs#L35),
[post-publication result](../../../../crates/layerfs-workspace/src/filesystem/write.rs#L390).

## 7. Operation-to-private-record work

Read-only operations can acquire charged caches, replies, Nodes, cookies or
leases without publishing dirty file records. Namespace operations can require
canonical metadata consultation; new identities reserve a serial remotely.

| Operation | Effective record work and custody |
| --- | --- |
| Lookup / attributes | Select `N` and `I`; tombstone stops fallback. Absent inherited facts can use C1 identity reads. Cache charged Node/name facts; no `D` solely for observation. |
| Ordinary regular open | Acquire stable-inode handle/rights. No dirty publication without truncate. |
| Native open with truncate | Reserve unready handle; publish truncate/I/D and ready handle together. Failed preparation removes unready handle. FUSE OPEN lacks atomic truncation negotiation and receives separate SETATTR(size). |
| Directory open/list | Pin selected View/path/parent; merge `N` and canonical names; own continuation cookies. |
| Read | Select `I` and inline/E intervals; resolve Base/Zero/Packed/Payload into a charged bounded reply. Optional exact read-origin memo. |
| New regular create/mknod | Reserve canonical serial; new `N`, child `I/D`, parent `I/D`, fresh-identity bookkeeping. CREATE publishes binding and ready handle together; MKNOD has no handle. |
| Existing nonexclusive create | Resolve existing identity and use open semantics; no fresh serial. |
| mkdir | Reserve serial; new directory `I/D`, parent `I/D`, `N`; pending directory declaration. No file payload. |
| Write / append / overlap | Nonempty input changes final intervals, touched `R/L`, selected `P` where packed, and file `I/D`; one revision. Accepted zero input keeps the current revision without dirty/candidate/notification work. Append validates the live EOF selected for the operation. |
| Write beyond EOF | Same work plus Zero extent for gap. |
| Truncate shorter | Cut crossing interval; remove suffix `E` and inverse keys; update `I/D`; identify dead owners. |
| Extend size | Publish Zero intervals and `I/D`; removed suffix bytes do not return. |
| File mode/mtime | `I/D`; keep content; first inherited touch can install inline Base span. |
| Directory mode/mtime | Directory `I/D`; size refused. Current resident attribute update still scans Nodes. |
| Hard link | Same live regular serial; new `N`; target link count and `I/D`, parent `I/D`. No data copy or new serial. Directory/symlink hard links refused. |
| Rename | Old `N` tombstone, new `N` binding to moved serial; parent `I/D`; moved/replaced identity bookkeeping and regular link counts. Prepare affected resident full paths and depth checks. |
| Unlink | Tombstone, parent `I/D`, regular-link decrement; fresh unbound identity may lose declaration/dirty eligibility while held data remains owned. |
| rmdir | Prove effective emptiness, root refusal, tombstone/parent `I/D`; detach resident directory. Old directory handles keep selected names. |
| Symlink create/readlink | New serial, `N`, child/parent `I/D`, target-bearing Payload `E/L`; readlink selects exact target or canonical inode-relative target. Empty target uses empty representation. |
| Flush | Report retained coherence failure; no implicit Commit or release. |
| File/directory release | Release handle/cookies/selected directory view and collect eligible Nodes. No implicit Commit. |
| Checked view release | Release exact old selection/retirement selector; refund only eligible actual unlinks. |
| Clean close | Refuse dirty/submission/mount/operation/handle/lookup/external-pin blockers; then checked owner drain. |

Unlink and rename replacement change reachability, not immediate physical
ownership. An open-unlinked file preserves inode lifetime and current data.
Tombstones alone refund nothing. A fresh links-zero file can have saved private
facts while being omitted from new rooted canonical declarations.

Current rename prepares resident replacement paths and can visit inherited
descendants when depth increases. The combined implementation retains inline
4096-byte paths with charged extensions up to 65,536 bytes/256 components.
Constant-count `N/I/D` changes do not imply constant total rename work or all
standalone namespace-lane complexity benefits.

Sources: [namespace view](../../../../crates/layerfs-workspace/src/filesystem/active_view.rs#L85),
[open](../../../../crates/layerfs-workspace/src/filesystem/open.rs#L97),
[create](../../../../crates/layerfs-workspace/src/filesystem/active_create.rs#L117),
[remove](../../../../crates/layerfs-workspace/src/filesystem/active_remove.rs#L105),
[rename](../../../../crates/layerfs-workspace/src/filesystem/active_rename.rs#L184),
[attributes](../../../../crates/layerfs-workspace/src/filesystem/active_attributes.rs#L31),
[resize](../../../../crates/layerfs-workspace/src/filesystem/resize.rs#L24),
[hard link](../../../../crates/layerfs-workspace/src/filesystem/link.rs#L30).

## 8. Reads, ordinary handles and explicit old views

```text
ordinary regular FD
  stable serial + rights/append intent; Handle.view = None
        |
each read selects CURRENT live inode/content
  writes through another hard link are visible

one ordinary read
  transient current-selection pin
        |
assemble <=128 KiB charged bytes
        |
ReadReply owns copied bytes + operation charge
  selection pin can end after source bytes are assembled

directory FD
  stable serial + selected View + charged directory path/parent
        |
selected names and continuation cookies
  active ".." uses live parent, or last parent after detach

explicit SDK PinView
  authenticated lease + exact selected root/revision
        |
old selected lookup/read/list/readlink across later writes and Commit
        |
explicit checked release; client value Drop sends no remote release

Commit capture
  immutable G1 selection + Submission + SAME G1 completion fund
```

The range reader resolves inline/E extents under the active selection lock.
Packed and Zero bytes are copied there. Base spans and cloned payload owners
are retained for RPC/physical reads after releasing that lock. Span arrays,
output buffers and locators are charged.

Packed reads validate logical page, ordinal, inode, source generation/revision,
original offset and length against authenticated bytes. Base reads require the
primary remote admission. Name/attribute reads have the separate metadata
admission described below. Identity reads address immutable inode serials,
so a renamed inherited path need not exist at its new location in the old root.

Ordinary file OPEN does not create an old-byte snapshot. The old-byte guarantee
comes from a selected directory/View/capture or explicit public lease. Public
leases are capped at 32, with tokens bound to Workspace/incarnation and entries
bound to one lease. The registry charges 256 bytes per lease and 512 bytes per
issued entry; actual paths and response allocations add separate terms.

The declared read maximum is 128 KiB, while retained full-byte SDK proofs used
16 KiB requests. The separate 32 KiB pinned SDK-read `Io` observation remains
unresolved. This source description creates no accepted 128 KiB read proof.

Sources: [read assembly](../../../../crates/layerfs-workspace/src/filesystem/read.rs#L14),
[selected range reading](../../../../crates/layerfs-workspace/src/backing/active/generation.rs#L233),
[handle view selection](../../../../crates/layerfs-workspace/src/filesystem/open.rs#L219),
[directory parents](../../../../crates/layerfs-workspace/src/filesystem/directory.rs#L42),
[selected namespace](../../../../crates/layerfs-workspace/src/filesystem/namespace_view.rs#L15),
[public lease workflow](architecture_refinement.md#8-public-old-view-reads-and-checked-release).

## 9. Hot and generic index routes publish the same semantics

```text
selected branch child: fixed fence + HotRef(slot7, reuse epoch3)
        |
selected directory D10: slot7/epoch3 -> physical leaf L1
        |
authenticate selected leaf
        |
prospective hot eligibility:
  inode/generation/dirty facts + ancestor bindings + fences + advancing coverage
        |
merge affected cells once -> verified leaf L2
stage D11: same slot7/epoch3 -> L2
publish matching root + selected directory + revision

branch reference stays stable                old pinned D10 still resolves L1
current D11 resolves L2                      old bytes remain owned
```

```text
ineligible optional hot coverage/admission
        |
generic floor lookup + affected-range scans, <=128 entries per scan page
        |
merge reached key partitions
        |
overflow? -> byte-balanced splits -> changed ancestor carry/root connection
        |
normalize unused hot closure where required
preserve unchanged physical birth facts
        |
publish coherent selection
```

Optional selection happens before physical allocation. An authentication or
physical-write failure does not license a second algorithm attempt. Generic
overlap pays for every affected interval and inverse reference; 128 is a scan
page bound, not an overlap count ceiling.

Hot state is bounded at eight inode cursors, 64 authenticated decoded nodes
and a 1 MiB selected resident reservation. Maximum branch level is 7. Old and
candidate copies, owner maps, Nodes and operation scratch are additional memory.

Sources: [hot cursor bounds](../../../../crates/layerfs-workspace/src/backing/active/hot_cursor.rs#L14),
[prospective hot path](../../../../crates/layerfs-workspace/src/backing/active/hot_path.rs#L190),
[generic splice](../../../../crates/layerfs-workspace/src/backing/active/splice.rs#L39),
[index selection](../../../../crates/layerfs-workspace/src/backing/active/index.rs#L268).

## 10. Completion credit is owned before dirty publication

RAM Budget and private physical quota are separate admissions.

```text
RAM Budget                              PRIVATE DISK QUOTA
----------                              ------------------
resident/scratch capacity                actual allocated physical blocks
Nodes/maps/windows/owner records          + outstanding reserved physical credit
```

Before the first dirty active publication, the generation reserves a completion
fund of 208 pages: `208 * 4096 = 851,968` bytes of private disk credit. The fund's
metadata separately charges 256 bytes of RAM. It is not an 851,968-byte heap
buffer. Frontier RAM is also admitted before publication.

```text
prospective first dirty publication
        |
reserve frontier RAM + G1 completion disk credit
        +-- refused -> no dirty publication
        |
live G1 owns ProgressFund Arc
        |
capture checks generation and untouched completion ownership
        |
Submission receives SAME fund Arc
        |
G2 first dirty publication reserves a DISTINCT fund
```

If a failure occurs after the live fund is precharged but before dirty
publication, the unused fund remains charged to that live generation. Later
admission reuses it; captured completion or checked clean close returns it.
An unsuccessful preparation need not restore reserved_bytes to its earlier
value. Ordinary Commit also permits clean capture: without a live fund,
capture explicitly reserves/installs its completion credit and can refuse
before the capture is published. Dirty capture transfers the existing Arc.

```text
fund.take(4096)
        |
exclusive page create; retain identity
        |
observe a = st_blocks * 512
        |
reserved -> allocated for actual a
unused (4096-a) -> SAME unfinished fund
        |
candidate verified/selected OR failed candidate kept in custody
        |
checked identity/block observation + successful unlink
        +-- fund unfinished -> actual a becomes reserved a in SAME fund
        +-- fund finished   -> ordinary allocated quota refund

successful installation -> finish fund -> refund unused reserved credit
published or pinned allocated pages remain charged
```

C5 uses the captured fund under its publication gate. Before canonical Commit,
the unused legacy reconciliation root is sealed so slot/reservation credit
returns to that same fund. No pinned owner or unrelated owner is released to
manufacture headroom. The reserve is bounded; arbitrary frontiers can still
refuse within unchanged quota.

Sources: [frontier admission](../../../../crates/layerfs-workspace/src/runtime/state.rs#L350),
[completion constants](../../../../crates/layerfs-workspace/src/backing/metadata.rs#L23),
[ProgressFund](../../../../crates/layerfs-workspace/src/backing/metadata/progress.rs#L10),
[capture transfer](../../../../crates/layerfs-workspace/src/overlay/snapshot.rs#L492),
[completion sealing](../../../../crates/layerfs-workspace/src/commit/completion.rs#L354),
[allocation transfer/refund](../../../../crates/layerfs-workspace/src/backing/active/pages.rs#L482).

## 11. Capture separates the Commit input from live writes

```text
public/native Commit admitted
        |
short ordered capture boundary
  one public Submission available?
  expected generation/revision/Base still selected?
  live generation completion owner intact?
  clean Commit with no live fund -> explicitly reserve/install credit first
        |
seal shared tiny tail
pin {root Target, height, selected directory, canonical context}
retain declared-new-directory facts and captured counts
        |
Submission = immutable G1 + captured revision r1 + SAME G1 fund
        |
advance live generation/revision; clear live per-generation frontier counts
        |
        +-------------------------------+---------------------------------+
        |                               |                                 |
        v                               v                                 v
Commit reads G1                    existing processes                 old views
selected D/I/E/N                   continue POSIX/FUSE                 stay selected
        |                               |
save files/metadata                G2 writes create new page versions
construct filesystem               first G2 dirty change owns new fund
conditional History publication         |
        |                               |
        +--------- C5 merges known G1 outcome with live G2 ----------------+
```

Capture shares selected directories/pages; it does not drain the hot tree,
remount FUSE or freeze processes. Public capture also performs explicit
maintenance and clones pending directory declarations, so the entire operation
is not universally O(1).

The lower active owner supports 32 captures and 160 distinct selecting revision
keys. The public Workspace allows one retained Submission. Its lifecycle and
transport admissions place narrower limits on simultaneous public operations.

Sources: [capture pin](../../../../crates/layerfs-workspace/src/backing/active/generation.rs#L838),
[selected index capture](../../../../crates/layerfs-workspace/src/backing/active/index.rs#L594),
[public capture](../../../../crates/layerfs-workspace/src/overlay/snapshot.rs#L436),
[composite Commit entry](../../../../crates/layerfs-workspace/src/commit/operation.rs#L16).

## 12. File preparation and authenticated SaveFile

### 12.1 Save content and metadata before preparing the filesystem

```text
captured immutable G1
        |
scan D|G1|* -> selected I facts
        |
regular file:
  enumerate inline/E extents
  inspect actual canonical Base length when present
  unchanged full Base -> retain same F0
  changed content -> descriptors + exact Local/Zero byte count
        |
grouped selected pack reads / OwnedPayload reads / synthesized Zero bytes
        |
authenticated internal SaveFile v2 -> Service/C1/C2
        |
known file-content root F1
retain known SavedInode observation BEFORE next fallible metadata call
        |
fresh inode -> ConstructPortableMetadata
existing inode -> UpdatePortableMetadata(M0)
        |
known M1 -> retained saved observation {captured revision, length, F1, M1}
        |
symlink: ConstructSymlink exact target + portable metadata
directory: portable attributes and later canonical directory construction
```

Content and metadata roots are separate. A saved file root survives a later
metadata failure as a known observation with precise custody; it is not a
reason to repeat the SaveFile command. Fresh open-unlinked facts can be saved
without publishing an unreachable new canonical identity.

Sources: [active preparation/upload](../../../../crates/layerfs-workspace/src/commit/active.rs#L255),
[saved observations](../../../../crates/layerfs-workspace/src/commit/save.rs#L266).

### 12.2 Bounded pack reads do not mean constant total upload memory

```text
selected final extents in output order
        |
one source window: <=1024 pack references AND <=32 KiB scatter output
        |
sort indices by logical pack; read/authenticate selected physical pack
scatter requested fragments into ORIGINAL output positions
        |
emit bytes in exact final order
        |
advance window
```

The fixed grouping capacity is 73,728 bytes on the reviewed 64-bit profile.
A separately charged decoded PackRecords cache owns record/payload vectors;
cache replacement also has transient old/new/readback overlap. Vector headers
and existing upload allocations are additional terms. Distinct packs can be
read again in later windows. Work follows the sum of distinct selected packs
per window, bounded by the reference count; it is not a whole-file
one-read-per-pack promise.

The upload also materializes `O(E_f)` final extents and `24 * E_f` descriptor
bytes per file. Geometric growth charges old/new allocation overlap and makes
growth amortized O(E_f). Dirty identities, names and reconciliation maps add
other proportional terms. There is no constant-total-RAM claim.

Source: [grouped source](../../../../crates/layerfs-workspace/src/commit/active_source.rs),
[extent/descriptor upload](../../../../crates/layerfs-workspace/src/commit/active.rs#L471).

### 12.3 Service resolves reordered Base bytes without recursive daemon RPC

SaveFile v1 remains opcode 20 with forward retained Base anchors. Authenticated
capability opcode 28 precedes LocalEdit attachment. Internal v2 is opcode 29
with version byte 2; it uses the existing SaveFile authority and preserves v1.

```text
Workspace -> Bridge -> Service
  version2 + E descriptors + L exact Local/Zero input bytes
        |
validate count/arithmetic/source order framing/EOF/deadline/authority
        |
        +----------------------+-----------------------+-------------------+
        |                      |                       |                   |
        v                      v                       v                   |
forward Base anchor     backward/duplicate Base    Local/Zero bytes          |
retained unchanged      fixed origin records      bounded byte spool        |
        |                      |                       |                   |
        +----------------------+-----------------------+-------------------+
                               |
                       ordered edit spool
                               |
                       C2 Store::begin_save
                               |
                 open actual canonical Base FileView
                 validate actual stored Base length
                               |
                 C1 canonical edit construction
                               |
                 ServiceResolvedSource for replacement bytes
                     fixed-record origin lookup
                       +-- Local/Zero -> byte spool
                       +-- Base -> provider-local C1 read_range
                                   <=64 KiB logical window
                               |
                     C2 Save finish -> checked file root
```

Backward Base bytes are read inside the Service through its C1 provider. They
are not uploaded into a file-sized daemon spool and do not recursively acquire
the daemon transport mutex. The actual Base length check happens when the
FileView opens after `begin_save`; it is not assumed before that Save owner.

For E descriptors and L Local/Zero bytes, wire sizes are `24E + L` for v1 and
`1 + 24E + L` for v2. Derived spool bounds are `48E + 32 + L` and
`80E + 32 + L`, respectively, capped at 8 GiB. Edits and origins use fixed
32-byte records; temporary Zero records use 16 bytes. Up to 64 KiB actual input
can remain resident, otherwise the byte spool has a 64 KiB window. These are
bounded resident windows, not a claim of zero temporary Service disk usage.

The same deadline and authenticated framing cover the stream and terminal
result. Counts, extents, exact Zero bytes, EOF and unknown versions fail closed.
The descriptor count does not authorize a public range-edit API or ioctl.

C1 can reuse untouched file subtrees and C2 can reuse existing objects. A
backward replacement can nevertheless read Theta(final file bytes). Changed
chunked content pays for actual comparisons, CDC, hashing and object work;
whole-file and representation-transition cases have their own assembly cost.
Different construction profiles need not produce the same file-tree root merely
because final bytes match. Exact bytes are the oracle; v1/v2 root comparisons
require the same anchors/construction profile.

Sources: [approved internal proposal](../SAVEFILE-V2-PROSPECTIVE-20260929.md),
[Bridge request contract](../../../../crates/layerfs-bridge/src/contract/request.rs),
[Service handler](../../../../crates/layerfs-server/src/service/handler.rs#L275),
[stream parser/spool](../../../../crates/layerfs-server/src/service/save/file_stream.rs#L279),
[origin resolution](../../../../crates/layerfs-server/src/service/save/file_stream/origin_runs.rs#L83),
[content Save ordering](../../../../crates/layerfs-server/src/service/save/content.rs#L23),
[C1 file construction](../../../../crates/layerfs-content/src/file/edit/apply.rs).

## 13. Full Commit constructs a filesystem and publishes History

### 13.1 The prepared body contains logical changes

```text
PreparedChanges envelope
  Workspace/incarnation, Branch ID, expected head Commit/expected Base Layer
  captured generation, construction filesystem R0, scope/root serial, totals
        |
ordered body
  stream tag
  directory rows:
    parent serial, byte-ordered name -> Some(child serial) or None tombstone
  identity rows:
    Existing/FreshFile      -> serial, F1, M1
    Existing/FreshSymlink   -> serial, target root, M1
    DirectoryPatch/Declaration -> serial, portable mode/mtime
        |
Service validates row roles/counts/order/bytes/subjects/EOF
```

Private PageRefs, HotRefs, `p-*` IDs and pack locators are not sent as canonical
identities. A fresh empty directory still needs both its directory binding row
(even empty) and its declaration. The Service builds canonical directory
content; it does not copy private index pages into C2.

Sources: [prepared stream](../../../../crates/layerfs-workspace/src/commit/stream.rs),
[directory preparation](../../../../crates/layerfs-workspace/src/commit/directories.rs),
[C1 filesystem input](../../../../crates/layerfs-content/src/filesystem/input.rs).

### 13.2 Composite public Commit and explicit native Stage are distinct routes

```text
ordinary public SDK WorkspaceApi.commit
        |
authenticated daemon WorkspaceCommit; lifecycle slot held
        |
Workspace::commit -> capture G1 -> save files/metadata -> PreparedChanges
        |
ONE HistoryCommand::Commit(PreparedChanges)
        |
Service reads initial Branch snapshot; checks expected head/Base
        |
resolve construction R0 and scope
begin filesystem C2 save; establish provider/consumer owner
open/check actual filesystem R0 and root serial
receive/validate RowSpool
  directory patch/declaration metadata is constructed as rows arrive
        |
C1 filesystem update
  changed directories + inode rows + reference counts
  alias/cycle validation (some cases still inspect Base/subtrees)
        |
C2 filesystem Save::finish -> saved candidate filesystem R1
        |
History stage_changes transaction -> exact stage token + captured context/R1
        |
History commit_staged transaction
  recheck CURRENT Branch head/Base
  check/insert Commit; conditionally advance Branch; remove exact stage
        |
authenticated Committed/UpToDate outcome
        |
Workspace records KNOWN outcome before local C5 installation
        |
reconcile G1 with live G2 -> checked capture release/maintenance/fund finish
        |
SDK completed Commit report
```

The initial branch check occurs before filesystem construction. File-content
and metadata Saves already completed during Workspace preparation. Stage
insertion validates captured roots/profile/scope; it does not require that the
branch's current head still equals the earlier snapshot. The later publication
transaction performs that current-head/Base check. Concurrent movement can
therefore leave a saved candidate and stage; there is no automatic rebase,
command replay or guessed candidate deletion.
The filesystem Save owner begins before prepared-row reception. Directory
patch/declaration metadata work can happen while receiving that exact body;
this differs from SaveFile's complete-input-spool-before-save ordering.
RowSpool cleanup and the Service's pre-finish checked abort are explicit
failure boundaries, not proof that a later saved candidate was never created.

```text
native Workspace.stage
  capture/prepare -> History StageChanges
  C2 candidate filesystem + retained History stage
  validated StageSelector returned; Branch head is not published
        |
native Workspace.commit_staged(exact selector)
  claim Submission/selector once
  History CommitStaged conditionally publishes retained candidate
  no repeat file/filesystem construction
  same known-outcome local C5 installation
```

Public SDK Commit is the composite route. This trace does not add public SDK
Stage/CommitStaged methods or change arbitrary Exec into a hidden carrier.

Sources: [SDK Commit](../../../../crates/layerfs-api/sdk/src/workspace.rs#L98),
[Workspace operation](../../../../crates/layerfs-workspace/src/commit/operation.rs#L16),
[Service History implementation](../../../../crates/layerfs-server/src/service/save/catalog.rs#L112),
[filesystem Save/receive/update](../../../../crates/layerfs-server/src/service/save/filesystem.rs#L49),
[prepared-row roles and metadata](../../../../crates/layerfs-server/src/service/save/prepared.rs#L185),
[C1 filesystem update](../../../../crates/layerfs-content/src/filesystem/update.rs),
[History staging](../../../../crates/layerfs-history/src/sqlite/staging.rs),
[History Commit](../../../../crates/layerfs-history/src/sqlite/commit.rs).

### 13.3 What actually reaches SQLite

```text
file/metadata/symlink Saves       filesystem Save
          |                            |
          +-----------+----------------+
                      v
                 C1 logical objects
                      |
                 C2 consumer Save
                      |
      encode/deduplicate/place persistent objects and object packs
                      |
                 C2 SQLite content Store
                      |
                 Save finish/watermark
                      |
          canonical candidate filesystem root R1
                      |
                 History SQLite catalog
                  stage -> conditional Commit/Branch publication
```

C2's current six tables are `store_policy`, `saves`, `object_packs`,
`metadata_value_groups`, `objects` and `content_signatures`. Persistent pack
bytes live in `object_packs.data` BLOBs; object rows locate selected content
and Save ownership/watermarks govern publication. C1 uses typed provider and
consumer boundaries rather than writing SQL directly.

History separately owns branches, commits, staged candidates and serial
allocation, including `workspace_stages`, `branches`, `commits` and
`scope_allocator`. An ordinary successful SDK Commit completes canonical Saves
and History publication before local C5 reconciliation is reported complete.
It is more than a Stage or upload-only benchmark. A benchmark must name which
of these boundaries its timer and verifier actually include.

C2 and History are separate SQLite owners, not one atomic cross-database
transaction. Both use the current MEMORY-journal/synchronous-OFF/temp-memory
profile with zero busy timeout and foreign keys enabled. Workspace backing
adds no fsync. Checked transaction/Save completion is not a power-loss durability
claim or a new WAL/crash-recovery contract.
History's short nonblocking transactions separately classify rollback,
known completion and unknown/quarantined connection state. An uncertain SQL
terminal is not safe permission to replay publication or reset an owner.

Sources: [C2 schema](../../../../crates/layerfs-storage/sql/schema.sql),
[C2 Save finish](../../../../crates/layerfs-storage/src/cas/store.rs#L554),
[C2 owner sealing/publication](../../../../crates/layerfs-storage/src/cas/lifecycle.rs#L239),
[C2 ownership watermark](../../../../crates/layerfs-storage/src/sqlite/ownership.rs#L155),
[C2 connection profile](../../../../crates/layerfs-storage/src/sqlite/connection.rs),
[History open/schema](../../../../crates/layerfs-history/src/sqlite/open.rs),
[History staging](../../../../crates/layerfs-history/src/sqlite/staging.rs),
[History transaction custody](../../../../crates/layerfs-history/src/sqlite/rows.rs#L72).

## 14. C5 installs the captured set without overwriting G2

```text
KNOWN canonical G1 outcome
        |
outside State gate: selected G1 I + known F/M observations + E/inverse keys
precharge row/delete/map ownership
        |
acquire current publication gate
recheck Branch/Base, Submission identity, generation/revision
        |
for EACH captured dirty inode
        +-- regular file AND live revision == captured revision
        |      delete its selected E/R/L as appropriate
        |      install saved content as inline Base F1
        +-- regular file AND live revision changed in G2
               retain later extents and existing nonzero OLD Base F0
               install only compatible saved metadata/context
        +-- symlink/directory
               keep kind-specific target/metadata/namespace representation
        |
remove D|G1|* only; keep D|G2|*
        |
ordered affected keys -> actual index/compaction work
        |
publish current selection; advance filesystem context R0 -> R1
record installed_revision
        |
checked G1 capture release -> maintenance -> finish SAME captured fund
```

Unrelated files, records and hot cursors remain shared. Bulk canonical work
happens outside the State gate, while actual affected C5 page work and
compaction still occur under it. No Workspace reset, remount or process-wide
pause is needed; contention and explicit admission refusals remain possible.
A later-written fresh regular inode whose Base is still the absence sentinel
can adopt its saved content root; preservation of a nonzero old Base is the
case requiring the original source coordinates. Symlink target Payload
extents are not collapsed into a regular-file inline Base representation.

### Exact-byte walkthrough with reordered immutable Base

The symbols below are illustrative, not a new test receipt.

```text
R0 -> inode42 -> F0 = "abcdefghij"
offset                0123456789

ordinary READ obtains F0[2,4) = "cd"
ordinary WRITE copies those bytes to [8,10)
exact read provenance permits Base source2 there
ordinary WRITE stores literal "XY" at [2,4)

captured G1 E spans
  [0,2)   Base F0 source0  -> "ab"
  [2,4)   Packed XY        -> "XY"
  [4,8)   Base F0 source4  -> "efgh"
  [8,10)  Base F0 source2  -> "cd"

G1 exact bytes: "abXYefghcd"
```

Capture G1, hold an explicit old view, then let G2 write `Q` at `[7,8)` while
G1 construction is in flight.

```text
CAPTURED G1                                LIVE G2
-----------                                -------
Base F0 + XY + backward Base                Base F0 + XY + Q + backward Base
"abXYefghcd"                               "abXYefgQcd"
       |                                          |
SaveFile v2 -> F1 = G1 bytes                       |
C1 filesystem/History -> R1                        |
       |                                          |
       +--------------- C5 recheck ---------------+
                         |
live revision changed: retain G2 Base F0 and its source coordinates
State.base advances to filesystem R1; D|G1|42 removed; D|G2|42 retained

WHY F0 MUST REMAIN:
  G2 final [8,10) says source [2,4)
  actual F0[2,4) = "cd"
  rebinding to F1 would read "XY" and corrupt the later live file
```

A later Commit captures G2. If no G3 write intervenes, F2 is
`"abXYefgQcd"`, filesystem context becomes R2 and matching reconciliation
collapses live content to `[0,10) Base source0 in F2`. The explicit old G1 view
still reads `"abXYefghcd"`. Old XY/Q packs become eligible only when all current
and selecting old references permit checked retirement.

```text
truncate/regrow example
  original:             "abXYefgQcd"              length10
  truncate to7:         "abXYefg"                 length7
  extend to10:          "abXYefg\0\0\0"           length10
  WRITE "!" at12:       "abXYefg\0\0\0\0\0!"     length13
```

Newly exposed Zero bytes do not resurrect a removed suffix. During SaveFile,
the declared Zero bytes are real replacement input work.

Sources: [C5 captured-set implementation](../../../../crates/layerfs-workspace/src/commit/active_reconcile.rs#L26),
[extent cutting](../../../../crates/layerfs-workspace/src/backing/active/extents.rs#L262),
[checked maintenance](../../../../crates/layerfs-workspace/src/backing/active/lifetime.rs#L112).

## 15. Live progress has specific admission and lock boundaries

Public control, Workspace upstream admission, the daemon session mutex and the
local publication gate protect different owners.
Primary/metadata/in-flight upstream slots belong to the shared Host across
its managed Workspaces. State and active publication gates belong to one
Workspace; neither scope should be inferred from the other's limit.

```text
SDK Exec/Commit/PinView/lifecycle request
        |
authenticated selector + lifecycle.slot.try_lock
        |
held for that full public dispatch
another new SDK control operation -> Busy
existing/background process FUSE callbacks -> separate projection route

Commit thread                              concurrent FUSE thread
-------------                              ----------------------
shared Host primary remote permit          needs only local selected facts?
        |                                         +-- yes -> local G2 work
daemon authenticated-session Mutex         needs Service Base BYTES?
        |                                         +-- primary occupied -> Busy
held across complete RPC                   needs Inspect / ReserveInodes?
        |                                         |
Service resolves Base locally              try primary first
        |                                  if occupied AND in_flight>0:
C1/C2 canonical work                          admit one secondary metadata call
        |                                         |
checked terminal                           same daemon session Mutex
        |                                         |
release Mutex and primary                  may WAIT, then original deadline check
```

`OperationGuard::remote` takes the primary slot and charges 128 KiB scratch.
`metadata_call` can reuse its own admission, take the primary, or admit one
secondary read-only Inspect/serial-reservation call when the primary is occupied
and `host.in_flight > 0`. That observed delivery counter includes mutex waiting.
ReadFile bytes require the primary; the secondary is not a second bulk transport
or construction worker.

The daemon holds one `std::sync::Mutex` around its authenticated session for
the whole upload/result. Mutex acquisition is not deadline-preemptive. The
original absolute deadline reaches the client; after acquiring the mutex,
remaining time is recalculated and can expire before BEGIN. No extended timer
or guaranteed zero waiting follows from the secondary admission. In-flight
counts include calls waiting for this mutex, not only network progress.

Cold lookup/list/readlink and fresh creation can need that metadata path. New
identity reservation currently requests one serial; it remains consumed if
later publication refuses. A secondary query can finish after local revision
movement and then fail its recheck with Busy. There is no automatic retry.

Arbitrary Exec runs `/bin/sh -c` in the mount and does not itself Commit.
Deadline/output failure attempts to kill the process group and retains an
unknown result; earlier acknowledged mutations remain. A new public Exec
cannot enter the held lifecycle slot during public Commit, but existing
processes and native FUSE requests do not acquire that control slot. The claim
that live Commit preserves the Workspace is therefore narrower than a promise
that every public command or every remote callback always advances concurrently.

Sources: [daemon public slot](../../../../crates/layerfs-daemon/src/control.rs#L307),
[Workspace admission](../../../../crates/layerfs-workspace/src/runtime/state.rs#L704),
[Inspect admission](../../../../crates/layerfs-workspace/src/runtime/host.rs#L650),
[session mutex](../../../../crates/layerfs-daemon/src/run.rs#L98),
[transport call](../../../../crates/layerfs-daemon/src/transport.rs#L76),
[absolute deadline](../../../../crates/layerfs-bridge/src/adapters/native/client.rs#L45),
[Exec process lifecycle](../../../../crates/layerfs-daemon/src/execution.rs#L55).

## 16. Failure custody and the scoped native local resume

| Observed boundary | Required ownership/disposition |
| --- | --- |
| Before mutation publication | Previous selected bytes remain; checked candidate abort or explicit failed charged custody. |
| After mutation publication | Selected mutation/receipt/handle survive; notification or cleanup error does not undo published state. |
| Known before canonical Commit | Retain precise saved observations and Submission failure; no fabricated canonical result. |
| Unknown/lost canonical result | Keep selectors, pins, observations and charge; do not resend, rebase or delete by assumption. |
| Known canonical outcome, local C5 failure | Record outcome first; retain saved G1, later G2, old views and completion owners as `KnownCommitLocalFailure`. |
| Installed revision recorded | Never pretend installation did not happen to rerun the canonical command. |
| Failed physical identity/block/unlink proof | Keep exact charged owner and stopped/error custody; no speculative refund. |

The explicit native same-selector resume is limited to a known outcome in
Reconcile, `KnownCommitLocalFailure`, and no installed revision. It validates
the same known outcome and performs only local installation.

```text
explicit SAME StageSelector / retained native Submission
        |
known canonical outcome + eligible Reconcile state + installed_revision=None
        |
failed candidate repair permitted only if:
  accounting complete; identity exact; never ready/published
  no direct physical pins; reserved=0; allocated<=4096
  SAME completion fund Arc; no incompatible stopped custody
        |
        +-- not eligible -> retain Busy/error custody
        +-- eligible -> checked release into SAME fund -> local C5 only
                        CANONICAL COMMAND IS NOT REISSUED
```

Repeating public SDK composite Commit is not this resume API. A retained
Submission prevents a new capture. The native clean-refund recovery proof and
SDK known-result/held-lease custody proof have different scopes and must not
be pooled. Some retained capacity failures still permit eligible G2 progress;
a failed Commit is not a universal process pause.

Sources: [known/unknown completion](../../../../crates/layerfs-workspace/src/commit/completion.rs#L147),
[native recovery conditions](../../../../crates/layerfs-workspace/src/commit/completion.rs#L366),
[eligible active repair](../../../../crates/layerfs-workspace/src/backing/active/pages.rs#L370),
[lifetime custody](../../../../crates/layerfs-workspace/src/backing/active/lifetime.rs#L14).

## 17. Retirement, compaction and checked refund

```text
current publication retires a physical page
        |
record lifetime [birth revision, retire revision)
        |
actual selecting pin in this interval?
        +-- none -> verify identity + actual blocks -> unlink -> refund
        +-- yes  -> retain in that selecting pin's cohort
                         |
                   final pin release
                         |
                   visit its cohort only
                    +-- another selector -> migrate owner
                    +-- none -> checked unlink/refund
                    +-- failure -> charged failure custody
```

Current `P` disappears only when no current `R` fragment selects a slot in that
logical page. Large payloads use `L` prefixes and retained owners. Empty current
inverse references do not authorize refund while an old view still needs the
physical data.
An unsealed current tiny tail can retain its `P` locator/page even with no
current `R` references so a later tiny append can reuse it. Ordinary no-pack
publication passes that retained tail to dead-reference pruning; sealed dead
pages can retire. Zero inverse references alone do not prove physical release.

Mixed sealed packs can relocate surviving slots. New `E/R/P` references and
old-page retirement publish together. Ordinary mutation relocation is limited
to at most one source; C5 can pay for additional selected work. Under pressure
its planner can scan all selected `P` locators/packs/inverse references and sort
partially dead candidates. Larger-payload maintenance and shared quota-status
checks separately scan retained payload registry owners. These are real costs.

Sources: [pin cohorts](../../../../crates/layerfs-workspace/src/backing/active/retirement.rs#L70),
[reference reclaim](../../../../crates/layerfs-workspace/src/backing/active/reclaim.rs#L13),
[pack compaction](../../../../crates/layerfs-workspace/src/backing/active/compaction.rs#L257),
[large-owner maintenance](../../../../crates/layerfs-workspace/src/backing/active/lifetime.rs#L62),
[quota registry scan](../../../../crates/layerfs-workspace/src/backing/payload.rs#L314).

## 18. Close, unmount and deletion have different outcomes

User FD close releases a handle; Flush checks retained coherence failure.
Neither saves the Workspace. FUSE fsync/fsyncdir return EOPNOTSUPP. Checked
unmount drains/detaches projection ownership and preserves dirty backing.

```text
native Workspace::close_clean_until
        |
dirty / Submission / mount / active operation / handle / lookup / external pin?
        +-- yes -> Busy, retain owner; no automatic Commit/discard/refund
        +-- no
              |
stop semantic admission
take/finish unused live fund; clear optional ReadOrigin
              |
ActiveBacking.close_clean: refuse frozen views/stopped custody; checked drain
              |
close legacy arena -> reclaim eligible incarnation payload segments
verify no retained payloads
              |
close verified EMPTY private directory; remove owned empty mount leaf
              |
mark closed; retire exact registry entry and resident tables
```

A failed leaf removal retains registry/count/table ownership for inspection.
Unknown or replaced directory identity is a failure, not cleanup permission.
Public SDK exposes unmount but no `close_clean` method. Graceful daemon stop
unmounts and attempts native clean close before disabling control; dirty or
uncertain ownership keeps the endpoint available for explicit resolution.
Sandbox deletion removes its container/volume under a separate owner and does
not establish a successful per-Workspace checked refund.

```text
attachment failure
  original cause + cleanup progress + exact retained resources
        |
one checked cleanup at original absolute deadline
  active owner -> metadata arena -> verified backing directory -> mount leaf
        +-- verified success/absence -> remove exact failed registry entry
        +-- error/unknown -> retain resources and original/current cause
```

Sources: [native close/unmount lease](../../../../crates/layerfs-workspace/src/runtime/lifecycle.rs#L73),
[attachment cleanup](../../../../crates/layerfs-workspace/src/runtime/attachment.rs#L43),
[verified directory close](../../../../crates/layerfs-workspace/src/backing/directory.rs#L149),
[FUSE lifecycle details](fuse_workflow.md#13-checked-unmount-stops-drains-detaches-and-proves-absence).

## 19. Retained bounds, costs and proof scope

| Dimension | Current bound/behavior |
| --- | --- |
| RAM Budget | Default 8 MiB configured charged admission; shared Host owner, not exact RSS |
| Disk quota | Shared allocated plus reserved physical charge; owner-specific files/refunds |
| Tiny regular input | 1–128 bytes, shared pack |
| FUSE read/write declaration | 128 KiB per request/callback |
| Native owned mutation | 8 MiB input |
| Logical file | 4 GiB |
| Index scan page / branch level | 128 entries / maximum level 7 |
| Hot state | 8 cursors, 64 decoded nodes, 1 MiB selected reservation |
| Semantic handles | 128 |
| Active lower captures / selecting revisions | 32 / 160 |
| Public Submission / held-view leases | One / 32 |
| Resident Node growth | Charged 256-Node chunks; not a total-file cap |
| Active locator | 65,536 bytes / 256 components; inline path still 4096 bytes |
| Canonical direct LogicalPath | Separate 4096-byte / 256-component bound |
| Symlink | Native target up to 4096 non-NUL bytes; FUSE return stricter at PATH_MAX |
| Callback deadline | Existing 10-second observation profile |
| Canonical construction | One worker for Commit/capture/snapshot; Init has separate exception |
| Workspace durability | No fsync/WAL/crash-recovery addition |

For D captured dirty identities, E_f final extents for file f, K affected C5
keys, J pressure-compaction candidates, and N pack references:

| Work | Source-derived cost terms |
| --- | --- |
| WRITE time | Input/comparison bytes + affected extents/inverse refs + reached keys/pages + physical verification + Node/owner/pin navigation + reached payload-registry scans |
| Tiny backing space | Packed live records plus index/owner overhead and selected old physical versions; not one full segment per byte write |
| Upload memory | O(E_f) extents and descriptors plus bounded source window and one pack |
| Grouped pack reads | Sum of distinct packs per bounded window, <=N; worst Theta(N) |
| v2 lowering | O(E+L+Q log E) navigation/input plus actual Base replacement reads and C1/C2 construction; Q is origin lookup/window work |
| C5 memory | O(D + sum E_f + K), plus changed-name/declaration bytes and actual retained owners |
| C5 ordering | Affected-key sorting can add O(K log K); pressure sorting O(J log J) plus selected scans |
| Total resident state | Nodes/full paths/name memo/cookies + physical/payload/retirement owners + frontier + hot/source/provenance windows + operation/response/lease owners |

Process RSS, kernel page cache, VM/backend/device/host caches and Service/C1/C2
owners are additional resource domains. Exact `st_blocks*512` demonstrates a
specific physical-allocation observation, not a phase RSS peak. The changes
reduce per-write physical amplification, unrelated index rewrites and some
Commit navigation work. They do not declare constant total memory or unlimited
file count.

The [retained functional report](../PREMERGE-FUNCTIONAL-COMPLETION-20260929.md)
covers its selected 3×3 schedules, separate 8192 case, exact old/new bytes,
headroom and scoped failure custody at its pinned source. It does not prove
arbitrary overlap permutations or every route. The frozen matched numeric
control remains NOT_RUN and historical numeric rows retain INELIGIBLE.
Additional memory qualification is deferred to [#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283)
under the owner's direction. [The integration checkpoint](../PHASE45-INTEGRATION-CHECK-20260929.md)
has separate regression identity and pending work. #256 many-file scale and
#270 pure-move C1 Commit remain NOT_PROVED; the 32 KiB pinned SDK-read observation
remains unresolved. This documentation update runs no physical benchmark,
approves no merge and promotes no receipt.

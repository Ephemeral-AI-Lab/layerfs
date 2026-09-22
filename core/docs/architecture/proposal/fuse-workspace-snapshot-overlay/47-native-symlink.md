# Native Workspace symbolic-link creation

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Implementation parent: `2fc2e8d7a100b812a46753c4b35d383bedac448d`.
> Frozen product input seal: `af105d8725996152b1b94082f82ad2d0d5aaf57fe846ccb24c2303bd15f9501d`.
> Exact implementation commit: `9060c26bcc3e905e031415da20cec54352b94192`; [confirmation](evidence/native-symlink/commit-confirmed.json).

This round adds `Workspace::symlink(parent, name, target, deadline)`, returning
`NodeAttributes` with one Local lookup reference and no file handle. The operation
requires LocalEdit access, uses mode `0777`, and reports the exact target length.
Targets follow the C1 grammar: 0..4096 opaque bytes without NUL, including empty
and non-UTF-8 targets. NUL is `InvalidInput`; a target over4096 bytes is `Capacity`.
The stored name still follows the existing UTF-8 component/path grammar.

## Admission and atomic publication

The operation shares `filesystem/create.rs` with mkdir and regular-file creation.
Kind and handle admission are separate: a symlink creates an I record with no
handle slot. It requires parent write/search permission and an absent name;
existing names return `Exists`. Node capacity, the complete prepared-request
bound, and the selected baseline/revision/generation/root stamp are checked
before reservation and again before publication.

Native creation while mounted returns `Unsupported` before inode reservation or
target acquisition. The mounted check is repeated at publication. One admitted
operation covers the complete attempt, so `reserve_mount` cannot pass an active
operation between these checks. No SYMLINK callback or projection permit is added.
The native empty-target rule is separate from later Linux syscall qualification.

One exact-scope `ReserveInodes { count: 1 }` precedes target acquisition. Its
returned scope/range must match. A lost or uncertain reservation is not replayed;
a successful reservation remains consumed if a later step fails. Nonempty target
bytes then enter the existing `PayloadHost::acquire` with the same deadline and
stopping state. This uses the already admitted operation, not a nested call to
public `own_payload`. The acquisition window is released before the metadata
writer and publication window are taken.

The child, one parent binding, parent mtime, D-parent/D-child markers, generation
accounting and one Local lookup reference become visible together. Failed
publication exposes no partial name or reference. Any acquired payload or
candidate metadata remains under the existing checked reclamation ownership;
neither immediate successful cleanup nor serial reuse is inferred from an error.

## Backed target and selected reads

The existing160-byte I record uses reserved byte26 as its symlink flag; fresh
identity remains byte25. A symlink I record is fresh, uncaptured, mode0777 and at
most4096 bytes, with zero canonical roots; completed saves are recorded separately
in R. Nonempty targets have one
Local piece at offset0 and one payload custody; empty targets have zero pieces
and no payload. The existing16-byte E value adds kind3. No target bytes or
authoritative namespace map are retained in the Node cache.

Namespace resolution checks the E/I kind agreement. Fresh symlink attributes can
therefore be reconstructed after forget/relookup, and directory handles retain
their existing immutable view and cookie policy. Regular-file open, range edit,
write and resize remain kind-restricted and do not follow or modify symlinks.

`readlink` pins the selected overlay/root and first looks for the local I record.
Its bounded target reader verifies the one-piece shape, custody, payload length,
complete read and non-NUL bytes. Local reads require no remote slot. If no local
I record exists, the caller uses `Inspect::Readlink` against the selected canonical
filesystem root. Metadata writer/window guards end before payload I/O or RPC.
The returned bytes keep their explicit allocation charge and operation ownership
until `ReadReply` is dropped. Payload reader errors preserve the embedded
`BackingFailure`, including phase, payload identity and accounting completeness.

## Capture, save and known completion

Fresh symlinks share the D/I frontier and existing R results with regular files.
The captured generation saves a symlink through `ConstructSymlink` with its target
in request metadata and an empty input body, then
`ConstructPortableMetadata { kind: 3, mode: 0777, ... }`. Exact result length and
portable fields are checked, and both roots enter the existing80-byte R record
before Stage/Commit. There is no content-object construction or Service/native
client import in Workspace. [Round45](45-construct-symlink.md) and
[Round46](46-prepared-symlinks.md) define the shared prerequisites.

Prepared lowering emits kind3 I rows and a separate S list. The existing public
`StagePhase::FileSave` phase and `saved_files` count now include regular-file and
symlink content saves; their names, fields and control tags are unchanged.
Source failure observations retain typed backing errors so uncertain accounting
is not relabelled a definite pre-Stage refusal. Save and publication remain
separate; failed/unknown operations retain the existing no-replay rules.

G-born targets are immutable in this scope. They remain readable from their
pinned I/payload while G is saving or staged. No symlink-target edit creates a D1
captured-file representation. Known own G completion removes G-only I rows and
installs the acknowledged filesystem root for later canonical Readlink. A symlink
first created in D1 remains fresh and keeps its piece/custody. Parent-directory
origin substitution reuses D1 E roots through the existing reconciliation path.
Held older directory views keep their own roots until released.

State and Captured each add a `fresh_symlinks` counter. It counts creation events
in that generation, resets at capture, and must match the captured S list. Both
namespace admission and sibling file-write admission use the same exact envelope:

```text
H + 73*non_directory_dirty + 34*dirty_directories + encoded_name_bytes + T
H = 195 without a head; 228 with a head or a pending G that can create one
T = 9 + 8*(F+S) when S>0
    7 + 8*F     when S=0 and F>0
    5           when F=S=0 and directory records exist
    0           otherwise
```

Final lowering checks its actual records against the captured counts and byte
accounting. The128 combined inode/name limits and32768-byte complete request
limit remain; target bytes are saved separately and are not counted as inline
prepared inode data. There are no smaller hidden Commits.

## Resource bounds

These are source-derived bounds, not compiler-layout, allocator, RSS, cgroup or
measured stack-peak results. No selected allowance is increased.

- A nonempty target uses one existing payload record and one segment:4096-byte
  header plus one4096-byte aligned data area,8192 disk bytes in total. An empty
  target creates neither. Existing per-record accounting and payload limits apply.
- Publication retains one E update and four global updates. A conservative bound
  is `2*(4*3)+1 = 25` immutable global pages, `2*4+1 = 9` E pages and one piece
  page:35 immutable pages,36 slots including custody, and at most two new ledger
  pages. This fits the128 temporary-slot and137 charged-page candidate allowances.
- E, piece-building and global-update vectors are sequential. Their existing
  dominating working bounds remain inside the640-KiB writer allowance. The target
  reader reuses the existing at-most1024-piece collector under that writer charge;
  it verifies that this inode actually has one piece. Materialized target bytes
  are at most4096 under the existing128-KiB I/O allowance before RPC. Readlink's
  returned allocation is charged separately; this is not wire-size-as-heap proof.
- Reclamation reaches at most seven frames through global/N/E or five through
  global/I/piece/custody, within the existing12-frame bound. Completion still uses
  the256-row/26-slot reconciliation reservation and64-page allowance. I/E/R widths
  and the shared index's permitted depth/role limits do not change.
- F+S still share at most128 serial elements. State/Captured gain one `usize`
  each; the in-memory Inode flag and borrowed Creation variant need actual layout
  comparison. Submission's captured state remains inside its existing32-KiB
  descriptor reservation, without using that reservation for I/O or target bytes.

The [23-type compiler comparison](evidence/native-symlink/layout/layout-01.json.gz)
uses exact before/after Linux AArch64 binaries and fully qualified DWARF paths.
State272→280, Inner456→464, Captured72→80 and Submission984→992 each grow8 bytes.
Creation16→24 grows8 bytes and alignment4→8. The other18 selected types retain
size/alignment, including Inode136, Piece48, Node4320 and Handle72. State is nested
inside Inner and Captured inside Submission: these are not four independent
allocation increases. No heap, RSS, cgroup or stack-peak result is inferred.

## Verification state and qualifications

Ten fixed selections passed on their first attempts, with all19 checks passing.
The six native symlink selectors are supplemented by one typed backing-failure
regression and existing create semantics/create successor/mkdir successor checks.

| Selection | Driver seconds | Complete command seconds |
| --- | ---: | ---: |
| create-semantics-01 | 1.273071333 | 1.385876916 |
| create-successor-01 | 1.280590625 | 1.382736750 |
| mkdir-successor-01 | 1.067684792 | 1.179881833 |
| symlink-backing_failure-01 | 1.033250542 | 1.132009875 |
| symlink-capacity-01 | 10.471748750 | 10.564129334 |
| symlink-refusals-01 | 1.422983166 | 1.534022166 |
| symlink-reserve_denied-01 | 1.171039208 | 1.292735459 |
| symlink-reserve_unknown-01 | 1.111690375 | 1.221805625 |
| symlink-semantics-01 | 3.889782417 | 4.016631000 |
| symlink-successor-01 | 1.369977625 | 1.485362000 |

Nine selections require native clean-close markers. The intentionally corrupted
payload case retains a failed native owner and uses external runtime/backing
teardown only. Stage performs the first corrupted read, preserving exact
BackingFailure Read/InvalidData, payload identity,8192 retained bytes and incomplete
accounting in both cause and source_failure. It returns Unknown/FileSave with
zero saved files/metadata and no constructor or Stage RPC. Restoring the header
is not owner recovery. Its Busy close assertion also has a live lookup reference;
that assertion alone does not isolate failure custody, which is checked separately.

Each selection uses a byte copy of the closed master and fresh live C5 authority,
a60-second complete budget, the same10-second operation deadline, one construction
worker and an actual two-CPU Linux runtime. No forced cleanup or failed attempt
occurred. These are functional elapsed times, not eligible cold/performance rows.
[Functional index](evidence/native-symlink/functional-index.json.gz),
[caller review](evidence/native-symlink/caller-review.json.gz) and
[archive manifest](evidence/native-symlink/archive-manifest.json) plus the append-only
[stdout/stderr supplement](evidence/native-symlink/log-supplement-manifest.json) retain commands,
identities, logs including client/fixture stderr, and cleanup qualifications.

Locked/offline Rust1.85.1 whole-core checks passed: host702 tests/3 ignored;
Linux700 tests/156 ignored; both all-target Clippy commands deny warnings; fmt,
host binaries/examples, host/Linux product checks,255-file boundary guard and
six boundary self-tests all pass. These builds/checks/samples ran serially with
Cargo jobs2 and Docker CPUs2. The seven new Linux ignored tests were separately
executed by the live selectors above. See the
[check index](evidence/native-symlink/checks-index.json.gz).

The fixed capacity workload is93 new255-byte names, first target4096 bytes and
the remaining targets empty, plus one existing4-byte file edit. Its head-bearing
prepared request is `32522 = 228+34+73+9+93*(73+265+8)` bytes. A94th such link
would require32868, exceeding32768, and must be refused before reservation or
publication. This is a separate workload from [Round43](43-native-create.md)'s
failed8-MiB file-capacity gate; that failure remains open and is not rerun or
reclassified by this round.

Kernel SYMLINK and its notification/custody rules, Linux empty-target behavior,
larger full-input admission, the unchanged prepared DSH upload followed by one
full explicit Commit, later incremental Commits, hard memory qualification and
matched R6 remain open. No cache or performance qualification is implied.

## Source comparison

Production LOC: **112586 ->112883 (delta +297)**; reference65417 ->65417 (+0),
core47169 ->47466 (+297), across255 core product files. This adds native symlink
behavior and reuses existing storage/save paths; it does not retire reference code.
The unchanged production counter `b5b9617d08204977176302311e0b2c72a811b420` counts
nonblank/noncomment product Rust and runtime SQL, excluding inline test modules,
external tests, docs, fixtures and tooling. Exact parent/final-staged snapshots
are compared before committing and the resulting tree is confirmed afterward.
The [inventory](evidence/native-symlink/source-loc.json) pins the frozen product
seal and unchanged reference tree; the next continuation pins the commit result.

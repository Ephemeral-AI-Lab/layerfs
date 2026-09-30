# R0 implementation interfaces and authority

> **Status: Current planning checklist; no release candidate exists.**
> Implementation freeze, 2026-09-30. Audited parent source:
> `7edddbdb8e8512627aed0ed42533ef099d802384`.
> These are prospective implementation contracts. They advertise no implemented
> capability, physical-memory PASS, benchmark result or release admission.

The owner's implementation assignment supersedes the research-only task language
in the preserved [packet](../../architecture/proposal/bounded-workspace-implementation-20260930/README.md).
Historical observations, receipts, forecasts and banners remain historical.
This freeze resolves shared record and ownership choices for R1–R7. A later
format change updates this contract before its producers/consumers are enabled.

Read the [canonical contract](R0-CANONICAL-CONTRACT.md),
[resource audit](R0-RESOURCE-AUDIT.md), [runtime contract](R0-RUNTIME-CONTRACT.md),
[current checklist](CHECKLIST.md) and [append-only log](IMPLEMENTATION-LOG.md).
SC-01–08 are design obligations; larger scenario tiers remain unselected.

## 1. Source, ownership and scope

The primary checkout is clean `main` at
`ffdfa022f21930f3e7325b95e6ae5c2b11b7d2a0`, 377 commits behind fetched
`origin/main`. Published main and the research pin are the same commit. The
implementation branch is `codex/issue287-implementation`, managed worktree
`/Users/yifanxu/.codex/worktrees/issue287-implementation/layerfs`.
The origin is `https://github.com/Ephemeral-AI-Lab/layerfs.git`.
No primary/unmerged/foreign worktree source or target is reused or changed.

The complete owned draft, its source/LOC captures, scenario catalog, report
extension, handoff and two AGENTS routing additions were copied from
`/Users/yifanxu/.codex/worktrees/issue286-phase-b/layerfs`. Its `output/` is
excluded. Source AGENTS/report diffs were inspected against the identical
destination parent; only the owned additions are present. Fresh read-only issue
captures are under [evidence](evidence/). They preserve original requirements
and comments; old directives are not competing assignments.

| Responsibility | Current owner | Exclusive file scope |
| --- | --- | --- |
| Integration, publication, private v3/Commit and coordinated Bridge contracts | Root implementation agent | Workspace and Bridge; checkpoint/checklist/log; issue #287 |
| R0 resource audit | `r0_resources` | `R0-RESOURCE-AUDIT.md` only |
| Independent canonical reference | `r0_oracles` | `R0-CANONICAL-CONTRACT.md` and `oracle/` only |
| R0 runtime/provider audit | `r0_runtime` | `R0-RUNTIME-CONTRACT.md` only |

Product work is assigned only after R0 publication, with explicit non-overlapping
files and shared interfaces. Root owns acceptance/LOC/commit/push/issue edits.
Benchmark runners, campaigns, performance qualification and #288 updates are
outside this assignment. No donor candidate is automatically adopted.

## 2. Private v3 pages and records

Preserve 4,096-byte pages, 128-byte header, 3,968-byte body, SHA-256 checksum at
80..112 with that field zeroed, 32-byte nonzero incarnation, and 16-byte
`PageRef=(id:u64,epoch:u64)`. The baseline header offsets are in
`backing/active/page.rs`. All integers are checked big-endian; reserved bytes,
padding and unused page tail are zero. Generation/id/epoch are nonzero. An absent
tree uses an explicit empty-root descriptor, never a fabricated authenticated page.

V3 page kinds allocate 5/6 for interval leaf/branch, 7/8 for catalog leaf/branch,
9/10 for owner leaf/branch and 11 for pack. Their version is 3; node magic is
`LFSAIDX3`, pack magic `LFSAPAK3`. Baseline kinds 1–4 retain their existing
versions/magic. Header 112..120 is file serial for interval pages (zero for
Workspace catalog/owner pages); 120..128 is nonzero selected scope epoch.
Catalog keys start with the closed table discriminator below. A reference is
checked against kind/incarnation/scope/epoch before decoding any record.

The key/value widths are logical encoded widths, not Rust allocation sizes.
`Root` is 32 bytes; `Ref` is 16. A selected version, generation, revision, owner
or member token is a checked nonzero u64. Serial is in 1..=i64::MAX.

| Table tag | Record/key | Exact value in field order |
| --- | --- | --- |
| 1 | InodeVersion / serial8 | version8, generation8, length8, kind1, flags1, canonical-profile2, mode4, seconds8, nanos4, links4, intervals16, metadata-root32, construction-Base32, captured-parent-token32, edit-policy32, construction-fact16, reference-owner16 =224 |
| 2 | NameBinding / parent8 + name-length2 + component | serial8, kind1, flags1, reserved6 =16; tombstone is serial0/kind0 with tombstone flag |
| 3 | ParentFact / directory8 | parent8, binding-revision8, generation8, kind1, flags1, reserved6 =32 |
| 4 | DirtyFact / generation8 + serial8 | version8, role-flags8, accepted-revision8 =24 |
| 5 | SavedFact / generation8 + serial8 + version8 | content32, metadata32, length8, member8, flags8, unit-completion16 =104 |
| 6 | DeclarationOrphan / generation8 + serial8 + version8 | version8, generation8, length8, kind1, flags1, profile2, mode4, metadata32, reference-owner16, roles8, reserved8 =96 |
| 7 | PayloadLocation / owner8 + epoch8 + selector8 | native owner8, epoch8, pack-or-segment8, offset8, length8, selected-location-epoch8 =48 |
| 8 | OwnerFact / owner8 + epoch8 | owner8, epoch8, kind2, phase2, flags4, birth8, retire8, allocated8, reserved8, device8, inode8, references8, outgoing-edges16, next-edge8, failure4, reserved20 =128 |
| 9 | CleanupJob / job8 | owner8, epoch8, root16, phase2, flags2, failure4, next-edge8, continuation16, fund8, allocated8, reserved16 =96 |
| 10 | IssuedReference / reference8 + epoch8 | kind1, flags1, reserved6, context8, serial8, version8, owner8, owner-epoch8, generation8, revision8, byte-charge8, actor-epoch8, root16, cookie-root16, reserved16 =128 |
| 11 | DirectoryCookie / handle8 + epoch8 + cookie8 | context8, directory8, page16, slot2, name-length2, flags4, view-owner8, owner-epoch8, reserved8, component0..255 =64..319 |
| 12 | UnitCompletion / generation8 + unit8 | the exact 96-byte FileSet completion below |

Tag11 checks its component length before copying. Public capability tokens remain
bounded; a batch limit
does not cap the total directory. Changing a fixed record width requires a new
private layout version; variable records reject inconsistent length before allocation.

An interval has key destination-start8 and value end8, kind1/reserved7,
source-token32, source-offset8, source-limit8 =64. Kinds are canonical1,
captured-parent2, payload3, zero4. Captured token is generation8/serial8/version8/
context-id8; payload token is owner8/epoch8/selector8/declared-length8. Zero has
zero token/offset/limit. Require start<end, exact coverage, source bounds and
immutable context/owner authorization.

Prospective interval node layout revision2 corrects the R0 draft before any v3
product exists. The old proposed48-byte branch layout1 is not admitted by the
target codec. An authenticated node's56-byte body header is layout:u16=2,
level:u8, flags:u8=0, local-count:u16, reserved:u16=0, lower8, upper8,
interval-count8, replacement-bytes8 (including Zero), zero-bytes8 and
captured-span-count8. Leaf level is0; branch level is1..7. One root read
establishes checked structural height without an owner-flag convention.

Each branch child contains lower8, upper8, interval-count8, replacement-bytes8,
zero-bytes8, captured-span-count8, source-kind-bitmask8 and Ref16 =72. Leaf
key8/value64 is also72 bytes. The3,968-byte body minus56 header holds54 cells,
with24 unused zero bytes at maximum occupancy. Child aggregates compose by
checked sums on modified paths. Zero<=replacement<=covered bytes; captured
spans<=intervals. Provenance uses four source-kind bits; reserved bits refuse.
Summaries/level/bounds are checked against children at construction and by the
independent verifier. No mutable side table or tag13 aggregate population exists.

The monotone Commit pass derives exact emitted Base/Local/Zero descriptors and
construction-run/EOF totals once from the selected current delta and terminal
source contexts. Hot writes do not rescan the delta to recompute coverage;
neither this pass nor opaque parent spans enumerate historical predecessor leaves.

Two boundary paths and two builder pages per level are admitted. Split/join
carries untouched/removed subtree roots; it does not emit one update per removed
interval. Page edges and candidate/abort ownership are persisted incrementally.
The inode lease owns preparation; the short publisher applies fixed key changes
to the CURRENT catalog. Capture/install cannot change the leased version's bytes.
First touch after capture uses one exact opaque predecessor span. A crossing
candidate has at most two retained parent boundary spans plus its current effect.

## 3. Captured, result and installation authority

`CapturedToken` names incarnation32, generation8, revision8, context-id8, scope32,
root-serial8, selected catalog Ref16 and immutable resolver Ref16:128 bytes.
Separate fixed selected-context records name location/owner/namespace roots.
Pinning copies these fixed facts and acquires one root owner, not descendants.

`SubmissionInputSeal` names the captured token, exact expected Branch/head/base,
canonical profile, file-set completion root, prepared counts/bytes/digest and
known saved/exception roots. Its checked private record is at most512 bytes.
It is immutable; its exact serial/version joins come from paged facts.

`ReadyTicket` owns a fixed outcome/install/failure record, returned validity-lease
slot, publication path scratch, root/job continuation and protected physical
credits. No final filesystem root is presumed before the one composite request.
`InstallCapsule` is at most512 bytes and names the exact submission/seal, checked
known head/root, selected result/exception roots, baseline epoch and cleanup
owner. Installation selects the actual CURRENT G2 roots under the publisher.
Predictable population-sized allocation after Branch send is forbidden.

Known saves, Unknown file-set finish, Unknown Branch outcome, known local install
failure and installed cleanup failure are different typed states. Accepted writes
and Unknown owners remain charged. Same-selector completion does local work only;
no resend, root-query guessed adoption, rollback or guessed refund.

## 4. FileSetConstruct v1 and prepared cursors

Native operation30 is reserved for FileSetConstruct; request profile1 is retained.
Operation31/32 and runtime responses34–38 belong to the runtime contract.
FileSetCompletion response tag39 is reserved. Retired tags/opcodes are not reused.
Parent canonical roles14/15 and C2 schema11 belong to the canonical contract.
Negotiation precedes BEGIN and dependent effects. An old peer returns Unsupported.

FileSet request extension (after the existing Request envelope) is116 bytes:
version1=1, canonical-policy2=2, declared-kind-mask1, incarnation32,
captured-selection32, generation8, unit-sequence8, members8, body-bytes8,
replacement-bytes8, result-bytes8. All counts/arithmetic are checked; unit and
selection are immutable. Require members1..512 and result-bytes=96*members.
Coalescing target is4MiB including member headers/descriptors/replacement bytes.
A larger singleton uses the same grammar and checked existing8GiB body envelope.
No total workload member cap is inferred from the unit bound.

Each member has a256-byte header: token8, serial8, selected-version8, length8,
kind1, flags1, canonical-policy2, mode4, seconds8, nanos4, reserved4, Base32,
Base-length8, metadata-root32, namespace-profile32, edit-policy32, descriptors8,
replacement8, member-body8, source-selection32, reserved8. Kind regular1,
symlink2, metadata-only3; unknown/reserved flags fail before member consumption.
Fresh Base is all-zero/length0. Metadata-only requires selected nonzero content
and metadata authority and no payload; symlink uses its existing bounded target
semantics. Flags distinguish explicit Base and selected metadata reuse.
Header totals bind actual typed descriptors and exact bytes, never an array.

Regular members nest the existing24-byte Base/Local/Zero descriptor grammar and
exact replacement bytes. The enclosing capability explicitly selects canonical
file-edit-policy v2; standalone SaveFile/SaveFileV2 remain separate v1-compatible
surfaces. Current Zero transfer remains charged. Read-only file authorization
cannot construct a member: validate the request's declared kind/metadata union
against existing construction grants (SaveFile0x08, symlink0x04, metadata0x80),
then require observed kinds to match that declaration. No implicit grant upgrade.

One C2 Save/producer serves a unit. Fresh bytes flow directly to construction;
inherited changed bytes have one required quota-owned replay source. Exact EOF
and all member proofs precede C2 finish. A provisional root is not a known Save.
Results are96-byte rows: token8, serial8, content32, metadata32, length8, flags8.
Only bounded ResultData frames carry them;512 rows are49,152 logical bytes and
cannot inhabit the current32KiB Success frame.

Completion is96 bytes: version1, disposition1, reserved6, selection32, unit8,
members8, result-bytes8, SHA-256 result-digest32. Digest covers exact ordered96-byte
rows; member order is increasing token/serial. The receiver incrementally matches
its captured cursor, persists provisional rows and seals one UnitCompletion.
Gaps/duplicates/surplus/mismatched terminal fail. Terminal loss keeps selection,
received prefix and Save/result custody; no final Branch request is then sent.

Prepared namespace wire versions and current25/12/10/73-byte field grammars are
retained while parser/sink representation changes. Directory access becomes a
header plus binding cursor; headers include exact parent/count/encoded bytes and
selection. Sinks accept begin/binding/end, then fixed identity rows. Compare
remaining aggregate names/bytes before each allocation; seal exact ordering,
section counts, full bytes and EOF before any semantic pass. A wide directory
never becomes one decoded value. The old caller-owned slice adapter delegates
to the same algorithms and has its explicit compatibility scope.

## 5. External state and resource contracts

C1 gets narrow caller-owned `IndexedState` and `OrderingBacking` capabilities.
Indexed tables are closed typed namespaces. Key<=288 bytes, value<=8KiB; keys
contain table/operation selection and typed serial/draft/continuation fields.
Paged queries and flushes obey BOTH128 records and64KiB encoded bytes; keys,
record framing and variable values all count. Mutating iteration requires sealed
phases. Successful cursors advance strictly; no repeated prefix scan.
C2 owns quota-backed metadata-only WITHOUT ROWID scratch, indexed bounded SQL,
small MEMORY-journal transactions, cache/mmap/engine readback and checked cleanup.
No payload CAS or second full-payload staging store enters scratch.

The resident path is prospectively selected only for input+slots<=64KiB,
exact state<=64KiB and a known bounded graph shape. Other inputs select external
state from the first record. There is no catch-and-spill path. Ordered inputs
join monotonically; genuinely different keys use fixed fan-in8/16KiB run windows.
Caches admit entry plus clone/decoded overhead before insertion.

The resource audit derives simultaneous standard72MiB Save windows,32MiB global
SQLite guard,176MiB first-party pool and256MiB physical target. These are proposed
numbers. Typed C2Save retains the persisted Store writer setting (default2);
typed C5Catalog admits one short
transaction on the separate catalog with64KiB protected pending bytes,1MiB
first-party work/terminal, one protected FD/channel and4MiB qualified engine
headroom. Idle Workspaces reserve fixed ownership, not full Commit windows.

Native HELLO v2 is version:u16=2, purpose:u8 (General1/Catalog2/Control3),
reserved:u8=0, encrypted and bound to the authenticated peer. V1 stays unchanged
and unprotected. Catalog accepts only HistoryQuery and metadata_mutation calls;
Control accepts finite capabilities/status/cancel/cleanup, never ExecStart,
construction or bulk reads/bodies. General slots cannot consume the protected
catalog/control owners. Bounded preauthentication/refusal admission, executor,
engine and FD ownership must all pass before protected native progress is claimed.
An extra generic socket or a separate integer counter is insufficient proof.

## 6. Service capability, certification and final-publication allocation

The baseline operation registry ends at29 and response registry at29 (retired
allocations remain reserved). Service profile6/payload1 is new and explicit.
It supplies the read-only negotiation required before FileSet BEGIN; daemon
RuntimeCapabilities alone cannot establish Server support. The Bridge owner
reserves these unused current allocations together:

| Operation | Native opcode / response | Closed grammar and authority |
| --- | --- | --- |
| ServiceCapabilities | 33 /30 | Zero input, profile6, version1; queried before dependent construction. Exact Store from Request and principal grant; finite existing deadline. |
| NamespaceProofControl | 34 /31 | version1, action1 Certify or2 Release, scope32, profile32, FS-root32; Release additionally exact issuer-epoch32/token33. Certify is full read/graph work on General, Release finite protected control. |
| ImportSnapshot | 35 /32 | version1, source-stack17/branch17/commit33, destination-stack17, destination-name u16+1..63 bytes, new namespace-profile32/edit-policy32, operation-selector32. General channel; explicit new Stack/scope only. |
| StoreMaintenance | 36 /33 | version1, action1 Begin or2 Continue or3 Observe, selector32, exact Store device8/inode8/incarnation32, deployment-quiescence authority32, optional known-phase receipt33. No body; ordinary admission closed before maintenance begins. |
| CommitQualified | 37 /40 | version1, CapturedToken128, Branch17, expected-head option1+33, expected-base33, issuer-epoch32/validity-token33, FileSet-completion-root32, canonical-profile32, prepared counts and bytes (six u64), prepared SHA-25632. General channel; one final composite namespace/History attempt. |

All option flags accept only0/1; missing expected head has no33-byte payload.
Selectors and opaque tokens are nonzero. Unknown action/version/trailing bytes
fail. Current finite deadlines/file/body/metadata envelopes remain unchanged.
No root/capability/action implies migration/provider enforcement automatically.

ServiceCapabilities tag30 contains version1, feature bitmap:u64, namespace and
file-policy masks:u8 each, resource-profile:u8, logical-file/body/prepared maxima
(three u64), members:u16, coalescing bytes:u32, result-width:u16, schema:u16,
issuer epoch32, namespace-v2 profile32:111 bytes including tag. Feature bits0–7
are FileSet1, CanonicalV2, NamespaceValidity, Schema11Maintenance, SnapshotImport,
QualifiedCommit, ProtectedCatalog and StrictServerMemory; higher bits reserved.
Unavailable versions/bits are zero. R1a direct catalog progress alone cannot
advertise ProtectedCatalog or StrictServerMemory. Source-bound support remains
separate from capacity reservation and physical qualification.

NamespaceValidity token is tag0x52 plus32 random bytes. Response31 binds the
token to issuer epoch32, Store4, scope32, FS/inode/parent/profile roots (four32),
origin1 (full-certification1/checked-successor2) and principal from authentication.
The encoded capability including response/version/option fields is <=256 bytes.
Only the private C1 verified owner can mint it; release names exact live authority.
It owns fixed admitted state, never a serialized recursive proof history.
Stale/forged epoch is refusal before Save, with no automatic certification.

Import/maintenance are separate ServiceRights version1 grants: Import0x0001,
Maintenance0x0002, NamespaceProof0x0004, QualifiedCommit0x0008. They are not
promoted from legacy u8 masks. Existing scope/content/history grants remain
required for their actual effects. DeploymentQuiescence authority is supplied
by a supported controlled deployment that excludes every configured old user,
then binds exact native Store identity and the exclusive lifetime lock. A caller's
token or claimed inventory alone cannot construct that authority.

Responses32/33 contain version, exact selector, disposition and fixed known-phase/
initialization outcome (at most512 bytes); uncertain effects retain that owner.
Response40 contains version, captured selection32, exact known Branch/Commit/root
fields and the returned <=256-byte validity capability, plus custody/cleanup
disposition, at most1024 bytes. All result/terminal owners are admitted before CAS.
CommitQualified consumes the existing prepared section field grammar under its
explicit selected v2 context, verifies exact FileSet/namespace coverage and
performs internal stage/CAS. It is not a public Stage+CommitStaged pair. The old
HistoryCommand Commit/Stage v1 surfaces retain their exact independent contract.

## 7. Migration and supported capability boundary

New private v3 owners attach explicitly. Existing private v2 owners drain in
their exact original custody before cutover; there is no writable v2 fallback.
V1 canonical readers/independent callers retain their declared compatibility.
The target v2 Stack has immediate-parent policy plus certified parent namespace.
Hash authentication alone cannot mint NamespaceValidityLease. Stale/forged epoch
refuses before effects; explicit full certification is a distinct acquisition.

Schema10-to11 is an explicit maintenance action. Deployment authority must prove
every configured legacy user closed, and the exact Store native identity is held
by an exclusive nonblocking lock. A process-local mutex/new lock cannot exclude
uncooperative old users. If deployment authority is unavailable, refuse before
copying. Use bounded indexed locator copy, ordered exact comparison, small table
swap, bounded retired-row deletion, empty-only drop, then final version publish.
Keep pack BLOBs/save IDs/ordinals unchanged and report physical freelist high-water.
Unknown phases retain closed admission and exact selector; no automatic recovery.

Selected v1 snapshot import builds a NEW v2 Stack/scope, paged serial remap and
independent forward/parent certification through existing C5 initialization.
Original history stays selected by its original profile; no silent conversion.
Linux strict physical containment and contained command/FUSE runtime are explicit
capabilities. Darwin logical tests, source arithmetic or Docker inventory cannot
prove them. Larger files/counts,10,240,DB mmap/durability,S3,conflict resolution
and strict sibling filesystem confinement remain separately selected capabilities.

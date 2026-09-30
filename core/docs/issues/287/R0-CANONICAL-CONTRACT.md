# R0 canonical identity and independent reference

> **Status: Current planning checklist; no release candidate exists.**
> Implementation contract freeze, 2026-09-30. Audited parent source:
> `7edddbdb8e8512627aed0ed42533ef099d802384`.
> This document selects exact prospective formats for R3. The independent
> reference is implemented; the v2 product, provider/resource proofs and release
> qualification remain unimplemented/unrun at this checkpoint.

The preserved [Server packet](../../architecture/proposal/bounded-workspace-implementation-20260930/SERVER.md)
selects immediate captured-parent file construction, a bound parent index and
separate v1 compatibility. This freeze fills its exact grammar/profile choices.
Read the coordinated [R0 interfaces](R0-FROZEN-INTERFACES.md) for native operation
30, response39, FileSet result seals and admission. This file owns canonical
bytes only; it allocates no competing Bridge opcodes.

## 1. Registry allocation and common framing

All integers below are unsigned big-endian, with checked addition and conversion.
Canonical identity remains
`BLAKE3("layerfs/object/v2\0" || canonical_bytes)`; the object-domain name is an
existing baseline primitive, distinct from the new namespace profile version.
The common envelope remains `LFSO[4], kind=1[1], payload_length[4],
value_length[4], value`. Require `payload_length=value_length+4` and exact EOF.
Value length remains <=8MiB; total canonical length remains <=16MiB.

[ObjectRole](../../../crates/layerfs-content/src/object/output.rs) persists these
codes: WholeFile1, Chunk2, ExtentLeaf3, ExtentBranch4, FileState5, InodeLeaf6,
DirectoryLeaf7, DirectoryBranch8, InodeBranch9, FilesystemRoot10, AttributeLeaf11,
AttributeBranch12 and Symlink13. Allocate **ParentLeaf14 and ParentBranch15**.
C2 schema11 must accept roles1–15 with typed role validation. Schema10's1–13
constraint cannot accept them; explicit quiesced migration precedes their Save.

Persisted object roles and inner codec role bytes are separate registries.
Existing directory inner tags1/2, inode tags7/8, root tag6, symlink tag5 and
extent tags8/9/10 remain unchanged. Parent pages use inner tags14/15 under their
distinct checked magic. No old persisted or inner tag is reinterpreted.

## 2. FileState v2 and exact write policy

The retained v1 surface uses `LFS4MAP\0`, mapping grammar version3, FileState
inner tag10, and a93-byte value. Its mapping-profile digest binds the frozen
CDC digest and mapping partition parameters. Preserve that exact surface for
v1 callers and readers, including its no-op and partition behavior.

New FileState v2 uses **magic `LFS7FST\0`, version2**, persisted role5, and this
**125-byte value**. Offsets exclude the13-byte common envelope.

| Offset | Field | Width |
| --- | --- | ---: |
| 0 | magic `LFS7FST\0` | 8 |
| 8 | version2 | 2 |
| 10 | inner role10 | 1 |
| 11 | flags0 | 1 |
| 12 | logical length | 8 |
| 20 | extent count | 8 |
| 28 | mapping-root level0..31 | 1 |
| 29 | unchanged mapping-profile digest | 32 |
| 61 | immediate-parent edit-policy digest | 32 |
| 93 | mapping-root ObjectId | 32 |

The existing mapping-profile field carries the existing CDC binding; no duplicate
direct CDC field is added. The independent
[contract descriptor](oracle/contract.json) records the exact CDC formula/table,
mapping descriptor and policy-description bytes. Mapping and chunk codecs remain
`LFS4MAP\0` version3 and `LFS4CHK\0`; extent widths remain40/48, fanout64–128,
root leaf0–128, root branch2–128 and level31. The frozen complete builder emits
128-entry pages when a level exceeds192 entries, then applies its finish half
partition. This is canonical construction policy, not an admitted-memory claim.

The policy description is exactly the following UTF-8 bytes, with one NUL after
`v2` and no trailing newline:

```text
layerfs/file-edit-policy/immediate-selected-parent/v2\0base=exact-captured-parent;coordinates=current-result;cdc=replacement-only;first-touch=opaque-parent-span;no-op=preserve-base;unresolved-parent-edges=1;split-join=extent-v3
```

Its digest is an ObjectId over those description bytes:
`271a0d010d80c16ecdbec7eea45084ea01b4e1cf1f9ebbe543346d94fe7cfe8f`.
The CDC digest remains
`09002d304e6872b322b31ae8e316db1d06b748fac8dd5ee3472bef6002aa0116`;
mapping-profile digest remains
`e99288f3bc4adea6901bcbb2b14c16f5f573c9cb436309a6cc73d51deb335a72`.

A v2 operation receives the exact known canonical root of its captured immediate
parent file version, not the original ancestor or latest-by-serial state. During
the single pending submission, one unresolved captured-parent edge is permitted;
its known FileSet result resolves that edge. Selected exceptions/orphans receive
their own exact known roots where later ranges depend on them. First touch starts
with one opaque full parent span. Lowering does not enumerate predecessor private
fragments. Source authorities and G1/G2 installation remain Workspace obligations.

Within one v2 edit sequence, edits address the result after preceding edits.
The finalization boundary advances to `start+replacement_length` for every edit,
including delete-only edits; the next start cannot precede that boundary. Never
reorder or merge replacement segments: each declared replacement restarts CDC.
V1 retains its original accepted applicability grammar separately. R3 must prove
the actual FileSet lowering satisfies this v2 admission before dependent effects.

For an existing chunked parent whose final result remains chunked, each edit is
`split(start)`, `split(delete_length)`, replacement-only CDC, left/replacement/right
concat, coalescing contiguous slices of the same payload, and final reachable
children-first emission. Unchanged subtrees retain IDs. It never re-chunks the
whole inherited file merely to erase lineage. Small->large uses the existing
complete-input conversion; large->small builds the final whole-file object from
retained ranges. The default cutoff remains131,072 bytes, with exactly the cutoff
chunked. Changed/fresh empty results use the empty mapping plus v2 FileState.
Whole-file bytes/encoding remain unchanged.

An empty edit sequence, or a proved no-op in which every declared replacement
equals its removed span, preserves the exact Base root, including a retained v1
root. Byte equality of two independently constructed representations does not
establish canonical identity. A changed chunked result receives v2 FileState.
No error selects another profile or recipe.

## 3. Parent index grammar and canonical page policy

Parent pages use **magic `LFS7PAR\0`, version1**, persisted/inner roles14 for a
leaf and15 for a branch. The31-byte value header has magic8, version2, role1,
level1, flags1=0, local-row-count2, subtree-row-count8, subtree-row-bytes8.
Including the common envelope, an empty page is44 bytes. Subtree row bytes count
parent leaf rows, excluding headers and branch rows; a branch sums its children.

| Page | Row in field order | Canonical length |
| --- | --- | --- |
| Leaf level0 | serial8, kind1, parent8, name-length2, name bytes | `44+sum(19+name_length)` |
| Branch level1..31 | exact child upper serial8, child ObjectId32 | `44+40*children` |

Serials are in1..=i64::MAX under the selected C5 scope. Parent is nonzero and
different from child, and names use the existing canonical UTF-8 component
grammar:1–255 bytes, neither `.` nor `..`, no NUL, slash or backslash. Leaf keys
and branch upper keys are strictly increasing. Kind2 means Directory; kind3
means Symlink. RegularFile1 has no parent row because regular aliases are legal.
The root directory has no parent row. Serial bounds, types and scope are checked
in semantic certification as well as applicable codec checks.

All pages are <=8,192 canonical bytes. Non-root pages satisfy byte fill2/5:
>=3,277 canonical bytes. Thus a255-byte name occupies274 bytes: a filled non-root
leaf holds12–29 such rows. Minimum one-byte names occupy20 bytes: a filled leaf
holds162–407 rows. A branch has81–203 children when non-root,2–203 at root; a
one-child root collapses. Empty or underfilled leaves are legal only as root.

Append checked rows in key order. When one append exceeds the page ceiling,
partition at the prefix whose doubled row-byte sum is closest to the total;
ties choose the smaller left prefix. Retain the right boundary for the next
row. On update, unchanged child ranges reuse their IDs. Adjacent result siblings
are retained/merged when either fails its format's fill rule; merging branches
first applies that same rule to their child neighbors, then repartitions the
combined entries. Root construction uses the resulting right spine and collapses
single-child roots. These are the existing sorted-engine decisions with the
new exact row widths, not inode count fill inherited by resemblance.

The reference implements the format, ordered construction and sibling update
independently in Python. Its30-to11 deletion vector exercises actual merge and
root collapse. Branch-boundary vectors use real encoded child leaves with exact
upper keys, not invented child IDs; they are codec/partition vectors rather than
complete certified namespaces.

## 4. FilesystemRoot v2 and profile binding

FilesystemRoot uses the existing `LFS6FSR\0` magic, **version2**, inner role6,
flags0 and persisted role10. Its **148-byte value** contains:

| Offset | Field | Width |
| --- | --- | ---: |
| 0 | magic | 8 |
| 8 | version2 | 2 |
| 10 | inner role6 / flags0 | 2 |
| 12 | namespace-profile digest | 32 |
| 44 | allocation scope | 32 |
| 76 | root directory serial | 8 |
| 84 | inode-table root | 32 |
| 116 | parent-tree root | 32 |

Direct references are inode-table root, then parent-tree root. Version1 retains
its116-byte value and exact profile. A v1-only reader refuses version2 instead
of ignoring the extra root. Serial remapping/import creates a new v2 Stack and
scope; no existing v1 Stack profile/history is silently rewritten.

The v2 namespace descriptor is exactly the following prefix bytes, with a NUL
after `v2`, followed immediately by the32 raw edit-policy digest bytes above:

```text
layerfs/namespace-profile/scoped-inline/v2\0scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5;parent19+name;parent-fill2/5;parent-branch40;parent-roles14,15;parent-magic=LFS7PAR;parent-version1;file-state=LFS7FST-v2;edit-policy32;
```

Its ObjectId is
`bfa09d6347e5f515861dd979b2cfd10b4035e3db94bc8881a0609dff4feb425d`.
The v1 namespace digest remains
`d9bc397d578dab71b49f0816eb2d4f3bc5699cfd755addef915a303b35748ee0`.
Exact descriptor hex is in contract.json; prose wrapping changes no hash input.

Authentication of both roots does not certify their correspondence. Full initial
build/import/open certification derives the non-file parent ledger from actual
forward bindings, checks complete inode membership/types, reference counts,
unique non-file parents, root/no-parent, reachability/cycles and exact forward/
reverse equality. Only then can C1 issue a private VerifiedNamespace owner.
The reference includes an authenticated mismatched inode/parent pair that must
refuse despite all its ObjectIds being valid.

Incremental publication requires the exact live verified Base or an explicit
CertifyExactRoot acquisition before Save. It derives parent/forward changes from
one complete final effect authority and follows the effective final parent
relation, including cross-moves. A successor owner is fixed authority, not a
recursive proof Arc. Epoch/principal/Store/scope/root-bound lease lifetime and
bounded registry admission follow R0 interfaces and the Server packet. A stale
or forged lease refuses before effects; no error-driven recertification/retry.
The finite initial-certification reference does not prove the future incremental
issuer/lease implementation or restart authority.

## 5. Independent proof method, vectors and remaining scope

[reference.py](oracle/reference.py) imports no LayerFS crate or candidate output.
It encodes envelopes/pages/states from this specification, runs the frozen CDC
mathematical recurrence, and uses immutable Python nodes for split/concat and
sorted sibling operations. A separate byte-vector model establishes each edited
file's final bytes. [verify.py](oracle/verify.py) decodes/authenticates child
relationships and namespace forward/reverse facts independently of the original
input ledger, and re-encodes decoded values for exact round-trip comparison.
The development-only [hash helper](oracle/hash/src/main.rs) supplies raw BLAKE3
only, through a framed byte pipe. It depends solely on the already published
blake3=1.8.5; its own lockfile retains Core dependency versions/checksums.

[vectors.json](oracle/vectors.json) freezes8 file vectors,6 namespace vectors,
9 parent transitions,10 refusals and262 canonical objects. Objects carry exact
ObjectId, persisted role, length and SHA-256; small objects also carry complete
hex. Larger canonical bytes regenerate from explicit recipes/method. The v1
section preserves6 existing fixed IDs and7 sealed codec fixture IDs separately;
their source files/seal remain untouched. New v2 roots do not relabel old roots.

| Reference case | Exact expected root |
| --- | --- |
| v2 empty file | `02b511538166d18b4c9f3fb6c2ae3610c0308b119bb19c933796b4eb6f019e72` |
| v2 complete131,072 bytes | `5fc196ed633252ad31186d56c87720c4f565306bc092d78784112f5962f9cc5a` |
| v2 successor of exact selected v2 parent | `6a693ab88d7c3a2eb8f1b1ceedc833a4e2849cfc1cc305a6cd35d28fea8caa21` |
| v2 empty namespace | `fe400d365a6a0befe7ad7a3ab7c7ccb0ab8184f03e4b4d341cae79eb3759723a` |
| v2 namespace with30 maximal-name directories | `9ecc51887a55b66edb133ca5dcbc0244c938dcfe00f8f8f22bed50454450960d` |
| parent tree after30-to11 root collapse | `d4043960c73b656ffea223d5a4723aa1507db3efed5714ef6b5d24a73300b7c7` |

Reproduce from repository root with the two commands in
[oracle README](oracle/README.md). [Validation record](oracle/evidence/VALIDATION.md)
retains exact actual commands, setup failures, draft defect and supported result.
The root checkpoint records the eventual actual publication SHA; this document
does not guess its own future commit.

The independent method includes multi-level fresh streaming and inherited
split/concat, but the executed file vectors have at most22 extents. The complete
>128-entry mapping split/unequal-level-join/root-collapse matrix, broad sparse/
shrink/hole/backward source ranges, cross-move incremental certification,
v1 full fixture campaign and candidate-v2 comparison are **NOT_RUN** at R0.
R3 must prospectively extend that independent ledger before using those results
as admission. Reference internal round-trip PASS is no product correctness,
resource, liveness, physical-provider, performance or release PASS. It uses
finite resident ledgers for clarity and establishes no production memory bound.

SC-01/02/04/06 depend on file byte/partition/parent proofs; SC-03/05 depend on
namespace width/topology/aliases; SC-07 depends on refusal and exact outcome
custody. All remain owning implementation gates. #288 qualification is delegated
and unrun; this oracle ran no benchmark family/campaign.

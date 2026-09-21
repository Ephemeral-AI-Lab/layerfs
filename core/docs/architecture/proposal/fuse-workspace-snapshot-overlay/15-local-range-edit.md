# R3b — bounded local range editing and maintained metadata

> Status: bounded local RangeEdit implemented and verified; mounted writes and Commit remain open.
> Implementation base: `4629b8d62de1e0df8a7bd9808b59a86d7c6669f3`.
> The source audit, v0.1.6 receipts and earlier round identities remain unchanged.

This round implements one operation: local `Workspace::edit_file_range`. It uses
R3a-owned immutable replacement inputs and a maintained private disk index. It
introduces no mounted-write, snapshot, Commit, SDK network edit or npm claim.
The full objective and remaining ordered rounds in [04](04-implementation-and-verification.md)
remain open.

## Public boundary

Attachment explicitly selects `WorkspaceAccess::ReadOnly` or `LocalEdit`.
A disk quota grants storage capacity, not filesystem edit authority. `LocalEdit`
requires both a Branch-backed attachment and private backing. It retains the
complete validated Branch snapshot. Root-backed readable attachments keep their
existing behavior. R3b rejects projection reservation and mounting a LocalEdit
Workspace until the R4 coherence binding is implemented.

The first operation is:

```text
WorkspacePath::new(bytes: &[u8]) -> Result<WorkspacePath, WorkspaceError>
RangeEdit { start: u64, end: u64, replacement: OwnedPayload }
Workspace::edit_file_range(path: &WorkspacePath, edit: &RangeEdit,
                           deadline: Instant) -> Result<MutationReceipt, WorkspaceError>
```

Paths are validated relative logical bytes. Coordinates refer to the current
regular-file version; the half-open range must fit that version. The replacement
must belong to the same consumer, private directory and incarnation. Borrowing
the edit preserves the caller's owned input on refusal. Successful local
publication includes bytes, length, one new mtime, inode revision, generation,
dirty entry and counters together; mode and shared hard-link identity remain.
The receipt identifies incarnation, generation, inode, revision and accepted
replacement length. It is not a C2 save or C5 publication acknowledgement.

The existing read, lookup, getattr and handle APIs observe the published local
version. Dropping node lookup references cannot discard its dirty inode record.
A dirty close refuses before setting stopping or removing owned backing. Explicit
metadata reclamation releases only unreachable, unpinned roots and confirmed
resources; it does not discard the live dirty root.

## Selected bounded representation

The global private tree indexes `I | inode:u64` records and
`D | generation:u64 | inode:u64` dirty entries. Each inode points to a separate
piece tree using the same checked 4,096-byte page codec. This is not a resident
namespace or dirty-set mirror. Normalizing the one edited inode may examine its
bounded maximum of 1,024 pieces. Canonical base ranges and owned-payload ranges
are implemented initially; the reserved zero tag gains behavior with R4.

Page references are eight bytes, big-endian `(slot:u32, epoch:u32)`. `(0,0)` is
null; active references require both fields nonzero. Slot zero is reserved and
reuse increments an epoch without wrapping. The per-consumer slot ceiling is
65,536 active physical slots. Headers occupy 128 bytes; cells encode two u16
lengths followed by key and value. A branch value is an eight-byte PageRef.
Checksums use the already locked sha2 dependency. Non-root pages use at least
1,024 cell bytes; both encoded bytes and the 128-cell count are bounded. Child
levels must equal parent level minus one; maximum level is seven.

The final native layout uses one immutable page file per `(slot,epoch)` under
its Workspace's private directory, plus paged ownership records. Computed
filenames avoid a resident segment registry. Each file's allocation, identity,
partial creation and failed cleanup require the same conservative accounting as
R3a. Ledger-file native identities are retained in a charged, demand-grown ownership
anchor vector, bounded across the consumer at 1,068 identities by the slot and
arena ceilings. Actual retained vector capacities also count. This is not a
resident map of page edges or inode/dirty records.
This revises the initially discussed multi-page segment layout. Filesystem
inode/directory implementation overhead is separate from file `st_blocks` and
must not be presented as a total-device quota guarantee.

The selected candidate bound remains 128 data pages. A piece leaf cell is
76 bytes, so the 1,024-piece cap allows at most 73 non-root leaves plus a root.
Global records use 173 bytes per inode and 22 per dirty entry. Even the later
G plus live-successor allowance of 256 of each uses 49,920 cell bytes and at
most 48 leaves plus a root. Piece branch cells are 20 bytes; longest global
branch cells are 29 bytes. Minimum branch occupancy rules out an extra level
at those leaf cardinalities. One final piece construction and one batched global
update therefore fit 74 + 49 = 123 pages; no sequence of retained intermediate
global trees is assumed in that arithmetic.

Candidate reservation is conservatively `(128 + 4 + 4 + 1) * 4096 = 561,152`
bytes (548 KiB), covering data, new ownership ledger pages and control/header
allowance. A separate 548 KiB generation-completion escrow is reserved before
the first accepted edit. This preserves later lowering/reconciliation headroom;
it is not a promise that R3d is already implemented. The per-consumer 768 KiB metadata allowance is divided into 640 KiB transient
writer/cleanup scratch and a 128 KiB retained ownership/registry budget, with
both also charged to the main Workspace budget. It covers actual retained
capacities, including bounded piece plans, decoded pages, cleanup cursors and
ownership anchors. There is no resident edge-change vector: completed candidate
pages own temporary root references that explicit ledger transitions release.
Uncertain mutable ledger writes quarantine their retained descriptors. Payload,
metadata, ownership, failed resources and reservations share the configured
disk budget; the allowance is not multiplied by Workspace count.

An on-disk ownership ledger tracks slot state, epoch, reference count, role and
payload custody. The resident root registry is bounded at 32 per consumer.
Root drops make work eligible; explicit cleanup traverses outside the short
Workspace lock with a bounded cursor. Unknown ledger or allocation outcomes
retain ownership and stop affected admission. Payload cleanup cannot clear an
unresolved metadata quarantine. No recursive destruction or filesystem I/O is
allowed under the Workspace state lock.

## Operation admission and later integration

The resulting inode must fit 1,024 pieces, 256 normalized shared EditFile edits,
8 MiB replacement replay and a maximum file length of 4 GiB. One live generation
admits at most 128 dirty inodes, with prepared request metadata checked against
32 KiB. This operation changes no namespace names or directory records. These
shared ceilings remain independent of private disk capacity.

A prepared mutation retains its exact head revision, generation and base/version
association. Publication compares the complete expected stamp. R3c must add
capture to that comparison: a candidate prepared against B in generation g
cannot be relabelled as g+1 after capture wins. A future captured base descriptor
must name the exact page root, inode, generation and revision. R3d may replace it
with a canonical root only after matching that captured version and length.
No capture, canonical-base relabelling or automatic retry is introduced here.

## Registered functional verification

External tests call the production public Workspace API. Their operation
delivery binds the existing authenticated native Client exactly as daemon
assembly does. The real service and its Store/catalog remain on the host;
Linux holds only Workspace/private backing. The closed R1 fixture is reused by
independent byte copies. The delivery observation refuses any attempted service
mutation, and both remote file hashes are checked unchanged after the test.

Each selection has one fresh output directory and the existing 60-second complete
functional-command budget. Failures and retained runtime names remain evidence.

| Selection | Required observations | Status |
| --- | --- | --- |
| `semantics` | Explicit unmounted capability; no immutable content reads during first edit; atomic attributes/bytes/hardlinks; current-coordinate overlap/insert/delete; forget/relookup; Branch unchanged; dirty close retains state | PASS |
| `refusals` | Range/deadline/type/missing path, read-only and foreign payload refusals preserve original view; 8 MiB + 1 replay refused | PASS |
| `frontier` | Exactly 256 disjoint normalized edits accepted; 257th refused; overwriting an existing interval reuses normalized capacity; explicit reclaim and forget preserve results | PASS |
| `aggregate` | Two LocalEdit Workspaces share the same 12 MiB disk quota and accounted RAM; a second 6 MiB payload refuses while both retain independent local roots | PASS |
| `large_base` | 16-byte overwrite near the end of an actual 64 MiB immutable file; zero base ReadFile bytes during edit; private allocation/reserve below 2 MiB; edited and distant window byte oracles | PASS |
| `inode_frontier` | Edit all 104 regular inodes available in the R1 fixture; public reads survive global index splits; an independent disk-cell oracle confirms exactly the 104 maintained D entries after reclamation | PASS |
| `native_shape` | Real truncation and enlargement of immutable metadata files invalidate prior allocation observations; cleanup of a healthy third Workspace cannot clear the incomplete/quarantined state | PASS |
| `ledger_write_failure` | A 2 KiB native file limit interrupts a 4 KiB update to an existing ownership ledger; typed write error and prospective payload custody survive uncertainty and caller drop | PASS |
| `ledger_collider` | A new inode carrying a valid copied ledger header is refused without modifying it; explicit cleanup succeeds after the external owner restores the original ledger inode | PASS |
| `corruption` | Native damage to live private page headers makes read/edit fail; the dirty root, pages and payload allocation remain owned during attempted cleanup | PASS |
| `metadata_failure` | Native 2 KiB file limit refuses 4 KiB metadata allocation; old bytes/attrs survive; payload cleanup cannot clear metadata quarantine; explicit checked cleanup releases known partial resources | PASS |
| `metadata_quota` | 1 MiB quota fits an owned byte but not candidate plus completion escrow; refusal preserves bytes and input; confirmed file blocks match accounting; clean close releases the owned directory | PASS |

These initial selections do not complete every B-series row. The existing R1
fixture has fewer than 128 regular inodes, and the current shared creation
surface is bounded; the 128-dirty-inode overflow case needs an appropriately
constructed fixture through its actual prerequisite. Full npm cardinality,
R3c/R3d, mounted edits, writable kernel coherence and matched R6 remain NOT_RUN.
No performance, RSS/cgroup, total-device quota or durability result is inferred
from local accounting or functional command wall time.

The receipts map each selection to 04's requirement IDs as **partial local
evidence**, not completion of the full mounted W/S/B row. Semantics contributes
to W-01/W-07/S-15/B-09/B-10; input/frontier refusals to B-04/B-05/B-14/B-16;
aggregate accounting to B-02; metadata capacity to B-01/B-15; native failure,
corruption and ownership cleanup to B-15/B-20. The initial semantics selection uses the existing 577,551-byte immutable
fixture. A separate 64 MiB immutable-base selection now has a real prepared
fixture and a passing local edit selection. It declares edited and distant
window checks, not full 64 MiB Workspace readback. These are separate
from R3a's 64 MiB local-input proof.

## Reusable large immutable fixture

`prepare_large_edit.py` completed once in 20.867322082995088 seconds using
archived R3a producer binaries and the exact R3a product-input seal. It byte-copied
the closed R1 Store, streamed 67,108,864 bytes through ConstructFile, and used
ordinary Initialize/Fork/prepared Commit operations in a fresh history catalog
to create the closed Branch fixture. Reopening the old R1 catalog would not
grant mutation continuity; no such recovery was attempted. This is fixture
preparation, not a Workspace new-inode operation or performance sample.

The master receipt is retained at
`core/target/pair1-evidence/large-edit-master-01/result.json`. Its input is the
exact recipe `byte[i] = i % 251`, SHA256
`98dc891b284e4d84ac25b0c0a24fdbe39a7f0dbd643ad5e8aa06e02fc6258254`.
Later functional selections must byte-copy the two closed files and verify the
master hashes. No repeated fixture generation or cache-state claim is selected.
An initial artifact-identity assertion used the Status receipt's daemon hash
for the service by mistake; it stopped before launching a producer. The corrected
role-specific checks and immutable binary archive are recorded separately.

The existing-ledger failure selection distinguishes an observed positive short
write (`WriteZero`) from a native direct-I/O/size refusal. Either must retain
the original typed cause and prospective payload custody until ownership is
resolved. Quarantine is an expected result in that selection; removing the
owned test runtime after its process exits does not claim clean Workspace close.

## Verified source, scope and retained evidence

All final local selections share product-input seal
`fdb03e06c68fa9058339e3c73bc9ef80b67233338311d0fab4427968b51b7b87`.
They use the real authenticated host service and Linux 6.12.76-linuxkit aarch64
ext4 backing. No shared-service mutation occurred during these local operations;
the closed Store and history clone hashes remained unchanged.

| Functional selection | Result | Complete command seconds | Evidence |
| --- | --- | --- | --- |
| `semantics` | PASS | 2.015964749996783 | [receipt](evidence/r3b-local-edit-20260921/local-edit-semantics-02/result.json) |
| `refusals` | PASS | 1.268661082998733 | [receipt](evidence/r3b-local-edit-20260921/local-edit-refusals-02/result.json) |
| `frontier` | PASS | 37.746799625005224 | [receipt](evidence/r3b-local-edit-20260921/local-edit-frontier-02/result.json) |
| `aggregate` | PASS | 0.9860383750055917 | [receipt](evidence/r3b-local-edit-20260921/local-edit-aggregate-02/result.json) |
| `large-base` | PASS | 0.9436482910095947 | [receipt](evidence/r3b-local-edit-20260921/local-edit-large-base-02/result.json) |
| `inode-frontier` | PASS | 7.505206332993112 | [receipt](evidence/r3b-local-edit-20260921/local-edit-inode-frontier-01/result.json) |
| `native-shape` | PASS | 3.449043125001481 | [receipt](evidence/r3b-local-edit-20260921/local-edit-native-shape-01/result.json) |
| `ledger-write-failure` | PASS | 0.9240117500012275 | [receipt](evidence/r3b-local-edit-20260921/local-edit-ledger-write-failure-02/result.json) |
| `ledger-collider` | PASS | 0.9053930420050165 | [receipt](evidence/r3b-local-edit-20260921/local-edit-ledger-collider-02/result.json) |
| `corruption` | PASS | 0.9279351250006584 | [receipt](evidence/r3b-local-edit-20260921/local-edit-corruption-02/result.json) |
| `metadata-failure` | PASS | 1.022568542000954 | [receipt](evidence/r3b-local-edit-20260921/local-edit-metadata-failure-02/result.json) |
| `metadata-quota` | PASS | 1.0590004170080647 | [receipt](evidence/r3b-local-edit-20260921/local-edit-metadata-quota-02/result.json) |

The 11-selection regression uses immutable caller archive
`27f045532bff41e95c18ff66607e72af5bc55e3d0b6b6c42d2b52795a7d5427d`;
the subsequently added 104-inode selection uses
`9ab9e8be201c346d25e27585a34a66e214630874566438549d7baaf0ba8913a0`.
Both caller sources are retained beside their executables in the local archive.
Only the external caller gained the final selection; product source did not
change, so earlier passing selections were not repeated for that addition.
Each receipt pins its own caller source, executable, driver, image, fixture and
product inputs. Earlier ten-selection proofs retain their original
`083b66fa094c0e5e5cb9480fbef2971e669ad452e7cb5d77d8a5945da9af19ab`
identity and are not promoted to the later native-shape correction.

The [R3a payload regression](evidence/r3b-local-edit-20260921/payload-05/result.json)
also passed all 13 checks in **1.4619471249898197 s**, including real ENOSPC,
native short writes and zero resident payload-inode pages after acquiring and
reading 64 MiB. Its largest Source-callback allocation observation was
1,984,165 bytes; that is not a process-wide peak. The
[actual read-only mount and authenticated Status regression](evidence/r3b-local-edit-20260921/status-05/result.json)
passed in **25.523878374995547 s** at the same final product seal. Neither route
qualifies mounted writes, capture, lowering or Commit.

The 64 MiB-base operation replaced 16 bytes, requested zero immutable file
payload bytes during editing, and retained **20,480 allocated bytes** plus
**561,152 reserved bytes**. Its boundary Workspace account was **1,986,276 bytes**.
The 104-inode case retained exactly 104 independently acquired payloads and
104 on-disk dirty entries after explicit old-root reclamation: **1,335,296
allocated bytes**, **561,152 reserved bytes**, and **2,016,300 accounted bytes**.
Two Workspaces shared one 12 MiB quota and one RAM account: **6,348,800 allocated**,
**1,122,304 reserved**, and **3,407,026 accounted bytes**. Native file block counts
matched the reported allocated totals in complete-observation cases. Process FD
counts at these boundaries were 6 or 7, including the proc-directory enumeration;
they are not peak or per-backing-domain measurements.

The positive ledger short-write oracle now verifies that native file bytes
changed in the limited prefix and the suffix remained unchanged, in addition to
the typed WriteZero result. Its first receipt derived the positive-write label
from the error kind alone; that earlier source/receipt remains preserved.
The strengthened result retains both the existing payload and the prospective
new payload after caller drop. Native truncation/enlargement instead marks
allocation observation incomplete and admission stopped; cleanup in a healthy
third Workspace preserves those flags. Known observed growth is charged without
clamping it to quota. No guessed resource refund or restart recovery is claimed.

## Working-allocation arithmetic

On the supported 64-bit profile Cell occupies 200 bytes and Piece 40 bytes.
A deliberately conservative transient upper accounting includes eight old/new
128/132-cell pairs (416,000 bytes), per-level update/output cells (9,600), an
additional 132-cell split buffer (26,400), a 128-reference edge list (1,024),
piece-build cells (14,400), two 1,024-piece vectors (81,920), two maximum path
buffers (8,192) and 16,384 bytes of hash/native/scalar scratch: **573,920 bytes**,
within the **655,360-byte** transient reservation. Actual admitted R3b trees are
shallower than the codec's level-seven ceiling. The native I/O windows retain
their existing R3a accounting; the separate metadata retention budget enforces
actual root/arena/ledger-identity capacities and their replacement headroom.
Checked metadata views currently share the finite metadata admission guard;
the guard and all state/metadata locks end before remote or payload reads.

## Checks, corrections and reproduction

Whole-core locked Rust 1.85.1 tests, examples/binaries and warning-denying Clippy
passed on the final product source. Linux whole-core all-target Clippy,
Workspace tests/test compilation and daemon build passed. Final boundary checks
cover 229 product files and all six guard self-tests. Formatting passed with
`--all` against the core virtual workspace. Commands used from the worktree root:

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR="$PWD/core/target" \
  LAYERFS_CONSTRUCTION_WORKERS=1 cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR="$PWD/core/target" \
  LAYERFS_CONSTRUCTION_WORKERS=1 cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR="$PWD/core/target" \
  LAYERFS_CONSTRUCTION_WORKERS=1 cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --offline --examples --bins
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Linux used its own `core/target-linux`, the repository-root ARMv8 flags, a
read-only registry mount and the reusable official Rust 1.85.1 Clippy/rustfmt
[tool image](evidence/r3b-local-edit-20260921/toolchain/identity.json). Runtime
functional proofs retain the original Rust bookworm image. The full commands
and stdout/stderr checks are retained under the evidence `checks` directory.
No CI, aggregate preflight, sync/WAL change or measured comparison ran.

Retained nonpassing checks are: the extra PayloadReader argument on both hosts;
the external caller's unsupported Cursor input type on Linux; one len-zero lint;
missing Clippy in the plain Rust image; and a fmt invocation without `--all`
that selected no targets. These were corrected rather than suppressed. No
functional selection failed. Source reviews also corrected ownership cursor
progress, prospective payload custody, ledger identity, range-tree coverage,
allocation incompleteness and admission/charge ordering before the corresponding
final proofs. Logs and original hashes are retained; LFT1-only filtering of
compiler/test logs is declared in `log-identities.json`.

Reproduce one selection with a fresh output path and the matching archived
caller. Use the large master only for `large_base`; all other selections use
`mounted-07/result.json`:

```sh
LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/crates/layerfs-workspace/tests/local_edit_route.py \
  --fixture "$PWD/core/target/pair1-evidence/mounted-07/result.json" \
  --test-binary "$PWD/core/target/pair1-evidence/binary-archive/<CALLER-SHA>/local-edit-test" \
  --case inode_frontier --output "$PWD/core/target/pair1-evidence/NEW-OUTPUT"
```

The native driver uses the existing per-worktree lock and records an interference
snapshot, owned targets and read-only inputs. Credentials are passed through the
child environment rather than command arguments. Raw databases/binaries stay in
the local target/evidence archive; the committed packet contains only receipts,
logs and identities. The advisory lock remains on disk and is ignored by Git.

## Immediate next operation

The next operation is **Workspace::stage(deadline) -> StageSelector**. R3c has
no independent production freeze API: stage therefore requires private capture
and the minimum R3d lowering for this already admitted edit envelope in the same
operation round. Keep the capture and lowering acceptance IDs distinct. This
explicit prerequisite overlap avoids an unused capture helper or an operation
that always fails after freezing. It does not bundle writable FUSE, publication,
new namespace operations or npm into this change.

Before that operation, completion escrow must become generation-owned. Capture
moves E_g to G; an empty successor owns none. Graph sealing must not assign a
live escrow before the exact root/revision/generation publication check. A stale
prepared edit remains unpublishable and explicitly reclaimable, without a
relabelled D key or an automatic retry. The first successor edit of a G-dirty
inode needs its exact captured-version coordinate base. Foreground remote
admission must become lazy where work is wholly local, so the actual G upload
cannot refuse all successor operations. Capture cannot acquire the metadata
writer gate or drain active operations to avoid that race.

Stage must perform actual C1/C2 saves and C5 StageChanges, retain G and its
submission slot, and leave Branch unchanged. The next separate public operation
is commit_staged with exact known-result reconciliation; composite commit follows
those same owners. Full save-overlap S-11, repeated Commits, writable projection,
remaining controls, namespace operations, npm and R6 remain open. The 128-dirty-
inode overflow, full 64 MiB Workspace readback and RSS/cgroup qualification also
remain NOT_RUN at this R3b scope.

The production file responsibility is: `filesystem/write.rs` owns the public
RangeEdit and publication stamp; `overlay/pieces.rs` normalizes its recipe;
`backing/metadata_pages.rs` defines the local codec; `metadata_index.rs` performs
bounded COW index updates; `metadata.rs` owns consumer admission/root/reserve
accounts; `ownership.rs` and `metadata_reclaim.rs` own native ledger/file identity
and checked release. Existing runtime, namespace/read/directory, payload/reclaim
and native-segment files integrate those owners. Daemon config only selects the
explicit ReadOnly access capability. The workspace manifest reuses locked sha2
and adds native bridge/FUSE only as external test dependencies; no third-party
package, service implementation dependency or reference path was introduced.

The exact final Linux command family uses the existing owning-worktree targets:

```sh
docker run --rm \
  --mount "type=bind,src=$PWD,dst=/work" \
  --mount type=bind,src=/Users/yifanxu/.cargo/registry,dst=/usr/local/cargo/registry,readonly \
  -w /work -e CARGO_TARGET_DIR=/work/core/target-linux \
  -e LAYERFS_CONSTRUCTION_WORKERS=1 \
  layerfs-pair1-rust-tools:c331f3815ef3cfb5c760 sh -c '
    cargo clippy --manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings &&
    cargo test --manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace --tests &&
    cargo build --manifest-path core/Cargo.toml --locked --offline -p layerfs-daemon'
```

## Commit production LOC

First-parent comparison against `4629b8d62de1e0df8a7bd9808b59a86d7c6669f3`:
**Production LOC: 101,202 -> 104,202 (delta +3,000)**. Reference remains
65,417 -> 65,417 (delta 0); core is 35,785 -> 38,785 (delta +3,000). Workspace
accounts for +2,999 and the explicit daemon access selection for +1. This is
new bounded local-edit implementation, not reference retirement or a performance
comparison.

The exact measured staged tree is `0e2f45915b218b690c4fd1be0855b7780fad735e`.
The same `tools/production_loc.py` counter, Git blob
`b5b9617d08204977176302311e0b2c72a811b420`, counted both snapshots archived by
`git archive <revision> crates core/crates`, using
`python3 tools/production_loc.py --root <archive> --json`. It counts nonblank,
non-comment Rust/runtime SQL, excluding legacy inline tests, external tests,
examples, tools, docs, manifests, locks and generated output. Adding this record
and the comparison JSON changes evidence/docs only; the final commit is checked
against its final staged tree and the staged product-input seal above.

The final diff check also removed one trailing space in the external Python
driver. The executed pre-whitespace driver is retained in the local SHA256
driver archive; this formatting-only adjustment did not repeat passing product
operations or rewrite their driver identities.

# Existing-inode resize and logical zero ranges

> **Status: one public operation implemented and natively verified; Pair 1 remains open.**
> Exact implementation parent: `573b4bbd35bdcd5d8fe55c8fc107f8c31cd791c5`.
> Product input seal: `a03cd4e97e6ca387ff89ca5f910f6bb0347c47f90fff051d8320a9ea7bd0ed88`.
> This prerequisite supplies size semantics before writable open/setattr binding.
> It does not enable a writable mount or complete R4/Pair 1.

## Operation and representation

`Workspace::set_len(serial, length, deadline)` targets an existing cached regular
inode in this Workspace. It shares immutable mutation preparation and publication
with RangeEdit, including owner DAC, kind, canonical baseline, revision/generation,
metadata admission and failure checks. A stale canonical descriptor refreshes
outside the state lock and must retain exact serial/kind/reference identity.
The operation refuses unknown identities, ReadOnly access, invalid/expired input,
unsupported length and unavailable resources without publishing a partial resize.

Shrink removes the selected live tail. Extension appends explicit logical zeros,
so a shrink followed by extension cannot rediscover old canonical tail bytes.
Same-length resize updates mtime/revision through the metadata-only route and
preserves content root. Length, mtime and dirty frontier publish together under
the existing short state lock; accepted caller bytes are always zero because
resize receives no payload input. Existing open handles and hard-link aliases
continue to identify the same inode.

The private piece value remains **64 bytes**. Its existing source tag now accepts
0 Base, 1 Local and **2 Zero**. Zero requires payload ID zero, null custody,
source offset zero and zero reserved bytes, plus checked nonzero length and
bounded ranges. Base and Local retain their previous validation. Piece slicing
preserves zero offsets, adjacent Zero pieces coalesce, and only Local pieces
create payload-custody edges in the ownership ledger. This is a private format
extension, not a released compatibility or daemon restart/resume guarantee.

Reads fill only the requested zero span directly into the already-owned reply
buffer. The captured replacement Source emits zero spans through the existing
bounded caller buffer, at most 128 KiB per step, opening no payload reader or
private payload extent for those spans. Inherited canonical and captured-base
pieces remain references. No full-file copy-up or zero-filled OwnedPayload is
constructed by resize.

Zero bytes count alongside Local bytes in the same normalized shared replay
recipe: at most **1,024 pieces, 256 edits, 8 MiB replacement input and 4 GiB logical
file length**. A logically sparse extension can therefore refuse before reaching
the file-size ceiling. Private representation does not widen EditFile or
PreparedChanges, split one Commit into hidden smaller Commits, or add Source
protocols. Capture/Stage/Commit continue to associate exact G versions with saved
roots. D1 shrink/zero/overwrite recipes are lowered against the acknowledged C_G,
not an unchanged base whose root label was replaced.

## Resource arithmetic

On the supported 64-bit profile, Piece grows from 40 to **48 bytes**, while Cell
remains 200 bytes and PageRef 8. Two admitted 1,024-piece vectors therefore grow
by **16,384 bytes**, from 81,920 to 98,304. Retain [15's conservative transient
allowances](15-local-range-edit.md#working-allocation-arithmetic), including its
path/native/scalar scratch, instead of reducing those allowances to offset growth.
The account becomes **573,920 + 16,384 + 6,144 = 596,448 bytes**, with an additional
256-entry/24-byte Edit FilePlan conservatively included even though the retained
submission also pays for it. This is **58,912 bytes below the existing 655,360-byte
transient reservation**. No main Workspace memory-budget increase is introduced.

The four 128 KiB aligned windows remain separately charged as 512 KiB of retained
R3a allocation. The 128 KiB metadata retained pool, remote-call scratch and bounded
ReadReply charges remain separate. Serial refresh's bounded path/remote work
finishes before mutation takes metadata writer admission. Source metadata views
and a local mutation share that finite writer admission; they do not retain two
simultaneous transient page plans. Zero synthesis creates no payload record,
extent, buffer cache or FD. The existing 137-page candidate and 208-page dirty-
generation completion reservations are unchanged. This is accounted working
allocation arithmetic, not RSS, cgroup or page-cache evidence.

## Native route declaration

External `resize.rs` exercises the production API and real Stage/composite/C5
routes through the existing native fixture helper. `resize_route.py` registers
nine functional selections on the common Stage route driver. Each independently
copies the closed 64 MiB Store fixture and starts a fresh live C5 producer; fixture
Init/Fork/alias Commit precedes Workspace attach. The Linux caller receives only
its immutable binary directory and private backing volume. Store/catalog and
credentials remain at the host service.

One construction worker, fresh outputs and a 60-second complete-command hard
limit apply. The 104-inode case declares a 25-second complete Commit deadline;
other selected Commit/Stage calls use ten seconds. Resize itself retains the
existing callback deadline clamp. Actual save overlap uses the read-only native
RESERVED-byte observer, SIGSTOP confirmation and D1 progress marker before resume;
an upload pause or delayed final reply does not substitute for that boundary.
Native metadata failure uses a real process file-size limit without product hooks.

Cases cover shrink/reextend/aliases/old replies, the exact 8 MiB zero envelope with
full chunked saved-content verification, G zeros plus D1 shrink/reextend, overwriting
G zeros before the saved root arrives, same-length metadata-only Commit,
identity/kind/deadline/length/quota refusals, native metadata failure, D1 resizing
during actual C2 save, and all 104 fixture inodes shrinking with one save per inode.
All declared cases and failures are retained below; declaration alone is not proof.

## Remaining prerequisites

Writable open still needs explicit handle rights and atomic truncation admission;
handle writes need retained inode selection and atomic append positioning. The
writable Linux kernel capability/coherence profile must be implemented and actually
verified before any mount-write claim. DiscardStage removes an exact C5 row only;
it does not settle local G/D1 or release their submission slot. Its operation and
the later failed-state disposition remain separate pending work. Namespace/new-
inode/metadata/symlink/larger-input prerequisites, full declared npm, R1-C controls
and matched R6 remain open. No performance, hard RSS/cgroup, durability or restart
continuity is claimed; no issue is closed.


## Actual operation results and retained failures

All nine declared operation selections now pass on the exact product seal above.
One earlier semantics selection remains FAIL because its test oracle ignored the
existing Q0 remote permit lifetime. Complete walls are functional budget checks,
not performance results.

| Receipt | Result | Complete wall seconds | 04 acceptance scope |
| --- | --- | --- | --- |
| resize-semantics-01 | FAIL | 3.335965750 | W-03, W-04, B-09, B-10 subsets |
| resize-semantics-02 | PASS | 0.957655750 | W-03, W-04, B-09, B-10 subsets |
| resize-envelope-01 | PASS | 3.765004625 | B-04, B-05, B-09, B-14, B-28 subsets |
| resize-successor-01 | PASS | 0.874870667 | S-03, S-18, H-03 subsets |
| resize-overwritten_zero-01 | PASS | 0.881341542 | S-18, B-28 subsets |
| resize-metadata_only-01 | PASS | 0.787382167 | B-28 subsets |
| resize-refusals-01 | PASS | 0.728577166 | W-12, B-01, B-15 subsets |
| resize-metadata_failure-01 | PASS | 0.743077250 | S-15, B-20 subsets |
| resize-native_save-01 | PASS | 1.701775875 | S-11, S-18 subsets |
| resize-frontier-01 | PASS | 7.218480583 | B-21, B-28 subsets |

The initial semantics caller held a canonical ReadReply, which owns its remote
OperationGuard until the reply drops, and then incorrectly expected another
canonical read to succeed. The product correctly returned Busy. The corrected
caller explicitly asserts Busy while that reply is held, verifies its old bytes
after local resize, releases it, and verifies the original full mixed-data window.
A wholly local Zero ReadReply remains across the subsequent Commits and truncation,
with its old bytes/length preserved. No product change or remote-admission increase
was made. The original caller `93e30f8...`, FAIL, service/test output and checked
owned-container/volume removal remain recorded. Corrected caller `e9fef8b...`
owns semantics-02 and all later selections. No failed receipt is promoted.

The zero-envelope case actually reads the saved 8 MiB file in bounded chunks and
asserts every byte is zero. Before Commit it proves no payload records and less
than 128 KiB of allocated private bytes for that zero representation. It rejects
8 MiB + 1 replacement byte without changing attributes/revision and separately
rejects combined owned-plus-zero input beyond the same envelope. These are selected
functional bounds, not a peak RSS/cgroup or unlimited sparse-file guarantee.

The successor case saves G with 64 inherited bytes plus 64 zeros while D1 shrinks
to 48 and reextends to 100. Its next actual EditFile uses exact C_G, base length
128 and replacement `(48,128,52)`. The overwritten-zero case writes four local
bytes inside G's zero region before its root arrives; its next input is exactly
those four bytes at `(80,84)`, without reuploading G's zeros. Native-save verification
resizes D1 while C2's real RESERVED lock is held, then confirms both the captured
65,536-byte zero tail and the later 32-byte tail through actual Commits. The
104-inode case saves each changed inode once, submits no replacement bytes for
shrink-to-zero, and preserves the hard-link alias identity.

The native metadata-file limit fails local preparation before visible publication,
retaining original length/attributes/revision and accounted failure state. It is
not simulated by a product branch and does not claim clean Workspace close.
Same-length resize saves its changed timestamp with no EditFile submission and
unchanged content root. Refusal tests retain the original version for wrong
identity/kind, expired deadline, oversized length and insufficient metadata quota.

An earlier Linux compile check also failed in three external test assertions:
`MutationReceipt` intentionally lacks PartialEq, so comparing whole Results was
invalid. The tests now compare their error values, with the original diagnostic
retained. The product and declared operation envelopes did not change. No other
operation failure has been observed on this source; regressions and exact checks
are recorded separately below.

## Checks and regressions

The following locked Rust 1.85.1 commands passed from the implementation worktree:

```sh
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --offline
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --offline --examples --bins
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
  CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --offline --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Host tests passed 649, with zero failures and three ignored. The boundary guard
scanned 239 production Rust/SQL files and its six self-tests passed. Linux used
the existing `layerfs-pair1-rust-tools:c331f3815ef3cfb5c760` image, this worktree at
`/work`, registry read-only, `CARGO_TARGET_DIR=/work/core/target-linux` and one
construction worker. Workspace `cargo test ... -p layerfs-workspace` passed ten
ordinary tests, with 58 native cases ignored by default. Whole-core Linux
`cargo clippy ... --all-targets -- -D warnings` and `cargo build ... --examples
--bins` passed with the same core manifest and locked/offline flags. The corrected
resize caller was rebuilt separately and whole-core Linux Clippy rerun after its
test-only change. Python drivers compiled. Root ARMv8 build-config SHA remains
`3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
No CI or aggregate/retired preflight gate ran.

All 58 native Workspace cases were explicitly exercised on this product seal:
nine resize, twelve RangeEdit, ten Stage, twelve CommitStaged, fourteen Composite,
and the one payload selection containing thirteen payload checks. All passed;
the earlier failed semantics attempt remains separate. The actual Linux read-only
mount/authenticated Status regression also passed. Thus the evidence index retains
**60 selections: 59 PASS and one FAIL**. Existing Piece base/local normalization,
ledger custody, corruption/failure ownership, quotas, incremental commits and
clean UpToDate retain current-source regressions. This does not enable or qualify
any writable mounted callback.

All complete-command walls were under 60 seconds. RangeEdit frontier was
28.905489792 seconds, payload 1.001427125 seconds, and mounted Status
25.149335917 seconds. These are functional budget observations, not performance
results. Fresh output directories and per-worktree isolation records retain
process observations; no build in this worktree overlapped a functional selection,
and no quiet-host claim is made. The new Stage-derived drivers mount only the
immutable caller archive; older payload/edit/mount regression drivers retain their
existing read-only repository bind. Historical receipts keep their original
identities and are not rewritten to claim a different exposure or source.

[The evidence index](evidence/resize-zero/functional-index.json) links every
selection. Exact inputs, original/corrected caller sources, failed-test cleanup,
raw outputs and checks are under [evidence/resize-zero](evidence/resize-zero/).
The runtime image remains `rust:1.85.1-bookworm`, immutable ID
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
Closed Store fixtures, private backing and binaries remain local owned artifacts,
not Git content. One exact zero-envelope reproduction after recorded builds is:

```sh
python3 core/crates/layerfs-workspace/tests/resize_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries core/target/pair1-evidence/binary-archive/e6727867e9de9e8d5c9fcde85b35794f9a01eca85993492f2a866201f937bc9e/host \
  --test-binary core/target/pair1-evidence/binary-archive/e9fef8b32a28cb6aeb11d49f1435e5fe65928177c3c0501353dedca659d8e289/resize-test \
  --case envelope --output core/target/pair1-evidence/resize-envelope-reproduction-NEW
```

Use a new output path. Native-save additionally uses the existing read-only lock
observer identified in its receipt. The six changed production files are the
shared filesystem write/read, overlay pieces, backing metadata index and Commit
lower/source owners. No reference, service/history algorithm, daemon protocol,
FUSE callback or dependency changed in this operation round.


## Commit production LOC

First parent `573b4bbd35bdcd5d8fe55c8fc107f8c31cd791c5`:
**Production LOC: 106,870 -> 107,029 (delta +159)**. Reference remains
65,417 -> 65,417 (delta 0); core is 41,453 -> 41,612 (delta +159).
Workspace is 8,625 -> 8,784. This adds serial-based resize and Zero semantics
through shared mutation/normalization/lowering; no source relocation, reference
retirement or measured simplification is claimed.

Counted staged tree: `616fc09398393b7280ed6407f4ab4be830b333b5`.
Both exact snapshots were archived with `git archive <revision> crates core/crates`
and counted by `python3 tools/production_loc.py --root <archive> --json`, counter
Git blob `b5b9617d08204977176302311e0b2c72a811b420`. Identical nonblank/non-comment
Rust/runtime SQL scope excludes inline/external tests, examples, tools, docs,
manifests and generated output. The final LOC paragraph and comparison JSON are
documentation only; final product paths are checked identical to the counted
staged tree before commit. [Exact comparison](evidence/resize-zero/production-loc.json).

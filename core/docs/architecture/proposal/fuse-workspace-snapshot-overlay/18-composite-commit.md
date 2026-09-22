# Ordinary composite Workspace Commit

> **Status: one public operation implemented and natively verified; Pair 1 remains open.**
> Exact implementation parent: `6702e31e629ada5e78981b6854e36721e619e65b`.
> Product input seal: `b6acbb4ef773fdaec17dd9922a47ec6a92ccb6f5e91ca963ba5f857d8953ee12`.
> This is one public operation after [CommitStaged](17-commit-staged.md).

## Selected operation and reused owners

`Workspace::commit(deadline)` performs coherent private capture, the existing
streamed file/portable metadata preparation, and exactly one existing
`HistoryCommand::Commit(PreparedChanges)`. The shared service already stages the
constructed filesystem and commits that exact stage under one service admission.
Workspace does not send a hidden StageChanges plus CommitStaged pair, build a
second filesystem candidate, or introduce a wire operation or provider. All
file/source/result-index preparation and known-own reconciliation reuse the
Stage/CommitStaged implementation bodies.

The composite success reply contains only `CommitOutcomeWire`, not a StageWire
or token. `CommitReport.stage_token` is therefore now `Option<u64>`: explicit
CommitStaged reports Some(exact token); composite Commit reports None.
`CommitFailure.stage` similarly retains an optional exact local selector, while
`observed_stage` separately carries native stage observations without creating
an adoptable selector. No token zero, follow-up query or synthetic stage is used
to populate these fields. This is an intentional local API refinement before
release; the existing service protocol is unchanged.

A composite Committed result is checked against captured stack, expected parent,
intended base and a changed effective root. The exact service-acknowledged root
is used with the captured per-inode saved associations; Workspace does not
construct a root to compare independently. UpToDate must match the exact captured
head and effective root. Explicit CommitStaged additionally retains its exact
candidate-root check. Observed and validated outcomes remain separate, and a
validated remote success is recorded before reconciliation or cleanup can fail.

## Clean Commit and admission

A clean ordinary Commit is a real service operation. It reserves a charged empty
capture root plus the existing **208-page completion escrow (851,968 B)** before
the cut, captures the immutable Branch/base context and submits empty inode and
directory vectors through the ordinary composite route. C1/C2 still construct/save
the filesystem, C5 stages it and C5 decides whether to consume it as UpToDate.
The local zero frontier avoids private index traversal; it does not synthesize a
successful Commit. Explicit `stage()` retains its existing clean InvalidInput.

The clean/dirty classification and clean generation are rechecked under the cut
lock. If an edit wins first, capture refuses and releases the unused empty root
and escrow outside the lock. If capture wins, an already-prepared edit is rejected
by its root/revision/generation checks. Later live edits see an empty captured
frontier, retain canonical B, and enter G+1. Shared reconciliation retains those
D1 records even when G's actual result is UpToDate. The cut copies no payload,
scans no frontier, performs no paging I/O and recursively destroys no graph.
An independent source review covers these ownership paths; actual native
successor tests are distinct evidence.

The existing remote operation permit is acquired before capture. An already-held
canonical read therefore causes plain Busy with unchanged generation, revision
and submission. That permit is consumed by the first prerequisite request, or by
the final composite request when G has no prerequisite saves; it is released after
that one call. Subsequent calls use the same immediate admission. No permit or
state lock spans the whole save sequence or local reconciliation.

Before the first remote prerequisite, composite Commit reserves the same
completion descriptor, **64 fund pages**, **26 slot credits**, next Branch context,
charged **16,928-byte ledger standby** and bounded attempt/failure owners as
CommitStaged. The 208-page fund already budgets these 64 pages alongside result
index work; no quota, memory allowance, window, FD limit or construction worker
is added. Clean escrow StorageFull is a pre-cut refusal. A completion descriptor,
RAM or slot refusal after the cut retains the existing StageFailure before any
remote prerequisite runs. Registry capacity can remain charged after ordinary
rollback; failed cleanup is never assumed released.

## Failure boundary

Prerequisite failure after attempt reservation returns CommitFailure in Preparing,
with the original charged StageFailure as its cause. That preserves FileSave,
MetadataSave or LocalBookkeeping phase, exact pending file/metadata roots,
source error and saved counters. Unknown prerequisite outcomes propagate Unknown.
Final composite failures use CompositeCommit and retain the service's code,
unknown, cleanup, conflict and stage observations. A reported acknowledged but
uncertain stage remains an observation, not an exact local StageSelector.

Known own success survives later local failure. An opaque lost terminal remains
Unknown even when separate later queries see an advanced Branch or no stage.
No automatic replay, token substitution, implicit discard, local resume or restart
recovery is added. All retained failures keep G and D1 accounted; the one-frozen-
generation-per-consumer policy remains in force. Explicit failure disposition
and DiscardStage are separate pending operations.

## Native route declaration

External `composite.rs` shares the existing native fixture and delivery helper.
`composite_route.py` registers fourteen cases on the existing Stage route driver.
The driver independently copies the closed 64 MiB immutable Store fixture and
uses a fresh live C5 producer; it does not reopen a writable history catalog.
Public fixture Init/Fork/alias Commit happens before Workspace attach. Only the
immutable caller directory and private backing volume are mounted in the caller
container. Store/catalog and credentials stay with the host service.

Each case uses one construction worker, one fresh output path and the existing
60-second complete-command hard limit. The 104-inode case declares a 25-second
absolute composite deadline; other cases declare ten seconds, including all
prerequisites. Native save cases reuse the read-only F_GETLK RESERVED-byte
observer and require D1 progress while the actual C2 writer remains stopped with
that lock held. Terminal gating is separately identified and never substitutes
for that S-11 boundary. Opaque result loss and actual local file-size failure reuse
the existing external mechanisms. There is no product fault hook, cache warming,
performance selection or third-party modification.

Registered cases cover remote pre-admission, clean UpToDate before/after dirty
Commit, clean G with late D1, A/B/A alias/incremental inputs, dirty G with live
insert/delete, actual C2 save/loss, composite terminal loss, known C5 success then
local failure, authenticated denial, stale clean Branch, clean quota/ReadOnly/
deadline/staged refusals, metadata-only content preservation and 104-inode Commit
followed by a one-inode incremental Commit. Exact results and retained failures
are appended after execution; declaration is not proof.

## Remaining work

This operation implements ordinary explicit local SDK Commit, not mounted writes
or daemon-targeted edit/Commit control. Existing-file kernel write/append/truncate/
extend, bounded zero ranges, required namespace/new-inode/metadata/symlink and
larger-input prerequisites, explicit failed-state disposition, the declared npm
workload and R6 remain open. Actual UpToDate proof belongs to the registered clean
case; source validation alone does not close it. No complete S-14 mounted cycle,
S-18 truncate/zero schedule, hard RSS/cgroup bound, performance gain, durability or
restart continuity is claimed. No issue is closed.


## Actual results and boundaries

All fourteen declared native selections passed on the exact product/caller
identity above. None failed or was rerun on this source. Complete-command walls
below are functional budget checks, never performance comparisons.

| Receipt | Result | Complete wall seconds | 04 acceptance scope |
| --- | --- | --- | --- |
| composite-remote_admission-01 | PASS | 3.505290375 | S-12, B-26 subsets |
| composite-clean-01 | PASS | 0.868278584 | H-03, H-04 subsets |
| composite-clean_successor-01 | PASS | 0.847286333 | S-04, S-18, H-03 subsets |
| composite-repeated-01 | PASS | 0.987910000 | S-14, S-18, B-28 subsets |
| composite-successor-01 | PASS | 0.927958125 | S-03, S-18, H-03 subsets |
| composite-native_save-01 | PASS | 1.675615000 | S-03, S-11 subsets |
| composite-unknown_save-01 | PASS | 1.315843833 | S-15, H-07, H-09 subsets |
| composite-lost_result-01 | PASS | 0.838521750 | H-07, H-09, H-10 subsets |
| composite-reconcile_failure-01 | PASS | 0.840287042 | H-03, B-20, B-26 subsets |
| composite-denied-01 | PASS | 0.804177875 | S-15, H-07 subsets |
| composite-head_moved-01 | PASS | 0.778897875 | H-04, H-07 subsets |
| composite-refusals-01 | PASS | 0.823960292 | S-12, B-01, B-26 subsets |
| composite-metadata_only-01 | PASS | 0.788802375 | B-28 subsets |
| composite-frontier-01 | PASS | 11.452742417 | S-18, B-21, B-26, B-28 subsets |

The clean case proves actual UpToDate on fresh attach and after a changed Commit,
with empty PreparedChanges and no file saves for each clean generation. Its
three real composite commands contain 0, 1 and 0 inode changes. The clean-successor
case holds a real acknowledged C5 reply, creates D1, installs UpToDate for G and
then saves D1 through a second real composite command. That gate is a completion
schedule, not an actual-save overlap claim. The separate native-save case
observes the C2 RESERVED lock and proves local D1 edit/read before service resume,
then commits that successor. Its unknown-save companion retains the original
preparation failure and permits a later wholly local D1 edit.

The A/B/A case sends one composite command for each explicit Commit, preserves
aliases and old read replies, and sends only the new replacement bytes against
the exact acknowledged A content root. The frontier case saves 104 inode versions
once, then sends only one changed inode in the next Commit; all 104 windows are
verified after both outcomes. Metadata-only Commit sends no EditFile and preserves
its content root while saving changed timestamps. Existing-file scope and the
128-inode/256-edit/8-MiB input envelopes remain unchanged.

The real denied principal leaves acknowledged file results and a known-before-
Commit failure. A separately advanced Branch makes clean Commit fail HeadMoved,
not report local UpToDate. Native terminal loss retains Unknown despite separately
observed advancement/absence; native local file-size failure after C5 success
retains the exact known outcome, no installed revision, 25 remaining slot credits
and the accounting-complete stopped backing. Failure ownership is retained, not
silently cleaned or retried. ReadOnly/deadline/staged refusals send no composite
command; the 512-KiB clean-quota refusal leaves generation/revision unchanged and
no submission or slot credit, and permits clean close.

Boundary resource observations are not peaks. The clean sequence ends with
1,994,442 accounted Workspace bytes, four root descriptors and zero reserved
slots/bytes before explicit clean close. The 104-inode case ends with 2,024,626
accounted bytes, four root descriptors and zero reserved slots/bytes; payload
records and eligible allocated space remain accounted until explicit reclaim.
No RSS, cgroup, page-cache or unlimited workload assertion follows from these
snapshots. The clean quota test proves ordinary refusal/rollback, not injected
cleanup failure or every clean-to-dirty publication race.

## Checks, regressions and reproduction

The exact current-source check commands were:

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

All passed: host tests 649 passed/zero failed/three ignored, 239 product Rust/SQL
files scanned and six boundary self-tests passed. Linux used the existing tool
image `layerfs-pair1-rust-tools:c331f3815ef3cfb5c760`, this worktree at `/work`,
registry read-only, `CARGO_TARGET_DIR=/work/core/target-linux` and one construction
worker. Its `cargo test --manifest-path core/Cargo.toml --locked --offline -p
layerfs-workspace` passed ten ordinary tests with 49 native cases ignored by
default. Whole-core `cargo clippy ... --all-targets -- -D warnings` and
`cargo build ... --examples --bins` passed with the same locked/offline manifest.
Both changed Python route drivers compiled. Root ARMv8 build config hash remains
`3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
No CI or retired/aggregate preflight gate ran.

Current-source native selections explicitly ran all fourteen new Composite,
ten Stage and twelve CommitStaged cases. All passed, including the shared
preparation/refusal and token-validation paths changed here. The Linux read-only
mount/authenticated Status regression passed in 25.235023292 seconds. These are
37 separate PASS receipts under the 60-second complete-command limit. RangeEdit
and payload implementations were not changed or rerun in this round; their prior
exact-source native results remain at [17](17-commit-staged.md), without promotion
to this seal. No failure occurred in this round; the earlier CommitStaged
successor-oracle FAIL retains its historical identity in 17.

[The append-only evidence index](evidence/composite-commit/functional-index.json)
links all current receipts. Inputs, check logs, native observations, isolation
and independent clean-capture source review are in
[evidence/composite-commit](evidence/composite-commit/). No same-worktree build
ran during a functional selection. Process snapshots record interference without
a quiet-host claim. The runtime image remains `rust:1.85.1-bookworm`, immutable
ID `sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
Build/fixture reuse removes only setup work; no cache or measured-time claim is
made. Binaries and closed Store fixtures stay in their owning worktree, not Git.

One actual UpToDate reproduction after the recorded builds is:

```sh
python3 core/crates/layerfs-workspace/tests/composite_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries core/target/pair1-evidence/binary-archive/9c3e43f4a9fa83534dd252f137b46348c910c2da6b1002171b962d4aa0436019/host \
  --test-binary core/target/pair1-evidence/binary-archive/83c832a66cc766325dfd3cc93540a2f908dd2b687c2a93def8be5535bcd67db4/composite-test \
  --case clean --output core/target/pair1-evidence/composite-clean-reproduction-NEW
```

Use a new output directory. Native-save cases additionally pass the existing
read-only lock observer, recorded in their receipts. The source change adds
`commit/operation.rs`, shares preparation/completion in save/completion, adds the
clean empty-root/zero-frontier path in metadata/snapshot/lower, and refines local
Commit result types. The helper and native caller changes remain external tests;
service, history and reference-product code are unchanged.


## Commit production LOC

First parent `6702e31e629ada5e78981b6854e36721e619e65b`:
**Production LOC: 106,670 -> 106,870 (delta +200)**. Reference remains
65,417 -> 65,417 (delta 0); core is 41,253 -> 41,453 (delta +200).
Workspace is 8,425 -> 8,625. This adds the composite orchestration, clean capture
and local result refinement while sharing existing algorithms; no relocation,
reference retirement or measured performance simplification is claimed.

Counted staged tree: `68324c6c7d12837e73e25c91b181d909f5522b0b`.
The identical counter `tools/production_loc.py`, Git blob
`b5b9617d08204977176302311e0b2c72a811b420`, counted exact archives from
`git archive <revision> crates core/crates` with
`python3 tools/production_loc.py --root <archive> --json`. Nonblank/non-comment
product Rust/runtime SQL count; inline/external tests, examples, tools, docs,
manifests and generated artifacts do not. Only the LOC documentation/JSON is
added afterward; the final staged product paths are verified identical to the
counted tree before commit. [Exact comparison](evidence/composite-commit/production-loc.json).

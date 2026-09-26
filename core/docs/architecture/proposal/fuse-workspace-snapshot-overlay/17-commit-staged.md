# Exact CommitStaged and live-successor reconciliation

> **Status: one public operation implemented; full Pair 1 remains open.**
> Exact implementation parent: `0b2c729bdb3f026f12beb667ccdc15c52853280f`.
> Product input seal: `31d3ba50b403e2fe6e61b4b6e79a56b002fd22510869f35e8df249e67ff9af89`.
> E-1 concurrency and retry update: source parent `6bb143e91`; this change
> documents the implementation committed with it. Earlier receipts retain their
> original source identities and results.
> Builder I/O overlap update: prior product source `f9de81520`; this change documents
> the implementation committed with it. Earlier receipts keep their identities.
> This extends [Stage](16-stage-capture.md) through the existing C5 CommitStaged
> route. Native SDK proofs below are not mounted-write or performance results.

## Public operation and ownership

`Workspace::commit_staged(&StageSelector, deadline)` validates the opaque local
selector identity and its exact captured context before admitting one existing
`HistoryCommand::CommitStaged { workspace, token }`. ReadOnly, expired, foreign,
consumed and already-attempted selectors refuse. A selector clone retains the
same identity; observing a remote stage cannot construct an adoptable selector.
A dropped selector does not silently discard the submission.

`CommitReport` contains generation, exact stage token, the existing
`CommitOutcomeWire`, and installed local revision. `WorkspaceError::Commit`
retains a charged `CommitFailure`: phase, original cause, exact stage, validated
known outcome, separately observed outcome and optional installed revision.
Its dispositions distinguish KnownBeforeCommit, Unknown and
KnownCommitLocalFailure. Submission status exposes bounded Commit progress.
FUSE maps this new semantic error to EIO; writable callbacks and daemon Commit
controls are not introduced in this round.

Temporary local admission failure leaves the stage usable. The existing remote
operation permit is acquired before reserving the completion attempt or recording
its permanent identity. A local Busy therefore sends no request and consumes no
attempt. After a real attempt begins, failures retain G, D1 and its exact outcome.
A retained reconcile-phase `KnownCommitLocalFailure` with a validated known C5
outcome can resume local reconciliation through `commit_staged` with the same
selector. It does not resend the consumed C5 token or finish the fund twice.
When a failed local page allocation has complete accounting, retry first
unlinks its identity-checked pending file, restores the candidate's slot credit
and refreshes arena admission. An incomplete or uncertain allocation remains
blocked; retry never guesses at ownership.
Unknown outcomes, denied commits and consumed stages retain their custody wedge;
there is no automatic retry, token substitution, implicit DiscardStage or restart
recovery. The active remote permit is released
as soon as the logical call returns, before validation, local I/O and cleanup.
All remote and backing I/O occurs outside the Workspace state/registry locks.

Known validated C5 success is recorded before local work. A later local failure
cannot turn that success into a guessed remote failure, replay permission or
permission to drop D1. Conversely, a later independent GetBranch/GetStage result
cannot promote an originally unknown attempt to known own success. A consumed
stage and matching current root do not prove which operation consumed it.

## Reserved resources and compact reconciliation

The existing dirty-generation completion fund remains **208 pages**. Before the
C5 attempt, CommitStaged acquires one root descriptor, **64 pages (262,144 B)**
from that fund, **26 fresh metadata-slot credits**, and a charged **1,058-entry
ledger replacement capacity (16,928 B)**. These are charged against the same
consumer disk and 128 KiB persistent metadata allocation accounts; there is no
new memory allowance, quota increase, I/O window, FD limit or worker. The attempt
also reserves its next Branch context and bounded status/failure owners before
remote publication.

Slot admission counts allocated plus reserved slots against the existing 65,536
consumer ceiling. Ordinary live growth cannot consume reserved credits. Fresh
IDs consume credits without charging them twice; reusable IDs do not. Unused
credits are refunded by seal/checked cleanup, and close refuses outstanding
reservations. Ledger capacity is preallocated before C5, shrunk to the required
actual capacity and transferred with its memory charge. Old capacity is released
outside the arena lock. Failed publication retains the appropriate charged owner.

Reconciliation visits only the current D1 dirty frontier, first to stream D keys
and then I records into a new compact global index. It reuses immutable piece
trees, validates each captured root/page/version/length association against its
saved R record, and replaces a captured-base reference with the exact saved
content/metadata roots. It does not materialize a complete saved-record Vec or
scan the namespace. G-only records and old generation prefixes are omitted.

The selected envelope has at most 128 D cells of 22 bytes and 128 I cells of
173 bytes: **24,960 bytes**. The builder enforces at most 256 cells, 25 leaves
plus one root, and the existing 1,024-byte minimum non-root occupancy. Thus 26
fresh slots and at most two new ownership-ledger pages require at most 28
physical pages, within the reserved 64. This is source arithmetic for the
selected existing-file envelope, not an assertion that arbitrary npm inputs fit.

After known C5 success, the old completion fund is finished/refunded before
constructing the mixed successor tree; the 64-page candidate reservation is
already owned separately. Later cleanup releases ordinary allocation and never
recreates that finished fund or misclassifies live D1 pages as G-funded pages.
The successor builder reads a pinned live root without the writer gate, so G+1
writes can publish while it builds. A short, deadline-bounded gate hold captures
each input root; a second compares the live revision/root and installs. If a
writer moved the frontier, reconciliation rebuilds from the newer root within
the same deadline. Intermediate trees stay temporary; only the converged tree
is sealed. Mounted writes and reads wait for a current gate holder up to their
deadline instead of exposing a transient `Busy`.
Routine metadata and payload maintenance defer eligible cleanup while the
builder owns their shared backing I/O window. The still-charged owners remain
eligible for a later pass; explicit metadata reclaim keeps its deadline-bounded
wait, and explicit payload reclaim keeps its contention result. A mounted write
can therefore publish during the build without mistaking routine cleanup for
required foreground work. The deadline and window count are unchanged.

The new overlay, Branch context, canonical base, baseline epoch and revision are
installed together under the final gate and state lock. Only eligible old roots are
reclaimed outside that lock. Reader-pinned/captured graphs and failed cleanup
remain charged. Success clears the submission; later explicit Stage starts from
the acknowledged base and submits only the new dirty frontier.

## Read/cache coherence within the implemented scope

State now owns the changing canonical base and Branch context plus a monotonic
baseline epoch. Cached logical attributes remain those already published by live
edits. Canonical original/content/metadata fields refresh lazily against the
selected immutable base, and only enter the cache if the selected epoch is still
current. Reads retain the selected overlay/base throughout; lookup rejects a
changed revision/epoch before cache publication. Edits reject a changed baseline
before construction and changed root/revision/generation before publication.
Directory requests retain one canonical base for both listing and child attrs.
Handles retain their inode identities across reconciliation.

An independent source review found no concrete mixed-version/cache-publication
issue in these paths. It did not dynamically qualify all reader/capture schedules
or audit resource-fund arithmetic. Concurrent release/collection can make a stale
cache refresh fail; it does not retarget a successful read. This reasoning relies
on the implemented existing-file scope: namespace membership, inode kind,
link-count and symlink target are unchanged. Future namespace/metadata operations
must revisit cache and directory-cookie assumptions.

## Actual native selections and retained failure

The external `commit_staged.rs` caller uses the production Workspace/bridge
API. Common native fixture/delivery helpers were extracted from the existing
Stage test into `tests/support/native_workspace.rs`; no private product source
is included. `commit_staged_route.py` reuses `stage_route.py` for closed Store
byte-copy setup and a fresh live C5 producer, with one registered case per fresh
output. The execution container mounts only its immutable caller directory and
private backing volume. Store/catalog and their credentials remain at the host
service. Existing C5 fixture Init/Fork/alias Commit occurs before Workspace
attach and does not implement Workspace namespace creation.

Every case uses one construction worker and a 60-second complete-command hard
budget. The 104-inode case declares a 25-second Stage deadline; other cases use
10 seconds. CommitStaged has a 10-second caller deadline. No timeout, budget,
worker or selection was relaxed after a failure. These are functional command
walls, not latency, throughput or cold-cache measurements.

| Receipt | Result / complete wall seconds | Verified subset of 04 |
| --- | --- | --- |
| commit-repeated-01 | PASS / 4.612481625 | S-14/18, H-03, B-28: A/B/A on the same Workspace, handles and alias; exact incremental file submissions; clean close |
| commit-successor-01 | **FAIL / 0.776233625** | Test incorrectly expected a mixed canonical/local read while the only remote permit was held |
| commit-successor-02 | PASS / 1.108692125 | S-04/05/16/18, H-03: late D1 insert/delete retains exact G coordinates, old reply and 26 reserved slots until completion |
| commit-remote-admission-01 | PASS / 0.820031584 | S-12, B-26: held read causes local Busy, no attempt/RPC/slot consumption; explicit later Commit succeeds |
| commit-selectors-01 | PASS / 0.817437375 | S-12, H-06: exact local selectors, deadline, drop/clone and no replay |
| commit-denied-01 | PASS / 0.811101208 | S-15, H-07: real authenticated peer denial preserves exact stage, unchanged Branch and later D1 |
| commit-lost-result-01 | PASS / 0.826320833 | H-07/09/10: actual encrypted terminal withheld; original outcome remains Unknown despite separate later observations |
| commit-reconcile-failure-01 | PASS / 0.851355750 | H-03, B-20/26: real C5 acknowledgement followed by native local allocation failure preserves KnownCommitLocalFailure |
| commit-consumed-stage-01 | PASS / 0.812898417 | H-06/07: independently consumed token and identical current root do not prove own success |
| commit-head-moved-01 | PASS / 0.815049666 | H-05/07: independent Branch advance preserves exact losing stage and HeadMoved |
| commit-cycles-01 | PASS / 1.857808084 | S-14, B-21/23/29: 12 explicit Stage/Commit cycles, maximum four boundary roots, zero reserved slots after each success |
| commit-headroom-01 | PASS / 0.819349916 | B-26: occupied ordinary quota still permits reconciliation from reserved resources and preserves D1 |
| commit-frontier-01 | PASS / 15.626170917 | S-18, B-21/26/28: 104 G files plus 104 D1 edits, two actual commits and all final windows verified |

The retained successor failure is a test-oracle error, not a passing run. While
a known C5 reply was held before Workspace observation, bytes 8..14 included
canonical bytes and correctly returned Busy. The corrected caller asserts that
Busy, reads the wholly local replacement byte while held, and still checks the
full original window after release and the second Commit. Product seal is
unchanged; there was no narrower final oracle. Caller `083a5ca...` owns repeated-01
and successor-01; corrected caller `c8e851d...` owns successor-02 and subsequent
cases. Raw failure, original caller identity and checked owned-container/volume
cleanup remain recorded. Neither failed cleanup nor Workspace clean close was
silently inferred from process removal.

The native lost-result proxy reuses the daemon fault test's opaque frame relay:
forward handshake and Hello frames, then withhold the actual encrypted operation
terminal and close the transport. It decrypts no payload and replaces no service
algorithm. Later direct queries observe Branch advance and stage absence while
the original Workspace failure stays Unknown with no known/observed outcome.
This is transport-loss evidence; it does not qualify database-unknown quarantine.
The local failure case applies a real process file-size limit only after the
actual C5 response, retaining the exact known outcome and no installed revision.
Holding a completed C5 reply proves late reconciliation schedules, **not S-11**;
actual C2 save-overlap proof remains the separately rerun Stage native-save case.

Resource observations are boundary snapshots, not peaks or RSS/cgroup bounds.
The headroom case fills the 4,194,304-byte quota exactly: 2,494,464 allocated plus
1,699,840 reserved. After Commit it reports the same allocation plus 851,968
reserved for D1, zero slot credits and 1,996,034 accounted Workspace bytes.
The 104-inode case ends with 120 allocated metadata pages, zero reserved slots,
four roots and 2,056,858 accounted bytes. Retained payload inputs/reader graphs
remain visible and charged until explicit release/reclaim; these values are not
a claim of automatic payload compaction or bounded process memory.

## Open dependencies

Actual Workspace UpToDate is **NOT_RUN**: current public mutations assign a new
mtime and a clean Stage refuses before capture. The CommitStaged validator covers
the existing UpToDate wire outcome, but this operation's public producer cannot
deterministically construct that native case. Do not forge private metadata,
adopt another producer's token or substitute a direct C5 test for that route.

The next public operation is ordinary `Workspace::commit`, using the existing
composite `HistoryCommand::Commit(PreparedChanges)` and shared lowering/completion
owners, not two hidden StageChanges/CommitStaged exchanges. Explicit failed-state
and DiscardStage disposition still require their own coherent local ownership
rules. R1-C edit/Commit/lifecycle controls, writable FUSE, truncate/zero ranges,
namespace/new-inode/shared larger inputs, declared npm target and R6 remain open.
The tighter one-frozen-generation-per-consumer policy is retained. No full S-14
mounted repeated-Commit or S-18 truncate/zero schedule is claimed by these SDK
subsets. No issue is closed, no durability/restart guarantee or performance gain
is inferred, and Pair 1 is not complete.

## Checks, regression identities and reproduction

All final checks ran on the product seal above from the implementation worktree:

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

Host tests: 649 passed, zero failed, three ignored. The guard scanned 238 product
Rust/SQL files; all six self-tests passed. Whole-core host/Linux warning-denying
Clippy, formatting and host/Linux example/binary builds passed. Linux Workspace
tests passed ten ordinary tests, with 35 native cases ignored by default; the
registered routes explicitly exercised those payload/edit/stage/commit tests.
Python compile checks passed for both changed route drivers. Native Linux Cargo
ran inside `layerfs-pair1-rust-tools:c331f3815ef3cfb5c760`, with this worktree bound
to `/work`, the host registry read-only, `CARGO_TARGET_DIR=/work/core/target-linux`
and `LAYERFS_CONSTRUCTION_WORKERS=1`; commands were `cargo test ... -p
layerfs-workspace`, `cargo clippy ... --all-targets -- -D warnings` and `cargo
build ... --examples --bins`, each with the same core manifest, locked/offline
flags. Root `.cargo/config.toml` supplies the required ARMv8 AEAD profile, hash
`3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9`.
No CI or aggregate/preflight gate ran.

Current-source regressions all passed: ten Stage selections (including actual C2
save/loss observations), twelve RangeEdit selections, thirteen payload checks and
the actual Linux read-only mount/authenticated Status route. The longest edit
selection was 29.694967667 seconds; Stage frontier was 9.552249292 seconds;
payload was 0.973537708 seconds and mounted Status 25.250047542 seconds. All are
functional complete-command walls under 60 seconds, with fresh output and no
same-worktree build overlap. Per-worktree isolation observations are retained;
no quiet-host or performance claim is made. The older payload/edit/mount drivers
retain their existing read-only repository bind; the new Stage/Commit driver
binds only the caller archive. No existing receipt is rewritten to claim the
new driver's narrower mount exposure.

[The evidence index](evidence/commit-staged/functional-index.json) links all 37
selections: 36 PASS and the retained successor FAIL. Raw outputs, exact binary
identities, original/corrected caller sources, source-review hashes, cleanup and
check logs are under [evidence/commit-staged](evidence/commit-staged/). The initial
external-helper extraction briefly produced invalid Rust import visibility;
it was corrected before Cargo/proofs and its diagnostic note is retained. The
local pre-admission/slot/ledger fixes were completed before these functional
selections. No passing receipt substitutes for the original failure.

Host binaries are pinned by [commit-inputs-02.json](evidence/commit-staged/commit-inputs-02.json).
The Linux runtime is `rust:1.85.1-bookworm`, immutable ID
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
The original closed Store fixture remains under the owning worktree's
`core/target/pair1-evidence/large-edit-master-01/`; binaries/databases are not
copied into Git. One runnable reproduction after the recorded builds is:

```sh
python3 core/crates/layerfs-workspace/tests/commit_staged_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries core/target/pair1-evidence/binary-archive/1318da2bbe95c842981fcd8ab7bb1ad33aace00d62050290c30733dbf1188c4d/host \
  --test-binary core/target/pair1-evidence/binary-archive/c8e851d1b17d81c04778c0dfce1b016dffeae184141914bf41102495784ce961/commit-test \
  --case successor --output core/target/pair1-evidence/commit-successor-reproduction-NEW
```

Use a fresh output path. Other cases use their declared registry without reducing
inputs. The exact production file changes belong to Workspace backing metadata,
ownership and streaming index build; Commit completion/reconciliation/types;
capture/state/base ownership; namespace/read/directory/write epoch handling;
and FUSE error conversion. External helper extraction and native route fixtures
remain tests, not product implementation or production LOC.


## Commit production LOC

First-parent comparison against `0b2c729bdb3f026f12beb667ccdc15c52853280f`:
**Production LOC: 105,652 -> 106,670 (delta +1,018)**. Reference remains
65,417 -> 65,417 (delta 0); core is 40,235 -> 41,253 (delta +1,018).
Workspace is 7,407 -> 8,425; the FUSE error mapping changes no LOC. Growth adds
completion, reserved slot/ledger ownership, compact reconciliation and canonical
epoch handling. No reference retirement or measured simplification is claimed.

Counted staged tree: `04d155dd021784deb60360101ecf82880170bcff`.
Both exact snapshots were archived with `git archive <revision> crates core/crates`
and counted by `python3 tools/production_loc.py --root <archive> --json`, counter
Git blob `b5b9617d08204977176302311e0b2c72a811b420`. The identical scope counts nonblank,
non-comment product Rust/runtime SQL and excludes legacy inline/external tests,
examples, tools, documentation, manifests and generated output. The final
LOC paragraph/JSON add only documentation; final staged product content is
compared against that counted tree before commit. Exact subtotals and method
are in [production-loc.json](evidence/commit-staged/production-loc.json).

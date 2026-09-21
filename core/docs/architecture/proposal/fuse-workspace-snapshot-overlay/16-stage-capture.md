# Workspace stage: capture, successor and shared save

> **Status: one public Stage operation implemented and verified; full Pair 1 remains open.**
> Exact implementation parent: `788a63950500e6ba79c6a55dc07f7b84ca0fde89`.
> This is the next public-operation round after R3b. Its real `Workspace::stage`
> caller requires private R3c capture and the minimum R3d lowering together;
> it does not expose a public freeze primitive or an unsupported stage stub.

## Operation and retained state

`Workspace::stage(deadline)` accepts a dirty Branch-backed LocalEdit Workspace.
ReadOnly returns ReadOnly. A clean LocalEdit Workspace returns InvalidInput
before capture; clean Commit/UpToDate semantics belong to the later Commit
operation. A failed pre-cut admission leaves generation, revision and ownership
unchanged. The production surface returns an opaque cloneable
`StageSelector` with a read-only `stage()` view of the actual C5 StageWire.
There is no public selector constructor that adopts an observed token.

One consumer may retain one frozen submission, and each Workspace has one
submission slot. The successful cut retains the immutable generation root,
maintained dirty frontier, exact Branch snapshot and its completion reserve;
it advances live generation and revision. It performs no paging I/O, frontier
scan, payload copy, operation drain or metadata-gate acquisition under the
short state lock. First edits of G-dirty files in the successor are relative
to the exact captured file version; its eventual saved root is an association
with that version, not permission to relabel an old base root.

Only the current file/metadata save result is resident while its exact association
is published to the private disk index. The existing final PreparedChanges
request still has a bounded complete inode Vec under its 128-inode/32-KiB
shared envelope. That wire input is distinct from a resident saved-root cache.
Workspace emits one StageChanges after file/metadata saves, with no preliminary
UpdatePreparedFilesystem and no implicit Commit or AddLayer.

A known stage leaves Branch/head/base unchanged, retains G and leaves newer D1
edits live. Dropping the selector does not release that logical slot, discard
C5 state or refund backing quota. Clean close refuses dirty/staged/unresolved
state. In this operation increment, all post-cut failures retain the submission,
including known failures before StageChanges. Status exposes phase, capture
identity, bounded progress and known token/root. StageFailure retains the original
cause and exact stage observations without treating Unobserved as Absent.
Unknown native/service outcomes never authorize replay or cleanup.

The next separately implemented public operation is `commit_staged` with exact
selector validation and known-own reconciliation. Explicit failure disposition,
DiscardStage and composite Commit must use the same owners. Their absence here
is a limited lifecycle, not automatic recovery or Pair 1 completion.

## Completion fund and interleaving correction

The ordinary local mutation candidate keeps its 137-page reservation (561,152 B,
548 KiB). A dirty generation now owns a separate **208-page completion reserve**
(851,968 B, 832 KiB). First dirty-generation admission is therefore 345 pages
(1,413,120 B), up from R3b's 274 pages by 290,816 B. This is an explicit disk
admission change; aggregate Workspace RAM, I/O windows and FD limits do not grow.

Result keys are `R|inode:u64` (9 bytes). Their 80-byte values contain captured
revision and length (8 bytes each), content root (32) and metadata root (32).
They reuse the private 4 KiB page/128-byte-header codec and checked ownership
ledger. A 93-byte leaf cell needs 12 records to satisfy the non-root minimum
occupancy. At most 128 results therefore require at most ten non-root leaves
and one root. Two pre-reserved result-root descriptors alternate. One ordered
insertion/update writes at most two leaves plus one root, and reserves eight
pages including native allocation/ledger slack. Checked retirement restores
released result-page allocation directly to the completion fund reservation;
there is no interval in which live edits can consume that refunded headroom.

The first draft incorrectly bounded fund-created ledger pages by dividing all
result-created slots by the ledger's 62-slot capacity. Live edits can advance
the shared slot position **between** result updates, so those slots are not a
contiguous run. The writer gate excludes such interleaving **within** one result
update. At most three fresh result slots in that update can therefore create
at most one new ledger file; over 128 updates, conservatively allow 128 ledger
pages. The reserve equation is **128 ledger + 14 result-data union + 64 next-round
reconciliation reservation + 2 spare = 208 pages**. After result-index work it
retains at least 66 pages (264 KiB) of reserve. The next CommitStaged implementation
must verify and use its tight global-update reservation within 64 pages; it may
not request another full 137-page candidate from that partially consumed fund.
CommitStaged itself is not implemented by this equation.

The original 548 KiB draft's native semantics receipt remains at product input
seal `a600dc4684dc0545639936c21a00d0ab38319d9f3b517df197f7c9d08ba48ce1`.
Its PASS does not qualify the revised reserve or the interleaved worst-case
arithmetic. Revised-source checks and proofs receive new identities/paths.

## Functional route declaration

External tests live in the Workspace crate. `stage_route.py` copies the closed
64 MiB immutable Store fixture by independent bytes, starts a fresh live service
history producer, and creates a small namespace plus a hard-link alias through
existing public Init/Fork/Commit operations before Workspace attach. This is
fixture setup, never a new-inode route for Workspace. The Linux execution side
owns ext4 private backing and invokes the real native bridge/service; Store and
catalog remain on the host. Each registered case has a fresh output path and a
60-second complete-command ceiling. The many-inode case declares a 25-second
absolute Stage deadline before its first run; other cases declare 10 seconds.
Stage honors one caller deadline across all child operations, without resetting
it per inode or applying the read/FUSE callback clamp. Child requests retain
the existing bridge progress and maximum-deadline validation. One construction
worker is retained.

| Case | Declared proof | Acceptance scope |
| --- | --- | --- |
| headroom | Fill remaining ordinary disk quota with an owned input, then perform real Stage using the already-reserved completion fund | B-26, H-01 subsets |
| frontier | All 104 captured inode associations pass through the disk completion index and one actual StageChanges; exact saved windows for all 104 | S-17, B-21/26/28 subsets; no complete capture-latency or large-namespace qualification |
| semantics | G bytes/mtime, D1 overwrite before saved root, alias identity, one save per changed inode, one StageChanges, consumer/Workspace refusal, dropped selector retention | S-02/03/10/12/13/15, H-01/02, B-28 subsets |
| lowering | Current-coordinate insert/delete/splice lowered to the shared sequential EditStream coordinates and streamed replacement count; edited and distant immutable windows | H-01, B-09/28 subsets |
| native_save | D1 edit/read while the real service holds an active C2 write transaction, then G stage and exact G bytes | S-03/11, H-01 subsets |
| unknown_save | Native service loss at that write boundary, retained unknown G and later wholly local D1 work | S-10/15, H-07/09 subsets |
| completion_failure | Actual local file-size limit causes completion-page/ledger publication to fail after known service file/metadata save; retain the pending result and original native cause | B-20/26, H-07, S-15 subsets |
| metadata_only | An accepted empty splice changes mtime/revision; Stage updates metadata and filesystem with no EditFile submission, preserving content root | H-01, B-28 subsets |
| metadata_denied | Actual native authorization refusal after the file result, preserved progress/original failure and local D1 | S-10/15, H-07 subsets |
| head_moved | An independent Branch writer advances the head after capture; exact captured conflict and stage observation survive without refresh/rebase | H-04/07, S-15 subsets |

`EditFile` consumes its complete Source before `store.begin_save`, so an input
pause does not by itself prove actual construction/save overlap. The native-save
observer calls read-only F_GETLK on SQLite's RESERVED byte `(1<<30)+1`, length
one. It neither takes a lock nor executes SQL nor reads/writes Store data. After
observing the owned service PID, the driver sends SIGSTOP, waits for the stopped
state and rechecks the same PID still holds the lock. It then permits the
Workspace caller to edit/read D1; only the caller's completed-progress marker
allows SIGCONT or the explicitly declared SIGKILL failure. A missed lock or a
lock released before stopping is a failed selection, not an invitation to retry
until a convenient schedule appears. The observer's independent native self-check
is separate from actual C2 evidence.

No timing in these receipts is a performance result. These are unmounted SDK
operations, with no full 64 MiB Workspace readback, hard RSS/cgroup proof,
namespace mutation/npm qualification, repeated Commit or R6 claim. Rename,
truncate/zero successor schedules and large retained-reader schedules still need
their own implemented operations and proofs. Existing R1 mounted-read and R3b
receipts keep their original identities.

## Verification and retained evidence

The final product input seal is
`2529e50a9dac4047ef45492b7295ef58e3859c0708c5ee5391e973086c0692bf`.
The immutable Linux caller is SHA256
`b113255aaf1cf8b515261ef20c3794d277ae162ed2360d2206224a40665a130d`;
its complete source and the exact host/runtime identities are retained in
[evidence/r3c-stage](evidence/r3c-stage/stage-inputs-02.json). Rust is
1.85.1 (`4eb161250`); the repository `.cargo/config.toml` supplies the ARMv8
AEAD flags. Linux is aarch64 6.12.76-linuxkit with ext4 private backing. The
runtime image is the same pinned official Rust 1.85.1 bookworm image as R3b.
The reusable tool image contains only the official Clippy/rustfmt additions.

| Final-source selection | Result | Complete functional command seconds |
| --- | --- | ---: |
| headroom-01 | PASS | 4.125176292 |
| semantics-02 | PASS | 1.231048084 |
| lowering-01 | PASS | 1.078406083 |
| metadata-only-01 | PASS | 1.041199708 |
| metadata-denied-01 | PASS | 1.058458917 |
| completion-failure-01 | PASS | 1.123894667 |
| head-moved-01 | PASS | 1.312057250 |
| frontier-01 | PASS | 13.520256875 |
| native-save-01 | PASS | 2.190486459 |
| unknown-save-01 | PASS | 1.777788333 |

The [actual save receipt](evidence/r3c-stage/stage-native-save-01/result.json)
observes the owned service PID 66609 holding the RESERVED byte both before and
after SIGSTOP. D1 edit/read completed before SIGCONT; the resulting C5 stage
contained G's bytes and live reads retained D1. The 0.006720834-second stopped
interval is a causal observation, not save latency or a performance result.
The [loss case](evidence/r3c-stage/stage-unknown-save-01/result.json) independently
observed PID 66628 at the same boundary, allowed local progress and then killed
the owned service. The library returned original `Unknown, unknown=true`, retained
G and accepted a later wholly local D1 edit. Neither case proves two overlapping
saves or #210 H04.

The completion failure is an actual Linux file-size limit refusal after both
file and metadata acknowledgements. It returns `LocalBookkeeping` with original
`BackingFailure { phase: Allocate, kind: FileTooLarge }`, retains the exact pending
content/metadata roots and refuses another submission/clean close. The native
permission refusal keeps the earlier file acknowledgement and its missing metadata
result distinct. The Branch-conflict case retains the actual captured and observed
head/base fields; an explicit later GetStage observation is not substituted for
the original operation's stage observation.

The full-ordinary-quota case reaches exactly 2,097,152 bytes of allocated plus
reserved backing. Stage allocates its result page by converting existing reserve;
it does not increase that sum. At completion: allocated 1,249,280, reserved
847,872. The 104-inode case saves exactly 104 file versions and 104 metadata
versions, then one StageChanges; all 104 immutable output windows are checked.
Its final counts are 119 metadata pages, 1,355,776 allocated backing bytes,
831,488 reserved bytes and 2,057,765 accounted Workspace bytes. These are boundary
observations, not allocation peaks, RSS/cgroup bounds or general scaling results.
The selection summary also extracts first-line observations from the original
Rust stdout, where the test harness prefixes a marker with the test name.

All 12 R3b local-edit selections passed again on the final source. The longest
complete command was the unchanged 256-edit frontier case at 40.472607625 seconds,
within its existing 60-second functional ceiling. The payload regression passed
its 13 checks, including actual short writes and ENOSPC, in 1.210599625 seconds.
Actual Linux readable mount and authenticated Status regression passed in
25.019103791 seconds. Their original outputs, per-worktree isolation companions
and any observed other-worktree interference are retained; these runtimes were
removed only after the owned test processes exited. The intentional service-loss
case records exit -9 separately from successful cleanup. No mounted-write or clean
Workspace-close claim is inferred from removing a test container.

The exact check families from the owning worktree were:

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

All passed. The boundary guard scanned 234 product Rust/SQL files; all six guard
self-tests passed. The pinned Linux tool container separately ran whole-core
`cargo clippy ... --all-targets -- -D warnings`, Workspace `cargo test ... --tests`
and the daemon build, using `CARGO_TARGET_DIR=/work/core/target-linux` inside this
worktree and `LAYERFS_CONSTRUCTION_WORKERS=1`. The ignored native tests were then
selected explicitly by the real-route drivers; host compilation alone did not
run them. No CI, aggregate preflight or retired preflight script ran.

Retained non-passing development checks are the initial duplicate-branch Clippy
finding and a Python cleanup-expression syntax error. Both were corrected before
final verification. The invalid initial ledger-fund bound and the old-seal
semantics PASS also remain recorded; the latter is not promoted to the revised
resource contract. A parent-relative source patch reconstructs that old candidate.
No functional selection failed in this round.

To reproduce one actual-save case after the recorded builds, from this worktree
use the archived caller/host paths in `stage-inputs-02.json`, the original closed
`core/target/pair1-evidence/large-edit-master-01/result.json`, and the native
observer built by the recorded compiler command. The runnable entry point is:

```sh
python3 core/crates/layerfs-workspace/tests/stage_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries core/target/pair1-evidence/binary-archive/5c3927be37149a830eaced701dd4fc4b9ee6a50469989d2738322544d7f40f78/host \
  --test-binary core/target/pair1-evidence/binary-archive/b113255aaf1cf8b515261ef20c3794d277ae162ed2360d2206224a40665a130d/stage-test \
  --lock-observer core/target/pair1-evidence/stage-tools/b65112174ebdc558a31053bf671878ae43a57abc9e32913a34e041233bf1a945/store-lock.dylib \
  --case native_save --output core/target/pair1-evidence/stage-native-save-reproduction-NEW
```

The output path must be new. Other registered cases select their exact declared
fixtures and deadlines through the same driver. Database fixtures and binaries
remain owned local artifacts; the packet stores receipts, source identities,
caller sources and logs without copying mutable databases or executables into Git.

## Next operation and remaining prerequisites

Implement `Workspace::commit_staged` as the next public operation: validate the
opaque selector against the retained Workspace/incarnation/G/context, call the
existing exact C5 operation, preserve known own success through local failures,
and reconcile acknowledged R1 while keeping live D1. Establish the actual bounded
reconciliation algorithm and its <=64-page reservation before relying on that
reserved portion of the fund. Repeated incremental Commits must then demonstrate
exact G-relative successor lowering against the acknowledged C_G; changing a
root label or reconstructing the whole Workspace is not reconciliation.

The runtime still has immediate metadata/reader/remote admission. Genuine
contention can fail a captured submission; it is retained rather than queued,
retried or discarded. Known pre-stage failure disposition, explicit discard,
composite Commit and remaining daemon controls need their own operation rounds.
Prepared-mutation/capture races, old in-flight reads and directory views, larger
fragmented generations, complete S-17 capture cost observations, namespace/zero
range schedules, mounted writes, full npm, RSS/cgroup qualification and R6 remain
open. No issue is closed, no durability/restart recovery is claimed, and Stage
alone does not complete R3d, R4 or Pair 1.

## Commit production LOC

First-parent comparison against `788a63950500e6ba79c6a55dc07f7b84ca0fde89`:
**Production LOC: 104,202 -> 105,652 (delta +1,450)**. Reference remains
65,417 -> 65,417 (delta 0); core is 38,785 -> 40,235 (delta +1,450).
Workspace accounts for the growth; the one-line FUSE error mapping changes
behavior without changing its production LOC. No reference retirement, relocation
or performance improvement is claimed.

The counted staged tree is `add19a86b6ce2ec62b2aaf5d0679e4e13f8bf902`.
Both exact snapshots were archived with `git archive <revision> crates core/crates`
and counted by the same `tools/production_loc.py` (Git blob
`b5b9617d08204977176302311e0b2c72a811b420`) using
`python3 tools/production_loc.py --root <archive> --json`. Nonblank/non-comment
Rust and runtime SQL count; legacy inline tests, external tests, examples, tools,
docs, manifests/locks and generated artifacts do not. This paragraph and its
comparison JSON add documentation/evidence only; the final staged product tree
must remain identical to the counted product tree before commit.

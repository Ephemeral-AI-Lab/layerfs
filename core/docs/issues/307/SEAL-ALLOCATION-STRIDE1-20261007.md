# macOS seal allocation correction and stride1 rerun

> **Status:** Dated measurement checkpoint; not release evidence or a product contract.

Owner-selected scope: release the excess macOS Store allocation, then run stride1
only. The [committed plan](../../../../docs/roadmap/0.1/0.1.7/macos-seal-allocation-stride1-20261007.md)
freezes the selection before execution. Init and strides10/3 are not selected.
Disposable/WAL/OFF remains the only executable Store profile; Durable is
`NOT_RUN — disabled by owner until explicit reauthorization`.

## Mechanism and bounded work

Persistence seal now captures original file identity, checkpoints once, checks
SQLite close and sidecar absence, then releases unused macOS extents beyond EOF
with the existing safe nix0.31.3 API. It keeps the same original inode and bytes.
The helper opens only a regular, singly linked Store and an exclusively created
empty sibling; checked transfer/close/unlink have one attempt. Unknown transfer
or temporary-close retains the sibling; no fallback, replay or guessed deletion.
The old per-pack preallocator and allocation owner remain retired. No new unsafe
code or third-party version is introduced. Linux's seal path has no extent call.

The helper retains constant metadata and two descriptors; it issues a fixed
number of filesystem calls, with at most one transfer and unlink. It scans/copies
no payload or namespace, and adds no SQL or write transaction. Kernel extent
release can depend on the number of unused extents; constant syscall count is
not a claim of constant kernel work. The final seal is inside the measured
operation. This is host/offline finalization, not live shared-daemon maintenance.
Canonical Content, encoding, pack format and GroupRowsIndexed schema3 are unchanged.

## Scoped proof receipts

All receipts are append-only under [checks](checks/macos-seal-allocation-20261007/).
All ordinary test invocations use a100s stop and one construction worker.

| Receipt | Outcome and scope |
| --- | --- |
|01|Plan commit exact production LOC165673→165673, delta0|
|02|Locked build of allocation regression before the fix, PASS|
|03|Expected regression FAIL: forced16MiB excess survived prior seal; original bytes/inode/length already matched before the allocation assertion|
|04|Locked host build of the two affected proof binaries, PASS|
|05|All5 allocation tests PASS, including all3 layouts, reopen/read, Busy, read-only, hard-link and symlink refusals|
|06|Host serverless binary:5 tests PASS, including real second-process Busy and reader progress|
|07|Owning harness tests:41 PASS, including preserved stride1 limits, roots and prior allocation failure|
|08|Clippy, Persistence/Project/SDK/Daemon all targets, `-D warnings`:PASS|
|09|Core formatting check:PASS|
|10|Product boundary:PASS,707 Rust/SQL files; semantic review separate|
|11|Guard/tool self-tests:46 PASS|
|12–13|All8 Linux/host source/config hashes match|
|14–15|Linux build and serverless proof:PASS,5 tests, native `/tmp` Store; bundled SQLite3.53.2|
|16–17|Locked release build PASS; binary/dependency/config hashes and protected-note/container identities retained|
|18|Added/new documentation links and whitespace PASS|
|19|Exact staged implementation production LOC165673→165813, delta+140|

The host uses system SQLite3.51.0. Linux proof image is
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Rust1.85.1 locked builds retain the repository ARM64 inputs. The existing unused
fuser patch warning is a Cargo graph warning; Clippy reports no lint failure.
Durable code compiles as part of these targets but no Durable path executes.

The original-file proof observes:

| Layout | Logical B | Forced allocation B | Allocation after seal B |
| --- | ---: | ---: | ---: |
|Monolithic|135168|16912384|135168|
|GroupRows|143360|16920576|143360|
|GroupRowsIndexed|147456|16924672|147456|

Each row has identical streamed file hash, device/inode and length before/after,
no leaked owned sibling, an untouched unrelated sentinel and successful object
read after reopen. A second seal in that test covers the no-excess path.
The pre-fix failed fixture is retained at
`/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-seal-allocation-45223-0`.
All successful test fixtures were cleaned by their owning test; no terminal
unknown occurred in these proofs. Error-path transfer/close fault injection was
not added to production or executed; its conservative uncertainty rules are
source reviewed, not presented as injected-failure qualification.

## Frozen measurement

Selection: `phase7-sqlite-disposable-history-stride1-group-rows-indexed-seal-allocation-v1`,
157 states, candidate only, one sample. Original allocated ceiling92342273B;
complete performance300s, separate proof30s, outer launcher370s. Existing
immutable corpus reused; all construction remains inside the operation. Enforce
source-content zero residency and Store/extant-WAL cold boundaries for each state.
All-state structure and the existing five-anchor bounded-content oracle remain.

The selected stride1 allocation gate is now **PASS** at source
`4156e90707b4b9fa8ca1bdc1b28cf3b5d8999521`. One sample ran; no Init or
stride10/3 rerun occurred. The [raw receipt](checks/macos-seal-allocation-20261007/22-stride1-seal-allocation-v1/receipt.json),
[wrapper](checks/macos-seal-allocation-20261007/21-stride1-wrapper/run.json) and
[result/preservation checks](checks/macos-seal-allocation-20261007/23-result-and-preservation.json)
retain exact operands and commands. All prior verdicts remain unchanged.

| Stride / states | Product ns | Complete performance ns / bound | Separate proof ns / bound | Original allocated B / ceiling | Functional / storage / cold / cleanup |
| --- | ---: | --- | --- | --- | --- |
|1 /157|177632200958|200933227625 /300s|18952711334 /30s|85348352 /92342273|PASS / PASS / PASS / PASS|

The outer launcher is221544088000ns /370s, exit0. The external driver itself
is180143386125ns; complete performance also pays20787560416ns source-cold
attestation and terminal observations. Seal is31452000ns inside the product
operation. Build/proof are separate; no nested spans are summed to manufacture
product time. C2 and C5 share one Store, so no split-file allocation is invented.

| Comparison | Prior | New | Raw difference |
| --- | ---: | ---: | --- |
|Original allocation vs prior WAL|101498880B|85348352B|−16150528B /−15.912026%|
|Logical file length vs prior WAL|85348352B|85348352B|0B|
|Product time vs prior WAL|182854812667ns|177632200958ns|−5222611709ns /−2.856152%|
|Allocation vs retained MEMORY incumbent|85172224B|85348352B|+176128B /+0.206790%|

Allocation is6993921B below the unchanged ceiling. No physical allocation beyond
logical EOF remains in this observation. The small logical delta against the
older MEMORY incumbent remains; this correction does not alter encoded bytes
or SQLite layout. The new original, new proof copy and old failed original all
hash to `94613822a21812af141537decea420256f5a6118a01bcce59d6796d4828fac19`.
The old Store still occupies101498880B and is untouched.

Historical speed arithmetic remains `10 × candidate_ns <= 11 × reference_ns`:
`1776322009580 <= 2119839614299`, PASS, versus the retained independent
Phase4.5 reference192712692209ns (−7.825375%). The prior WAL and MEMORY values
are unpaired historical observations; one sample does not establish repeatability
or a causal speedup. `admission_eligible=false` remains explicit.

## Correctness, identities and resources

All157 producer roots exactly match the prior WAL receipt and the independent
retained root expectations. Current independent proof checks904143 structural
path-states,157 custody states and66 selected content paths, authenticating
921174B from5347088B acquired under the existing five-anchor policy. This is
all-state structure plus bounded sampled content, not every payload byte.
Canonical census is104618 objects /871337620B; inventory SHA256 remains
`f0966c7721109a06a84ccad49f1e7b209090df627fb31f5094a0476bf80f1d87`.
Original-file SHA256 and device/inode remain stable through the proof; no
WAL/SHM/journal or owned allocation sibling remains.

| Identity | Value |
| --- | --- |
|Source / tree|`4156e90707b4b9fa8ca1bdc1b28cf3b5d8999521` / `6d107c7036dea2dbafb6cbc3f405548e32da84b5`|
|Product source seal (history family)|`b25d332dd008f191aa44d281438f13e51e5ef982659e3cc21dd5cc16f9f4e010`|
|Compilation seal|`db72ff16e78d32353a3c7d8f144db04f186d4ae07401bd7527e2af0e9793de9c`|
|Cargo lock|`9b140178803f618c44d37ca5fd3e2dee0f6809ab9405858db56a6e7ea270c1af`|
|Harness seal|`82a6fc0b40888994f77cc8a6149d0180e43e95f6427aee8b85153e93eacec399`|
|benchmark_history SHA256|`2cf43b242e8b32bce909fa8b4e4aacf427433a798d72de6c1ddef63c8c8f2dda`|
|verify_history SHA256|`a4b042159638070e37bdde392cdbffe517651985de303f2c2808f5e9c10492e4`|
|Corpus manifest|`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`|
|Topology / image|Native macOS26.4.1 arm64, system SQLite3.51.0; benchmark image N/A|
|Profile / layout|`sqlite-wal-off-v2`, WAL/OFF, GroupRowsIndexed schema3|
|Cache|`history-source-cold-and-database-state-boundaries-v1`; source180444 files, resident_after0;157 Store/extant-WAL checks all0|
|Build/setup|Locked release1.85.1, existing target reused; runner incremental check233739166ns; immutable corpus reused, fresh Store built inside operation|

Filesystem metadata and bounded product buffers retain the declared cache
exclusions. Observer inputs/binary and cold-helper inputs/binary are hashed in
the raw receipt and reused through seals. Observer overhead remains inside the
driver. Driver wait4 lifetime CPU120397312000ns and peak RSS327057408B are
reported as lifetime observations, not phase peaks or residency admission.
Exact continuous phase/native allocation and physical device I/O remain
unqualified; no new resource threshold is introduced. Existing Docker services
remained active and are listed as competing work. No concurrent owned Cargo,
test or measurement ran in this worktree.

Reproduction of the registered selection (original outputs already exist and
must never be overwritten or replayed unchanged):

```sh
LAYERFS_CAMPAIGN=macos-seal-allocation-20261007 LAYERFS_CONSTRUCTION_WORKERS=1 \
python3 -B core/docs/issues/307/checks/incumbent-restoration-20261007/measure.py \
/Users/yifanxu/Ephemeral-AI-Lab/layerfs \
phase7-sqlite-disposable-history-stride1-group-rows-indexed-seal-allocation-v1 candidate 370
```

## Dispositions and retained custody

- Receipt03 remains FAIL at the pre-fix identity. Its failed fixture remains.
  The new receipt closes this selected allocation defect; no old FAIL is relabeled.
- Init and stride10/3: NOT_RUN in this correction, per owner scope. Their existing
  matrix results, every speed/allocation failure and100k cold ineligibility remain
  in [the original report](DISPOSABLE-WAL-MATRIX-20261007.md).
- Strict-allocation selections: NOT_RUN — mechanism removed. This sole-owner
  completion does not restore the former per-pack preallocator or its gates.
- Durable execution: NOT_RUN — disabled until explicit owner reauthorization.
- S8 daemon FUSE/Exec and S10 live namespace normalization: NOT_RUN here. The
  fix is common macOS Persistence seal; the live Linux Store is not sealed per
  construction or Commit. No new daemon performance claim follows.
- The old failed original Store, new original Store, independent proof copy,
  raw logs, census, canonical roots and archived binaries remain at their original
  ignored benchmark paths. Compact/text evidence is copied byte-for-byte into22,
  with hashes in23. No terminal unknown occurred. Successful scoped test fixtures
  and the run's own allocation/ordering scratch were cleaned by their owners.
- The three untracked owner-preserved notes remain unchanged and unstaged, with
  hashes in17/23. Containers `9cf2fe345496`, `ce75ac504df9`, `d2433851ea59`,
  `d2550144998b` remain running. Root `crates/`, unrelated worktrees and processes
  remain untouched. No push, release, deployment or new worktree occurred.

## Source size

Plan commit `122b8c1b3`: Production LOC:165673→165673 (delta0).
Implementation commit `4156e9070` (receipt19): Production LOC:165673→165813
(delta+140). Core100256→100396; active57382→57522; reference65417,
excluded predecessor36325 and excluded integration6549 unchanged. Same pinned
`tools/production_loc.py` counter, first parent against staged product tree;
no reference or host-mediated transport retirement is included in this change.

Receipt20 verifies both committed product trees against their exact LOC snapshots.

Results-only commit (receipt25): Production LOC:165813→165813 (delta0);
Core100396, active57522, reference65417, excluded predecessor36325 and
excluded integration6549 all unchanged. Runtime checks are reused from the
identical product implementation; only evidence/reporting changes follow the run.

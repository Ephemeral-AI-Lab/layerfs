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

Performance is pending the final source commit and release-binary seal. This
checkpoint does not claim the allocation gate closed until that receipt exists.
Prior WAL stride1 failure101498880B at logical85348352B remains unchanged; the
historical MEMORY result85172224B is separate. Releasing excess allocation does
not promise removal of the176128B logical-file delta.

## Source size

Plan commit `122b8c1b3`: Production LOC:165673→165673 (delta0).
Implementation staged tree (receipt19): Production LOC:165673→165813
(delta+140). Core100256→100396; active57382→57522; reference65417,
excluded predecessor36325 and excluded integration6549 unchanged. Same pinned
`tools/production_loc.py` counter, first parent against staged product tree;
no reference or host-mediated transport retirement is included in this change.

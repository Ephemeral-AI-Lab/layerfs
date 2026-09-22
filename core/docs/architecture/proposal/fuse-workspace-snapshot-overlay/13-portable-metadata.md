# R2: save portable metadata through the shared service

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Implemented from `f566015b1ed2905f987f3fa9c10c99ecf959aea3`, 2026-09-21. The
> Workspace-consumption rules below (bounded second metadata call, metadata-only
> setattr fallback) were refreshed in the issue-179 documentation round against
> product source `f802cc124`.

R2 adds one public operation, `UpdatePortableMetadata`, through the existing
bridge/service C1/C2 route. It constructs a changed attribute tree from an
existing metadata root. The result is a saved metadata root; the caller remains
responsible for associating it with its captured inode/version and retaining
that inode's content root. Saving this tree does not attach an inode or publish
a Branch, stage, Commit or Layer.

The operation takes base metadata root, kind, mode, signed mtime seconds and
nanoseconds. It uses content profile 1/opcode 9 and independent Store grant
`0x80`, with empty input and zero ResultData budget. Its exact encoded request is
76 bytes. Result tag 11 is 98 bytes and echoes every input field before the new
metadata root and actual inserted/reused counts. Native validation rejects a
mismatched field or any ResultData; mutation delivery loss remains unknown.
The existing three-byte failure format and authentication are reused.

Service admission and `operation/write.rs` retain the single save owner. C1
validates typed base fields and streams two sorted patches for portable mode and
mtime, preserving generic value roots. A bounded provider/consumer delegation
checks the existing absolute deadline before and after each read wave/object
handoff. Retained C2 failure takes precedence; definite failure aborts once.
There is a final deadline check before finish. Acknowledged finish success is
not converted into a guessed abort by a subsequent clock observation.

The [attribute hierarchy prerequisite](10-attribute-hierarchy.md) fixes wide
construction and checked descent. [Early-refusal teardown](12-early-refusal.md)
fixes a native response race observed by the full-core check. The operation adds
no network object reads, full attribute-map materialization, replacement file
construction, hidden smaller Commits or new dependency. It leaves existing
prepared-update and EditFile count/byte limits unchanged.

Product files are the bridge contract request/outcome/permission unions and new
`contract/metadata.rs`, existing native request/response codecs and matching,
plus service dispatch/write integration and `operation/metadata.rs`. External
tests reside in bridge `tests/portable_metadata.rs`, service `tests/history.rs`
and daemon `tests/metadata_route.py`. [Architecture 14](../../14-service-runtime.md)
describes the resulting shared boundary.

## Verification and exact scope

The native driver starts the production host daemon and authenticated service.
It reuses R1's closed Store/catalog through independent byte copies, verifies
their original hashes, and leaves the master unchanged. This is functional
verification with no cache or performance claim.

The first actual route passed in 3.38372199999867 seconds. It predates the
subsequent early-refusal fix and is retained under its original product seal.
The final route uses the rebuilt source after that fix; both receipts remain
append-only in [R2 evidence](evidence/r2-metadata-20260921/).

The final metadata command passed in **3.356219292007154 seconds**. The actual
Linux-mounted Status refresh passed in **24.895686834002845 seconds**, including
held-read progress, handle lifetime, independent authority and clean shutdown.
Both carry product-input seal
`fef867b4753d1a2d199efc9ea529f7bb95464c28090d24c48ffc8331ed17e380`.
The refreshed registry accounting is 1,387,596 bytes idle and 1,535,052 during
the held read; the difference remains 147,456 bytes. Those observations are
accounted working allocations, not RSS, a peak or a resource-performance gate.
They do not replace the original R1/Status receipts or rerun every R1 callback.
All functional command walls are below their unchanged 60-second hard budget.

Verified observations include:

- Portable mode `0600`, seconds `-2` and nanoseconds `987654321` are saved and
  echoed exactly; the original inode/tree remains unchanged.
- A separate prepared filesystem save installs the metadata root on existing
  serial 3, retaining its exact content root and both hard-link aliases.
- All 577,551 bytes read through the real service match the independent fixture.
  The Branch descriptor and effective root remain unchanged after both C2 saves.
- A no-op preserves the metadata root, with zero inserted and seven reused
  objects in this fixture. These are operation results, not a performance gain.
- Grant 127 is denied; missing and wrong-role bases receive definite failures.
  Invalid kind/mode/nanoseconds rejected by the headless daemon's shared decoder
  are labelled NOT_SUBMITTED, separately from authenticated service refusals.
- Direct service tests preserve all 300 generic value roots through a multi-page
  tree, validate all supported inode kinds, exact empty input, deadline refusal,
  writer admission/release and repeated-save identity.
- Bridge tests cover exact sizes, truncated/oversized data, profiles, portable
  fields, independent grants, every echoed field and forbidden ResultData.

The first new service-test build failed because two test helpers used the wrong
existing timing signatures. They were corrected to call the public APIs directly.
The first complete-core run retained the separate Status refusal race; its
assertion was preserved and the shared close route was fixed. The following
whole-core command passes. Host examples/binaries and all-target Clippy pass;
Linux daemon build and daemon/FUSE/Workspace Clippy pass. Formatting, the boundary
guard and all six guard self-tests pass. Logs identify each attempt.

From the implementation worktree root, with repository ARM flags and owned
targets, the checks are:

```sh
LAYERFS_CONSTRUCTION_WORKERS=1 CARGO_TARGET_DIR="$PWD/core/target" \
  cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
CARGO_TARGET_DIR="$PWD/core/target" \
  cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --workspace --examples --bins
CARGO_TARGET_DIR="$PWD/core/target" \
  cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/crates/layerfs-daemon/tests/metadata_route.py \
  --fixture "$PWD/core/target/pair1-evidence/mounted-07/result.json" \
  --output "$PWD/core/target/pair1-evidence/NEW-METADATA-OUTPUT"
```

R2 qualifies the shared typed metadata-save operation. Mounted timestamp changes
still require the Workspace mutation/index/capture/lowering pipeline and R4
proof. Generic attribute inspection over transport, larger shared inputs, all
remaining R1-C lifecycle/edit/Commit controls, R3a–R5b and R6 remain open. No issue
is closed and no durability, npm or matched performance claim is made.

## Workspace consumption: bounded second call and metadata-only fallback

Two implemented rules govern how the live Workspace consumes this operation's
surface (`runtime/host.rs`, `runtime/state.rs`, `filesystem/write.rs`), stated
here against product source `f802cc124`:

- **The bounded second metadata call and its derived floor.** The host admits
  one bounded metadata call — a read-only inspection or one serial reservation
  — that may overlap a remote call **actually in flight**, so an ordinary
  namespace operation can still resolve a name its delta inherits and reserve
  one serial while a save is in flight. The primary admission still admits
  exactly one save, construction, history or attach call, so one construction
  worker stays one producer; the second slot carries no content — never input,
  construction, a save or a history Commit — and it is refused while no call
  is in flight, so a retained admission (a held read reply, a pending
  submission) keeps the single-call refusal. The host's minimum memory budget
  grows by one call allowance (128 KiB, `CALL_SCRATCH`) for that slot — the
  derived floor reserves `2 * CALL_SCRATCH` — while the 8 MiB default budget
  and every other registered bound are unchanged.
- **The metadata-only setattr fallback.** A portable-attribute request changes
  exactly the fields it names; the fallback for an unnamed field is the
  selected record's own current value, never the base version's — a mode-only
  change keeps the live mtime, and an mtime-only change keeps a mode an
  earlier request in the same generation selected. A metadata-only request
  also never selects replacement pieces: the selected content root this
  generation already is the exact desired content.

Both rules are evidenced through the real mount: `round57/final3-ns-setattr-01`
(native mode/mtime/size combinations and refusals),
`round57/final-ns-mounted-kernel-01` (chmod, `UTIME_OMIT` mtime, and a later
mtime-only setattr does not revert the mode) and
`round57/final-ns-mounted-durability-01` (post-Commit metadata unchanged).

## Production source comparison

Production LOC: **99,337 -> 99,637 (delta +300)**. Reference: 65,417 unchanged;
replacement core: 33,920 -> 34,220 (+300). The unchanged `tools/production_loc.py`
(blob `b5b9617d08204977176302311e0b2c72a811b420`) counts exact first-parent and
staged Git archives of `crates` and `core/crates`: first-party production Rust
and runtime SQL, excluding inline tests, tests, fixtures, examples, tools, docs,
manifests, comments and blank lines. The staged product-input seal matches both
final native metadata and mounted Status receipts. Reference source remains;
there is no legacy retirement or measured improvement claim.

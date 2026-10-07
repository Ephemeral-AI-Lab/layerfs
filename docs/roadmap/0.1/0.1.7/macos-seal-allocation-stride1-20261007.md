# macOS seal allocation correction and stride1-only selection

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Issue #307. Owner request: fix the excess allocation now and rerun stride1 only.
This authorizes a narrow macOS sole-owner sealing correction after `b4524542d`.
It supersedes O-22 only for releasing unused extents at this final host boundary;
the former per-pack preallocator, persistent allocation owner and public
checkpoint API remain retired. No daemon cleanup, Content/pack format change,
Save reservation-policy change, Durable execution or broader benchmark sweep.

## Source evidence and correction

The prior WAL stride1 sample has101498880B allocated at logical length85348352B,
against the unchanged92342273B ceiling. Its independent byte copy has the same
hash/length and85348352B allocated. The old implementation explicitly released
unused macOS extents; current seal only checkpoints, closes and checks sidecars.
Canonical object inventory and all157 roots match the earlier baseline. The
separate176128B logical-file increase is not this correction's scope.

After successful checkpoint, checked SQLite close and sidecar absence, perform
one macOS F_TRANSFEREXTENTS operation when main-file allocation exceeds logical
length. Reuse the safe fs API in already-locked nix0.31.3. Capture and recheck
original file identity, require a regular singly linked file, refuse symlinks,
use an exclusively created empty sibling, check descriptor closes and remove
only that owned sibling after known success/known nonmutation. Retain it on
uncertain transfer/close. Preserve exact uncertainty; no retry, copy, vacuum,
payload hashing or preallocation enters production. The caller's existing seal
contract must prevent new openers throughout provisioning. Linux has no new
allocation operation; the daemon's live shared Store remains open.

## Deepest-file plan

| File under repository root | Action and requirement |
| --- | --- |
| `core/crates/layerfs-persistence/src/backend/sqlite/seal_allocation.rs` | New macOS-only original-file checks and one extent-release operation; no long-lived owner or unsafe code |
| `core/crates/layerfs-persistence/src/backend/sqlite/{seal,mod}.rs` | Declare module and call only at the consuming exclusive host seal boundary |
| `core/crates/layerfs-persistence/src/store/seal.rs` | Document the completion step and retained exclusive-provisioning obligation |
| Persistence `Cargo.toml`, `core/Cargo.lock` | Reuse existing exact nix0.31.3 fs capability only under macOS; add its dependency edge, no new package/version |
| `core/crates/layerfs-persistence/tests/seal_allocation.rs` | Public-API proofs with real forced excess allocation, byte/inode/length preservation, no leaked owned temporary, Busy/read-only/alias refusals |
| Existing `tests/serverless_store.rs` | Reuse Disposable seal/reopen and real-process Busy/read-progress coverage |
| `core/benchmark/fs-bench-pro/registry/disposable-wal-matrix-v1.json` | Add only `phase7-sqlite-disposable-history-stride1-group-rows-indexed-seal-allocation-v1`; preserve all old selections |
| `shared/disposable_wal.py`, owning test | Record prior WAL receipt comparison and prove unchanged limits/fixture/reference scope |
| Core instructions, Persistence architecture and handbook | Describe the implemented macOS seal step, platform scope and current disabled Durable policy |

Write a failing allocation regression first, build before execution, then apply
the source correction and run covering proofs once at final identity. Tests have
100s wall stops. All product execution is Disposable/WAL/OFF. Build/check the
affected package targets and the scoped Linux seal proof on native container
storage with the pinned image; preserve source hashes and ARM64 configuration.
No Store is placed under `/work`.

## Frozen benchmark selection

One new candidate performance sample after committed final source. Reuse the
original immutable history corpus and existing canonical root expectations.
No prepared Store removes construction from timing: all157 states, one producer,
GroupRowsIndexed schema3, WAL/OFF, full existing lifecycle and source/database
cold attestation remain. `LAYERFS_CONSTRUCTION_WORKERS=1` throughout.

- Complete performance command:300s. Separate existing all-state structural /
  bounded-content proof:30s. Outer launcher stop:370s. These are the retained
  owner-approved long-history family limits; ordinary tests remain <=100s.
- Original allocated Store ceiling:92342273B. Measure original main/WAL/SHM
  before independent proof; never substitute the proof copy's allocation.
- Preserve old WAL failure101498880B and product182854812667ns at `06dc05a7d`.
  Report raw time/storage differences against it and the original historical
  reference. Comparisons remain unpaired; existing1.10 arithmetic is unchanged.
- Correctness:157 retained roots/custody states,104618 canonical objects,
  871337620 canonical bytes, all-state structure and existing five-anchor bounded
  content oracle. Full original Store hash/identity remains stable through proof.
- No Init, stride10 or stride3 rerun. No new numeric memory/latency threshold,
  best-of selection, retry loop, private MEMORY profile or Durable execution.

Retain append-only numbered receipts under
`core/docs/issues/307/checks/macos-seal-allocation-20261007/` and original runtime
outputs under the worktree's ignored benchmark results. Keep the prior failed
Store and every historical receipt. Record exact production LOC for each local
commit, before/staged/committed trees using the pinned counter. No push, deployment,
new worktree or unrelated process/container cleanup.

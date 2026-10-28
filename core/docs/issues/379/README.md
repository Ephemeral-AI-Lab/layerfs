# #379 Full-disk Commit: error codes and free-space guard

> **Status:** Current fix record for the first part of #379; the orphan-pack
> cleanup it leaves open is a proposal awaiting owner approval.

Issue: [#379](https://github.com/Ephemeral-AI-Lab/layerfs/issues/379).
Baseline: `origin/main` at `2e5061798d` (2026-10-22); rebased onto `caebd268dc`
(2026-10-28), after the #463 layer split moved persistence, store-service
and the daemon to `engine/crates`.

## Problem

A Commit refused for a full disk keeps the packs its Save already wrote. No
code can delete them: the `pack` and `pack_unit` triggers abort any DELETE.
The Store then has no free bytes, and every later write needs at least one WAL
page, so every write is refused `Capacity`. Two error codes were also wrong:
`fs.write` reported `Unknown`, which keeps the call's private Workspace (the
#372 path), and a Workspace Commit reported `Failed`.

## What changed

| Part | Change | Files |
|---|---|---|
| Error codes | `StorageError::is_capacity_refusal()` recognises a definite persistence refusal with status `Capacity`. | `layerfs-storage/src/store/error.rs` |
| | The daemon maps it to `ControlCode::Capacity` instead of `Failed`. | `layerfs-daemon/src/control/failure.rs` |
| | `fs.write` maps a Store port's `History(Capacity)` or storage capacity refusal to `Capacity` instead of `Unknown`, and its Commit step maps the same refusals to `Capacity` instead of `Failed`. | `layerfs-store-service/src/write/mutation.rs`, `write/files.rs` |
| Guard | Before a publication that carries packs, the SQLite provider refuses `Capacity` if the batch would leave less than 64 MiB (`STORE_HEADROOM_BYTES`) on the Store's filesystem. Pack bytes count twice for the WAL. | `layerfs-persistence/src/backend/sqlite/headroom.rs`, `storage/provider.rs` |

The guard is a pre-effect refusal with no retry. It does not change the Store
format, and metadata-only writes are not checked. See
[05-storage.md](../../architecture/05-storage.md) ("Free-space guard").

## What it does not fix

The packs a refused Save already wrote still stay. After the guard trips, free
space sits just above the margin, so a later Commit is refused too until
something frees space. Reclaiming those packs needs Store-side custody of a
Save's packs and narrows the documented "no garbage collection" stance (16-history.md §16.9). It
is proposed on the issue and waits for owner sign-off.

## Tests

| Test | Covers |
|---|---|
| `layerfs-storage/tests/capacity_refusal.rs` | Only a definite `Capacity` refusal counts; uncertain cleanup does not. |
| `layerfs-store-service/tests/write_capacity_code.rs` | `fs.write`'s mapper returns `Capacity` for both port refusals and keeps `Unknown` otherwise. |
| `layerfs-persistence/tests/store_headroom.rs` | On a real small filesystem, a pack batch is refused `Capacity` while the margin is free, and a metadata write still lands. Ignored by default; needs `LAYERFS_TEST_DISK_ROOT` on a dedicated 96 to 256 MiB filesystem. |

The daemon change is one branch that calls the tested helper; it has no
separate test. No tests were deleted.

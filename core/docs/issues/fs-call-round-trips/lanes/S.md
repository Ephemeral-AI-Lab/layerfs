# #818 lane S (per-object storage reads): notes

> **Status:** Working notes of lane S on branch `rt818-lane-s`, base
> `2a9a328930` (lane RH's port commit). Predictions are written before the
> code; measured values come from one run of the item's tests.

Identity: host cargo `+1.85.1`, test profile, private target cloned from the
rollout worktree; PostgreSQL container `lfs818-rt-pg`, database `rt818_s`.
Scope: `layerfs-storage` `read/objects.rs`, `read/prefetch.rs`, `read/fetch.rs`,
`read/length.rs`, `read/provider.rs` (granted by the coordinator for the
additions only), `encoding/pool/read.rs`, and new storage test files.
`encoding/pack_cache.rs` needs no change: `acquire_groups` already serves one
pack, so its failure is attributed by its callers.

## S1 — exact per-object results for one batched demand (H5/H6 support)

**Prediction (written before the code).**

- Additive: `ReadState::read_each` beside `ReadState::read`, `prefetch::chains_each`
  beside `chains`, `PoolReader::prepare_leaves_each` beside `prepare_leaves`,
  `length::read_each` beside `length::read`. The existing functions keep their
  code; the one shared change is a pure extraction (`Fetch::inode_values_found`,
  the body of `Source::inode_values` up to its final mapping, and
  `length::whole_file_length`).
- Nothing calls the new path until lane RH forwards it, so **no existing pin
  moves**: PostgreSQL trips and SQLite statements of every pinned case are
  unchanged (0 delta, both backends).
- On healthy input `read_each` issues exactly the calls `read` issues for the
  same ids (same locate, page, whole-pack, group-read, chain-locate,
  acquisition and value-lookup calls, in the same order): predicted equal
  provider call lists and equal `Diagnostics`, both encodings in use (ordinary
  delta chains and pooled inode leaves).
- On damaged input each damaged object (and only each object whose chain or
  pack or values need the damaged unit) gets the error its point read
  returns; no extra provider call is made for the damaged unit (it is never
  requested twice), and healthy objects are served from the same demand.
- Memory: the per-object outcomes are the output vector (one slot per
  requested position, bounded by `storage.read_objects`); the chain walk keeps
  one base id and at most one error per visited node (bounded like today's
  `visited` set by the cohort's roots × chain depth); the leaf window keeps its
  existing bounds (`PREPARED_LEAVES`, `VALUE_ROOM`).

**Measured (one run each; receipts in `receipts/S1/`).**

| Check | Predicted | Measured | Within 15% |
|---|---|---|---|
| Existing pins (`sdk_read_round_trips`, `sdk_history_round_trips`, both backends) | unchanged | pass unchanged (no pin edited) | yes (0 delta) |
| `sdk_round_trip_equivalence_seeded` | pass | pass (2/2) | yes |
| `layerfs-storage` suite (45 binaries incl. lib) | pass | 191 passed, 0 failed | yes |
| Healthy `read_objects_each` vs `read_objects`, every stored object of a 5-file × 5-version delta history, plain (SQLite-like) and batched-chain (PostgreSQL-like) ports | equal call lists, `Diagnostics`, pooled counters, bytes | equal (also through `read_each_with`) | yes (0 delta) |
| Same for 7 pooled inode leaves (one 4-version pooled chain), both ports | equal, one window value lookup | equal | yes (0 delta) |

Damaged demands (`tests/per_object_reads.rs`): every per-object result equals
that object's point read on a fresh handle (`Debug` of the `StorageError`, or
the bytes' id), some fail and some are served: missing object (exactly the 2
absent ids fail), digest mismatch, undecodable body (flipped group byte),
broken chain (deepest base removed: it and its dependents fail with its
absence), missing pack body (plain port: only its objects and their
dependents fail, the pack is requested once), a pooled leaf's missing stored
value (window lookup) and a pooled leaf whose record is not in its group
(rebuild). A transport failure (`Uncertain` / `BackendUnavailable`) at every
read call of the demand, both ports, fails the whole call. File lengths: each
id's result equals its point `file_lengths`.

Decisions and limits:

- `read_canonical_batch`, `read`, `chains`, `prepare_leaves` and `length::read`
  are unchanged in behaviour; the per-object path is new code beside them.
  Shared by pure extraction only: `Fetch::inode_values_found` (the body of
  `Source::inode_values` before its final mapping) and
  `length::whole_file_length`.
- Call failures (`call_failure`): any `Io` except a provider `Missing` or
  `Malformed`, `Busy`, `UnknownOutcome`, `CleanupFailed`, `Aborted`,
  `OwnershipUnavailable`, `UninspectedState`, `UnsupportedPolicy`. Every
  failure of a multi-unit call (demand locate, page / level `read_groups`,
  whole-pack batch, level locate) stays a call failure, as the contract allows
  ("a failure the provider cannot tie to one object fails the call").
- Limit: on the batched (PostgreSQL) route a pack refused inside a group read
  over several packs fails the call, because `read_groups` reports it for the
  call. Per-pack attribution there needs `fetch_page` (`read/hints.rs`, not
  lane S's file) to use `read_groups_classified` in per-object demands; not
  built. Tested as a call failure with the pack requested once.
- Limit: a chain longer than the depth bound reached through a shallower
  requested element is walked from that element in a batch; Saves never write
  such chains.
- Hint replay: as `read`, a walk whose failures include a hint-caused one
  (`hint_failure`) is rebuilt once without hints when hinted locators were in
  use; the rebuilt walk's per-root results stand.
- Memory: outcome slots per requested position (≤ `storage.read_objects`);
  per cohort walk one base id, at most one error per visited element and per
  failed pack; the leaf window's `PREPARED_LEAVES` / `VALUE_ROOM` /
  `PREPARED_BYTES`. No new bound, cache or constant.
- Deleted tests: none. New test file `core/crates/layerfs-storage/tests/per_object_reads.rs` (9 tests).

Production LOC: 230,606 -> 231,175 (delta +569)
  core     46,362 -> 46,931 (+569)  layerfs-storage +569 (read/objects.rs +229, read/prefetch.rs +130, encoding/pool/read.rs +91, read/provider.rs +59, read/length.rs +53, read/fetch.rs +7)
  engine  109,094 -> 109,094 (0)
  control  64,634 -> 64,634 (0)
  api      10,516 -> 10,516 (0)
  reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)
  Method: tools/production_loc.py --detail on parent 2a9a328930 (git archive) and the staged tree

## S2 — per-pack results inside multi-pack reads (closes the S1 limit)

Coordinator 2026-11-27: lane S also owns `read/hints.rs`.

**Prediction (written before the code).** A per-object demand reads every
pack batch with `read_groups_classified` (PostgreSQL: one statement, as
`read_groups`; per-pack `Missing`/`Malformed`) instead of `read_groups` /
`read_packs`: the same calls with the same packs, so trips are unchanged (0
delta; the statement text differs: `objects::read::classified_ranges` instead
of `ranges` / `read`). Existing callers (every non-per-object read, Saves,
Commit) keep `read_groups` and their counts exactly: no pin moves on either
backend. SQLite: no batched page and no whole-pack batch, so no call changes.

**Built.** `Fetch::per_object()` (a guard `read_each` holds) keeps the
demand's refused packs on the handle (`Fetch::refused`, `None` otherwise), so
every page the demand reads is classified, the walk's, the reconstruction's
and the pool reader's `prefetch_chain` included: an own pack's `Missing` /
`Malformed` or failed admission is refused; a member-only pack's failure stays
a stale hint (and is refused when `Missing`); a refused pack is skipped by
page planning, by `chains_each` and `read_each` acquisitions and by
`Fetch::whole_packs_each` (the whole-pack batch as one classified call of
whole reads, frame-checked per pack). Without the guard `fetch_page` is
today's code path (the refused checks are no-ops), so `chains_each` lost its
local failed-pack map in favour of the handle's.

**Measured (one run each; receipts in `receipts/S2/`).**

| Check | Predicted | Measured | Within 15% |
|---|---|---|---|
| `sdk_read_round_trips`, `sdk_history_round_trips` (both backends) | unchanged | pass unchanged, no pin edited | yes (0 delta) |
| `sdk_round_trip_equivalence_seeded` | pass | pass (2/2) | yes |
| `layerfs-storage` suite | pass | 191 passed, 0 failed | yes |
| Healthy per-object vs batched read, plain / batched-chain / whole-pack ports, ordinary and pooled | same calls and packs; classified replaces `read_groups`/`read_packs` 1:1; plain port no classified call | as predicted; `Diagnostics` and pooled counters equal | yes (0 delta) |
| Missing pack inside a read of several packs (batched-chain and whole-pack ports) | only its objects and dependents fail, equal to point reads; pack requested once | as predicted | yes |

Decisions and limits:

- A per-object demand's pack reads now use the classified port call; its
  PostgreSQL statement differs from the batched read's (same trips). Pins of a
  per-object caller (lane RH) should be taken with this path.
- Residual: a pack first refused by a reconstruction's own acquisition after
  the walk (only if the pack cache evicted a group the walk read, within one
  cohort) is acquired by that reconstruction; the refused pack is then that
  root's failure, equal to its point read.
- Deleted tests: none. `per_object_reads.rs`: the S1 test that asserted the
  batched route fails the call now asserts per-object results on all three
  ports and one request of the refused pack inside a classified read of
  several packs; the healthy, missing-object, mismatch, pooled and transport
  tests run on the whole-pack port too.

Production LOC: 231,175 -> 231,300 (delta +125)
  core     46,931 -> 47,056 (+125)  layerfs-storage +125 (read/hints.rs +118, read/objects.rs +6, read/fetch.rs +2, read/prefetch.rs -1)
  engine  109,094 -> 109,094 (0)
  control  64,634 -> 64,634 (0)
  api      10,516 -> 10,516 (0)
  reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)
  Method: tools/production_loc.py --detail on parent dec2216e23 (git archive) and the staged tree

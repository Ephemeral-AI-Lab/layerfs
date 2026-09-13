# Phase 2 payload component evidence

Status: the host payload component passes its focused checks. This is not a
Phase 2 completion claim, public-path snapshot acceptance, or benchmark result.
V1 remains open. Existing HostSpool/FUSE/SDK consumers are not yet replaced by
this component.

Source: `crates/layerfs-workspace/src/overlay_payload.rs`, with the disk ownership
hooks in `overlay_index.rs`. Exact source hashes and commands are recorded per
attempt. Only those source dependencies, fixtures, or applicable custody changes
invalidate a retained pass; a new repository HEAD alone does not.

## Implemented component contract

- Two private create-then-unlink arena files; the existing disk COW index stores
  stable source/range locations, ownership tokens, interval reference counts, and
  deferred release work. There is no per-inode/range file descriptor registry.
- Each metadata leaf independently retains its external payload token using
  `Index::set_owned`. Cloning a root does not enumerate payload references.
  `OwnedRange` represents one admitted in-memory owner, not the complete disk
  graph. Dropping it queues release of that owner's reference only.
- Source-relative ownership intervals are block-normalized, split at affected
  boundaries, and coalesced when adjacent counts agree. A large unchanged span
  does not increment every 4-KiB block. Release advances by one interval per
  maintenance step, with queue/owner capacity admitted ahead of installation.
- Physical release uses portable `File::set_len` after bounded tail evacuation:
  at most 64 KiB of live intervals move per step, copied bytes are compared with
  destination reads, then one catalog root changes. Existing physical readers
  retain their old location until they finish. Failed copy/publication/truncation
  cannot make the old logical state unreadable; unresolved cleanup remains
  charged. No unsafe code or unsupported hole-punch claim is used.
- Ordinary allocation cannot consume the 64-KiB payload move reserve or the
  conservatively calculated catalog recovery reservation. Outstanding writes
  reserve their future catalog work. Memory, active writes/readers, owner records,
  and index pages have explicit independent bounds.
- Appends can extend the existing contiguous physical source frontier beyond its
  disk high-water without rewriting previously published bytes. The source
  high-water never rewinds after successful publication. Adjacent same-source
  owners can be joined into one independently owned range; old owners keep their
  exact earlier lengths. When extension is unavailable, `append` returns `None`
  without consuming input, allowing the caller to create another source.

Raw-token append/join/subrange APIs require the caller to hold the metadata root
that owns those tokens throughout the operation. A bare copied integer is not
ownership. This requirement must be preserved during range-tree/live integration.

## Verification ledger

| Attempt | Identity and result | Validity / reason |
| --- | --- | --- |
| [01](attempt-01.json) / [raw](attempt-01.log) | `partial_retention_disk_owner_reader_quota_and_short_write`: PASS, 0.25 s | Superseded for current-source claims: later review found missing interval-neighbor coalescing and outstanding-write catalog reservation accounting. No failure was hidden; this initial check did not exercise churn. |
| [02](attempt-02.json) / [raw](attempt-02.log) | Same focused check plus 32 released-subrange churn oracle: PASS, 0.36 s | Superseded for current-source claims: SOURCE high-water/count records subsequently changed creation/subrange/release; the shared input-copy helper changed for append. Shared Index uncertainty handling was also bound by the recorded hash. |
| [03](attempt-03.json) / [raw](attempt-03.log) | Partial-retention regression and new sustained-append check: **2 PASS**, 0.91 s | Superseded for current-source claims by the Index physical backend and later payload fixes. No unrelated test families were rerun. |
| [04](attempt-04.json) / [raw](attempt-04.log) | New `writes_continue_while_old_source_reader_defers_truncation`: **FAIL**, 0.01 s | Retained original defect: blanket pending-truncation check returned StorageFull despite free active-arena quota. |
| [05](attempt-05.json) / [raw](attempt-05.log) | **NOT RUN: compilation failed** | New range-module closure index inferred i32 instead of usize. No payload test ran; its owning agent repaired that type annotation. |
| [06](attempt-06.json) / [raw](attempt-06.log) | Exact reader/write regression, oversized-constructor-cap check, and two affected payload regressions: **4 PASS**, 1.59 s | Corrected retired-source versus active-destination rollback predicate; fallible constructor reservation. Bound the new Index physical backend. The reader/write, constructor, and partial-retention passes remain compatible: later changes affect a nonfrontier append branch and a more complete cleanup-status flag; their exercised ownership paths and oracles are covered here and in the exact range cleanup rerun. |
| [07](attempt-07.json) / [raw](attempt-07.log) | One affected payload append regression: **PASS**, 1.36 s | Current source. Shared logical-frontier guard changed this fixture's append-fallback path; only this affected standalone check was repeated. |

The integration review additionally found two real failing range cases. Original
failures remain in [range attempt 01](../phase2-ranges/attempt-01-tests.log):
truncated-owner append consumed input before a noncontiguous join failure, and
cleanup status omitted queued/disk release work. The shared Payload fixes passed
the unchanged [exact slice cleanup oracle](../phase2-ranges/attempt-02-slice.log)
and [exact truncation/append oracle](../phase2-ranges/attempt-03-append.log).
Two unrelated passing range cases were retained by that component's ledger.
See [integration-review.md](integration-review.md) for the callback/lock-order
analysis and remaining aggregate policy wiring.

Attempt 03 measured actual host filesystem allocation via `st_blocks * 512`:

| Focused check | Observed value |
| --- | --- |
| Streamed source size / physical allocation before release | 67,108,864 / 67,108,864 bytes |
| One byte retained through a disk metadata snapshot | 4,096 physical payload bytes |
| Payload moved during partial-retention recovery | 4,096 bytes |
| Payload allocation after all owners release | 0 bytes |
| Payload index allocation at retained-byte observation | 57,344 bytes, reported separately |
| Total interval visits including bounded sweep/churn | 1,357 |
| Sustained append workload | 257 appends × 17 bytes plus initial 3-byte prefix |
| Final sustained-append logical length | 4,372 bytes |
| Physical source-location records for sustained append | 1 |
| Sustained-append physical payload allocation | 8,192 bytes |
| Retained owners at append observation | 2: current range and original 3-byte prefix |

The partial-retention check additionally verifies metadata-leaf ownership after
in-memory range handles drop, a physical reader remaining readable while catalog
relocation defers truncation, quota rejection preserving existing bytes, short
write rollback, stale-token rejection, and physical cleanup. The append check
verifies exact current bytes, the unchanged retained prefix, short-append
high-water rollback, and unread input on the no-extension path.

The updated Index backend in attempt 06 reports 36,864 physical catalog/index
bytes at the retained-byte observation, separately from the same 4,096 payload
bytes. The attempt-03 value above remains its historical observation; neither
value is a benchmark comparison or a speedup claim. The sustained append check
in attempt 07 still reports 4,372 readable bytes, one source-location descriptor,
and 8,192 physical payload bytes.

Environment: macOS 26.4.1 arm64, `rustc 1.96.0
(ac68faa20 2026-05-25)`. No container or benchmark workload was used. The timing
above is the Rust test harness elapsed time, not a performance measurement.
CPU/RSS/I/O-rate statistics were not collected. No #122 case was executed.

## Remaining integration / qualification work

Attach these owners to the actual file-range/inode metadata graph and host
mutation acknowledgment path; preserve source-root leases across preparations.
Connect the existing ResourcePolicy/aggregate host budget and reporting surfaces
to these component limits and physical observations. Qualify contention,
relocation/cleanup I/O failures, real live readers and FUSE/SDK operations through
the integrated path. This component does not resolve kernel mapping visibility,
canonical substitution, all capacity proofs, or the final benchmark campaign.

The newer metadata Index physically moves tail pages before truncation and keeps
its ownership/reverse catalog separately charged. Its own fault and reclamation
evidence is maintained by that component. This payload ledger does not convert
logical metadata slot reuse into a physical-release claim.

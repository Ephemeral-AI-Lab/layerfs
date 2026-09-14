# Step 1 capacity: persisted-token admission defect — diagnosis, repair, evidence

Issue: [#124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124) execution Step 1
("Fix ownership accounting and prove resource bounds"). Related capacity contract:
[#123](https://github.com/Ephemeral-AI-Lab/layerfs/issues/123) (stays open).

Status: **component repair verified; the million-file public-path proof remains NOT_RUN.**
This record is not capacity qualification and not a roadmap-phase completion.

## 1. What the audit predicted, and what actually happened

The predecessor audit (`evidence/phase3-host-capacity/../payload-token-capacity-audit.md`)
inferred from source that `Payload` counted **persisted TOKEN rows** in the same
`State.owners` counter that bounds resident handles, and therefore predicted
rejection at the **8,193rd** independently retained ordinary payload token under
`ResourcePolicy::default()`.

The first observed failure was **different and earlier**. With 8,193 distinct
regular files, one deterministic one-byte ordinary write per file and one bounded
`HostOverlay::maintain()` per write, file **6,841** was rejected with

```
StorageFull: "payload catalog recovery reserve"
owners=6841 persisted_tokens=6841 pending_releases=0
physical=28020736 payload_index_physical=1082957824
```

Raw log: [`attempt01-before-fix.log`](attempt01-before-fix.log) (source
`3f04d4146ebe508a240076e4540de5123b9142d1` plus the uncommitted reproducer).
The rejected file stayed empty and all 6,840 earlier acknowledged bytes were
readable, so the failure was a clean admission rejection rather than corruption.
The payload catalog — not the owner counter — was the binding constraint.

## 2. Root causes (measured, not inferred)

Both defects are in the shared source, not in the reproducer.

### 2.1 The payload catalog's own index was never recycled

`Payload::reclaim_step` released handles, evacuated arenas and finished
truncation, but never called `Index::reclaim` on the payload index. Retired page
versions therefore accumulated for the whole life of the Workspace.

Isolated measurement (`one_byte_write_index_page_cost`, retained as a
non-gating diagnostic):

| files | live index pages | pages written |
| --- | --- | --- |
| 5 | 15 | 58 |
| 50 | 194 | 1059 |
| 200 | 1966 | 5321 |

With a bounded recycle after each write the same run settled at 399 live pages
for 200 files, i.e. the pages are genuinely reclaimable and the growth was a
missing-reclamation defect, not a leak.

### 2.2 A busy release queue starved the zero-reference chain

`Index::reclaim(max_pages)` spent its entire budget on the retired-root queue
before touching the zero-reference `reclaim` chain. Under ordinary writes the
queue is continuously non-empty, so the chain barely advanced, and the caller
loop's "no pages freed" check treated a still-advancing cascade as completion
(a cascade can release hundreds of child references while freeing zero pages).

Two independent faults therefore compounded: the payload index was never asked
to reclaim, and when it was, one of its two queues could be starved.

## 3. Repair

Kept the structural model; corrected which quantity bounds which resource.

1. **Resident vs persisted ownership is now explicit.** `State.owners` counts
   only resident owners: live `OwnedRange` handles plus their queued release
   tickets/jobs. `State.tokens` (exposed as `Stats::persisted_tokens`) counts
   live payload TOKEN rows and is charged to the independent payload-index and
   arena disk quotas. `OwnedRange::drop` transfers its charge to the queued
   release job; releasing a handle whose token is still owned by a metadata leaf
   frees the resident slot immediately.
2. **Admission is unchanged in strictness for resident handles.** The cap still
   rejects a genuinely simultaneous extra handle with `StorageFull`; it simply
   no longer becomes an accidental ceiling on the number of changed files.
   `8192` was **not** raised for the file-count dimension.
3. **Bounded admission assistance.** `Payload::assist` recycles the catalog and
   retires queued handles before the state lock, bounded by a fixed step budget
   and by the resident charge, serialized on the existing maintenance mutex. It
   keeps the release queue and the catalog backlog proportional instead of
   requiring an external maintenance loop.
4. **Reclaim always makes progress.** `Index::reclaim` returns the number of
   retired roots consumed and always advances the zero-reference chain, even
   when the retired queue exhausts the budget. `Index::reclaim_pending` provides
   a cheap backlog probe (no metadata syscalls) for bounded drive loops.
5. **Bounded drive loops judge progress by backlog, not by pages freed.**
   `Payload::reclaim_step` and `HostOverlay::maintain` stop when the index
   reports no backlog, not when a step happens to free zero pages; the catalog
   burst is bounded (8 × 256 pages for the step, 8 × 256 for admission assist).
   The fixed 16-page `HostOverlay::maintain` and payload step were also below
   the retirement rate of ordinary writes, which is why the original checkpoint's
   bounded maintenance could not have held the catalog steady either.

## 4. Verification

| Check | Identity | Result |
| --- | --- | --- |
| Original reproduction, before fix | 8,193 files, default policy, one bounded `maintain()` per write | **FAIL at file 6,841**, `payload catalog recovery reserve`; preserved in `attempt01-before-fix.log` |
| Ordered repair reproductions (600/1,500 files) | same fixture, diagnostic censuses | PASS; catalog ratio fell from ~16.2 pages/file to ~1.97 pages/file |
| **8,193-file reproduction after fix** | same fixture, default policy, `cargo test -p layerfs-workspace --all-features --locked many_ordinary_files_do_not_exhaust_payload_owner_admission -- --ignored --nocapture --test-threads=1` | **PASS**: `files=8193 verified_bytes=8193 peak_resident_owners=0 peak_pending_releases=0 persisted_tokens=8193 owners=0 pending_releases=0`, payload index physical 71,593,984 B ≈ 2 pages/file, arena 33,611,776 B |
| Tiny-cap oracle | `tiny_owner_cap_bounds_resident_handles_not_persisted_tokens` (`owners = 4`) | PASS: 4 simultaneous handles admitted and the 5th rejected with `StorageFull` while admitted owners stay usable; 64 independently retained tokens admitted at `peak_resident=3`, catalog 119 live pages; dropping the retaining metadata root converges to `persisted_tokens=0`, `owners=0`, `index_live_pages=0`, `cleanup_pending=false` |
| Index-level invariant | `repeated_prefix_insertions_release_every_retired_page` | PASS: pages reachable from the live root equal live pages; no unreachable-but-live page after a drained reclaim |
| One-byte page cost diagnostic | `one_byte_write_index_page_cost` | PASS (non-gating measurement) |

The 8,193-file case is an explicitly selected scale regression. It is **not** the
#123 million-changed-file qualification: it uses one byte per file, keeps no
snapshot per file, and never Commits.

## 5. Reused and invalidated evidence

- **Invalidated by this change** (they exercise the modified shared functions):
  payload ownership/reclamation component results, `Payload::reclaim_step`
  callers, and any result that depended on the old fixed 16-page index recycle
  budget in `HostOverlay::maintain`. The affected set was rerun (see the
  verification ledger).
- **Retained**: the three bounded checkpoint fixes (mounted shutdown, actual-spill
  fixture, maintenance oracle) do not exercise these functions and were not
  rerun. SDK/actor/cancellation passes, canonical correspondence and retained-stage
  receipts are likewise untouched by the payload-index and reclaim-budget changes.
- The original failure is preserved exactly as observed; it is not relabeled.

## 6. Honest remaining gaps

- The million-changed-file proof (#123 contract) is NOT_RUN. Its fixture, declared
  RAM allowance and explicit disk quota must be frozen before the definitive run,
  and it requires the public-path integration and V1 resolution that this record
  does not claim.
- Snapshot-reader cache accounting, canonical construction/admission
  reservations, admission-owned seen-spill lifetime and full-attempt peak
  sampling remain open; they are separate Step 1 obligations from this defect.
- `Stats::persisted_tokens`, the index censuses (`catalog_census`,
  `catalog_tree_census`, `catalog_header_census`, `deep_reclaim`) are
  test-only diagnostics; `HeaderCensus`/`Census` do not participate in admission.

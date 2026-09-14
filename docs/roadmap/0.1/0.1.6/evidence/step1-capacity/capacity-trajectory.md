# Capacity trajectory toward the #123 million-file target

Status: **measured trajectory, not the million-file proof.** The #123 proof
remains NOT_RUN. This record exists to freeze, with numbers, what an
"adequate explicit disk quota" actually has to be — which the #123 contract
requires to be declared before the definitive run.

## What was run

The existing scale regression
`host_overlay::tests::many_ordinary_files_do_not_exhaust_payload_owner_admission`
is now parameterised by `LAYERFS_SCALE_FILES` (default 8,193, so the original
selected boundary is unchanged when the variable is unset). Same fixture as
before: distinct regular files, one deterministic one-byte ordinary write each,
one bounded `HostOverlay::maintain()` per write, `ResourcePolicy::default()`,
no Commit, no per-file snapshot, no resident node map.

| Files | Result | Wall | Payload index (physical) | Index pages | Arena (physical) | Metadata index pages |
| --- | --- | --- | --- | --- | --- | --- |
| 8,193 (L28) | PASS | 312 s | 71,593,984 B | 16,236 | 33,611,776 B | 34,560 |
| **25,000** | **PASS** | **1,099 s** | **233,033,728 B** | 49,504 | 117,489,664 B | 105,515 |

Raw log for the 25,000-file row: [`run-25000.log`](run-25000.log). Command:

```
LAYERFS_SCALE_FILES=25000 RUSTUP_TOOLCHAIN=1.85.1 \
  cargo test -p layerfs-workspace --all-features --locked \
  many_ordinary_files_do_not_exhaust_payload_owner_admission \
  -- --ignored --nocapture --test-threads=1
```

Both rows show `owners=0 peak_pending_releases=0` — no resident-owner pressure
at either size, which is the property L28's repair established. The reclamation
gap is flat: 45 free pages at 25,000 files versus 33 at 8,193, against 1.5
million pages reclaimed, i.e. reclamation keeps pace rather than falling behind.

## What extrapolates, and what does not

Per-file costs are close to linear across a 3× scale step:

| Per-file cost | at 8,193 | at 25,000 | implied at 1,000,000 |
| --- | --- | --- | --- |
| Payload index physical | 8,739 B | 9,321 B | ≈ **9.3 GB** |
| Metadata index pages | 4.2 | 4.2 | ≈ **4.2 M pages ≈ 17.4 GB** |
| Arena physical | 4,102 B | 4,700 B | ≈ **4.7 GB** |
| Wall time | 38 ms | 44 ms | ≈ **12 hours** |

**This is the concrete finding: the shipped default quotas bind long before one
million files.**

| Default quota | Value | Per-file cost | Binds at roughly |
| --- | --- | --- | --- |
| `max_payload_index_bytes` | 1 GiB | 9,321 B | **≈ 115,000 files** |
| `max_index_bytes` | 4 GiB | 17.4 KB | **≈ 247,000 files** |
| `max_retained_payload_bytes` + `max_spool_bytes` | 1 GiB + 1 GiB | 4,700 B | ≈ 456,000 files |

That is a **configured quota** becoming the limit, not the accidental
resident-owner ceiling that L28 removed — an important distinction, because the
#123 contract explicitly requires "adequate explicit disk quota" and permits a
declared one. But it must be *declared*, and the definitive run needs:

1. An explicit policy with raised index/arena quotas (order **≥ 32 GiB** of
   index plus ~5 GiB of arena for 1,000,000 files), passed as a configured
   policy rather than by editing a default.
2. An explicit disk allowance for the fixture, and a fixed RAM limit.
3. **≈ 12 hours** of wall time for a sequential one-byte-per-file workload at
   the measured per-file rate. This is a practical constraint on the definitive
   run's scheduling, not a performance gate — the contract is reporting-only, so
   no deadline is asserted here and none may be invented.

The extrapolation is an estimate from two points, not a measurement. It must be
replaced by a real run at each size before the definitive proof, and the
per-file cost may not stay linear if page-tree height grows (height was already
8 at 25,000 files).

## Owner observation: correctness holds, but the rate is slow

The owner reviewed this evidence and stopped the campaign with the note that it
"shows correctness but it is actually quite slow". That is the correct reading
and it is recorded here rather than buried.

Measured per-file rate, `maintain()` called once per completed write:

| Files | Wall | Per file |
| --- | --- | --- |
| 600 | 14.5 s | 24 ms |
| 1,500 | 41 s | 27 ms |
| 8,193 | 312 s | 38 ms |
| 25,000 | 1,099 s | 44 ms |

The per-file cost is not flat: it grows with file count, so the aggregate is
superlinear even though each individual write is small and each reclamation
step is bounded. At the measured 44 ms/file, one million files would take on the
order of 12 hours. **No performance gate is asserted or invented here** — the
contract is reporting-only — but any future million-file run must budget for
this, and the growth warrants its own investigation into where the per-file time
goes (per-write index page versions, the bounded per-write maintenance burst, or
page-tree height growth: height was already 8 at 25,000 files).

This is the same subject as the parked issue #130 (fresh-Workspace-per-tool-call
overhead), which the owner deferred until after this campaign. It is not being
started here.

## What this does not claim

- Not the #123 million-file proof: wrong scale, one byte per file, no Commit, no
  fresh Store reopen, no reopen verification, no cleanup verification.
- No performance gate, no speedup, no latency claim. The wall times are recorded
  as environment context for scheduling the real proof.
- No product change accompanies this record beyond making the existing
  regression's size configurable; the default remains the original 8,193.

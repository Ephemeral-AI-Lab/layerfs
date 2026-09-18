# Stage 6 qualification plan

> **Status:** written during the Stage 6 round
> ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)). This is the
> section the governing review's §10 action 10 assigns to the Stage 6 owner: the
> four unmeasured rows recorded in one place, with a reason each, and the D1 owner
> decision stated beside them so the absence reads as a decision rather than as an
> omission.
>
> Supersedes nothing. Stage 5's records stand as the historical record and are not
> edited. The frozen case specification is
> [`core/docs/benchmark/fs-bench-pro-storage-content/`](../../../../core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md).

## 1. The D1 withdrawal, stated explicitly

Review §11 requires Stage 6 to supply *"a frozen complete-operation comparator (or
an explicit owner decision that none will exist and the claim is withdrawn)"*.

**The second branch is the one taken.** Owner decision **D1**
([`CONTRACT.md`](../../../../core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md)
§5) sets

```text
claim_kind = structural-complexity
```

and withdraws the comparative claim, for a measured reason rather than a
convenience: the only library-matched pair in the tree is `component.primitives`
(3 cases), whose same-tree spread (0.909 → 0.883) is as large as its cross-tree
delta, and `pipeline.filesystem` / `pipeline.c2` are `NOT_RUN` because the
reference's workspace update is private and `WorkspaceAdmission` has no public
method. **No C1/C2 family may be paired with a v0.1.6 family**: they are different
operation surfaces, which `benchmark_rules.md` §8 makes a separate-row question and
not a comparison.

Consequences, frozen and visible in every receipt this round produced:

* every gate is absolute and single-arm;
* `elapsed_ns` is diagnostic and can never produce a `FAIL`;
* the scaling gates read counters, heap and disk.

Reopening is a **scenario change, not a re-label**: it needs a reference-tree entry
point, a new scenario identity and new receipts. Historical rows are never
re-labelled.

## 2. The four unmeasured rows

Recorded here in one place. None of them is replaced by a Stage 5 component row,
and no Stage 5 `NOT_RUN` or owner-WAIVED row is promoted into this list.

### 2.1 The complete-operation comparison (`VF-6`)

**Status: withdrawn by D1, not deferred.**

`VF-6` asked for a complete-operation comparison. It is the row Stage 5 handed
forward explicitly, and its Stage 5 bookkeeping is inconsistent: the verdict column
of `../evidence/stage-5-terminal-20260918T120000Z/verify-VF5-VF6-F5.md:16` reads
`PASS` for a row the matrices record as `NOT_RUN` under an owner disposition. **The
matrices govern** (`stage-5-report.md` §16), and the sibling verifier
`verify-close-evidence-limits.md:33` records the correct reading.

Stage 6 closes it the only way that is honest: the comparative claim is withdrawn
by owner decision, so the row is not a missing measurement — it is a measurement
the contract no longer asks for. The three `component.primitives` rows stay
registered and receipted as diagnostics, excluded from the 217 and from every count.

**What would change this:** an owner decision to commission a reference-tree entry
point. That is a change to `crates/`, not to `core/`, and it needs a new scenario
identity.

### 2.2 Cold-cache rows

**Status: `NOT_RUN` by contract, with the instrument present and unused.**

`CONTRACT.md` §9 states plainly: *no cold-cache claim except under a parameterized
cold contract with verified residency*. The instruments exist and self-check —
`mincore` residency, the ordered `msync(MS_INVALIDATE)` de-warm, and
`disk_read_bytes` device attestation (`>= 0.9 x requested`, or the row is
`INELIGIBLE`) — but a **cold** row additionally needs a way to make the *device*
cold, and nothing in this harness owns the machine's cache. De-warming the pages a
process mapped is not the same claim as reading from a cold device, and presenting
the first as the second is the #151/L18 error this whole discipline exists to
prevent.

The rows that would carry the claim, and their measured state:

| Row class | Measured state |
| --- | --- |
| `c1.*` edit and transition families | declared `warm-in-process-fixture`; the fixture is built before the timed region |
| `c2.*` families | declared `prepared-dewarmed`; `resident_pages == 0` is gated |
| any row claiming a device read | `disk_read_bytes` is recorded per row; no row claims one |

**What would change this:** an owner decision on how device cold is established on
this host. Until then a row that cannot show a device read is `INELIGIBLE` for the
claim and `PASS` for everything else it does measure.

### 2.3 Pack-footprint rows

**Status: instrumented and gated; the `st_blocks` axis is refuted on this host.**

`c2.footprint` runs, and its accounting gate is live: `space.py` asserts the
`object_packs` table exists and sums `length(data)` **without `COALESCE`**, so a
renamed table is `INCOMPLETE` and never a zero that passes `pack_bodies <=
database`. The reflink rung is refused for these rows by `copyladder.py`, in code
rather than in prose.

**E1 refuted the reason the contract gives for that refusal, and the rule survives
on better evidence.** `CONTRACT.md` §6 and `c2-families.md` §3.2 say a COW clone's
`st_blocks` "double-counts blocks shared with the master". On this host it does not:
E1 measured a 64 MiB clone at `st_blocks * 512` = 67,108,864 — the full apparent
size, not a shared figure. The honest statement is therefore the weaker and more
damning one: **`st_blocks` cannot distinguish shared from exclusive allocation on
this volume at all**, so it is not an allocation control for a clone in either
direction. The prohibition stands; its stated basis is corrected here.

Measured state of the axis:

| Axis | Instrument | State |
| --- | --- | --- |
| `st_size` (apparent) | `st_size` | measured and reported |
| `st_blocks * 512` (allocated) | `st_blocks` | measured, `exclusive` attribution only, refuted as a clone control by E1 |
| `page_count`, `freelist_count` | SQLite pragmas | measured and reported |
| `pack_bodies_bytes` | `object_packs` sum, no `COALESCE` | measured and gated |
| sidecars | directory scan | measured and gated |

### 2.4 Process-memory rows

**Status: three instruments, one gate, and an explicit refusal to pool them.**

| Instrument | What it measures | Role |
| --- | --- | --- |
| counting `GlobalAlloc` | exact requested bytes, deterministic for a fixed input | **the** gate for the O(1)-memory claim |
| 10 ms RSS sampler | `proc_pid_rusage` / `VmRSS` | a G4 bound and an anomaly detector, **never** the gate: at 10 ms it cannot cover any phase under ~200 ms |
| product-declared charges (`peak_deferred_bytes`, `peak_pending`, `peak_scratch_bytes`) | the operation's own declared charges | gated against the product's own declared ceilings |

`ps -o rss= -p <pid>` is not used anywhere: it forks a process per sample, capping
the rate near 100 Hz and perturbing the measurement.

**No total-RSS cap is claimed**, and `CONTRACT.md` §9 says so: the contract
establishes application-owned bounds, not a process cap. A lifetime high-water mark
is reported as one and never substituted for a phase peak.

## 3. What this plan does not do

* It does not re-open a Stage 5 row, and it does not promote a Stage 5 `NOT_RUN` or
  owner-WAIVED row into Stage 6 evidence.
* It does not carry the Stage 5 component rows as a substitute for any of the four
  above. Section 2.1 states why the first is withdrawn; 2.2-2.4 state what was
  measured and what was not.
* It does not claim a comparison, a cold-cache number, an allocation control for a
  clone, or a total-RSS cap.

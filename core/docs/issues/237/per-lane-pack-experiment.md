# #237: per-lane pack materialization inside a bounded wave

> **Status:** Research; stopped after a source/count feasibility review and
> focused synthetic check. **No 10k control or candidate public sample ran.**
> The isolated experiment worktree's active product was restored to the
> seven-COMMIT source `3e321080a`; this main research worktree retains its
> integrated 4-MiB-wave source.
> SQLite database pages remain **4,096 B** and the C1 default cutoff remains
> **128 KiB**. The original preregistration below is retained.

## Source hypothesis and feasibility boundary

The [previous 10k pair](bounded-wave-experiment.md) cut the file Save from
80 COMMITs to **7**, but the public caller slowed from 1.364420 to 1.439507 s,
sampled Service RSS rose from 60 to 247 MB, and the dense Store grew about
1.07 MB. That candidate is the **control algorithm** here; its earlier sample
is not reused as a new control measurement. The new treatment changes only
how C2 collects encoded groups for placement inside one of the same bounded
waves. The 8,192-object / 64-MiB wave and its commit-before-unlock rule stay
fixed, as do four file workers and the 128 KiB construction cutoff.

`LanePlacement::select_many` already makes one `SelectedWrite` per receiving
pack **within a call**. The current C2 queue holds one lane and flushes on a
lane switch, so later groups of that lane may append to a pack it wrote earlier
in the same wave. A historical count of about **1,170** pack appends was
reported for an older **4-MiB-wave** control; it was **not measured** on this
seven-COMMIT source and is not the matched baseline. The new pair will print
the exact file-Save `SaveOutcome.pack_appends` for both arms.

An object locator has an immediate SQLite foreign key to an `object_packs`
row. Deferring pack-row creation while inserting locators is therefore invalid.
Deferring the BLOB body after inserting the row would leave a locator whose
pack cannot yet be decoded, forcing barriers at every same-save read, reuse,
delta-base acquisition and collision check. The treatment instead keeps the
existing **encoded-group** queue before placement, with a separate bounded
buffer for each ordinary, native and whole-file lane. It changes no schema,
pack grammar, page size, compression profile or canonical object ID.

## Prospective treatment

- Each of the three eligible lanes may queue at most **256 KiB encoded group
  bodies and 512 locator rows**. Simultaneous queued payload is at most
  **768 KiB** plus bounded member metadata and the existing one-open-group
  per lane. Pooled and singleton groups retain the immediate path.
- A lane switch alone does **not** flush. A projected per-lane bound flushes
  only that lane, and `select_many` then writes each receiving pack once for
  that flush. The queue drains all lanes before the wave collision check,
  candidate-index flush or COMMIT; no queued group crosses a wave.
- An exact reuse, same-save read or delta-base lookup that demands an open or
  queued identity seals and drains before reading SQLite. For this first
  treatment, the demand barrier may flush **all queued lanes**; it must never
  serve a locator whose pack bytes are unmaterialized. Immediate pooled or
  singleton placement also drains earlier queued groups. Direct-reference
  availability continues to accept identities still in bounded open/queued
  groups. Aborted waves roll back placed pack rows and locators together.
- Draining a later lane can change pack-ID order and physical packing. Pack ID
  order is not a canonical file identity, but this is a real physical change:
  record exact root, complete object-ID digest, pack capacity/use/space and
  reopened readback. Any existing test that assumes cross-lane pack-ID order
  must be analyzed and its failure retained, not silently weakened.

Shared, once-per-file-Save instrumentation will be committed **before either
arm**. It adds exact pack creations/appends, pack-write nanoseconds, SQL
nanoseconds and aggregate queue flushes by cause (lane switch, per-lane
capacity, demanded identity, wave/final boundary, immediate lane) to the
existing SaveOutcome line. There is no per-object or per-flush stderr I/O.
The candidate layers only placement/queue behavior and directly affected
external checks over that instrumented control. Source/build/harness and the
shared diagnostic diff will be pinned in the result section.

## One-sample 10k protocol and decision

Run exactly one release-profile public `namespace-10000` Init per arm,
control then candidate, at distinct fresh output paths. Use the exact H3
driver SHA-256
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`
with `--independent-source-copy --fixed-operation-identity`; omit `--verify`
during performance. The sealed seed-1 fixture is **10,000 files and
300,000,000 logical bytes including the 100 MB anchor**. Each run gets a new
writable byte copy. Full source hash/invalidation and immediate nonfaulting
residency check must find **0 resident payload pages** before each timed call.
The host lacks usable `purge`; directory/inode metadata may remain cached
from each arm's own setup, so both rows are exploratory and ineligible for a
fully cold admission claim. Source payload from a previous run may not serve
either timer. The public caller pays for all product reads and Store writes.
Request an exclusive host timing window first; preserve every `FAIL`,
`INCOMPLETE`, `INELIGIBLE` and `NOT_RUN` row without retry or best-of.

After both timed arms, run each arm's own archived full verifier **once** on
its retained Store/History against the sealed manifest, outside throughput.
Require the same fixed public root, all object IDs, 10,101 paths, kind/mode/
mtime and SHA-256 of all 300 MB. Check `PRAGMA page_size=4096`, file-Save
commits **<=8**, pack and Store geometry, Service/daemon CPU, sampled RSS and
lock holds. Exact pack-appends in this matched pair are the mechanism count;
the old 1,170 is not substituted. The treatment's prospective mechanism screen
is **at least 50% fewer pack appends** with no missed dependency/readback or
rollback checks. A public caller slowdown or material Store/RSS regression
rejects speed adoption even if that count falls. A dense 10k Store does not
prove #229 sparse-history compactness; that separate lane remains `NOT_RUN`
unless actually exercised. The 518.8 MB/s historical time is 0.578245 s and
its cache contract was unqualified.

Focused external checks cover queued predecessor and duplicate, same-save
read after a wave, lane switches, rollback, interleaved writers, pack
watermark and reopened pack bytes. No release-admission or full-workspace
verification claim follows from focused checks alone.

## Attempts and outcome

The shared aggregate diagnostic was implemented and committed at `743ca5a1e`
on the isolated `codex/issue237-per-lane-pack-control` branch. It logged
pack creates/appends, pack-write/SQL time and queue flush reasons once per
file Save. It was **never built or timed as a public 10k control**. The
[exact untimed instrumentation diff](evidence/per-lane-pack/untimed-shared-counts.diff.gz)
is retained. A per-lane queue prototype was then edited without a product
commit or public sample. Its
[exact rejected diff](evidence/per-lane-pack/rejected-per-lane-prototype.diff.gz)
changes only C2 queue/placement, a synthetic external check and descriptive
docs. Both research refs remain available in this isolated repository; the
active branch's product files were restored to `3e321080a` before this report.

The first focused command was
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p
layerfs-storage --test c2_bulk_admission --test multi_writer --test
pack_watermark --test persistence_failure --test storage_limits`. It exited
**101** after `c2_bulk_admission`: **4 passed, 1 failed**, so its four later
targets did **not run**. The new interleaved-lane test had asserted zero
`SaveOutcome.pack_appends` but observed **2**. Source tracing found the two
final open lane tails append **after** the preparation wave, during final
publication. A lane-local buffer cannot remove those writes without changing
the wave/finalization boundary. The test oracle was corrected to expect those
two appends and zero lane-switch drains; one focused
`c2_bulk_admission` rerun then passed **5/5** (2.453 s complete command,
0.52 s test execution). That check proves a small synthetic readback and
source behavior, **not** a 10k speed or pack-write result. The candidate's
multiwriter, pack-watermark and full rollback targets did not execute after
the initial failure, and no reopened 10k Store exists for this prototype.

During review, another agent's separate
[4-MiB integrated profile receipt](evidence/per-lane-pack/adjacent-4mib-profile-receipt.json)
became available. It is a **different source/algorithm identity**:
`a9f8b7de4bd5c7451b52a1037dfac1ffce450ced` with temporary instrumented
product seal
`25293a2fc42e0101bbb9165fc495f9d742587f486e0d5d413cdcd282a81deaf9`.
Its one file Save made **1,347 queued drains**: **1,153 encoded-byte bound,
118 lane switch, 75 wave end, 1 row bound, 0 dependency and 0 direct**;
it reported **1,089 pack appends** and **80 COMMITs**. This row had 4-MiB /
512-object waves, unlike the proposed 64-MiB / 8,192-object control. Its
118/1,347 lane-switch share (**8.76%**) warns that the per-lane treatment
targets a minority cause on that adjacent identity. It is **not** a measured
ceiling on the seven-COMMIT source, cannot establish how many of that source's
appends the treatment would remove, and does **not** mark the preregistered
50% appends screen FAIL.

**Decision:** stop this narrow treatment before the public pair because the
new adjacent count points at encoded-byte-bound drains, which this treatment
leaves intact. The 50% matched append screen, public caller speed, 10k RSS,
Store space, canonical root, 10k readback and #229 sparse-history guard are
all **NOT_RUN / NOT_MEASURED** for it, not passing or failing results. No
control/candidate receipt was replaced or hidden. The prior seven-COMMIT
candidate remains a measured transaction-count success and speed/space
regression; this per-lane idea was a smaller feasibility iteration that did
not justify another public sample. A distinct
[wave-wide pack-buffer proposal](wave-wide-pack-proposal.md) examines the
dominant byte-bound cause without editing product source or claiming a result.

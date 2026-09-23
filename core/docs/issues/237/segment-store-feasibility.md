# #237: immutable transaction segments outside SQLite

> **Status:** Feasibility result, 2026-09-23. Source inspected at
> `f26865747daac9da42ecded085ca6ae835a9a514`. No product prototype,
> public operation, or throughput claim was produced in this worktree.

## Decision

An external pack segment is **feasible as a new Store format**, but replacing
`blob_open` with ordinary file writes is not a safe local optimization. The
smallest credible 10k treatment changes the Store from one SQLite file into a
bundle, changes the pack catalogue, closes pack tails at every transaction
boundary, changes readback and abort, and updates the benchmark's copy and
space accounting. Until those changes work together, a public Init pair would
measure a different or broken operation. This round stops at the format design;
the proposed pair below is **NOT_RUN**.

The motivation is the [bounded-wave result](bounded-wave-experiment.md): one
10k file Save reduced SQLite commits **80 to 7** (−91.25%), but its public Init
grew **1.364419666 to 1.439506625 s**, total COMMIT time grew **228.068 to
276.719 ms**, and sampled Service RSS grew about **60 to 247 MB**. Its largest
accounted transaction carried **134.19 MB**. Those observations make large
MEMORY-journal work a useful hypothesis; they do not establish that a segment
will be faster. The earlier actual-owner [pager diagnostic](pager-10k.md)
reported zero cache spills and 94,513 SQLite dirty-page-write events over 79
file-Save commits, but it is another source identity and not a sidecar baseline.

Keep SQLite `page_size=4096`, the whole-file cutoff **128 KiB**, the canonical
pack grammar, the public operation, and all source reads and Store writes inside
their current timer. This proposal changes physical placement only. It must not
use OS cache warmth, `purge`, a platform-specific product API, WAL, sync calls,
third-party patches, or an extra construction worker.

## Why a per-Save append file alone fails

Current `object_packs(pack_id, save_id, data BLOB)` holds up to 16 MiB of pack
capacity. `sqlite::write::{insert_pack,append_pack}` writes a pack's body,
reserved directory entries, then control header through SQLite incremental
BLOB I/O. `pack::placement::LanePlacement` retains one open pack per lane across
preparation waves. `MutationOwner::with_wave` commits each wave before
releasing Store arbitration. A later wave can therefore append to a pack whose
earlier header and groups were already committed. SQLite's MEMORY rollback
journal can restore that BLOB if the later wave fails.

With a plain sidecar file, the later wave would overwrite the earlier pack's
header and directory outside SQLite. A partial write or later SQL rollback
would leave the **previously committed locator** pointing to damaged bytes.
Deleting or truncating the file would damage that earlier group too. The
current runtime transaction-atomicity contract forbids this even though the
product makes no power-loss durability promise. The alternatives are:

1. Copy the whole pack to a new region on each append and atomically switch its
   SQLite locator. This restores rollback but reintroduces whole-pack rewrites.
2. Add an append-only fragment index and reconstruct packs from fragments.
   This changes every pack read and adds index rows and random reads.
3. **Seal every segment at each acknowledged transaction.** A pack tail never
   crosses that boundary. This is the smallest candidate worth testing.

The same rule applies to ordinal-reservation steps: `pool_lane::commit_reservation`
can close the current transaction inside a wave. Segment lifetime must follow
the **actual SQL transaction**, not an assumed preparation-wave count.

## Proposed format and API

Create a Store **directory** at the supplied path, with `catalog.sqlite` and a
`segments/` directory. A directory is necessary to give the catalogue and
segment files one movable identity. A sidecar path derived from a SQLite
filename would break the current hard-link alias behavior: two names for one
database inode can resolve to different sidecar directories, including across
processes. Hard-linking only `catalog.sqlite` must not create a second valid
Store. `Store::path()` would denote the bundle directory; a new explicit
`catalog_path()` is needed for tooling that inspects SQLite. Callers that now
pass `sample.sqlite` may keep that spelling for the directory only after their
raw SQLite opens and copy logic are updated. No in-place migration of a
schema-10 file is implicit.

Use schema **11** with the same application ID and `format_profile=1`; enforce
`PRAGMA page_size=4096` at create and validate it at open. Replace only the
physical pack row shape, leaving object and value-group foreign keys and the
save/publication predicates intact:

```sql
CREATE TABLE object_packs (
    pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),
    save_id INTEGER NOT NULL REFERENCES saves(save_id),
    segment_seq INTEGER NOT NULL CHECK (segment_seq > 0),
    lane INTEGER NOT NULL CHECK (lane BETWEEN 0 AND 4),
    byte_offset INTEGER NOT NULL CHECK (byte_offset >= 0),
    stored_length INTEGER NOT NULL CHECK (stored_length BETWEEN 32 AND 16781312),
    UNIQUE (save_id, segment_seq, lane, byte_offset)
) STRICT;
CREATE INDEX packs_save ON object_packs(save_id, pack_id);
```

Each writer creates at most one file per active lane in an actual transaction,
named only from validated integer `(save_id, segment_seq, lane)`. The file
contains complete canonical **pack bytes**, not a new codec. The lane's single
open pack stays at the end of that lane file; when a new pack starts, the old
one is final and the next begins at the old pack's declared length. This avoids
the current 256-KiB BLOB capacity padding between packs. A pack may grow while
its SQL transaction is open; `stored_length` is updated in the same transaction
after every successful complete write. Before each COMMIT, close all lane tails
and retain the segment bytes as immutable. The next transaction starts new
files and pack IDs. The existing singleton lane already holds one record per
pack. A failed `create_new` must fail explicitly; never overwrite an existing
segment name or derive a path from stored free text. Segment reads must reject
symlinks, missing files, short ranges and malformed pack headers as integrity
failures, not object absence.

`sqlite::lookup::pack_bytes` remains the single read gateway: query the same
save/publication eligibility as today, fetch the segment locator, read exactly
`stored_length` bytes, validate `pack::layout::declared_length`, then hand the
unchanged pack to existing delta and pooled readers. Both same-Save reads and
foreign collision checks go through that gateway; a private save may read its
own already written transaction segment before COMMIT. Every read scope must
carry a bundle path alongside its SQLite connection. `Store::open` must validate
the schema and presence/length of every referenced segment without silently
repairing or warming pack payloads. Its retained pack ceiling remains a range
check, not visibility authority.

This costs up to one partially filled pack per four 256-KiB lanes at each
transaction boundary; the singleton lane has no fixed padding. With seven
file-Save transactions, a coarse *additional* capacity-loss ceiling is
`4 × (7−1) × 256 KiB = 6 MiB` if every lane held an open pack at each boundary.
That is a bound, not a predicted measured increase. Segment files can omit
unused capacity, so apparent and allocated bytes must be measured from the
actual complete bundle rather than inferred from this bound.

## Multiwriter, failure and custody rules

- Keep persisted `saves.active_slot`, the writer budget, SQLite `BEGIN
  IMMEDIATE`, zero busy timeout, and the native arbitration rule. Different
  saves use disjoint `(save_id, segment_seq, lane)` files; the SQL transaction
  still commits **before** arbitration is released. Early committed segment
  files stay private through the existing `save_id` publication filter. The
  global pack allocator is advanced in every transaction that assigns pack
  IDs. Same-Save dependencies and foreign collision validation remain under
  their existing scopes and lock.
- Write all segment bytes and check the returned I/O result **before** inserting
  or updating their SQLite locator. On a definite error, roll back the current
  SQL transaction, then run the existing save-scoped cleanup. Only after SQL
  rollback/cleanup is known to have removed references may that save's segment
  files be deleted. A failed cleanup retains the slot and all uncertain files.
  A failed or unknown COMMIT/ROLLBACK retains the files and quarantines the
  save; never truncate, resend or delete on a guess.
- `Store::open` preserves unresolved private save rows/slots as today. Orphan
  files after a process crash are not automatically reclaimed. Missing or short
  **referenced** segment data is an integrity error. This design adds no crash
  durability promise, checkpoint service, `fsync` or WAL. SQLite MEMORY journal
  and `synchronous=OFF` remain; a power loss may still lose a completed Store.
- A Store is now a bundle. A backup, clone, rename, archive, checksum and
  verifier must include the catalogue **and all referenced segment files**.
  Copying only SQLite is invalid. A closed-bundle independent byte copy is
  allowed for setup; an online consistent copy needs a separate explicit
  quiescence/snapshot protocol and is outside this prototype. Reopening a
  copied bundle must authenticate the object graph. The harness's current
  `byte_copy` and `copyladder` copy one file; `shared/space.py` reads
  `SUM(length(data))` and treats it as pack capacity. Both must gain
  schema-aware bundle handling before any space or throughput gate is credible.

## Minimal bounded experiment, if implemented

1. Start both arms from the seven-commit candidate source `343e4e029`,
   reapplying it in an isolated worktree if necessary. The BLOB control and
   segment candidate must use the **same 64-MiB/8,192-object wave policy and
   final-seal coalescing**; comparing a segment build to this document's
   `f26865747` checkout would confound format and transaction cadence. Make a
   **schema-11 only** candidate with the bundle path,
   transaction-sealed lane files, centralized read gateway, definite-failure
   deletion, and unknown-outcome quarantine. Keep one worker; do not layer a
   second algorithm change into this pair. Update the affected
   storage/multiwriter architecture documents in the same product commit.
2. Focused correctness before timing: pack append then transaction failure
   leaves an earlier committed group readable; same-Save read across segment
   boundaries; second writer interleaving/pack watermark; abort and failed
   cleanup retain only the right files and slot; unknown COMMIT keeps files;
   published reopen and copied-bundle full readback. Run the Core boundary,
   locked formatting/test/Clippy checks at the frozen source identity. No
   performance sample substitutes for these checks.
3. Before timing, preregister one matched release 10k public Init arm per
   source identity with exact source/binary/harness/fixture identities and
   fresh output/Store bundles. Use the H3 independent source byte-copy driver,
   zero-resident payload check and identical cache handling. The current
   metadata state remains unqualified, so label both exploratory rather than
   cold PASS. Keep verification outside the performance timer and default the
   fast lane to `SKIPPED`; separately reopen/read all 10,101 paths and 300 MB
   once at the final identity. Do not repeat an arm to select a number.
4. Compare file-Save commits (target **≤8**), caller time against the
   0.578245-s / 518.8-MB/s historical target, COMMIT and arbitration wall,
   sampled/phase-qualified RSS, CPU, read/write bytes, pack counts, roots and
   object IDs. Report **SQLite + every segment** apparent and allocated bytes,
   segment file count, pack stored/used/slack bytes, and any orphan bytes.
   Require actual page size **4096**, cutoff **131072**, equal canonical
   root/readback, no unaccounted sidecars, and explicit non-passing rows.

**Blocker to a credible prototype in this turn:** current public and benchmark
APIs treat one SQLite file as the complete Store. A write-only sidecar sketch
cannot pass runtime rollback or readback, and a raw 10k number before bundle
copy/space gates would silently omit most Store bytes. No source edits, tests,
segment microbenchmark or timed arms were run for this proposed format.

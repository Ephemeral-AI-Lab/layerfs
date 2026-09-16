# Stages 3–4 measurement addendum

Versioned before collection. Written on 2026-09-17, before any receipt in
`evidence/stages-3-4-timing-20260917T031000Z/` was produced. If any declaration here
changes, a new addendum must be committed before a new receipt is taken; the
receipts already collected stay as they are and are never re-labelled.

## What this round is, and what it is not

It is a **wiring, correctness and single-sample demonstration** for the Stage 3
physical metadata encoding and the Stage 4 localized edits: the real product runs,
the real counters are printed, and the timing tree the operation recorded is saved
with the run. It is **not** a benchmark and **not** a release qualification:

- no warm-cache credit is claimed anywhere; the fixture is built by the same process
  immediately before the timed scope and the Store is created inside the run, so the
  page cache is in whatever state the preceding command left it and the numbers are
  exploratory;
- the runs are **debug-profile** (`cargo build`, no `--release`); the profile is part
  of the receipt, not an optimisation to be tuned later;
- one sample per case per arm, no repeats, no best-of, no discarded attempts;
- no v0.1.6 comparison is claimed in this round. The only reference-tree claim for
  Stages 3–4 is the sealed edit oracle equality
  (`stages-3-4-verification.md`), which compares results, not times, because the two
  products do not share a public edit surface to time.

## Tools and identities

| tool | purpose |
| ---- | ------- |
| `core/crates/layerfs-storage/examples/measure_pooled.rs` | pooled-metadata lane: save-to-acknowledgement, readback, counters, retained footprint |
| `core/crates/layerfs-storage/examples/measure_edits.rs` | C1-only, C2-only and integrated edit lanes (`--timing on\|off`) |

Both tools take `--output FRESH_DIR`, refuse an existing directory, and write
`*.json` timing trees with `create_new` so no receipt is ever overwritten. Both take
`--timing on|off`; `off` runs the same bodies through the timing-disabled path and
prints the same identities, counters and verification lines, which is the
observability-equivalence arm (E3): the two modes must agree on every printed
identity, counter and readback check, and only the timing tree may differ.

Fixture sizes are bounded by the tools themselves (`LEAF_LIMIT = 4 096`,
`ROW_LIMIT = 200`, `FIXTURE_LIMIT = 4 MiB`).

## Arms

| arm | command | case | recorded besides time |
| --- | ------- | ---- | --------------------- |
| E1a | `measure_pooled --leaves 24 --rows 100 --output <fresh>` | small pooled lane | pooled counters, save counters, retained footprint, catalogue |
| E1b | `measure_pooled --leaves 128 --rows 100 --output <fresh>` | medium pooled lane | as E1a |
| E1c | `measure_pooled --leaves 512 --rows 100 --output <fresh>` | larger pooled lane | as E1a |
| E2 | `measure_edits --mode {c1,c2,pipeline} --case {small,chunked,small-to-large,large-to-small,batch} --threshold-bytes 131072 --output <fresh>` | 15 runs | result roots, emitted objects and canonical bytes, save counters, readback verification |
| E3 | the E1a and `--case chunked` pipeline commands repeated with `--timing off` | equivalence | printed identities, counters and verification lines must match the timed run byte-for-byte |

## What each timed scope contains

`pooled.save` (E1): Store creation, the per-leaf `begin_save`/`accept`/`finish`
operations including acknowledgement, fixture construction and the fixture identity
check. One operation per leaf is required because a pooled base must already be
stored before it can be read for a trial; the number of operations is therefore
part of the case, not an artefact.

`pooled.readback` (E1): one authenticated read wave returning the first and the
deepest leaf, verified against the canonical bytes the run admitted.

`file.edit` (C1): base reads through the supplied provider, the validated edit
stream, construction and consumer acceptance. No database, pack, Store or file is
opened.

`storage.save` (C2): Store creation, one base whole-file object, one dependent
whole-file object carrying the base as an explicit predecessor, acknowledgement and
one authenticated read of the dependent object.

`edit.save` (integrated): the real C1 edit driven from a stored base through
`SaveHandoff` until the C2 acknowledgement; the base save is acknowledged before the
scope and is not timed. `verify.readback` is a separate scope.

## Recorded evidence per receipt

- the exact command and the tool identity (commit, plus `sha256` of the executable);
- wall time of the whole command, to be compared against the 15 s per-command budget
  (a small, declared exception list is allowed up to 25 s; no arm is tuned to fit);
- the timing tree (`*.json`, written with `create_new`);
- product counters: `SaveOutcome` (`inserted`, `reused`, `packs_created`,
  `pack_appends`, `commits`, `acknowledged`, `full_records`, `prefix_records`) and
  `PoolCounters` (`leaves`, `new_values`, `reused_values`, `groups`, `full_leaves`,
  `delta_leaves`, `trials`, `work_exceeded`);
- retained footprint: `Store::pool_index_entries`, `Store::pool_index_bytes`, the
  Store file size, and an external catalogue read of group and ordinal counts;
- readback verification: byte-for-byte equality against the admitted or
  independently modelled bytes.

## Simultaneous-allocation ledger

Two sources, declared as such and never mixed:

- **product-reported live capacity**: `Store::pool_index_entries` and
  `Store::pool_index_bytes` are the size of the bounded derivation the Store owns
  (documented as a reported live capacity, not a test hook) and are recorded with
  every E1 receipt;
- **externally accounted C1 live state**: the committed external tests
  `edit_bounds`, `edit_localized` and `memory_bounds` account the deferred node map,
  the peak simultaneous payloads and the live store bytes from outside the product.
  This round does not re-measure them; it cites them by target and commit.

No lifetime cgroup figure is used as a phase number, and no heap figure is inferred
from a file size.

## Limits and honesty rules in force

- per-command wall time ≤ 15 s, declared exceptions up to 25 s (none declared here);
- verification-style commands ≤ 60 s;
- receipts are append-only; a failed or INELIGIBLE arm stays on disk and is reported;
- work that cannot fit the budget is recorded as `NOT_RUN` with its measured wall
  time, never made to fit by moving work outside the timer;
- no aggregate gate, workflow or wrapper is introduced.

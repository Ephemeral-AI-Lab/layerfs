# #190 read-path attribution campaign protocol

> Status: Research; informative and not a product contract. Frozen before the
> first sample.

Base `9fe8eb290072d09594cde3ba67c3bbd1963d5e56`, the #190 continuation handoff on
top of PR #199. Clean isolated worktree on branch `codex/history-data-access`;
actual HEAD recorded in every identity file. Scenario and corpus pins unchanged.

**Question.** Which parts of the filesystem provider's elapsed time are catalogue
lookup, pack BLOB acquisition, control-area validation, group decoding, value
authentication and reconstruction, and which of them holds at least one second of
avoidable work?

**Order.** Attribution first, then one treatment selected from the attribution.
The pre-registered selected-group BLOB range read was **not** selected; the reason
is recorded in [TREATMENT.md](TREATMENT.md) and the README. Priority B's pack-cache
scope and Priority C's pipeline were reviewed but not implemented.

**Frozen before measuring.**

- One sample per case/arm. No repeats, no best-of, no retry of a consumed sample.
- Fresh output paths; append-only receipts. Failures, deferrals and superseded
  attempts stay on disk and are named.
- Both shared global flocks, deduplicated by resolved path, held for the whole
  resource command; the harness's own private `O_CREAT|O_EXCL` marker is taken
  through `shared/receipt.py` and is never opened with append+flock. A held lock
  defers; it never waits and never interrupts.
- Quiet preflight: no named `cargo`/`rustc`/`fs-bench` competitor and at least 70%
  CPU idle on the second of two one-second observations. A busy preflight consumes
  no sample and is retained as a deferral.
- Diagnostic complete-command caps 120 s (stride10) and 240 s (stride3), separate
  verification hard cap 60 s, verification work targets 10 s / 20 s. These are the
  established #190 diagnostic caps and are not ordinary 15/25-second admission
  rows.
- All eight behavioral history switches unset, `LAYERFS_CONSTRUCTION_WORKERS=1`,
  `LAYERFS_HISTORY_PHASES=1`. One construction worker, no second lane.
- Cache contract: fresh growing Store, OS/intra-chain residency uncontrolled.
  Admission INELIGIBLE; no cold claim.
- Rust 1.85.1, `--locked`, no third-party edits, no vendoring, no `[patch]`.
- New instrumentation must be identical in both matched arms. The instrumentation
  here is a separate recorded patch applied to both; the retained product change
  carries none of it.

**Instruments.** `source/instrument-read-path-product.patch` and
`source/instrument-read-path-harness.patch`. Instrument v1 (`baseline`) charged the
whole canonical row-resolution loop to `rebuild_ns`, which made that field a
container rather than a leaf measurement. Instrument v2 (`baseline2`) splits the
loop into `body_decode_ns`, `catalogue_ns`/`catalogue_calls`/`covering_reuse` and
`leaf_encode_ns`. The v1 sample is retained and superseded by the named instrument
defect; it is not used as a matched baseline.

**Treatments.** `candidate` = one catalogue statement per leaf ordinal span
(measured, **rejected**: stride3 regressed). `candidate2` = ordinal-ordered
resolution, the retained treatment.

**Retention rule.** Retain only if stride10 improves by at least one second with
preserved correctness, unchanged stored bytes and an attributable mechanism; then
confirm once on stride3 and run a separate identity-matched verification for each
performance identity. A stride3 regression rejects the treatment.

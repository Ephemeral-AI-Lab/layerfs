# Retained history storage — Phase 0 and Phase 1 evidence

> **Status:** Phase 0 and Phase 1 complete; Phases 2b–6 not started.
> Tracking: [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186), a sub-issue of
> [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Specification: [`docs/roadmap/0.1/0.1.7/retained-history-storage.md`](../../retained-history-storage.md).
> Case documents:
> [`core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/).

```text
claim_kind = history-storage-efficiency
```

**No row of this lane has been measured.** Nothing in this directory is a measurement of the
replacement product. Phase 1 proves that the **corpus reader** authenticates the pinned input
and enumerates the three selections correctly; the product has not been asked to save anything.

## Phase 0 — specification and rulings

Exit gate: *the README's seven owner decisions are ruled, the three row IDs and lanes are frozen,
and the pins are written down.* **Met.**

| deliverable | where |
| --- | --- |
| the roadmap specification | [`retained-history-storage.md`](../../retained-history-storage.md) |
| the seven rulings | its §9, and the campaign README §7 |
| the proposal table, kept verbatim | campaign README §7.2 |
| six errata, each verified against the corpus first | campaign README §9 |
| the declared verification ceilings | specification §11.1 |

The seven rulings, in one line each: the **verification ceilings are 10 / 20 / 30 s** for
stride10 / stride3 / stride1, declared before collection, with the per-tier complete-command
ceilings still declared at Phase 3 from the stride-10 baseline; `operation_ns` is the sum of the
N named per-state children; a sampled row may be `PASS` **in this lane only**; the sampled unit
is the state's file manifest with endpoints included and the rule lane-scoped; the §4 canonical
totals are gates; the v0.1.6 timing comparison is a labelled one-sided tripwire; and the storage
comparison against v0.1.6 is a gate **at all three sizes**.

**The errata matter more than the rulings did.** Six were raised, and E1 is the one that could
have changed a verdict: `path_states` was undefined in the file plan, and both obvious readings
are wrong. Summing `checkpoints[].files` gives **86,064 / 259,771 / 765,054** against pins of
**101,477 / 306,861 / 904,143** — short by 17.9 / 18.1 / 18.2 % at every size. The pins
reproduce **exactly** as the entry count of `oracles/<sha>.json`, which counts files **and**
directories. Had it been implemented as the manifest line count, all three Phase 1 pin checks
would have failed against numbers that are correct, and the natural next move — adjusting the
pins — is exactly what the stop rules forbid.

## Phase 1 — corpus reader

Exit gate: *the corpus authenticates against every identity in §3; the three selections
enumerate exactly 17/53/157 at the right indices; the path-state and logical-byte pins match; an
unidentifiable blob is refused.* **Met.**

### The three corpus facts the reader is built on, each verified on all 157 checkpoints

Verified before the reader was written, because each one, read the other way, produces a reader
that looks right and measures the wrong thing.

1. **`previous.tsv` of checkpoint *k* is the manifest of checkpoint *k−1*** — 157/157, 0
   mismatches. It is *not* the previous **selected** state's tree. For `history-stride1` the two
   coincide; for stride-3 and stride-10 they do not. A reader that diffed `manifest.tsv` against
   `previous.tsv` would treat every skipped state as unchanged.
2. **`blobs/` of checkpoint *k* is exactly the set of oids referenced by paths *k* added or
   whose oid changed** — 157/157, 0 mismatches. It is a **superset** of
   `oids(manifest) − oids(previous)` by 1–11 entries per state, because a copy or a rename
   introduces a path whose oid already existed. Keyed on the set difference, the reader drops
   those blobs and refuses a tree it could have served.
3. **The oracle's entry count is files *plus* directories**, and its directory set is exactly the
   proper ancestors of its file paths — 157/157, 0 mismatches. This is erratum E1.

### The probe

`fs-bench-storage-content --history-corpus <row> --corpus <path> [--history-walk]`, run against
`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`. Raw output is in `probe/`.

| row | states | path-states | pin | logical bytes | pin | manifest files | identity wall | walk wall |
| --- | --: | --: | --- | --: | --- | --: | --: | --: |
| `history-stride10` | 17 | 101,477 | ✓ | 561,010,345 | ✓ | 86,064 | 0.24 s | 17.69 s |
| `history-stride3` | 53 | 306,861 | ✓ | 1,676,767,835 | ✓ | 259,771 | 0.54 s | 20.84 s |
| `history-stride1` | 157 | 904,143 | ✓ | 4,936,693,030 | ✓ | 765,054 | 1.60 s | 38.11 s |

`manifest_files` is published beside `path_states` precisely so erratum E1 is visible in the
evidence rather than only in the prose: the two differ by ~18 % at every size and are different
quantities, not a rounding of one another.

### The walk — every changed blob, re-identified both ways

`--history-walk` walks every transition and reads every changed blob, re-hashing each against
**both** its Git object id (`sha1("blob <len>\0" || bytes)`) and its recorded SHA-256. Any
disagreement is `BlobIdentity` and the probe exits non-zero. All three rows walked clean:

| row | blobs read | blob bytes | changed paths | changed bytes |
| --- | --: | --: | --: | --: |
| `history-stride10` | 45,182 | 375,535,047 | 52,417 | 377,435,167 |
| `history-stride3` | 78,246 | 709,826,687 | 91,445 | 713,667,213 |
| `history-stride1` | 179,814 | **1,711,057,104** | 217,646 | 1,719,804,402 |

**The stride-1 figure is the independent check.** An enumeration of `inputs/<sha>/blobs/` across
all 157 checkpoints, done separately in Python before the Rust reader existed, gives
**1,711,057,104 B** for the sum of every blob file in the corpus. The Rust reader read exactly
that many bytes — no more and no fewer — and every one of them passed both identity checks. The
reader is not sampling the corpus or skipping a state; it is reading all of it.

### An unidentifiable blob is refused

Proved twice, and both are needed.

- **Hermetically**, in `src/workload/history.rs`'s tests: a blob whose bytes do not match the oid
  it is filed under is `BlobIdentity`; a blob whose bytes match its oid but not its recorded
  digest is `BlobIdentity`; a malformed tree line names its line number; a selection that is not
  strictly ascending from 1 to 157 at the declared length is `SelectionShape`; an absent corpus
  is `CorpusMissing` and is never defaulted.
- **Against the real corpus**, by the walk above: 179,814 blobs, zero refusals.

### The reader refused before it agreed — and the smoke tier caught it

The first `--history-walk` of `history-stride10` refused at state 1 with `BlobIdentity`: the
reader looked up the receipt's `blob_digests` under `blobs/<oid>` when the field is keyed by the
**bare oid** (`files` is the map keyed by path). The corpus was right and the reader was wrong.

The second attempt hung. The cause was a real defect, and the smoke tier is what surfaced it at
17 states instead of at 157: the reader re-read and re-parsed the **whole receipt JSON once per
blob**, so `history-stride1` would have parsed ~180,000 receipts of a few hundred kilobytes each
— tens of gigabytes of JSON for one selection. Receipts are now parsed **once per checkpoint
span**; the stride-10 walk went from unbounded to 17.69 s.

This is the tier policy working as designed. It is recorded rather than tidied away, because the
next candidate that touches this path should expect the same thing: **iterate on stride10, and
let it reject the defect cheaply.**

### Corpus read-only (preparation.md §8 test 9)

`corpus-readonly-after.json` records the manifest SHA-256, the tip, the checkpoint count, the
root and six sampled mtimes, and the count of writable files under the corpus. The manifest hash
is the pin, the sampled mtimes are all `2026-09-07` — before this campaign — and the reader opens
every path read-only. Nothing under the corpus was written.

## What this evidence does not show

- **No product run.** No Store was created, no state was saved, no oracle was re-derived. The
  performance, verification and cleanup phases have not run for any row.
- **No canonical totals.** The §7 canonical pins (589,423,458 B / 73,476 and 871,588,115 B /
  104,705) are quoted from the v0.1.6 record and have not been reproduced; they are Phase 4's and
  Phase 5's gates.
- **No storage reading.** Allocated, apparent, pack bodies and the `object_role` split are Phase
  2b and Phase 3 work.
- **No budget measurement.** The verification ceilings of §11.1 are declared; the per-lane
  complete-command ceilings are declared at Phase 3.
- **The corpus probe is skipped, not passed, when the corpus is absent.** `runner.py self-check`
  reports `history-corpus SKIP` with the reason rather than failing, because the corpus is a
  campaign input rather than a harness dependency. On this host it is present and reports `PASS`.

## Commands run

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml
cargo +1.85.1 clippy --locked --manifest-path $H/Cargo.toml --all-targets
python3 -m unittest discover -s $H/shared -p 'test_*.py'
python3 $H/shared/history_corpus.py
python3 $H/runner.py self-check
$H/target/release/fs-bench-storage-content --history-corpus <row> --corpus <path> [--history-walk]
python3 tools/production_loc.py
python3 core/tools/check_product_boundary.py
```

`cargo fmt --check` is **not clean on this tree**, before this change or after it: it reports
diffs in 22 files this campaign never touched (`src/families/*`, `src/ops/*`, `src/support/*`,
`src/registry.rs`, `tests/*`) as well as in the three new ones. The harness is not
`cargo fmt`-enforced, the new files follow the surrounding style, and no file outside this change
was reformatted. `cargo clippy` reports **no warning in any file this campaign added or changed**;
its 18 remaining warnings are pre-existing in `src/ops/*` and `src/support/phases.rs`.

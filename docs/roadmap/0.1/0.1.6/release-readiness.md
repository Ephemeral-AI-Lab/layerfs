# v0.1.6 release readiness

> **Status:** preparation record, 2026-09-16, **closed**. The owner authorized the
> closure, the release preparation, the tag and the publication in the same
> session; the execution record is below and the released documents are
> [release-notes/0.1.6](../../../../release-notes/0.1.6/README.md). The analysis in
> this page is kept as written — the audit that found the gaps is part of the
> record — and only this header and the final section were updated after the tag.

## Where the line stands

The **benchmark line is complete**: [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122)
and [#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154) are closed with
all 33 regular cases and 3 extensions terminal at seed 1 on one identity chain
(performance 28 PASS + 8 declared `N/A`; verification 36 PASS), evidence committed
under [`evidence/issue154/`](evidence/issue154/) and recorded in ledger
[L11/L12](evidence/issue154-rollout-ledger.md).

The **product line is not closed**: [#151](https://github.com/Ephemeral-AI-Lab/layerfs/issues/151)
("execute #149 + #150 and qualify create-500, bulk-create-500, and 25k") is still
open. Its three gates are recorded as owner-accepted with limitations attached in
the [roadmap README](README.md) (L18–L20), but that text also states plainly that
the adoption recommendation "is not a release decision: no merge, issue closure or
tag follows from it", and its own limitations list is unfinished work — sandbox
*process* memory is not emitted by the frozen harness, dirty shared-mmap visibility
is unsolved, and the breadth families were not run.

## `docs/general/release-policy.md` requirements

| requirement | state | evidence / gap |
| --- | --- | --- |
| workspace builds, all tests pass | **met** | `tools/preflight.sh` all steps passed on `3e6f2dac5`; workspace fast suite green |
| formatting, `-D warnings` clippy, `git diff --check` | **met** | preflight steps 1/4; `git diff --check` clean |
| public SDK and CLI reference match their exported surface | **partial** | the workspace suite passes; the *versioned manual* that states the surface for this release does not exist yet (`docs/versioned/0.1.6/` missing) |
| SQLite schema and static SQL manifest structural tests | **met** | part of the workspace fast suite; every benchmark receipt observed `schema_version 10` |
| supported FUSE and container-runtime gates on a capable host | **met for the benchmark scope** | 36/36 verification rows PASS on macOS + Docker Desktop (2 CPU / 2 GiB container); no Linux-host gate is claimed |
| versioned manual and limitations complete | **missing** | `docs/versioned/` stops at 0.1.5; the 0.1.6 manual, its CLI/SDK/container/storage pages and `limitations.md` must be written from the released source |
| every published benchmark links to reproducible raw evidence and identifies the exact source | **prepared** | `release-notes/0.1.6/benchmark-{performance,verification}.csv`, derived by `release-notes/0.1.6/derive_tables.py` from the committed matrices; every row carries its receipt path, source seal `86f14b2d…` and product seal `970964e9…` |
| release tag resolves to the reviewed source tree | **missing** | no `v0.1.6` tag exists; the workspace version is still `0.1.5` |
| compatibility boundary stated explicitly for this release | **owner decision** | the benchmark side observed schema 10 and an unchanged product seal `970964e9…`; whether v0.1.6 changes the public API, daemon protocol or Store/canonical contract must be stated by the owner before the manual and contract are written |

## Prepared now (no new claims)

* `release-notes/0.1.6/derive_tables.py` and the two CSVs it writes: 36 rows each,
  every receipt path verified present on disk, one source seal and one product seal
  across all rows, the declared target (15 s) and declared exception (25 s, or 30 s
  for the one owner-declared verification row) carried next to the measured wall.
* The audit above, the [roadmap README](README.md) status and the ledger, so the
  release notes can cite committed evidence rather than paraphrase it.

## Closure executed (2026-09-16)

The six steps this page listed, with what actually happened:

1. **#151 disposition.** The three B1/B2/B3 gates are recorded as accepted after
   ledger L20 (B1's commit phase passes with one informational container-CPU line
   over its sub-gate; B2's Commit/CPU-sum deltas are inside the owner's tolerance
   and its sandbox-memory line is recorded as not measurable on this harness;
   B3 passes both absolute 25k gates and both verifications). #151 is closed with
   that disposition and its non-passing lines linked, not summarised away.
2. **Compatibility boundary.** Stated once, in
   [`release-contract.md`](../../../../release-notes/0.1.6/release-contract.md):
   **Store format unchanged** (`SCHEMA_VERSION` 10, no schema or static SQL change
   since v0.1.5, `layerfs-content` and `layerfs-layerstack-store` byte-identical),
   public CLI unchanged, SDK additions behind `test-instrumentation`, daemon
   protocol additive (the sandbox's snapshot backing root), and mixed-version live
   sessions refused.
3. **Versioned manual.** `docs/versioned/0.1.6/{README,quickstart,specification,cli,sdk,container-runtime,storage-format,limitations}.md`
   written from the released source, with the sandbox-local facts and the
   one-worker cost stated where the manual makes a promise.
4. **Release notes.** `release-notes/0.1.6/` carries the contract, record,
   acceptance, waivers, verification, generated closeout, derived tables,
   `release-evidence.json`, the artifact preparation helper and its fixture check,
   and the announcement text.
5. **Version bump and gates.** `Cargo.toml` `0.1.5` → `0.1.6` plus the 12
   project-owned packages in `Cargo.lock`; `tools/preflight.sh` **PASS**
   (`preflight: all steps passed`, 72 test binaries, 512 tests, 0 failures,
   258 s; the 120 s warm-suite soft ceiling was exceeded by 138 s and reported as
   a warning), `cargo +1.96.0 fmt --all --check` **PASS**, clippy `--workspace
   --locked -D warnings` **PASS**, harness tests 97/97 **PASS**, and
   `check_plan.py` **PASS** (33 regular + 3 extended). The seal prediction on this
   page was verified exactly: the **product seal stays `970964e9…`** and the
   source seal reconstructs byte-for-byte from the measured revision's Cargo files
   plus the three pre-declaration harness files, so the benchmark claim is the
   recorded product identity, not the tag's own source seal.
6. **Tag and publish.** Annotated tag `v0.1.6` →
   commit `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; six assets built from the
   tag by `prepare_artifacts.py` (validation PASS, 4921 source members, 180
   evidence members) and published at
   <https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6>; the
   published `SHA256SUMS` re-downloaded and compared byte-identical; the
   checksums recorded in
   [`artifacts.md`](../../../../release-notes/0.1.6/artifacts.md) in a post-tag
   documentation commit, as v0.1.5 did.

What this closure did **not** do: re-measure anything, relabel a `FAIL`,
`TARGET_MISS` or declared exception as a pass, add a worker back, enlarge a
timeout at run time, or claim endurance, crash durability or sandbox process
memory.

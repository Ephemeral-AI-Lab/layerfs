# Disposed v0.1.7 evidence payloads

> **Status:** Archived; retained for historical evidence only.

[#398](https://github.com/Ephemeral-AI-Lab/layerfs/issues/398) removed the bulky
binary payloads from this evidence tree to shrink the checkout. This ledger records
that disposal. The receipts themselves are unchanged.

## What was removed

| Kind | Files | Bytes |
| --- | ---: | ---: |
| SQLite sample databases (`*.sqlite`) | 496 | 2,363,252,736 |
| Archived macOS (Mach-O) executables under `binaries/`, `binary-archive/` and `LANE/frozen-bin` | 20 | 73,147,360 |
| **Total** | **516** | **2,436,400,096** |

[DISPOSED-PAYLOADS.tsv](DISPOSED-PAYLOADS.tsv) lists every path with its size,
Git blob id and kind. The last commit containing all of them is `5fb68ddc1acc3163e3ac3486133bde014c5fbed3`.

The payloads come from closed diagnostic, review and timing rounds of the v0.1.7
development campaign, 2026-09-16 to 2026-09-20. No source, test or tool reads them.
The oracle fixtures read by `core/crates/layerfs-content/tests/edit_reference.rs`
are JSON files and stay in place.

## What stays

Every README, receipt, contract, manifest, log, CSV, JSON and JSONL record, script
and verdict in these runs is unchanged, and every original verdict keeps its
meaning. A receipt's link to a removed file now points at an absent path; look the
path up in the TSV.

## Availability

A checkout of this tree no longer holds these databases and executables, so they
are unavailable for new proof reuse, as
[the cleanup policy](../../../../general/cleanup.md#6-preserve-evidence-meaning-and-accessible-transcripts)
requires for disposed payloads. Their bytes remain in Git history. From a full
clone at the repository root, restore one file or all of them:

```sh
git restore --source=5fb68ddc1acc3163e3ac3486133bde014c5fbed3 -- '<path>'
tail -n +2 docs/roadmap/0.1/0.1.7/evidence/DISPOSED-PAYLOADS.tsv | cut -f1 |
  git restore --source=5fb68ddc1acc3163e3ac3486133bde014c5fbed3 --pathspec-from-file=-
```

## Per run

| Run | Databases | Bytes | Executables | Bytes |
| --- | ---: | ---: | ---: | ---: |
| [`phase0-baseline-20260917T221759Z`](phase0-baseline-20260917T221759Z) | 12 | 3,411,968 | — | — |
| [`phase1-execution-20260918T090000Z`](phase1-execution-20260918T090000Z) | 254 | 72,585,216 | — | — |
| [`phase2-execution-20260918T150435Z`](phase2-execution-20260918T150435Z) | 156 | 44,355,584 | — | — |
| [`stage-5-pipeline-timing-20260917T073302Z`](stage-5-pipeline-timing-20260917T073302Z) | 2 | 90,112 | — | — |
| [`stage-6-history-188d-20260920T000000Z`](stage-6-history-188d-20260920T000000Z) | 5 | 232,427,520 | 1 | 3,546,000 |
| [`stage-6-history-190-opt-20260919T232858Z`](stage-6-history-190-opt-20260919T232858Z) | 4 | 221,642,752 | 2 | 7,150,656 |
| [`stage-6-history-205-save-split-20260920T081022Z`](stage-6-history-205-save-split-20260920T081022Z) | — | — | 2 | 7,211,808 |
| [`stage-6-history-209-commit-20260920T194819Z`](stage-6-history-209-commit-20260920T194819Z) | 12 | 617,324,544 | 3 | 10,993,120 |
| [`stage-6-history-209-confirm-20260920T212625Z`](stage-6-history-209-confirm-20260920T212625Z) | 5 | 259,338,240 | 1 | 3,683,856 |
| [`stage-6-history-209-format-20260920T222118Z`](stage-6-history-209-format-20260920T222118Z) | 2 | 106,868,736 | 2 | 7,367,536 |
| [`stage-6-history-209-ingest-20260920T232000Z`](stage-6-history-209-ingest-20260920T232000Z) | 1 | 270,336 | — | — |
| [`stage-6-history-209-instr-20260920T213740Z`](stage-6-history-209-instr-20260920T213740Z) | 3 | 131,473,408 | 1 | 3,721,728 |
| [`stage-6-history-209-merged-20260920T225623Z`](stage-6-history-209-merged-20260920T225623Z) | 1 | 85,733,376 | 1 | 3,683,856 |
| [`stage-6-history-209-rca-20260920T191016Z`](stage-6-history-209-rca-20260920T191016Z) | 6 | 311,205,888 | 6 | 22,104,688 |
| [`stage-6-history-209-stmtcache-20260920T210202Z`](stage-6-history-209-stmtcache-20260920T210202Z) | 5 | 259,338,240 | 1 | 3,684,112 |
| [`stages-1-2-review-20260916T185553Z`](stages-1-2-review-20260916T185553Z) | 6 | 7,041,024 | — | — |
| [`stages-1-5-review-20260917T160000Z`](stages-1-5-review-20260917T160000Z) | 1 | 32,768 | — | — |
| [`stages-3-4-review-20260916T233008Z`](stages-3-4-review-20260916T233008Z) | 5 | 5,324,800 | — | — |
| [`stages-3-4-timing-20260917T031000Z`](stages-3-4-timing-20260917T031000Z) | 16 | 4,788,224 | — | — |

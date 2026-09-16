# #154 evidence index

Which file is authoritative, and which are superseded phase reports that stay on
disk for the record. Nothing here is rewritten after the fact: a superseded file
keeps the numbers it was taken on, and the identity next to it says which source,
product and image those numbers describe.

## Current — the seed-1 result set of #154

| file | contents | identity |
| --- | --- | --- |
| [`final-complete-matrix.json`](final-complete-matrix.json) | **authoritative** one-run-per-case matrix, 33 regular cases, one performance and one separate verification invocation each | image `layerfs-bench-infra:86f14b2d68ece2ae`, source seal `86f14b2d…`, product seal `970964e9…`, tag `final3-seed1` |
| [`final-complete-extended-matrix.json`](final-complete-extended-matrix.json) | **authoritative** matrix for the three declared extensions | same chain, tag `final3-ext` |
| [`final-report.md`](final-report.md) | the family report posted on [#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154) and [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122) | same chain |
| [`../../issue154-rollout-ledger.md`](../../issue154-rollout-ledger.md) | append-only ledger L1–L12: per-family reports, the defects fixed at the root, the declared exceptions and the arithmetic | per entry |

Verdicts in the authoritative set: performance 28 PASS + 8 declared `N/A`,
verification **36 PASS**, and **no** `FAIL`, `TIMEOUT` or `NOT_RUN`. One declared
exception: `v016-branch-mixed-500mb-30000-k100-v1` verification carries a 30 s
ceiling (owner ruling, L12) instead of 25 s.

The matching release tables are derived from these two files by
[`derive_tables.py`](../../../../../../release-notes/0.1.6/derive_tables.py) into
`release-notes/0.1.6/benchmark-{performance,verification}.csv`.

## Superseded phase reports (retained, not rewritten)

| file | what it recorded | identity it was taken on | superseded by |
| --- | --- | --- | --- |
| [`f1-file-size-transition.json`](f1-file-size-transition.json) | the F1 port probe and family rows, 7/7 PASS | `layerfs-bench-infra:8ef48ec2b762f720` | `final-complete-matrix.json` |
| [`f2-mixed-load-bearing.json`](f2-mixed-load-bearing.json) | F2 rows with three verification `TIMEOUT`s before the verifier redundancy work | same | `final-complete-matrix.json` |
| [`f3-multi-workspace-development.json`](f3-multi-workspace-development.json) | F3 rows with two verification `TIMEOUT`s | same | `final-complete-matrix.json` |
| [`f4-branch-development.json`](f4-branch-development.json) | F4's four M1 branch rows; the two compact controls were `NOT_READY` | same | `final-complete-matrix.json` |
| [`f5-dedup-branch-history.json`](f5-dedup-branch-history.json) | F5's four measured rows plus the two `namespace-inode` `FAIL`s from the missing HN orchestrator | same | `final-complete-matrix.json` |
| [`final-seed1-matrix.json`](final-seed1-matrix.json) | the L10 one-run-per-case matrix: 23 PASS, the two `namespace-inode` `FAIL`s, and the eight unregistered cases | `layerfs-bench-infra:221f445fabac7e39` | `final-complete-matrix.json` |
| [`phase-q-seeds.json`](phase-q-seeds.json), [`q2-regular.json`](q2-regular.json), [`q3-regular.json`](q3-regular.json) | the earlier seeds 2 and 3 sweeps, retired as the qualification criterion by the one-run-per-case ruling | `layerfs-bench-infra:8ef48ec2b762f720` and successors | the seed-1 matrices above |

Raw receipts for every row stay in the gitignored local tree
`benchmark-results/v016/<tag>/…`, named by each row's `receipt` field; the matrices
are what is committed.

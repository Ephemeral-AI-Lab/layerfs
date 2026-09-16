# Stages 3-4 independent review evidence

Round: stages-3-4-review-20260916T233008Z (UTC 2026-09-16T23:30Z-23:46Z).
Reviewed snapshot: 91c3a0741fff64e8161d5c1b6e759f347ffbf757 (clean tree).
Report: ../../component-decoupling/stages-3-4-review-20260916T233008Z.md

## Raw receipts (produced by this review)

| Path | What it is |
| --- | --- |
| checks.log | Per-command stdout of the focused suites, workspace suite, clippy, fmt, guards and the LOC counter, with individual exit codes. |
| run-checks.sh | The exact commands behind checks.log. |
| examples/ | Six real example runs on the pinned snapshot (measure_edits c1/c2/pipeline at 128 KiB and 1 MiB, measure_pooled 24x100), their timing trees and examples.log. |
| run-examples.sh | The exact commands behind examples/. |
| boundaries/ | API boundary probes: existing --output rejected, cutoffs 65536/100000/2097152 rejected, 262144/524288 accepted, --timing off. |
| run-boundaries.sh | The exact commands behind boundaries/. |
| oracle-replay/ | This review re-executed the sealed v0.1.6 reference oracle for all nine cases; oracle-replay.log records identity and exits, and each <case>.json is byte-identical to the retained fixtures under ../stages-3-4-oracle-*. |
| run-oracle-replay.sh | The exact command behind oracle-replay/. |
| issue-165.json, issue-168.json, issue-169.json | Issue bodies and comments as fetched (gh issue view) on the review date. |

## Analysis

| Path | What it is |
| --- | --- |
| loc-structure.md | Full per-file / per-directory before-after-delta table, phase-1 tree, per-commit first-parent accounting, file-plan comparison, counter audit. |
| loc-per-file.csv | Every file under core/crates with classification, production LOC and physical lines. |
| loc-before-after.csv | Per-file before/after production and physical lines vs the real pre-Stage-3 base. |
| loc-file-plan-comparison.csv | Every stages-3-4-file-plan.md row against the actual size and range. |
| loc-tree.md | Annotated ASCII tree. |
| criteria-stage3.md | Every #168 acceptance item, E1-E3 gate and handoff obligation with source, oracle, evidence, status and gap. |
| criteria-stage4.md | The same for #169 and D1-D4. |
| limits.md | The 12-column supported-envelope table (60 rows) plus binding-constraint derivations and the boundaries not run. |
| simplification.md | Ranked simplification findings with locations, smallest change, callers, invariants and verification cases, plus the not-removable list. |
| evidence-audit.md | Audit of every retained evidence generation: identities, sample counts, budgets, timing trees, matching, clipping defects. |
| memory-safety.md | Allocation-owner ledger, boundedness findings, unsafe/FFI inspection and the memory-safety conclusion with coverage limits. |

No product source, commit, issue state or other agent's evidence was modified by this review.

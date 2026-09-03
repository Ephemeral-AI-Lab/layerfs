# LayerFS 0.1.2 SDK-only benchmark evidence

> **Status:** Terminal performance and correctness evidence passes the explicitly approved policy. Publication is not authorized; v0.1.2 remains unreleased.

The [complete report](../../benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/report.md)
contains both arms' per-operation/per-size timing and memory tables, five
repetitions, medians, min–max ranges, units, nominal/tolerance classifications,
and raw links. [Inputs](../../benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/inputs.json)
pin every manifest and raw stream; [classification](../../benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/classification.json)
retains original findings and final-policy decisions. Final repository gate
and admission eligibility are recorded by [the selector](sdk-edit-evidence.json).

| Family | IDs | Sizes (MiB) | Performance rows | Verification subproofs |
| --- | ---: | --- | ---: | ---: |
| Length preserving | 12 | 1/10/100/500 | 120 | 24 |
| Length changing | 32 | 1/10/100/500 | 320 | 64 |
| Canonical chunk count | 12 | 1/10/100/500 | 120 | 24 |
| **Total** | **56** | | **560** | **112** |

Each sample performs one singular public SDK range edit, one Commit, and one
End with a real FUSE projection. Mutation uses no container Exec or POSIX/FUSE
write path. Mutation-caused FUSE payload and Workspace spool are zero; route,
CDC/candidate-work, correctness, cleanup, and resource checks pass. All three
performance families finished before the separate verifier stage.

## Acceptance and limitations

Nominal median targets are Edit/Commit/combined **10/10/20 ms**; approved
absolute ceilings are **20/20/30 ms**. The combined ceiling is independent,
not 40 ms. Tables distinguish nominal-pass from accepted-with-tolerance.
Edit size and matched-operation parity remain binding except for these
explicitly reviewed results; their original 2 ms-rule failures are retained:

| Reviewed Edit discrepancy | Observed spread (ms) |
| --- | ---: |
| Delete-middle across sizes | 2.571958 |
| Replace-shrink across sizes | 2.111083 |
| Delete versus truncate at 1 MiB | 2.484458 |

Commit/combined size and matched-operation spreads are diagnostic. The claim
is size-stable localized edits with these disclosed exceptions and bounded
Commit latency, **not size-independent Commit**.

Memory uses ack-window-v1: sampler readiness precedes Edit and observation
finishes after Commit (after End in performance mode). Native whole-worker RSS
and whole-container peaks are conservative lifetime bounds. Category/window
maxima are sampled, not exact-phase or continuous proofs. The 128 MiB peak,
32 MiB incremental-upper-bound, 16 MiB native cross-size spread, 8 MiB sampled
dirty/writeback, and zero observed swap/OOM gates remain binding. Sampling
cannot rule out every transient swap or prove continuous category ceilings.

## Source and reporting custody

- Baseline: dc7aeff9a7e4f9e849a48022142f86801273f0bd.
- Measured candidate: 3337728e9846a200d7a5cc08d076de18f1d5436c.
- Common harness SHA-256: 5d2d2995aca098e1e3c8878b2e45d5cd460cdc8b6dfff8681e6cc0df93561ec4.
- Frozen reporter SHA-256: da606681f5c4222e724eb6273c2417f7ec3960cd15d12a25082228550f25eb19.

The separately identified final consumer corrects only the unavailable-attribution
field alias in memory and applies recorded user approvals. Original raw rows,
subproofs, and frozen FAIL classifications are retained, not relabeled as
strict passes. Its self-check rejects conflicting aliases and confirms real
resource failures remain failures. Final source validation allows the approved
specification amendment and documentation/evidence changes only; compiled
product and harness remain byte-identical to the measured candidate.

One baseline 10 MiB zero-extension verifier returned InvalidRequest despite
normal container exit without OOM. Its cause is unproven. The failed attempt
is retained under the length-changing bundle's verification/attempts/. Only
six missing proofs were retried; all passed, and the original 58-proof prefix
and all performance bytes stayed unchanged. Temporary read-only metadata
copies required scoped cleanup recovery; input caches/build receipts remain intact.

Historical POSIX/FUSE results and earlier incomplete campaigns are not pooled,
comparators, or admission evidence. Empirical claims stop at **500 MiB**; no
100 GiB or other-environment extrapolation is made. Issue #12 remains the
separate release-finalization decision; these results create no tag or Release.

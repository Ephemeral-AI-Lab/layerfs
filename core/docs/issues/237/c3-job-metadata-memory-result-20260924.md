# #237: compact Job metadata memory treatment

> **Status: Research; informative and not a product contract.** One
> locked-release candidate diagnostic against the retained C3 phase
> control. This is a measured memory treatment, not a speed or release
> admission claim.

## Change and proof

[`scan_and_save`](../../../crates/layerfs-service/src/save/import/scan.rs)
now copies only the five source metadata values that a `Job` later
checks: device, inode, length, mtime seconds and mtime nanoseconds.
It no longer clones the complete `std::fs::Metadata` into every file
job. File path, entry index, four workers, source-open identity check,
post-read stability check, Save ordering, public SDK route, C1/C2/C5
formats and SQLite profile are unchanged. The
[prospective plan](c3-job-metadata-memory-treatment-plan-20260924.md)
froze that scope and its decision criteria before source was edited.

The candidate used the same temporary phase markers, runner/harness
seal `7614161b4c70cad74ca93c684c105c86f778870e2481c56ad761348ea19b9ae2`,
locked release profile and seed-1 SHAKE 100k/500-MB manifest as the
retained [C3 control](c3-memory-phase-cause-result-20260924.md).
Control product seal was
`ca72aa0f1e627661f4ad3009918e30c46998d0321f8929929c73125bbd6ad158`;
candidate instrumented product seal was
`c2e50eecfed39038a1d6f943acd6741ebe5fa85c57e57846d12af21428c83c3a`.
The [freeze record](evidence/c3-job-metadata-20260924/freeze.json),
[build](evidence/c3-job-metadata-20260924/build.json),
[receipt](evidence/c3-job-metadata-20260924/receipt.json),
[trace](evidence/c3-job-metadata-20260924/driver.stderr) and
[comparison arithmetic](evidence/c3-job-metadata-20260924/comparison.json)
pin the remaining identities and figures.

Both arms launched with **0/126,206 resident source payload pages**;
inode/directory metadata cache state remains unqualified. Each made
one public `Client::init_project` call and separately reopened its
content Store and History. The candidate
[full oracle](evidence/c3-job-metadata-20260924/verification.json)
passed all 101,001 paths, portable metadata, every size and SHA-256,
and 500,000,000 bytes. The runner's retained hash-manifest check was
`PASS`. The candidate inserted 112,424 objects, as did the control.
The raw output, including closed databases and independent source
copy, remains at
`benchmark-results/fs-bench-pro/issue237-job-metadata-20260924-01/`
in this worktree. Curated files are indexed by
[SHA256SUMS.json](evidence/c3-job-metadata-20260924/SHA256SUMS.json).

## One-shot memory result

| Measure | Retained C3 phase control | Compact-Job candidate | Candidate minus control |
| --- | ---: | ---: | ---: |
| Jobs / entries | 100,000 / 101,001 | 100,000 / 101,001 | 0 / 0 |
| `Job` vector reserved | **23,068,672 B** | **9,437,184 B** | **−13,631,488 B** |
| Entry vector reserved | 14,680,064 B | 14,680,064 B | 0 B |
| Path payload lengths | 16,300,000 B | 16,000,000 B | −300,000 B from output-path spelling |
| Scan-end process peak RSS | 98,287,616 B | 76,480,512 B | **−21,807,104 B** |
| File-loop-end process peak RSS | 125,386,752 B | 105,857,024 B | −19,529,728 B |
| Namespace-Save-end / call-end process peak RSS | **156,467,200 B** | **144,703,488 B** | **−11,763,712 B** |
| Current RSS at call end | 147,685,376 B | 135,921,664 B | −11,763,712 B |

The vector's `Job` element fell from 176 to 72 B on this target;
both vectors retained capacity 131,072. Its deterministic reservation
reduction is **13 MiB**. The prospective ≥8-MiB local scan-RSS
criterion was met: the one-shot scan-end reduction was **20.80 MiB**.
The final process high-water reduction was **11.22 MiB**. The
different output directory names made candidate file paths three
bytes shorter, or 300,000 B over 100,000 files; that small input-path
difference and allocator behavior mean the **21.81-MB RSS reduction
cannot all be causally assigned to the metadata fields**. The
structural 13-MiB reservation reduction itself is exact.

The candidate's namespace arrays retained the same 18,838,440-B
reservation as control, and namespace construction still set the
operation's maximum. Candidate final peak remains **52,297,728 B**
above the retained v0.1.6 92,405,760-B t1 reading; this is context
across distinct diagnostic identities, not a paired old/new memory
admission rule. The earlier uninstrumented C3 research and matched
t1 memory receipts remain unchanged. Candidate public call and
complete-command times are diagnostic observations only; source
metadata residency is unqualified and no speed comparison is made.

## Decision

**Keep the compact Job representation.** It is a small source change
with the same source-identity checks and full readback, and it cuts
both explicit reservation and observed process peak in this one
diagnostic. It does **not** solve the full old-to-Core memory gap.
The next potential target is the retained full job/path list, then
the second peak produced by C1 namespace input and tree construction.
Those are larger changes with separate correctness and memory proof;
the present treatment does not authorize them, add workers, or change
the #229 sparse-history gate. Final Core source checks and production
LOC are reported in the commit carrying this result.

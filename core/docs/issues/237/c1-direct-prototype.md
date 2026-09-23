# #237 C1 direct fresh-build prototype

> **Status:** Research; informative and not a product contract.

## Preregistration (before any timed sample)

This isolated worktree is `/Users/yifanxu/.codex/worktrees/issue237-c1-direct-prototype/layerfs`, branch `codex/issue237-c1-direct-prototype`, starting at `a6d1d563f98b40755c43684a0449c74d39d894ff`. The existing Core native Init at that commit is the control. The candidate is an **unmerged research diff** confined to `core/crates/layerfs-content/src/filesystem/update.rs`: when `base=None`, count the directory merge's observed binding additions in one `u64` per sorted declared new serial and stream final typed values directly into the existing sorted inode writer. Updates with a base retain the reducer. The vector's cardinality must fit the existing `ordering_bytes / 16` touched-serial ceiling. Missing values, unbound new serials, count overflow, failed object writes and cleanup still fail the operation. The canonical root format and Store policy are unchanged.

Run at most one control and one candidate release-profile `namespace-10000` public `daemon-host` Init using the same `core-native-import-fixture-v2` seed-1 manifest (10,000 files, 100 data directories, 300,000,000 logical bytes including its 100 MB anchor), same four construction workers, request deadline, C2 packing/cache and 4,096-byte SQLite page. Use the existing research `cold_diagnostic.py` for each fresh Store: hash all source files, invalidate their data pages, require whole-input zero residency, and immediately recheck every file without faulting it before the caller timer. Reuse the sealed source workspace only for preparation. Record full preflight/recheck and launch gap. The Core runner's own `source-cache-uncontrolled-v1` admission label remains; this is a research comparison.

Freeze each arm's product/harness/fixture/binary seals, source diff, command, original caller timer, complete command, Service CPU/RSS, scratch, Store size/pack geometry and all failures. The runner's default performance-only mode records verification `SKIPPED`; its wall contributes nothing to throughput. A separate focused external semantic test must establish the same canonical root, hardlink counts and error behavior. A cold-source, successful-root pair qualifies as a **matched research timing comparison** only if its route, fixture, cache method and release build agree. The candidate is useful if it reduces the matched caller time by at least 5% while keeping Store root, physical size, `sum(length(data))`, pack allocated/used bytes and 4 KiB page size equal, and its Service CPU no higher than control by 5%. The 10k target corresponding to 700 MB/s is `300,000,000 / 700,000,000 = 0.428571429 s`; report the actual result even if it misses. A sampled RSS value is descriptive only; the code-level added resident count array is exactly `8 × new_inodes.len()` bytes (80,808 B at 10,101 inodes, 808,008 B at 101,001) and the total process bound still includes existing input vectors, directory pages, C2 and file workers. Do not call sparse #229 compactness or full reopened readback proven by this dense Init comparison.

No unchanged arm is retried for a better time. If compilation, source residency, capacity, timeout, root, cleanup or identity checks fail, retain the attempt and report it without a selected replacement number. A distinct source change needs a new prospective hypothesis. Timed samples wait for coordination with the parent worktree so host work does not overlap.

## Attempt ledger

The candidate diff is retained as [candidate.diff](evidence/c1-direct/candidate.diff.gz)
(SHA-256 `de2554f665ba7cdab318369000019afc3f99177d66eedb9f226b8791380a4206`).
The control restored `update.rs` exactly from `a6d1d563`; the candidate reapplied
that diff. Both used the same harness seal
`6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
fixture manifest SHA-256
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`,
4 KiB SQLite pages, and release builds in this worktree's private `core/target`.
The exact commands were:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --out benchmark-results/fs-bench-pro/issue237-c1-control-a6d1
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --out benchmark-results/fs-bench-pro/issue237-c1-candidate-direct-a6d1
python3 core/docs/issues/237/evidence/c1-direct/geometry.py benchmark-results/fs-bench-pro/issue237-c1-control-a6d1/daemon-host/init_namespace/namespace-10000/store.sqlite
python3 core/docs/issues/237/evidence/c1-direct/geometry.py benchmark-results/fs-bench-pro/issue237-c1-candidate-direct-a6d1/daemon-host/init_namespace/namespace-10000/store.sqlite
```

The runner reused one prepared fixture after the first run. Each arm rehashed
all files and invalidated their pages independently. Both preflights and
immediate rechecks found **0 resident pages / 27,503 pages** across all 10,000
files and 300,000,000 source bytes; recheck-to-caller gaps were 5.455 ms and
1.125 ms. These sidecars qualify source **payload page residency only**:
metadata/dentry cache was not independently evicted or qualified. The runner
still declares `source-cache-uncontrolled-v1`, so neither row is a fully cold
gate PASS. Full verifier was `SKIPPED` in both arms by the fast lane. The raw
[control](evidence/c1-direct/raw/control/receipt.json) and
[candidate](evidence/c1-direct/raw/candidate/receipt.json) receipts, binary
identities, telemetry, cold sidecars and failures remain under `evidence/c1-direct/raw/`;
the two closed Stores remain in the named private result directories.

| Measure | Control | Candidate | Candidate minus control |
| --- | ---: | ---: | ---: |
| Public caller Init | 1.563611417 s | 1.317538583 s | −0.246072834 s (−15.737%) |
| Caller throughput, 300 MB decimal | 191.864 MB/s | 227.697 MB/s | +35.834 MB/s |
| Complete performance command | 2.502396917 s | 2.251171625 s | −0.251225292 s |
| Service + daemon lifecycle CPU, user + system | 2.161254 s | 1.955754 s | −0.205500 s (−9.508%) |
| Service sampled maximum RSS | 59,015,168 B | 59,752,448 B | +737,280 B (not a phase peak) |
| Store apparent bytes | 334,176,256 | 334,401,536 | +225,280 |
| Store allocated bytes (`st_blocks × 512`) | 335,609,856 | 335,609,856 | 0 |
| Pack BLOB capacity | 331,350,016 | 331,612,160 | +262,144 |
| Pack assembled/used bytes | 305,977,872 | 305,982,016 | +4,144 |
| Pack count | 1,264 | 1,265 | +1 |
| Objects | 24,683 | 24,683 | 0 |

The control product seal is
`1f24a0fd8a1ab9207fec22ae837da6e790ae3e931938db41108b5f513ac5a5d9`
and its Service binary SHA-256 is
`732c8f65f63187579d7c4a6519e4b10d2ef9b34945675d1c1349eee8944dc541`.
The candidate product seal is
`086df9cf44aec9d990635ed333a3bbb0971235d77560caaeb9a55832eba92a59`
and its Service binary SHA-256 is
`cdbfe5ab95621d1c81b1f8cc2b5c39a0c3126af2d87b8fc04629c3e0b44e3397`.
The runner reused no measured result. Both returned a C5 root, telemetry and
cleanup `PASS`, with its own `DIAGNOSTIC` status and `admission_eligible=false`.
There was no concurrent LayerFS build or timed sample observed in either receipt;
the runner's process list did include background Docker Desktop processes.

**The preregistered root and whole-Store equality gate failed.** The runner
passes fresh `os.urandom(16)` stack and `os.urandom(32)` scope seed into *each*
public Init (`core/benchmark/fs-bench-pro/runner.py:301`). The control root is
`d2bd98910f0cc8835bd1641f9652d146163c789cf66ce59df9084567df2d0ad7`;
the candidate root is
`a2e8eb157372ede4401802dd385e2693519a72236d1e46e0403a632282d67f03`.
Their object-ID sets overlap in 24,682 of 24,683 IDs; the two nonoverlapping
IDs are exactly those roots. This supports, but does not prove by itself, C1
canonical parity at a fixed scope. The external sealed-reference construction
test passed and checks exact root, reachable objects and logical readback at
fixed inputs. The pack-capacity difference belongs entirely to Save 1's file
ingest: 1,259 versus 1,260 packs; the C1 tree Save 3 has **308 objects and
3 × 262,144 B pack capacity in both arms**, with 489,196 versus 489,195 used
bytes ([control geometry](evidence/c1-direct/control-geometry.json),
[candidate geometry](evidence/c1-direct/candidate-geometry.json)). This is a
specific pre-C1 packing variation, not evidence that the C1 candidate grew its
tree. It still fails the prospectively strict whole-Store comparison; a later
deterministic-identity pair and #229 sparse-history check are needed.

The 700 MB/s 10k goal would require ≤0.428571429 s; this candidate is
0.888967154 s above it. `history.import_files` alone was 1.152547208 s
and 1.162958542 s in the two raw receipts. The caller reduction therefore
cannot be attributed to file ingest; the C1 route change is the plausible
cause, but this telemetry has no dedicated C1 child and does not isolate its
wall time. The exact count array is 80,808 B at 10,101 inodes and needs
O(N) serial binary searches (one per observed edge) plus one sorted final
pass; it makes no ordering-run writes. Scratch-file peak and fully covered
process RSS were not recorded, so a whole-process memory-bound PASS is open.

### Focused checks and failed compatibility assertions

`filesystem_reference` passed 2/2 sealed root/page/readback tests;
`filesystem_hardlinks` passed 6/6; `filesystem_topology` passed 17/17;
`filesystem_bounds` passed 17/17. The added
`a_fresh_build_counts_bindings_without_ordering_runs` passed and pins 101
observed bindings, 102 final typed values, and zero spilled or created runs
with a one-record pending setting. The full `filesystem_ordering` target
finished **11 passed, 4 failed**:

- `the_pending_threshold_changes_only_where_the_rows_live`
- `a_high_pending_ceiling_runs_spill_free_to_the_byte_bound`
- `a_successful_operation_releases_its_ordering_resources_once`
- `append_read_flush_and_release_failures_fail_the_operation_without_a_root`

Those four tests require a fresh build to spill/reference an ordering backing.
The direct path intentionally avoids that backing, so their old work/fault
injection expectations no longer apply to `base=None`. The production follow-up
must move their spill/fault coverage to an update operation and add equivalent
direct-path failure/boundary assertions before acceptance. This research commit
retains their failure; it does not claim a green C1 suite or full readback.

## Implementation handoff

The direct counter path is viable as a **timing prototype**, not yet a complete
#237 solution. Apply its mechanism in C1 `filesystem/update.rs` only after
freezing deterministic public Init scope/stack inputs for a true canonical and
Store-space pair. Keep the existing `ordering_bytes / 16` cardinality refusal,
checked count additions, zero-count/root rules and reducer for updates. Review
the initial-build quota and failure-injection contract with the four affected
ordering tests; preserve one attempted operation and explicit cleanup. Run the
fixed-scope full oracle and the #229 sparse-history compactness/readback lane at
the frozen product identity. The four-worker, 4 KiB SQLite, cold-source and
performance-only exploration policies remain unchanged. File ingest now owns
most of the remaining 1.318 s and needs its own count-driven intervention to
approach 700 MB/s.

Production LOC for this research commit is **116,912 -> 116,994 (delta +82)**:
reference 65,417 -> 65,417; replacement core 51,495 -> 51,577. The exact
staged-snapshot method and changed-file counts are retained in
[production-loc.json](evidence/c1-direct/production-loc.json). This is an
unmerged prototype size change, not a production optimization admission.

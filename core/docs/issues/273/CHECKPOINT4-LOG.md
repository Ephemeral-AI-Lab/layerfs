# #273 checkpoint 4 evidence and review

Checkpoint 4 is complete at the active backing lifetime and public functional
boundary. Its final **product** source is `05fca30fb46a616a6988a60d3532f4255c9b3cf8`
(tree `30c099c68cc895f812d6f8c913623a9b0468bd23`). The external count
oracle was extended in test-only `6f8c5a684`; no product code changed there.
All three commits follow checkpoint-3 evidence commit `da2c96ee9` on draft
PR #274. This log and the byte-for-byte [checkpoint-4 receipts](evidence/checkpoint4/)
are the append-only handoff. The receipts' `SHA256SUMS` covers 66 functional
route results and the retained check logs. Neither this log nor any receipt
is a benchmark speed sample.

## Implementation and review

`38bcf9912` removes final G1 `E`/`R`/`L` references in the same active index
publication as saved-root reconciliation. Physical pack and large Payload
owners retain their birth/retirement revision until every selecting capture or
read pin ends. Cleanup unlinks through the existing charged PageStore or
PayloadHost custody; uncertain or failed cleanup retains ownership and charge.
Mixed sealed pages relocate surviving slots with their inverse references
and locator in one candidate publication. Ordinary mutations relocate at most
one touched source page; Commit drains touched pages. Sorted index updates
share reached-leaf work, and final SaveFile upload reuses one charged,
authenticated pack page at a time. Backing status exposes successful pack and
index page reads/writes; the public Stage diagnostics use those counters for
`Q_fetch`, without asserting OS cache residency.

Review found one concrete quota-pressure defect in the first implementation:
it could copy a single partly dead page to another page without freeing any
blocks, spending scarce headroom during an unrelated Commit. `05fca30fb`
restricts the low-headroom sweep to pairs whose surviving records fit one
destination. The public quota-refusal path still refuses before publication
when the required candidate pages cannot be reserved. The external
`active_backing` proof now shows one two-to-one pool at **122,880 B** quota
remaining: **68 slots moved, 68 inverse references changed, one new pack-page
write, and 4,096 actual pack blocks refunded**. A following one-page partial
death triggers no extra pack write or page allocation. All 128 selected bytes
are checked after relocation. This is a deterministic backing count/space
proof, not a mounted speed result.

## Public functional receipts at final product source

All 13 `checkpoint4-reviewed-*` native Service selections below pin source
`05fca30fb`, the final product-input hash and release Stage binary in their
own result JSON. Each uses an independent writable byte copy of the closed
fixture, a fresh live Service/history producer, one owned Linux ext4 volume,
`LAYERFS_CONSTRUCTION_WORKERS=1`, and complete-command cleanup. Each reports
`cache_claim=null` and `performance_claim=false`. Walls below include setup,
container lifecycle, the public call and cleanup; none receives a latency
PASS or is compared with a control arm.

| Case | Checks | Complete functional wall | Observation |
| --- | ---: | ---: | --- |
| `active_generation` | 2/2 PASS | 0.980 s | G1 Stage beside G2 live bytes; both Commits |
| `active_mounted` | 2/2 PASS | 0.927 s | mounted FUSE write/append, unmount, Commit and canonical bytes |
| `active_many_file` | 3/3 PASS | 7.641 s | 128 files; two shared pack pages before, zero after Commit |
| `active_repeated` | 3/3 PASS | 7.128 s | 4,097 same-offset writes; full byte oracle and close |
| `active_retained32` | 3/3 PASS | 1.560 s | 32 pinned generations; old/new reads and release |
| `active_mixed_compact` | 3/3 PASS | 4.015 s | G1/G2 mixed sealed pages pool with correct bytes |
| `active_mutation_compact` | 2/2 PASS | 3.999 s | ordinary WRITE removes a mixed sealed source page |
| `active_payload_refund` | 3/3 PASS | 0.920 s | large `p-*` owner removed after Commit |
| `active_separated4096` | 3/3 PASS | 16.306 s | 4,096 separated one-byte writes; full 8,194-byte oracle |
| `active_cleanup_failure` | 2/2 PASS | 0.936 s | postpublication error retains receipt, new byte and charge |
| `active_split_slot` | 2/2 PASS | 0.932 s | packed subrange after overlap, full oracle and close |
| `active_quick_controls` | 3/3 PASS | 15.448 s | count-only clean/one-edit Commit diagnostics |
| `active_quota_refusal` | 2/2 PASS | 0.921 s | full 2 MiB quota refuses candidate; old byte and charge remain |

The **4,096 separated-write physical charge was 1,224,704 B before Commit**
and 24,576 B afterward, with 52 then zero pack pages. The pre-Commit charge
is below the prospective 3 MiB checkpoint target by 1,921,024 B. The
public backing charge equalled the private files' actual `st_blocks * 512`
before and after; clean close left zero private blocks. The selected Commit
diagnostic counted 104 pack-page fetches (`Q_fetch`) and 2,046 index-page
fetches in 267,548,417 ns. Its 16.306 s full command wall is a functional
observation; it is **not** the unchanged 4,097-write #248 gate.

The 128-file case charged 258,048 B before and 126,976 B after Commit. The
repeated-overwrite case charged 16,384 B then 12,288 B. The 32-generation
case retained 274,432 B and 32 pack pages while pinned, then 12,288 B and
zero pack pages after release. The mixed-page public case observed 364,544 B
and three pack pages before G1 CommitStaged, then 172,032 B and two pack pages;
its isolated Commit phase counted one new pack-page write, four pack-page
fetches and 1,181 index-page fetches. Every clean-close case checked zero
private blocks. The intentional cleanup-corruption case instead returned
`Published` with `accepted_bytes=1`, kept the new `B` readable, kept its
physical charge, and refused clean close; it is a PASS for that failure
contract, not a successful cleanup.

The combined quick-control diagnostic held 4,097 unrelated old records. Its
clean Commit read **zero pack pages** and four index pages in 7,203,125 ns;
the one-edit Commit read two pack pages and 18 index pages in 19,895,375 ns.
The **combined** fixture, both controls and cleanup took 15.448 s, above a
future 15 s selection limit. Checkpoint 5 must run the two separately
registered public cases with its declared cache contract and complete walls;
these count-only observations cannot be promoted to gate rows.

## Checks, failures and scope

At final product source, locked release warning-denying Core workspace
Clippy, Core examples, Core fmt, the product boundary scan (342 source files),
its nine self-tests, targeted macOS `layerfs-workspace` tests, Linux ext4
`active_backing` **25/25**, the focused pressure count oracle and the 13
public cases above passed. The full locked Core test command still **FAILS**
in the same two unchanged `layerfs-content/tests/filesystem_ordering.rs`
cases: `a_fresh_build_charges_its_count_array_to_the_ordering_ceiling` and
`a_high_pending_ceiling_runs_spill_free_to_the_byte_bound` (19 objects
against limit 18). No C1 or C2 source changed in this checkpoint. This tree
has no CI or aggregate preflight gate; neither is claimed green. The copied
`checkpoint4-reviewed-core-test.log` retains the exact failure.

All 10 `FAIL` route receipts from development remain alongside the passing
receipts, unchanged: `checkpoint4-refund-close-01` double-counted metadata
inside the already combined Host allocation; `checkpoint4-repeated-01`
exposed an evicted resident Node used for base length, fixed by reading the
captured canonical `Inspect::File`; `checkpoint4-cleanup-failure-01` exposed
read-pin admission being stopped after successful publication, fixed without
allowing new writes; `checkpoint4-quota-refusal-01` left one external lookup
reference and therefore got expected close `Busy`; and
`checkpoint4-pressure-compact-01` through `-06` tried to prove low quota via
a public Stage fixture that released its large reserved headroom before
reconciliation. The last of those recorded 69,632 B available before but
688,128 B after that Stage. The invalid pressure fixture was removed and
the direct exported-backing proof above replaced it. Earlier passing
`separated4096` diagnostics with 41.54, 31.30, 19.61 and 16.94 s complete
walls were mechanism iterations, not repeat samples of a registered
performance arm. None was relabelled as a gate result.

Production LOC was computed separately for every commit from its exact first
parent and staged tree with `tools/production_loc.py --root <snapshot> --json`,
including Core and the unchanged reference implementation:

| Commit | Reference | Core | Combined |
| --- | ---: | ---: | ---: |
| `38bcf9912` implementation | 65,417 → 65,417 (0) | 63,547 → 64,503 (+956) | 128,964 → 129,920 (+956) |
| `05fca30fb` review fix | 65,417 → 65,417 (0) | 64,503 → 64,548 (+45) | 129,920 → 129,965 (+45) |
| `6f8c5a684` test-only count oracle | 65,417 → 65,417 (0) | 64,548 → 64,548 (0) | 129,965 → 129,965 (0) |

Checkpoint 5 remains **NOT_RUN**: the 3 × 3 public 10 MiB matrix, separate
clean and one-edit Commit controls, the unchanged #248 4,097-write gate,
their independent verifiers and cache-qualified paired arms. The v1 external
registry's `issue273-multi-exec-v1`, three-generation
`issue273-g1-g2-v1`, and aggregate `issue273-mutations-v1` cells also still
need their named receipts; existing focused checkpoint-3/4 tests cover parts
of those behaviors but are not those registered selections. No 2× claim,
release admission, PR merge or issue closure follows from checkpoint 4.

## Later owner direction: phase 4.5 before checkpoint 5

On 2026-09-28 the owner requested a researched
[phase 4.5 hot-backing implementation spec](PHASE4.5-IMPLEMENTATION-SPEC.md)
before checkpoint 5, with the [three-subagent audit](PHASE4.5-RESEARCH-AUDIT.md).
Scope is hot WRITE; concurrent Exec remains a compatibility constraint.
Repository AGENTS.md now requires incremental private backing before and
after Commit and continued running processes without pause/drain/remount.
Fully complete #273, including checkpoint 5, before the main lane/#264 merges;
its namespace/resource work is a later integration contract.
This planning addition changes no checkpoint-4 result and takes no new
performance sample. The [checkpoint-5 prompt](HANDOFF-CHECKPOINT5.md) now
carries the phase 4.5 prerequisite.

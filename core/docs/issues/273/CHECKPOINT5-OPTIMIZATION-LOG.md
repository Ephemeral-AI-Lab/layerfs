# Issue 273 checkpoint-5 optimization iterations

## Iteration 001 — bounded inverse-reference patch lookup; observer repair

- Source: first parent `6bfb9f009d164bcd35208ecaf0458dff164b78cc`;
  verified candidate `f81b7413b0b614d6cd8b05c8752b3f9c89d0ec65`.
  Artifacts: `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-001/`
  (individual SHA-256 hashes in its `SHA256SUMS`). No image, workload arm,
  prepared binary, matched performance or timer was produced. The source tree,
  retained arithmetic input hash and commands are in `diagnostic.json`.
- Cause: both `generation` and `lifetime` consumers of `prune_dead` and
  `prune_dead_payloads` made the shared R/L `live_refs` inspect the *entire*
  patch map for every touched logical page. Hypothesis: a bounded ordered
  prefix range preserves liveness and removes unrelated patch work. Exclusive
  upper bound handles `u64::MAX`; selected-index scan remains paged in blocks
  of 128, including later live references and retained/pinned owners.
- Retained source-derived operands (not new measurements): at 100/512/4,097
  dispersed writes, deletion-shape whole-map patch checks were
  607/10,794/640,614 against initial patch sizes 303/1,539/12,294.
  New source has **zero whole-map scans** in `live_refs`; patch search has
  O(P log M + R) map-lookup/relevant-reference bound for P touched logicals,
  M patch entries and R matching entries examined. Selected-index lookup,
  index publication and earlier SaveFile remain. No actual new-runtime count
  or speed ratio has been observed; these operands are not elapsed times.
- Observer: validated both cloned input identities **before** all eviction,
  then final residency of both immediately before launch; external mock test
  observes `hash,hash,evict,evict,final,final,launch`. Records final-check gap.
  Prepared-v2/attempt-v3/cache-v2 prospectively replace old receipts, which
  remain unqualified. Clean public Commit can return `UpToDate` with head;
  one-write progress has one reachable checkpoint. `--diagnostic` skips both
  verifiers explicitly and is ineligible. Missing C1 rows are not zero.
- Checks: harness unittest 11 PASS; self-check PASS; workspace host release
  test PASS (17 actual tests across test executables; most Linux-gated tests
  discovered 0 on macOS); SDK example compile/tests PASS (zero tests);
  workspace all-target clippy PASS; fmt check PASS; product boundary PASS;
  core tools unittest 9 PASS. Exact commands and retained output files in
  the iteration artifact. Linux aarch64 zigbuild FAILED before compile:
  `Failed to find zig: cannot find binary path`. Consequently the changed
  Linux-gated reconcile coverage and mounted proof were NOT_RUN.
- Production LOC for commit `f81b7413`: Core 67,158 -> 67,158;
  reference 65,417 -> 65,417; combined 132,575 -> 132,575 (delta +0).
  Method: `python3 tools/production_loc.py --json --root <exact snapshot>`
  comparing the first-parent archive, staged tree and committed tree. The
  same source-line count is coincidence, not a performance result.
- Qualification: no cache-cold matched sample, phase wall, charge/resource
  domains, new pack-load counts, cleanup/interference observation or immutable
  binary/image seal; all NOT_RUN. Historical campaign-3 rows remain unqualified.
  No latency/admission claim. Next shared causes: Commit's pack-source access
  thrashing and WRITE's inner publication cost; retained controls also require
  a live pinned private journal in the same Workspace. None is yet fixed.

## Iteration 002 — captured packed-source locality (functional causal counts)

- Algorithm source `cc9ce918c077e319d5848f3e67ff64d56a60f5bd`;
  expanded *test-only* tier coverage `51c1838a334e285d489e0987205e2405c2621102`.
  No benchmark performance arm sampled. Worktree-local retained artifacts and
  raw public-route receipts live under
  `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-002/`.
  `RESULTS.json` records source, exact test-binary SHA, fixture/image identities,
  workload, bounds, formulas and every status. Functional route receipts declare
  `performance_claim=false` and `cache_claim=null`.
- Mechanism: file-offset replacement order thrashes the one-pack source
  reader over chronological tiny packs. Committed algorithm gathers only required
  Packed references within 256 references **and** 32 KiB of replacement bytes,
  precharged with 256 index entries; sorts each fixed window by captured G1
  logical pack; reads and validates each required record/slot exactly; scatters
  into bounded memory, then emits in file-offset order. Base/Zero/Payload
  stream separately. The reader still caches one page; budget remains 8 MiB.
  Descriptor/extent O(E_f) vectors remain fully charged, not constant RAM.
- One *functional/count* public-stage test per tier, on a fresh independent
  8,194-byte file and 100/512/4,097 distinct dispersed single-byte writes:

  | Tier | Old one-page source model | Window distinct-pack model | Actual pack loads | Locator seeks; index reads | Windows | Decoded records | Window fill time | Public Commit wall | Complete functional route wall |
  | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
  | 100 | 14 | 2 | 2 | 2; 4 | 1 | 100 | 0.645 ms | 26.614 ms | 1.180 s |
  | 512 | 430 | 14 | 14 | 14; 28 | 2 | 1,024 | 3.934 ms | NOT_RECORDED | 4.480 s |
  | 4,097 | 4,097 | 560 | 560 | 560; 1,680 | 17 | 44,296 | 188.404 ms | 352.563 ms | 12.006 s |

  The models sort this stage fixture's `(104729 + i*2654435761) % 8194`
  positions, assign one chronological pack per 80 writes, then count
  output-order pack transitions versus distinct packs in consecutive
  256-reference windows. The observed counts come from production source
  telemetry and phase-local backing fetches, not a baseline performance arm.
  The registered 10 MiB schedule has different predicted counts (41/512/4,097
  before; 2/14/827 for these fixed windows); it has **not** been measured.
  Full-byte oracle, exact clean-close refund and controlled ext4 private
  allocation PASS in all three functional selections. The 512 route uses the
  earlier `cc9ce918c` test binary; 100 and 4,097 use the test-only expansion.
- Verification: host workspace release tests PASS; workspace all-target clippy,
  fmt and product boundary PASS; aarch64 release zigbuild PASS using the
  installed Zig tool on PATH; the exact `active_backing-f325d5782a2161dc`
  on an owned ext4 Docker volume with `TMPDIR=/work`,
  `LAYERFS_ACTIVE_TEST_ROOT=/work` and `--test-threads=1` passed 32/32. Public
  stage routes `active_source_grouping`, `_100`, `_4097` PASS, including
  full bytes and clean-close. Their service/container volumes were removed by
  the route. Exact selection commands and checksums are retained in iter-002.
- Production LOC for `cc9ce918c`: Core 67,158 -> 67,378 (+220);
  reference 65,417 -> 65,417; combined 132,575 -> 132,795 (+220).
  Test-only `51c1838a3`: Core 67,378 -> 67,378; reference 65,417 -> 65,417;
  combined 132,795 -> 132,795 (delta +0). Method for both:
  `python3 tools/production_loc.py --json --root <exact first-parent/staged/committed snapshot>`.
- Qualifications: private O_DIRECT requests plus this ext4 proof do not prove
  cold metadata/VM/backend/host cache. There is no matched performance sample,
  source-index cache bound diagnostic across all cases, phase RSS or cgroup
  reset, independent benchmark oracle or all registered selections. No numeric
  speed ratio. NEXT: split WRITE's dominant publication region into actual
  physical, merge/codec, retirement and temp-input costs; use that split to
  remove demonstrably avoidable work, not authentication or quota custody.

## Iteration 003 — measure the inner WRITE publication cost, choose CPU reuse

- Product `fb53ac28cbcdc4c14ab4424f5beaa202987e3ff4`, first parent
  `51c1838a334e285d489e0987205e2405c2621102`. Functional public
  `active_page_profile` route on an independently prepared 8,194-byte
  `publication` file: 4,097 append WRITE acknowledgements, explicit Commit,
  complete byte oracle and clean-close refund PASS. Source/image/binary/fixture
  seals and exact raw route stdout/stderr are retained under `iter-003`.
  Status: functional PASS, cache claim null, performance/admission INELIGIBLE;
  no baseline arm, no hard RSS/cgroup or matched timer.
- Phase-local operation operands: 4,097 pack + 16,683 index page writes =
  20,780 immutable 4 KiB creations, so 85,114,880 requested write bytes and
  the same mandatory immediate authenticated readback bytes (83,120 KiB
  each). The public write loop took 5,450.864 ms with the counters active.
  Measured disjoint inner regions, including their instrumentation overhead:
  fit merge 57.439 ms; actual selected merge 46.864 ms; node encode
  10.810 ms; hot cache decode 221.887 ms; page framing 317.184 ms;
  create/stat identity 147.665 ms; allocation/accounting 337.630 ms;
  direct write 1,731.537 ms; direct readback 861.366 ms; authenticated
  comparison 359.257 ms; retirement release/unlink 163.367 ms. Sum
  4,255.005 ms; unassigned 1,195.858 ms (temp acquisition/reading,
  other mutable work and timer scopes, not presumed idle time). Direct
  write+readback = 2,592.903 ms / 5,450.864 ms = 47.57% of this WRITE loop.
  Old recorded campaign's 5.075 s publication was on different identities;
  it is not a speed comparator for these numbers.
- Hypothesis: reuse the already encoded Node after the PageStore has read back,
  authenticated and byte-compared its physical page instead of allocating and
  decoding that same Node again. Cache decode has a 221.887 ms *optimistic*
  loop-wide ceiling, 4.07% of observed write-loop wall, not a guaranteed gain.
  Page physical write/readback remain required by the v2 custody contract.
  No format change, unpinning, narrowed budget or weakened authentication is
  justified by the profile. Next iteration tests prepared-node reuse and
  separately addresses avoidable tiny-input file round trips. Preallocated
  physical free slots require a new compatible design/custody proof before
  collection; no undocumented reuse was attempted here.
- Production LOC for `fb53ac28c`: Core 67,378 -> 67,479 (+101);
  reference 65,417 -> 65,417; combined 132,795 -> 132,896 (+101),
  first-parent versus staged/committed source by `tools/production_loc.py`.
  Workspace host release PASS (17 host tests); clippy/fmt/boundary/tools/
  harness PASS; aarch64 zigbuild PASS. Public stage route complete wall
  8.958 s under its 60 s functional limit; this is **not** the registered
  15/25-second mounted performance case.

## Iteration 004 — reuse the authenticated prepared hot Node

- Verified product `c561576dc23715711980448dcd6138f2507a0901`, first parent
  `fb53ac28cbcdc4c14ab4424f5beaa202987e3ff4`. `Mutation::page` transfers
  the already encoded Node into its hot cache only after `create_from` verified
  exact physical readback and page identity; the cached kind and record count
  still match the selected slot. Previously selected/cold nodes continue to
  authenticate and decode. No physical readback or node-validation omission
  applies to incoming bytes, existing pages or another revision. The existing
  conservative cache charge precedes the retained Arc allocation.
- Causal, public functional append-4,097 diagnostic on the same independently
  prepared 8,194-byte seed (NOT registered mounted performance):
  cache decode cumulative WRITE-region time 221,887,020 -> 14,292 ns,
  difference 221,872,728 ns, denominator 16,683 index writes in both cases;
  physical creations 4,097 pack + 16,683 index in both. WRITE loop
  5,450.864 -> 5,162.682 ms; **do not use** that one wall difference as a
  qualified speed gain or noise distribution. New direct-write/readback stayed
  necessary and cumulative (1,671.236/965.444 ms). Full bytes and clean-close
  refund PASS, complete functional route 8.593 s (60 s bound), ext4
  `active_backing` 32/32 PASS on an owned Docker volume. Stage result and
  production phase counters are preserved in `iter-004/RESULTS.json` and raw
  stdout/stderr/receipts. Cache claim null, numeric performance INELIGIBLE.
- Host workspace release tests, all-target clippy, fmt, product boundary and
  aarch64 zigbuild PASS. Production LOC for `c561576dc`: Core
  67,479 -> 67,506 (+27), reference 65,417 -> 65,417, combined
  132,896 -> 132,923 (+27), exact first-parent/staged/committed snapshots
  counted with `tools/production_loc.py`. Next cause: extra FUSE small-input
  temporary-file create/read/release, previously bounded in retained evidence
  at 723 ms acquisition for 4,097 append writes; keep that work in the timed
  WRITE and preserve ordinary projection custody.

## Iteration 005 — remove the temporary tiny FUSE input owner

- Product `9c286b02c7b7a38aaca8f8d36f72e700c44ba754` and test-only tier
  expansions `3c5e7976571b410fded14328cc31f5b7970e9e82` (512) and
  `609045ed1e7cc54a9b246b54ed52f50f9d7cc903` (100/4,097).
  Through the ordinary public projected permit, each <=128-byte callback
  makes a Budget-charged copy before mutation or reply and does not create,
  read or unlink a `p-*` temporary input file. The acquired-copy charge is
  included in publication time, not hidden in setup or after acknowledgement.
  Large-input and local OwnedPayload routes retain their original custody.
  The charged copy shares EOF/append, handle, revision, deadline, notification
  and failed/unknown-result checks with the previous mutation route.
- Three *functional* mounted ext4 routes (not registered 10 MiB performance
  rows) use an independently prepared 8,194-byte file and 100/512/4,097
  one-byte POSIX append syscalls. All public FUSE callbacks were observed
  exactly (100/512/4,097), full bytes, C5 Commit, mounted unmount/process
  continuity and clean-close block refund PASS; no temporary input owners
  remained. Cumulative FUSE acquisition at all four sampled checkpoints was
  **0 ns** per case; their charged copy and all page work remain in the
  publication timer. At sampled W=4,096 on the 4,097 case: publication
  4,841,970,187 ns, pack writes 4,097 after W=4,097, index writes 16,678;
  this is an observer result with instrumentation overhead, **not** a matched
  speed gain against any earlier timed source. Functional route complete wall
  1.136/1.730/8.437 s at 100/512/4,097, each under its separate 60 s
  functional bound. The mounted `active_mounted` and process-spanning
  `active_hot_continuity` routes and the exact Linux `active_backing` 32/32
  also PASS at the product identity. Counts, phase-local samples, raw stdout,
  exact product/test/image/fixture SHA seals and individual failure statuses
  are retained under `iter-005/RESULTS.json` and `SHA256SUMS`.
- Limitations: prior retained 723.286 ms acquisition on campaign-3 append was
  from a different unqualified identity and **not** an admissible speed
  denominator. No cached/uncached or cgroup phase claim. Production LOC for
  `9c286b02c`: Core 67,506 -> 67,602 (+96), reference 65,417 -> 65,417,
  combined 132,923 -> 133,019 (+96). Test-only commits `3c5e79765` and
  `609045ed1`: all production totals unchanged (delta 0). All commits used
  `tools/production_loc.py` on first-parent versus staged/committed trees.
  Next cause: count generic-route admission and normalization turnover on
  dispersed WRITE and distinguish necessary source carries and physical
  publication from avoidable revalidation.

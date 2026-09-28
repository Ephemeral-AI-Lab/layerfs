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

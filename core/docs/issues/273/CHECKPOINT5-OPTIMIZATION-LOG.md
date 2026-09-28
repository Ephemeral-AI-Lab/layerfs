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

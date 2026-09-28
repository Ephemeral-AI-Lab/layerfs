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

## Iteration 006 — generic changed-closure and live quick controls

- Product stays `9c286b02c7b7a38aaca8f8d36f72e700c44ba754`; test-only source
  `478f4cd54babcd8084c60b2242ec63c6ce59aa2b` (4,097) and
  `845473c4161f219326d2aa1adf4d4142d5d88990` (100/512). No unchanged
  performance arm was resampled. Full-byte / clean-close public functional
  oracles PASS at each tier; exact binary/image/fixture seals, route receipts,
  stdout/stderr, counts and complete walls are in `iter-006/RESULTS.json`.
- Dispersed 100/512/4,097 writes on an 8,194-byte file: hot writes 0/0/0;
  admissions 38/425/3,973; normalizations 38/425/4,785; seeks
  1,519/7,699/61,327 (15.19/15.04/14.97 per accepted WRITE); index reads
  239/2,561/28,470 (2.39/5.00/6.95 per WRITE); index writes
  390/2,866/27,780 (3.90/5.60/6.78 per WRITE); pack writes exactly
  100/512/4,097. Admission/normalization of the *same changed inode's*
  replaced hot cursor and up to 64 bound slots is visible, but no
  growing unrelated-retained-prefix walk: seeks/WRITE do not rise across
  tiers. Index reads/writes per WRITE rise with changed working-set,
  selected height and cache eviction; they are not explained as a
  universal O(1) factor. Source ties obsolete bindings to the superseded
  same-inode frontier; it still probes for prospective hot continuation
  rather than falsely granting random edits frontier status. Fixing that
  probe without losing eligible follow-on writes needs an explicit
  verified replacement admission policy, not a benchmark-schedule branch.
  Actual pack loads under the 256-reference Commit window remain 2/14/560
  for these independently run functional rows. No speed ratio is claimed.
- Separately, `active_quick_controls` PASS with a real pinned, live
  4,097-record private state in **one** Workspace. Clean Commit read 0 pack
  pages and 6 index pages, phase wall 7.649 ms; one-edit Commit read 1 pack
  and 33 index pages, wall 23.063 ms, while 52 older pack pages remained
  retained and clean-close exactly refunded them. Whole functional command
  including preparation was 6.915 s; no prep moved outside it to fit.
  This public Workspace proof does not turn a Store reattached by the SDK
  checkpoint runner into a live journal. Its selections 10/11 remain NOT_RUN.
- The common checkpoint harness now refuses the invalid retained preparer,
  records those blockers in their original row slots, emits no ratio from
  unqualified pairs and accepts explicit complete C1 zero only with emitted
  fields. Its attempt/report/campaign versions advance prospectively;
  historical receipts stay untouched. Test/docs-only commit `845473c`:
  Core 67,602 -> 67,602; reference 65,417 -> 65,417; combined
  133,019 -> 133,019 (delta +0); method `tools/production_loc.py`
  on first-parent and staged/committed snapshots. C1 observer tests 13
  PASS, cargo clippy/fmt and Linux stage build PASS.
- QUALIFICATION: `performance_claim=false`, cache claim null, complete
  functional route walls 1.123/2.212/11.739 s plus quick control 6.915 s.
  Neither wall makes a matched latency claim; no phase RSS/cgroup reset,
  host/backend private-cache proof or all five independent selection
  coverage yet. Continue the remaining correctness/space and matched
  runnable rows; report SDK retained contract blocker without fabricating
  a pinned state or lengthening its 15 s limit.

## Iteration 007 — registered 10 MiB source diagnostic and atomic telemetry repair

- Committed candidate `00553ef49...` (product source still `9c286b02c...`,
  harness `845473c41...`) prepared once using frozen closed 10 MiB/8,194-byte
  masters, locked incremental release host/aarch64 artifacts and three sealed
  images. `oracle` and `prepare` wrote fresh worktree-local artifacts under
  `iter-007/`; Darwin cold positive control detected 32 warm pages and 0
  resident pages after invalidation. One **labelled, unverified causal**
  candidate diagnostic per registered dispersed 10 MiB tier 100/512/4,097:
  all three complete commands finished within 15/15/25 s, at
  1.663/2.023/10.689 s, host store/history final whole-input residency 0,
  final-check-to-launch gaps 3,125/3,541/3,375 ns, public driver COMPLETE,
  C1 counters present and independent verifiers explicitly SKIPPED. These
  are not matched numeric speed samples or release admission.
- The 512 count row is complete: two 256-reference windows, 14 distinct
  logical packs and **14 actual** pack loads, 28 index reads, 1,024 decoded
  records. At 100 and 4,097 the production `LFS_ACTIVE_SOURCE` line was
  bisected by an independent LFT1 line (the 100 line even contains
  `decoded_bytes=4900LFT1`); full anchored source-count ingestion is
  `INCOMPLETE_INTERLEAVED` for both, not the favorable partial-prefix 2/827
  prediction or a measured zero. Raw receipts remain intact; no arm was
  resampled at this unchanged identity. Source model still predicts
  2/14/827 loads for this registered schedule, subject to the changed-candidate
  count diagnostic.
- Corrective product work builds a bounded 1,024-byte precharged stack
  observation and emits it in one short write, rather than multiple
  fragmentary `eprintln` writes. The common prospective attempt-v5 harness
  requires exactly one anchored, internally consistent complete production
  line. Missing, malformed, overflowed or interleaved lines explicitly
  mark candidate source diagnostics INCOMPLETE; baseline product has no such
  telemetry and its absence is not zero. Source-code reader, grouping and
  WRITE algorithms do not otherwise change in this correction. New product
  and harness identities must be committed before any new diagnostic.
- Old-source complete-command walls and O_DIRECT requests are **not** a
  cache/VM/backend/device proof, and no phase speed ratio is permitted.
  Iter-007 raw attempts, cache evidence, seals and checksums are retained
  below the same artifact root. New correction verification and exact LOC
  are appended after the prospective identity is frozen.

### Iteration 007 correction frozen and verified (new source identity)

- Product/harness/contract commit `adbee51588d9f6efd35cab3ebb3919f5083fa6b9`;
  Core production LOC 67,602 -> 67,639 (+37), reference 65,417 unchanged,
  combined 133,019 -> 133,056 (+37), first parent versus staged/committed
  snapshots by `python3 tools/production_loc.py --json --root <snapshot>`.
  Prepared a **new** immutable candidate binary/image identity from the same
  closed protected masters; no mutated sample or warmed reader was reused.
- One prospective labelled candidate-only registered-10-MiB causal attempt
  at each dispersed tier, all with strict anchored v1 source telemetry:

  | Writes | Old source one-page miss model | New distinct-pack/window model | Actual pack loads | Locator seeks / index reads | Records decoded | Complete command wall |
  | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
  | 100 | 41 | 2 | **2** | 2 / 4 | 100 | 2.685 s |
  | 512 | 512 | 14 | **14** | 14 / 28 | 1,024 | 1.897 s |
  | 4,097 | 4,097 | 827 | **827** | 827 / 2,481 | 65,530 | 10.217 s |

  Every row had one production `LFS_ACTIVE_SOURCE` line, parser `PASS`,
  final host Store/history residency zero, final-check-to-launch gaps
  4,792/4,917/5,666 ns, full driver COMPLETE, C1 counters complete and
  independent verifiers **SKIPPED** as declared. Row status INELIGIBLE;
  no qualified control arm or numeric speed comparison. Physical requested
  whole-pack read bytes from the 4,097 row's 827 loads are
  `827*4096=3,387,392`, not measured device traffic. The 4,097 model
  improvement is `4,097/827=4.95x` fewer *pack loads*, not a latency ratio.
  Locator index reads and decoded records stay charged and observed.
- Host workspace release PASS (17 executed tests), all-target clippy/fmt,
  product boundary and core-tools 9/9 PASS; harness 14/14 PASS; release
  aarch64 zigbuild PASS. Exact Linux `active_backing-f325d5782a2161dc`
  on an owned ext4 volume, `TMPDIR=/work`, `LAYERFS_ACTIVE_TEST_ROOT=/work`,
  `--test-threads=1`: 32/32 PASS; public functional stage
  `active_generic_profile`: full-byte/refund PASS. All raw receipts, source
  code/artifact/fixture/image hashes, phase/complete times, test output,
  unsupported-domain statuses and checksum manifest live in `iter-008/`.
- Old iter-007 corrupted rows remain INCOMPLETE and cannot be relabelled.
  The correction validates the planned source mechanism but **not** a
  qualified matched speedup or release gate. Runnable control arms, the
  independent verifier, additional registered proof selections, phase-local
  RSS/cgroup, private metadata/VM/backend/device/host cache proof, and
  SDK retained live-journal controls remain NOT_RUN/INELIGIBLE as applicable.
  No universal constant-RAM/CPU or 10x claim. The attached PR remains draft.

## Iteration 009 — bounded source-window increase and amortized extent growth

- Product/harness/spec commit `686d002fa889f016cac3eb491b5712229e511360`
  (tree `ae48b2e1eb3c118b7b375e7929a40c08a4b4b08f`), first parent
  `4968cc9d31127c04e442899f98f4bd09e27f5292`. Source-reference
  cap 256 -> 1,024; byte cap remains 32,768, one resident decoded pack.
  The charged two reference/order vectors increase by at most
  `768 * (size_of::<Reference>() + size_of::<usize>())` (30,720 bytes on
  the 64-bit build), reserved before allocation. The existing full-file
  extents/descriptors are still charged O(E_f); paged extent collection now
  doubles charged capacity instead of requesting exact expansion on each
  128-entry page. The scan remains selected-G1, with no cache/format, worker,
  stream-order, page authentication or deadline change. Telemetry is v2;
  the parser rejects old v1 lines rather than reinterpreting old receipts.
- Production LOC: Core **67,639 -> 67,649 (+10)**; reference
  **65,417 -> 65,417 (+0)**; combined **133,056 -> 133,066 (+10)**.
  `python3 tools/production_loc.py --json --root <HEAD snapshot>` versus
  `<staged-tree snapshot>`; committed tree confirmed against the staged tree.
  Host workspace release tests PASS, workspace all-target clippy PASS, fmt
  PASS, product-boundary PASS, core tools 9/9 PASS, harness 14/14 PASS,
  runner self-check PASS; aarch64 release zigbuild PASS after adding the
  installed Zig directory to PATH (initial PATH-only invocation FAILED to
  locate `zig`; this environmental failure is not erased). Exact Linux
  `active_backing-f325d5782a2161dc` on a new owned ext4 volume:
  **32/32 PASS**, `TMPDIR=/work LAYERFS_ACTIVE_TEST_ROOT=/work`, serial
  execution; volume removed after output retention. Three new public stage
  selections `active_source_grouping_100`, `active_source_grouping`, and
  `active_source_grouping_4097` PASS full-byte/Commit/refund with exactly
  2/7/209 pack reads; their 4,097-read bound is now 300, rejecting the old
  560-read functional source. Stage output is **functional**, not a measured
  performance arm.
- Exactly one new committed-identity candidate causal diagnostic per cell,
  with independent writable clone, final host input residency zero and one
  complete atomic production source row. Each row is **INELIGIBLE**, each
  verifier **SKIPPED**, no matched control. Times include observation cost;
  do not compare their single walls with another source as a speed ratio.
  Figures below are actual pack loads / windows / decoded records /
  locator index reads / complete command seconds:

  | Schedule | 100 | 512 | 4,097 |
  | --- | --- | --- | --- |
  | Append | 2 / 1 / 100 / 4 / 1.591 | 7 / 1 / 512 / 14 / 1.613 | 52 / 5 / 4,097 / 156 / 6.055 |
  | Dispersed | 2 / 1 / 100 / 4 / 0.993 | 7 / 1 / 512 / 14 / 2.000 | 209 / 5 / 16,468 / 627 / 10.867 |
  | Repeated | 1 / 1 / 20 / 1 / 1.006 | 1 / 1 / 32 / 1 / 1.612 | 1 / 1 / 17 / 1 / 6.379 |

  Accepted FUSE WRITE counts match 100/512/4,097 for each pattern. Packed
  final replacement references were 100/512/4,097 (append and dispersed)
  and 1/1/1 (repeated). Thus loads/reference at 4,097 are 52/4097 append,
  209/4097 dispersed and 1/1 repeated; do not divide repeated source
  work by historical overwritten records. Source fill time for dispersed
  4,097 is 74.181 ms; Commit phase 0.591 s; Exec 9.403 s (different
  phases). C1 nodes read for dispersed 100/512/4,097 are
  2,028/10,532/84,982 (~20.3/20.6/20.7 per WRITE); append 4/4/4;
  repeated 17/17/17, explicit emitted provenance, not a missing-zero.
  Repeated decoded records 20/32/17 reflect the final *pack's* occupancy,
  not a 4,097-record source read. Append 4,097 distinct packs/windows 56
  but physical loads 52: the single page can remain cached across window
  boundaries. Each row has Budget charge/backing, cache checks, phase walls,
  resource status and binary/image/workload seals in the raw receipts and
  `iter-009/RESULTS.json`; cgroup domain remains unavailable/PARTIAL.
- Changed-source count comparison with iter-008, **not a numeric speed pair**:
  dispersed 100 2 -> 2, 512 14 -> 7, 4,097 **827 -> 209** loads;
  `827/209 = 3.957` fewer physical pack loads at 4,097, decoded records
  65,530 -> 16,468 (`65530/16468 = 3.980`). Read identities and
  authenticated bytes are unchanged; cross-window revisits still exist.
  General per-file bound is `loads <= sum(distinct packs in each bounded
  window) <= N packed references`; this is **not** O(P) total loads for
  arbitrary pack permutations. The old 256-window receipt is unchanged.
- Reproduce from this commit (PATH includes `/opt/homebrew/bin` and
  `/usr/local/bin`): `checkpoint5_273.py oracle --repo "$PWD" --output
  benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-009/oracle`;
  `prepare --arm candidate --repo "$PWD" --oracle .../iter-009/oracle/oracle.json
  --output .../iter-009/prepared-candidate`; for each cell,
  `run --diagnostic --prepared .../iter-009/prepared-candidate/prepared.json
  --selection issue273-<pattern>-<count>-10m-v1 --output .../iter-009/<pattern>-<count>`.
  Actual exact argv, source/tool/binary/image hashes, phase and complete
  walls, cleanup, cache evidence, stdout/stderr, result and checksum manifest
  are retained under `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-009/`.
- **NOT_RUN / INELIGIBLE**: all nine independent verifiers SKIPPED; matched
  control arms NOT_RUN; live/pinned SDK clean and one-edit controls NOT_RUN
  (the public SDK has no same-Workspace journal pin across the required
  sequential commands); five independent registered selections NOT_RUN;
  private metadata/VM/backend/device/host-cache and phase-local cgroup
  proof incomplete. No qualified speed or checkpoint-5 completion.
  Next: inspect further per-row amortization risks (including exact-reserving
  directory-binding rows), run targeted public custody cases, then rebuild
  prospective identities for any changed code. The stronger source count
  alone does not settle WRITE publication or final admission.

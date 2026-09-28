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

## Iteration 010 — remove captured directory-row prefix relocation

- Commit `14da22d48c20de169c9224903d02eda18975d1a7` (tree
  `6a3b02c5d35566b276560c4415aaf64d2a65b22e`), parent
  `686d002fa889f016cac3eb491b5712229e511360`. Captured
  `directory_rows` previously asked for *one exact Vec slot per binding*;
  if each request relocates, the 128-file control copies up to
  `0+1+...+127 = 8,128` prior row descriptors. The new 8/16/32/64/128
  precharged capacities require at most five growth requests, copying at
  most `0+8+16+32+64 = 120` old row descriptors if each relocates.
  This is a source-derived worst-case **allocation operand**, not a runtime
  byte counter or a latency gain. Name bytes/selected-index pages and each
  actual C1 binding remain O(D) required work; Budget charge precedes
  allocation, including new capacity and names. Selected G1, identities,
  quota refusal and clean-close custody are unchanged. Source locality from
  iteration 009 is unchanged. No new worker, format or hot cache capacity.
- Production LOC (exact first parent/staged/committed snapshots via
  `python3 tools/production_loc.py --json --root <snapshot>`): Core
  **67,649 -> 67,662 (+13)**; reference **65,417 -> 65,417 (+0)**;
  combined **133,066 -> 133,079 (+13)**. This commit also records the
  prior iteration-009 log. Host workspace release tests PASS; workspace
  all-target clippy/fmt, product boundary, tools 9/9, harness 14/14,
  self-check and aarch64 zigbuild PASS. Exact aarch64 Linux `active_backing`
  on owned serial ext4 Docker volume: **32/32 PASS**, volume removed.
  New committed-identity functional public routes PASS: 128 files with
  shared packs, exact committed byte/mode/refund; quota refusal; G1/G2
  generation bytes; 32 retained generations/pins/refund; mutation
  compaction; real live/pinned 4,097-record clean and one-edit controls
  (one Workspace). They are stage proofs, **not** the missing public SDK
  retained performance controls, not a 3-sequential-100-write Exec proof.
- One registered candidate attempt per nine matrix selections at the new
  committed source, with **no** same-identity performance resample. All
  nine independently verified oracle `PASS`, byte/function/cleanup `PASS`,
  production source-row parser `PASS`, final host whole-input residency 0,
  callback count exact, C1 telemetry complete, 15/25 s complete limit met.
  The common frozen arm verifier returned PASS at 100 and NOT_APPLICABLE
  (it hard-codes 100) at 512/4,097. All nine row statuses nevertheless
  **INELIGIBLE**, not a matched numeric or cache/resource admission:

  | Schedule | 100: loads/refs/windows; Exec/Commit/complete s | 512: loads/refs/windows; Exec/Commit/complete s | 4,097: loads/refs/windows; Exec/Commit/complete s |
  | --- | --- | --- | --- |
  | Append | 2/100/1; 0.092/0.019/2.585 | 7/512/1; 0.618/0.029/1.489 | 52/4097/5; 4.713/0.131/5.845 |
  | Dispersed | 2/100/1; 0.136/0.027/0.991 | 7/512/1; 1.025/0.059/2.124 | 209/4097/5; 9.362/0.645/10.920 |
  | Repeated | 1/1/1; 0.144/0.029/0.968 | 1/1/1; 0.698/0.029/1.579 | 1/1/1; 5.719/0.030/6.668 |

  For append/dispersed final references grow with accepted WRITEs; for
  repeated the final file has one replacement regardless of historical
  WRITEs. Dispersed 4,097 still requires 209 loads/627 locator index reads,
  16,468 decoded records, 73.035 ms source fill and 2,834,432 bytes
  sampled charged backing. 100/512 dispersed have 2/7 loads and 4/14
  index reads; repeated 1/1/1 loads and 1/1/1 index reads. Across all
  cases, index reads/loads follow selected height (2 at 100/512, 3 at
  4,097 dispersed) and the necessary pack/window model. C1 nodes/read for
  dispersed are ~20.3/20.6/20.7 per accepted WRITE; append 4, repeated
  17 per SaveFile. Per-WRITE immutable acknowledgement and canonical
  construction remain necessary and charged; no global O(P) locality or
  constant-RAM Commit is claimed. The O(E_f) descriptor/extent vectors
  remain charged. Stage source-window 2/7/209 full-byte results from
  iteration 009 continue to identify the bounded mechanism; the current
  verified matrix is the only new sample at this committed identity.
- The additional #248 4,097-write gate was attempted **once** at this
  identity: public byte/function/cleanup PASS, independent oracle and arm
  verifier PASS, complete 6.084 s (25 s bound), pack loads 52, but
  `row_status=INCOMPLETE`: **no `LFS_C1_EDIT_LOAD` record**. The full-file
  streamed construction emits `LFS_FILE_INPUT` and a complete
  `LFS_FILE_STREAM_CAUSE`; neither proves explicit zero C1 mapping reads
  when the C1 observer line is absent. No fabricated zero and no rerun of
  this unchanged gate arm. The fixed baseline product likewise lacks an
  explicit zero on that source route; repairing common observer provenance
  without changing frozen baseline product remains unresolved.
- Clean Commit and one-edit public SDK controls are **NOT_RUN** with
  retained `blocker.json` in original slots: a committed/reattached Store
  cannot preserve a *live private pinned* 4,097-record journal in the same
  Workspace through the public sequential SDK contract. The above public
  Workspace quick-control stage proves bytes/counts, not this performance
  fixture. Exact three sequential 100-write Execs before Commit and the
  full mutations selection remain NOT_RUN (stage G1/G2/mutation subsets
  are not substitutes). Matched baseline comparison NOT_RUN; private
  metadata/VM/backend/device/host-cache and phase-local cgroup domains
  remain unsupported/PARTIAL. The observed 100-write outside-phase
  difference is not a speed claim or a permission to resample.
- Reproduction from commit `14da22d48`: use the `oracle`, `prepare --arm
  candidate`, `run --prepared ... --selection issue273-<pattern>-<count>-10m-v1`
  commands in the runner with each new output path, one sample/selection;
  `run` (without `--diagnostic`) ran the independent verifier here. Exact
  commands, stdout/stderr, 9 attempts, gate, two NOT_RUN blockers, stage
  receipts, binary/image/source/harness/workload/spec SHA seals, per-phase
  Budget/backing/cache counters and the 263-file checksum manifest are in
  `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-010/RESULTS.json`
  and `SHA256SUMS`. Candidate source/hash and its comparison with iteration
  009 are *not* a qualified matched speed comparison. Next work needs a
  source-bound public observer for #248 zero C1, a real SDK pin contract
  for retained controls or a documented external ruling, matched control
  in an owned checkout, phase-local resource/cache proof, exact multi-Exec
  and mutation coverage, and separate WRITE publication amplification
  analysis. **Checkpoint 5 is not complete.**

### Iteration 010: phase-local WRITE scaling from retained samples

The final sampling checkpoint is WRITE **100/512/4,096** (not 4,097) in
these three tiers; the last WRITE and Commit occur after the last 4,096
sample. These are cumulative-from-command-start backing counters sampled
*inside Exec*, not lifetime cgroup peaks or final Commit source reads. For
each row below, columns are hot writes / admissions / normalizations /
index seeks / index page writes / pack page writes / retirement inspections:

| Schedule | At 100 | At 512 | At 4,096 |
| --- | --- | --- | --- |
| Append | 98 / 1 / 0 / 45 / 255 / 100 / 347 | 510 / 1 / 0 / 45 / 1,940 / 512 / 2,416 | 4,094 / 1 / 0 / 50 / 16,672 / 4,096 / 20,488 |
| Dispersed | 5 / 27 / 27 / 1,443 / 373 / 100 / 461 | 5 / 396 / 398 / 7,623 / 2,827 / 512 / 3,295 | 5 / 3,936 / 4,330 / 61,389 / 26,392 / 4,096 / 30,142 |
| Repeated | 0 / 0 / 0 / 1,601 / 100 / 100 / 198 | 0 / 0 / 0 / 8,203 / 512 / 512 / 1,022 | 0 / 0 / 0 / 65,637 / 4,096 / 4,096 / 8,190 |

Index seeks per accepted WRITE are append 0.45/0.088/0.012,
dispersed 14.43/14.89/14.99, repeated 16.01/16.02/16.02.
The repeated route always changes one file-offset record but still performs
~16 bounded selected-index seeks each WRITE; this is not evidence of a
*new growing* unrelated-prefix search. Dispersed index writes per WRITE
3.73/5.52/6.44 and retirement inspections per WRITE 4.61/6.44/7.36
rise with selected height, changing closure and evicted working set;
they must not be called an O(1) guarantee. Append index writes per WRITE
2.55/3.79/4.07 coincide with 3/26/224 hot carries and a growing
height; its independent pack and directory page writes remain one each
per accepted acknowledgement. Repeated index/pack creations are one each
per acknowledgement, with no hot eligibility falsely granted.

At 4,096 append, 4,096 pack + 16,672 index = **20,768** new 4-KiB
pages, requiring 85,065,728 source-requested direct write bytes and the
same immediate authenticated readback bytes. Physical I/O is cumulative;
retained allocation at that checkpoint was 1,146,880 bytes, a different
domain. Direct write/readback CPU-inclusive scoped times are
1,833.316/943.854 ms in the append row; dispersal's sampled index work
and retired-owner closure are larger. These counters do **not** prove
that every index page and one immutable version per affected owner is
necessary, nor establish phase-local RSS/cgroup or host/device traffic.
Avoidability of generic admission/normalization turnover and the remaining
physical publication multiplication therefore remains a source-analysis
item; the nine verified but INELIGIBLE candidate rows cannot close the
§8/§10 no-bad-factor or numeric admission gates by themselves.

## Iteration 011 — transient Vec overlap custody and failed observer row

- Committed product/spec correction `2c4b0752c9557b15aacad0ac3f5d6ebcb4b763be`
  (tree `9cb97c0f78bc07f4eea324612821101b26c48fc6`), first parent
  `29efd57ce63d43dd8c9cf3b44dd0dcc4fa98ee46`. Iterations 009/010
  reserved new geometric capacity, but did not charge the old vector while
  its allocator might still retain it. The corrected selected extent and
  captured directory scanners precharge old + target capacity (and required
  name bytes), allocate a separate destination, charge actual old + actual
  new capacity, move descriptors and release the empty old allocation.
  This changes neither canonical bytes nor source order. The earlier
  009/010 pack counts remain raw facts, **not** proof of correctly charged
  transient peak. `try_reserve_exact` may return a larger-than-requested
  capacity; actual capacity is checked before transfer, and any failure
  leaves unpublished work unwritten. No constant-RAM claim.
- Production LOC: Core **67,662 -> 67,684 (+22)**, reference
  **65,417 -> 65,417 (+0)**, combined **133,079 -> 133,101 (+22)**;
  method `python3 tools/production_loc.py --json --root <first-parent and
  staged/committed snapshot>`. Host workspace release tests PASS,
  all-target clippy/fmt, boundary and core tools 9/9, harness 15/15,
  self-check PASS; exact Linux `active_backing` 32/32 PASS in owned ext4
  Docker volume with serial tests and `TMPDIR=LAYERFS_ACTIVE_TEST_ROOT=/work`.
  Seven registered public stage subsets PASS: 128-file shared-pack/bytes/
  close, quota refusal, G1/G2, 32 generations, mutation compaction,
  live 4,097-record Workspace quick controls and 4,097 dispersed source
  grouping at exactly 209 reads. These do not establish matched timing.
- Nine **new** candidate-only, independently verified registered rows at
  this changed product identity: all nine full-byte/cleanup and oracle
  verifications PASS and have complete anchored v2 pack-source lines;
  observed pack loads remain append 2/7/52, dispersed 2/7/209, repeated
  1/1/1 (100/512/4,097), each one sample/arm/identity. Eight rows are
  INELIGIBLE for unsupported private cache/cgroup and no control; the
  **repeated-100 row is INCOMPLETE**: only 25/50/75 of its four required
  backing checkpoints parsed. Its final 100 checkpoint was *emitted* by
  `write_sample.rs`, but a separate LFT1 writer split its multi-fragment
  `eprintln!` format. Its valid byte oracle, pack line and command wall
  do not repair that observer. Raw stderr/receipt remain intact and must
  not be relabelled or replayed on the same product identity.
- The independently verified #248 gate is also INCOMPLETE due missing
  explicit `LFS_C1_EDIT_LOAD`; the public SDK live-journal clean and
  one-edit controls remain NOT_RUN. Control arm, matched numeric evidence,
  exact three-Exec and full mutations selections and private cache/cgroup
  proofs remain NOT_RUN/unsupported. All raw outcomes, six phase-local
  domains, identities, source/provenance/statuses, stderr, stage proofs,
  complete wall and a 274-file checksum manifest are retained under
  `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-011/`.
- Next product/observer fix: emit a bounded *single-write* FUSE status line
  and require an anchored complete v4 row prospectively. No earlier
  interleaved row is repaired. Preserve v3 accounting for baseline; no
  emission defect is permission to resample the unchanged arm. The #248
  missing-zero and SDK live-pin blockers are independent and remain open.

## Iteration 012 — atomic public WRITE snapshots; frozen candidate count proof

- Product/observer/harness/spec commit
  `52878a58429a61fa40a17fc615b21f847aaacb3c` (tree
  `ce59ca331084e3b3a2c183f13bc5bc42b81200e9`), parent
  `2c4b0752c9557b15aacad0ac3f5d6ebcb4b763be`. Optional operator
  `LFS_WRITE_SAMPLE v=4` is formatted into a fixed 4,096-byte FUSE
  process-stack record, written in one sub-PIPE_BUF call; overflow yields
  explicit INCOMPLETE. It is not a persistent Workspace cache or a claim
  of Budget residency. The common external observer requires a complete
  *anchored* line; v1/v2 historical self-checks and baseline v3 parsing
  remain available, but fragmented/interleaved v3 is not reconstructed.
  Product READ/WRITE/Commit, pack/cache/worker, pre-acknowledgement byte
  ownership and independent verifier semantics do not change.
- First-parent/staged/committed production LOC by
  `python3 tools/production_loc.py --json --root <exact snapshot>`:
  Core **67,684 -> 67,719 (+35)**; reference **65,417 -> 65,417 (+0)**;
  combined **133,101 -> 133,136 (+35)**. Workspace and FUSE host release
  tests PASS; all-target clippy/fmt, product boundary, tools 9/9, harness
  15/15, checkpoint self-check and separated-writes self-check PASS;
  exact release Linux cross build for workspace and FUSE PASS. Initial
  two-`--tests` zigbuild invocation failed at CLI parsing and the first
  shared self-check exposed v1/v2 parser regression; both attempts remain
  on disk, were corrected *before committing*, and passing commands
  covered the final source. Exact Linux `active_backing` 32/32 PASS in an
  owned ext4 volume with serial execution, volume removed. Six new
  committed-identity public stage routes PASS: mounted 100-callback
  tiny inputs, same-process G1/G2 hot continuity, 4,097 dispersed source
  grouping (209 reads), shared-pack 128 files, quota refusal, and real
  live/pinned 4,097-record Workspace quick controls. This stage profile
  has `performance_claim=false`, `cache_claim=null`.
- One new registered candidate attempt per matrix cell (one per
  case/arm/changed identity), all source counters complete, public full
  bytes/cleanup PASS, independent oracle verifier PASS, exactly four
  **v4** checkpoints including the final checkpoint, final host whole-input
  residency zero, 100/512/4,097 accepted callbacks and 15/25-second
  complete limits met. At 100 the frozen arm verifier PASS; at 512/4,097
  it is NOT_APPLICABLE because it hardcodes the 100-write schedule, not a
  silent PASS. Actual source pack loads per 100/512/4,097 are append
  **2/7/52**, dispersed **2/7/209**, repeated **1/1/1**; 4,097 dispersed
  has 5 bounded windows, 4,097 final references, 16,468 decoded records
  and 627 selected locator index reads. All nine **row_status=INELIGIBLE**
  despite functional PASS: no control match or complete private cache/
  cgroup proof. Phase Exec/Commit/complete seconds, respectively:

  | Schedule | 100 | 512 | 4,097 |
  | --- | --- | --- | --- |
  | Append | 0.086/0.023/0.920 | 0.596/0.028/1.404 | 4.671/0.129/5.696 |
  | Dispersed | 0.124/0.026/0.994 | 1.019/0.051/1.921 | 9.308/0.504/10.666 |
  | Repeated | 0.144/0.019/1.612 | 0.685/0.017/1.492 | 5.188/0.022/6.013 |

  The repeated-100 complete wall exceeds the 512 row despite shorter
  Exec and Commit: the difference remains outside those measured phases;
  do not assign it to warm cache or infer a noise distribution. Pack
  count/reference and WRITE publication scaling retain the source-bound
  explanation and open questions in the previous section. The v3
  repeated-100 attempt at iteration 011 remains **INCOMPLETE**, not a
  previously passing row repaired or replaced at unchanged identity.
- #248 gate: full bytes/cleanup and both independent verifiers PASS; the
  new four WRITE samples are complete v4 but `row_status=INCOMPLETE`
  because the complete `LFS_C1_EDIT_LOAD` observation is absent. A missing
  C1 edit log is **not** automatically zero on the whole-file streaming
  route. Clean/one-edit SDK performance controls retain individual
  **NOT_RUN** blockers: there is no public same-Workspace live-journal pin
  for the mandatory sequential SDK lifecycle. The public Workspace
  quick-control stage does not satisfy that fixture. Baseline matched
  control and the exact three-sequential-Exec/full mutations selections
  remain NOT_RUN. O_DIRECT is a request, not a VM/backend/device/
  host-cache guarantee; container phase cgroup fields remain unavailable.
- Reproduction: from source `52878a584`, run `checkpoint5_273.py oracle
  --repo "$PWD" --output .../iter-012/oracle`, `prepare --arm candidate
  --repo "$PWD" --oracle .../iter-012/oracle/oracle.json --output
  .../iter-012/prepared-candidate`, then `run --prepared
  .../iter-012/prepared-candidate/prepared.json --selection
  issue273-<pattern>-<count>-10m-v1 --output .../iter-012/<pattern>-<count>`
  *only on a new changed identity*, never resample this frozen attempt.
  The nine raw receipts, #248 attempt, both NOT_RUN blockers, six stage
  receipts, stdout/stderr, complete binary/image/product/harness/workload
  seals, phase resource counts and a **267-file** SHA manifest are retained
  under `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-012/`.
  No numeric control/candidate speed comparison or checkpoint-5 completion
  is qualified. Further work must resolve explicit zero C1 provenance
  without altering the frozen baseline product, obtain a supported public
  retained pin/ruling, complete unmatched selections, and account for the
  remaining generic hot-turnover/physical publication amplification and
  private cache/cgroup domains. Do not rerun the unchanged rows to select
  a more favorable wall.

### Research-first continuation checkpoint (documentation only)

The next agent's [final scaling research handoff](HANDOFF-FINAL-SCALING-RESEARCH.md)
proposes competing WRITE/Commit architecture and amortized-cost models before
further product changes. It retains iteration-012 raw receipts and all
historical qualifications; no additional performance arm, product change,
cache admission or release claim accompanies this documentation checkpoint.

### Prospective scaling decision record (documentation only)

The [source-pinned decision record](PROSPECTIVE-SCALING-DECISION.md) compares
changed-closure WRITE admission, physical owner/page publication and alternate
ordered Commit sources, with count/RAM/quota oracles, adversarial falsifiers,
format compatibility prerequisites and existing admission blockers. This is
**not** iteration 013: no product, harness, registered arm, Docker volume or
historical evidence changed; iter-012 remains the newest sampled candidate.

## Iteration 013 — generic WRITE representation count; extended C5 refusal

- Production telemetry / external test / architecture commit
  `6ea57701b910f3b1e64add84a77de3cdae15b1b4` (tree
  `6b86ddf888e5064d6e6dff930d466d5a9b7959a2`), first parent
  `e80d3cd288672f708f92fcb0dc0ebc7342802889`: counts only pages
  emitted from generic subtrees with no key updates. It changes no selected
  bytes, page format, Budget, hot slots, worker, cache policy or admission
  algorithm. Count is a lower-bound witness of representation-only page
  creation, **not** proof a page is safely avoidable. Production LOC by
  `python3 tools/production_loc.py --json --root <exact first-parent and
  staged/committed git archive snapshots>`: Core **67,719 -> 67,732 (+13)**,
  reference **65,417 -> 65,417 (+0)**, combined **133,136 -> 133,149 (+13)**.
  Architecture counter description co-committed. Extended *test-only* tier
  commit `91aca43f54627fe26100cc020042a9c5377ba1e4`, tree
  `51a4332630ca1e25ae3d00efed97bcdfc1f33cfa`, parent `6ea57701b`:
  Core **67,732 -> 67,732 (+0)**, reference **65,417 -> 65,417 (+0)**,
  combined **133,149 -> 133,149 (+0)**, same snapshot counter.
- Affected locked release Workspace and FUSE host tests PASS (macOS Linux
  gated tests are not host coverage), all-target warning-denying Clippy,
  fmt, boundary guard (350 files), tools 9/9 and Python route syntax PASS.
  Exact aarch64 release Linux `active_backing-f325d5782a2161dc` on an
  **owned ext4** volume with `TMPDIR=LAYERFS_ACTIVE_TEST_ROOT=/work`, serial
  tests **32/32 PASS**; volume removed. First combined host `--bins
  --example public_key` build was rejected because the example belongs to
  another package; retained CLI failure was corrected by a normal locked
  `--bins` build. No unchanged performance arm was resampled.
- New committed-identity public Workspace stage tests have
  `performance_claim=false`, `cache_claim=null`: changed-closure one-offset
  and alternating 48-WRITE groups both create exactly 144 index versions,
  48 directory and 48 pack pages, **0** representation-only pages and 0
  normalizations; full private and committed bytes and exact clean-close
  PASS. Dispersed 100/512/4,097 public source/count tests PASS full bytes,
  cleanup and Commit: seeks **1,519/7,699/61,327** (15.19/15.04/14.97
  per WRITE), index writes **390/2,866/27,780**, representation-only index
  pages **0/0/810**, and normalizations **38/425/4,785**. Hot/G1-G2
  continuation, 32 retained generations/refund and quota refusal separately
  PASS. These tests reuse a protected closed functional fixture by verified
  independent byte copy; no cold-cache, whole-command speed, phase-cgroup or
  matched control claim.
- Deliberate **nonregistered** 8,192-WRITE public diagnostic at test-only
  source `91aca43f5`: all 8,192 WRITEs acknowledged; source captured seeks
  **114,714** (14.00/WRITE), 62,117 index versions (7.58/WRITE), 3,154
  representation-only versions (5.1% of index), 8,192 pack writes, 11,206
  normalizations. Its SaveFile source line reports 8,192 final refs, 8
  windows, 824 pack loads and 2,472 index reads. **FAIL**: after a known
  canonical Commit result, local C5 reconcile returned `Capacity` on default
  Budget; final byte oracle, Workspace clean close and refund **NOT_RUN**.
  Exact failing allocation uninstrumented; O(E_f) reconcile patch/selected
  descriptors and concurrent charged scratch are plausible sources, not a
  proven diagnosis. Do not resample the same changed identity, enlarge
  Budget, or reclassify this as a passing 8,192 selection. Raw service/test
  stderr, failed result, retained Docker inspect/logs and explicit cleanup
  of **only its owned** container/volume are preserved.
- Raw public receipts, full commands, test-binary and fixture SHA seals,
  failure/cleanup evidence, guard/build logs, derivation script, nine unchanged
  iter-012 row statuses and the SHA manifest are in
  `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-013/`.
  [Decision record §8](PROSPECTIVE-SCALING-DECISION.md#8-subsequent-experimental-decision-product-telemetry-6ea57701b-extended-test-91aca43f5)
  states the restricted source bound and design choice: no demonstrated
  unbounded quadratic point-WRITE sweep; extra no-key rewrites are bounded
  locality/eviction costs, not obviously removable. Keep v2, investigate
  authenticated unchanged closure only if the counts justify it. The extended
  C5 memory-headroom failure is a separate open issue. Numeric matched
  comparison, full cache/cgroup proof and other missing selections retain
  their previous INELIGIBLE/INCOMPLETE/NOT_RUN statuses; PR #274 stays draft.

### Post-013 clarification and four-step owner handoff (documentation only)

The [iterative four-step handoff](HANDOFF-POST-ITER013-FOUR-STEP-LOOP.md)
responds to the owner's quota/Commit and registered scaling questions.
The **64 MiB** disk quota came from the external Stage fixture, not a product
numeric default. C5's ≥64.125 MiB `(updates.len()+32)*4096` operand is a
**pressure heuristic, not allocated/reserved backing**; it necessarily
selects the pressure scan at 8,192 but does **not** identify the eventual
`Capacity` refusal domain or site. The separate **8 MiB** RAM Budget is still
fixed. Correct incremental Commit unlinks/refunds G1 after final-owner/pin
release, while retaining live G2, metadata and old pinned bytes; no wholesale
quota reset is permitted. The 8,192 remote canonical Commit is known but
local reconcile failed, so no byte/clean-close refund proof exists there.
The registered 100/512/4,097 dispersed Exec/pages/normalization acceleration
still requires causal explanation; 810→3,154 no-key-subtree versions from
4,097→8,192 look near-quadratic *locally* without proving a global W² route
or that those selected ancestor pages are removable. Earlier source-bound
wording must not be read as declaring the generic WRITE factor closed. No
new product change, performance arm, qualified matched result or case status
accompanies this handoff.

## Iteration 014 — refusal-instant domain pinned; no algorithm selected

- First parent `9fce3e158ce69f5c36fb7cd4dd3ebd773dee9f7d`, product/observer,
  Stage-forwarding, external-parser and architecture commit
  `94ce9dec5247541d33602659ed1a77db0a4cd6f9` (tree
  `8d3b207f0bdbfb59bf8f1744a129854f547c8cc6`). This is **opt-in
  refusal-only telemetry**, not a fix, format change, workload change or new
  registered performance sample. `LFS_CAPACITY_DIAGNOSTIC=1` prints the
  failed request's Budget or shared physical Host domain, checked numbers and
  Rust caller; C5's separate post-unwind snapshot identifies known outcome,
  installed revision, physical charge, active revision/pages/pins. Absence of
  a log is not zero; the latter snapshot is not the refusal-instant charge.
  Strict external parser and active-quota-refusal public test separately
  prove a physical denial: allocated **2,097,152**, reserved **0**, request
  **4,096**, limit **2,097,152**, `active/pages.rs:383`; public bytes and
  clean close PASS. No test quota or 8 MiB product Budget was changed.
- One **new instrumentation identity**, nonregistered public Stage 8,192
  diagnostic, `admission_eligible=false`, **FAIL**. All 8,192 WRITEs were
  accepted; before Commit: 114,714 seeks, 62,117 index page writes,
  3,154 no-key-subtree versions, 11,206 normalizations, 8,192 pack writes.
  SaveFile emitted 8,192 refs, 8 windows, 824 pack loads, 2,472 index
  reads. Canonical remote Commit **known**; local C5 returned `Capacity` and
  `installed_revision=None`. The **actual refusal** is *memory Budget*
  `active/index.rs:316` at `scratch.resize(budget)`: instantaneous observed
  used **7,051,861** + additional request **2,472,699** = **9,524,560**,
  above **8,388,608** by **1,135,952 bytes**. This is an additional
  **charged scratch request**, not physical backing or proven resident RAM.
  Reconciliation's `prepare_file` fails *before* index candidate publication;
  `active_revision=8197` after unwind, no local C5 installation. The already
  known remote Commit must not be resubmitted. G2 continuation after this
  failure, final byte oracle, selected owner release, clean Workspace close
  and refund remain **NOT_RUN**.
- At the **post-unwind** snapshot, shared Host allocated **1,982,464**,
  reserved **262,144**, disk quota **67,108,864**; active 482 pages,
  `pinned_pages=0`, Budget used **2,070,893**. Inspected the **owned failed
  container** before removal: 484 backing files, `st_blocks*512` totals
  **1,982,464 bytes**. This is not the peak nor clean-close evidence.
  The `(updates.len()+32)*4096 >= 67,239,936` heuristic ensures pressure
  at this tier but is *not* the failing physical reservation. Pressure may
  still scan unrelated pack locators; this receipt does not give separate
  per-pack pressure reads versus attempted physical allocation.
- **Decision at step 2:** no proved winner. `index::prepare_file` charges
  `128 KiB + Σ(128+key.len()+value.len())` for candidate scratch on top of
  selected G1, mutable G2, charged `O(E+R)` deletion/ordered vectors and
  compaction pressure lists. Rough *pre-compaction* E+R scratch terms for
  100/512/4,097/8,192 point edits are respectively
  `128 KiB + ~300*W` = ~157/278/1,328/2,528 KiB; source changes and
  compaction can add keys. The actual 8,192 request is **additional** to
  an already charged 128 KiB. One cannot uncharge the scratch merely to
  make the old test pass: `Mutation` retains keyed nodes, page versions,
  cursors and cached pages; the simultaneous peak needs a distinct ownership
  proof before streaming/chunking or pre-admission. Physical demand is
  `allocated_old + reserved_old + 4096*new_pages`, not the pressure
  estimate; no 100/512/4,097 simultaneous peak proof is yet recorded.
  Option C (remove no-key parents) also lacks a fence/slot/epoch/pin
  equivalence proof. No limit raised; no uncharged spool or patch selected.
- Final-identity host locked release Workspace+FUSE tests PASS, warning-denying
  all-target release Clippy/fmt PASS, product boundary 350 files, tools 9/9,
  external parser 5/5. Exact aarch64 release Linux `active_backing` on **owned
  ext4** with `TMPDIR=LAYERFS_ACTIVE_TEST_ROOT=/work`, serial, 32/32 PASS;
  owned volume removed. The first host build attempt rejected an incorrect
  optional-payload access and was fixed before committing; the first cross
  build failed for Zig absent on PATH and was reissued using the installed
  `/opt/homebrew/bin/zig`. Keep both logs. Initial external parser invocation
  pointed at the service instead of the *Linux test* stderr and correctly
  reported a missing domain; corrected parser output is retained, not an
  invented zero. Stage public quota refusal PASS; extended 8,192 FAIL is
  retained; only its inspected owned container/volume were then removed.
- First-parent/staged/committed `python3 tools/production_loc.py --json
  --root <git archive snapshot>`: Core **67,732 -> 67,802 (+70)**,
  reference **65,417 -> 65,417 (+0)**, combined **133,149 -> 133,219
  (+70)**. Raw source/test/fixture/binary seals, commands, failures,
  diagnostics, observer logs, Docker inspection, quota arithmetic and own
  directory `SHA256SUMS` are **local gitignored** files at
  `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-014/`;
  they have **no GitHub raw-evidence URL**. Iter-012/013 receipts remain
  unchanged and separately hashed.
- **Next falsifier / step 1 again:** count the actual `index::prepare_file`
  scratch constituents and simultaneous charged owners at each registered
  100/512/4,097 tier, and distinguish pressure-scan pack reads from actual
  physical attempts at 8,192; classify changed-key, necessary connection,
  hot admission/eviction, carry/height and pinned index versions *per WRITE*.
  Only then choose a charged reconciliation bound/streaming design (or seek an
  owner supported-limit ruling) with same-commit architecture and custody
  proof. The registered nine-cell frozen-identity evaluation at this new
  telemetry identity is **NOT_RUN**; historical iter-012 nine verified rows
  stay **INELIGIBLE**. #248 C1-zero is **INCOMPLETE**; same-Workspace SDK pin,
  exact three-Exec/mutations and frozen matched control are **NOT_RUN**;
  private cache/phase-cgroup admission is **INELIGIBLE/NOT_RUN**. No numeric
  comparison, release gate, universal scaling bound, merge or issue closure.

### Iteration 014 supplementary public functional counts (same product, later docs HEAD)

After committing the first iteration-014 ledger entry (`ba808dfc4`; **docs
only**), six additional one-per-case public Stage diagnostics used the
unchanged `94ce9dec5` product/test binaries and fixture, with `capacity_diagnostic=true`,
independent closed-fixture copies, `performance_claim=false` and
`cache_claim=null`. Their driver records correctly name the later docs HEAD
rather than relabelling the earlier 8,192 attempt. Dispersed 100/512/4,097
all PASS full bytes, Commit and clean-close; WRITE counters respectively:
seeks **1,519/7,699/61,327**, index page writes **390/2,866/27,780**,
representation-only **0/0/810**, normalizations **38/425/4,785**,
pack writes **100/512/4,097**. These repeat the source counts from a
*changed telemetry identity*, not a matched or eligible speed resample;
per-page role/fence/eviction/pin classification is still **INCOMPLETE**.
Hot-publication, G1/G2 hot continuity and 32-retained-generation/refund
public routes also PASS under this product. None proves the failed 8,192
post-known-Commit final bytes, continuation or clean-close refund. The
corrected local `iter-014/RESULTS.json` and its own-directory `SHA256SUMS`
include these distinct source-head rows and all earlier FAIL evidence;
no GitHub URL is claimed for the gitignored raw records.

## Iteration 015 — four-tier C5/WRITE causal RCA (no algorithm or bound change)

- Prospective product **observer-only** commit
  `512742de6d84e9c3ebb299964b405c766bf7dca8` (first parent
  `99008d31c0726f03c5c385ccb94760bad9316049`): charged C5 owner
  checkpoints, pressure P-scan/pack-read/physical-pack-attempt separation,
  and one disjoint leaf/branch/direct/no-key/root-height page-cause record
  per **prepared** index revision, with selected height, hot occupancy and
  capture/frozen-revision counts. The [active backing architecture](../../architecture/proposal/fuse-workspace-snapshot-overlay/60-active-backing.md)
  describes counters in this **same product commit**. Production LOC from
  identical first-parent/staged/committed `python3 tools/production_loc.py
  --json --root <git archive snapshot>`: Core **67,802 -> 67,965 (+163)**,
  reference **65,417 -> 65,417 (+0)**, combined **133,219 -> 133,382
  (+163)**. No limit, format, allocation algorithm or canonical route changed.
  The follow-up external parser/docs correction `0d7f4127b` (first parent
  `512742de6`) changes production LOC **133,382 -> 133,382 (0)**,
  Core **67,965 -> 67,965 (0)**, reference **65,417 -> 65,417 (0)**.
  The first parsed 100-WRITE diagnostic remains on disk: the parser initially
  included one *pinned prior C5* generation-2 candidate and forgot that
  `StoreStatus.index_page_writes` includes **HotDirectory** pages; the
  correction excluded that pre-WRITE revision and added directory pages to
  its cross-check. **No unchanged Stage attempt was resampled.** A prepared
  page is not an ACK; the external observer checks the contiguous WRITE
  revisions against the public count/byte oracle before drawing a count
  conclusion.
- Exact release host Workspace+FUSE tests PASS, all-target warning-denying
  Clippy/fmt PASS, product boundary 350 files, tools 9/9, parser 4/4.
  Exact aarch64 release `active_backing` **32/32 PASS** on an owned ext4
  volume with `TMPDIR=LAYERFS_ACTIVE_TEST_ROOT=/work` and one test thread;
  volume removed. New-source single public Stage dispersed 100/512/4,097
  each PASS Commit, full bytes and clean-close refund; changed-closure,
  hot-publication, G1/G2 hot continuity, 32-pin retirement/refund, quota
  refusal and 4,097-record quick controls also PASS. All runs have
  `performance_claim=false`, `cache_claim=null`, 8 MiB Budget, unmodified
  quota, one worker and protected independent fixture copies; their
  instrumented walls are **not** speed samples.
- **C5 simultaneous charged owners**, in bytes, at the last public Commit
  of each tier. `before` retains selected G1, mutable G2 and other Host
  owners, *not* an independently measured allocator peak. The row/deletion,
  map and ordered charges overlap this baseline and one another. The last
  column is Budget used before index prepare **plus its full proposed index
  scratch** (128 KiB already charged before the additional resize); a PASS
  at 100/512/4,097 does not imply this is the later candidate peak.

  | Dispersed WRITEs | Budget before C5 | Rows + deletion charge | Update-map charge | Ordered scratch | Before index | Full index scratch | Proposed charged total |
  | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
  | 100 | 1,939,200 | 448 + 38,592 | 45,343 | 14,640 | 2,038,721 | 176,689 | 2,215,410 |
  | 512 | 1,959,902 | 448 + 196,800 | 228,683 | 74,208 | 2,461,784 | 360,714 | 2,822,498 |
  | 4,097 | 2,036,588 | 448 + 1,544,064 | 1,801,823 | 585,264 | 5,981,135 | 1,940,019 | 7,921,154 |
  | **8,192 diagnostic** | 2,071,290 | 448 + 1,573,248 | 2,458,588 | 791,568 | 6,920,789 | 2,603,771 | **9,524,560 FAIL** |

  At 8,192, the 16,386 deletion keys retain 1,573,248 charged bytes;
  there are 16,491 ordered updates. `128 KiB + 128*16,491 + 361,435
  key bytes + 416 value bytes = 2,603,771` index scratch. Budget used
  **7,051,861** with the initial 128 KiB charged; the requested additional
  **2,472,699** exceeds the **8,388,608** limit by **1,135,952**. This is
  an intentionally charged *scratch bound*, not proof of simultaneous RAM
  allocation or permission to remove the charge. `Mutation::change`
  traverses borrowed sorted updates but still stages cloned leaf/branch
  cells, pages, cursor bindings and split groups; proving exact overlapping
  RAM for every path (including an empty selected root and failures) is the
  next falsifier before narrowing this bound or streaming the patch.
- **C5 physical versus pressure:** at 100/512/4,097, the heuristic
  `(updates+32)*4096` is **1,380,352 / 6,463,488 / 50,073,600** bytes,
  below observed remaining physical headroom **66,797,568 / 66,625,536 /
  65,441,792**, so no pressure P pass. At 8,192 it is **67,678,208**,
  above **64,864,256** remaining; pressure scans **103** P locators in **2**
  paginated index scans. All 103 were already in the touched logical set;
  it makes **0 `pack.records` reads**, selects **0** relocation source packs
  and makes **0** compaction pack-page physical creation attempts. No
  unrelated pack read or physical compaction allocation caused this
  particular denial. The separate SaveFile's **824 pack loads** are not
  pressure reads. This does **not** disprove a future pressure-scan cost for
  other cases or prove a peak physical charge: the inspected failed Docker
  backing again has 484 files and `st_blocks*512 = 1,982,464` *after*
  unwind, with **262,144** Host reserved. It is not Workspace clean close.
  The index scratch fails before candidate/index physical publication;
  known canonical remote Commit remains in custody and local
  `installed_revision=None`. 8,192 final bytes, continuation and refund
  **NOT_RUN**; this raw FAIL is retained alongside iter-013/014.
- **Every public dispersed WRITE revision was counted once**, including
  root-height/capture/slot occupancy; the extra pinned prior-base C5
  candidate was excluded. All accepted WRITE revisions have
  `(captured,frozen_revisions)=(0,0)`; the later C5 index-preparation records
  `(1,1)`. Categories are **causes of creating staged pages**, not a proof
  of mandatory or removable parent pages:

  | WRITEs | Changed-key leaf | Changed-subtree parent | Root-height pages | Hot-normalization no-key | Hot admission / residual no-key connection | Directory pages | Total index+directory |
  | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
  | 100 | 230 | 59 | 1 | 0 | 0 / 0 | 100 | 390 |
  | 512 | 1,882 | 471 | 1 | 0 | 0 / 0 | 512 | 2,866 |
  | 4,097 | 16,425 | 6,446 | 2 | 810 | 0 / 0 | 4,097 | 27,780 |
  | **8,192 diagnostic** | 35,465 | 15,304 | 2 | 3,154 | 0 / 0 | 8,192 | 62,117 |

  All four dispersed groups use the **generic** path (`direct=false`), so
  the direct hot-carry class is zero; generic split/carry *events* are not
  separately counted by this observer and remain **INCOMPLETE**. Height
  rises only at WRITE **16** (0->1) and **1,416** (1->2). No-key versions
  first appear at WRITE **2,659**, exactly **one per affected WRITE**;
  8,192's first 4,097 WRITE-role records match the shorter tier
  byte-for-byte. The 1,024-WRITE no-key increments after the threshold are
  **223, 586, 586, 586, 586, 587** (last short prefix boundary accounted
  separately in raw `RCA.json`), rather than an ever-rising per-WRITE
  count: the finite 810->3,154 near-quadratic-looking ratio crosses a
  **turnover threshold**, not evidence of global unbounded W². The hot
  occupancy peaks **4/5/8/10**, well below 64 slots; the no-key class is
  a hot node marked for normalization by the generic path, not a proven
  global-slot-capacity eviction. At 4,097 no-key WRITE occupancy
  transitions are **8->7 on 412**, **8->8 on 398**; same-occupancy may
  clear and re-admit a slot, and selected fences/epochs/pins still need a
  necessity proof before skipping any page. Direct/hot functional controls
  separately PASS with zero no-key pages; they are not the registered
  append/repeated mounted performance arms.
- **Next falsifier / honest admission:** identify why hot normalization
  chooses a no-key ancestor at the 2,659 threshold, including generic split
  count and exact fence/slot/epoch/pin equivalence, before choosing C;
  account the allocator-live and Budget peak *after* successful index
  staging at 4,097 and prove an appropriately charged narrower scratch or
  a streaming candidate before choosing A/B. If required old+new owners
  cannot fit fixed 8 MiB, retain explicit refusal and ask an owner profile
  ruling rather than changing the quota. No WRITE/Commit algorithm patch
  was proved by this RCA. Append/repeated three-tier per-WRITE roles,
  registered nine-cell changed-identity performance, same-Workspace SDK
  pinned controls, #248 explicit C1-zero, private cache/phase-local cgroup,
  control comparison and general scalability/speed admission remain
  **NOT_RUN/INCOMPLETE/INELIGIBLE**. PR #274 stays draft; #273 stays open.
- Raw `RCA.json`, `RESULTS.json`, a per-WRITE `write-causes.json` for every
  dispersed tier, preregistration, exact commands, independent seals,
  retained first-parser FAIL, failed 8,192 receipt, owned failed-container
  inspection/removal and checksum manifest are **local gitignored** files
  at `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-015/`.
  Verify `SHA256SUMS` *from its directory*. No GitHub raw-evidence URL exists.

### Iteration 015 RCA addendum — source-proven *charged* map-node overlap

The original C5 owner table above reports real **Budget charges**, not
allocator-live memory. Further source audit of `lifetime.rs::publish_reconcile_map`
identifies a specific avoidable *accounting overlap* after
`updates.into_iter().collect::<Vec<_>>()`: every original `BTreeMap` node has
been consumed/deallocated, while `_updates_charge` still retains its
`128 bytes/entry` map-node allowance; separately `_scratch` now charges
`48 bytes` per ordered `(Vec<u8>, Option<Vec<u8>>)` tuple. The keys/values
were **moved**, not freed; their actual retained capacities, the ordered
Vec's actual capacity, mutation staging and selected G1/G2 still require
charging. This is not permission to drop `_updates_charge` wholesale or
remove the index scratch estimate on a guess.

For the observed last Commit in the four public dispersed tiers, initial
map entries are exactly `deletion_keys+2` (one dirty key and one inode key).
The archived charge matches
`128*(deletion_keys+2) + deletion_key_bytes + 17 + 9 + 416`
**exactly** at each tier:

| WRITEs | Initial map entries | Map-node-only charge retained *after* move | Hypothetical proposed total after safely transferring just freed map-node allowance |
| ---: | ---: | ---: | ---: |
| 100 | 303 | 38,784 | 2,176,626 |
| 512 | 1,539 | 196,992 | 2,625,506 |
| 4,097 | 12,141 | 1,554,048 | 6,367,106 |
| **8,192 diagnostic** | 16,388 | **2,097,664** | **7,426,896** |

The last total is **961,712 below** the unchanged 8 MiB Budget *at the
previous refusal boundary*, not a proof that subsequent candidate staging
or owner cleanup succeeds. The old map node allocation has ended, so this
is a narrower, source-backed version of design A: prospectively precharge
and verify the **actual old+new Vec capacity** during conversion, retain the
moved key/value buffer capacities and any separate compaction additions,
then resize the map's charge only after old nodes have been consumed. Prove
charged peak and post-canonical failure custody at every tier before an
algorithm commit or new diagnostic. If it still cannot fit, refuse and seek
an owner profile ruling. The derivation script's first attempt misspelled a
raw field, **FAIL** in the local evidence; its corrected arithmetic and all
raw public attempts are hashed in iter-015. No additional Stage attempt was
run for this addendum and no historical failure was replaced.

## Iteration 016 — charged map-node transfer; extended default-budget lifecycle PASS

- **Source-bound algorithm and failure proof:** product/architecture/test commit
  `ee4032e2dd389003cdb37ee781e3937d6b656a24`, first parent
  `f9b6847a14c57da70a6e0d414f3d236c7c4d9838`. C5 precharges the
  ordered tuple request against the *still-live* `BTreeMap`, uses fallible
  `try_reserve_exact`, then charges the Vec's **actual capacity** before
  draining the map. Only **after** all old BTreeMap nodes are consumed does
  it retain the *actual moved key/value buffer capacities* in the old map
  `Charge`. The ordered tuple charge and existing index scratch remain;
  no selected G1 page, live G2 extent, pin, backing format, quota, Budget
  limit or canonical Commit identity changes. Every fallible precharge,
  over-capacity adjustment and buffer-charge resize still aborts the
  compaction candidate and leaves the index unpublished; the known canonical
  outcome remains owned. The [architecture source description](../../architecture/proposal/fuse-workspace-snapshot-overlay/60-active-backing.md)
  and exact bound are co-committed. Generic split events/new split-group
  pages and direct carries now have separate optional per-revision **v2**
  counters, overlapping—not added to—the existing disjoint page roles.
  No normalized page was removed.
- Exact first-parent/staged/committed `python3 tools/production_loc.py --json
  --root <git archive snapshot>`: Core **67,965 -> 68,022 (+57)**,
  reference **65,417 -> 65,417 (+0)**, combined **133,382 -> 133,439
  (+57)**. The explicit *test-only* refusal-control correction
  `c70af60b176f41068caf97ae0fb1554f4486ea66` (parent `ee4032e2d`)
  and later test-only assertion correction
  `784a337e8c50d1ec0a4053b296bfcb0a1a964d84` (parent `c70af60b1`)
  each have Core **68,022 -> 68,022 (+0)**, reference **65,417 ->
  65,417 (+0)**, combined **133,439 -> 133,439 (+0)** under the same
  counter and first-parent/staged scope. They are *different test identities*
  but the product-input seal is unchanged.
- One new product-identity **nonregistered** public 8,192 dispersed
  count/functional diagnostic with **unchanged** 8 MiB product Budget and
  64 MiB fixture disk quota: **PASS** all 8,192 accepted WRITEs, SaveFile,
  known canonical Commit, **installed local C5**, full **8,194-byte**
  independent remote byte oracle and exact Workspace clean-close physical
  refund. Eight bounded SaveFile windows still use 824 pack loads; the C5
  pressure pass still scans 103 P locators in two index scans, with **zero**
  pressure pack-record reads and **zero** compaction pack-page creation
  attempts. The changed charge does not hide those counts. Instrumented
  functional command wall **25.804 s** is **not** a registered performance
  sample or a 25-second speed PASS. The old iter-013/014/015 default-budget
  FAIL receipts retain their original source identities and status; this is
  a **new product identity**, not an unchanged rerun or historical repair.
- Actual C5 charge transfer and proposed index scratch boundary (bytes):

  | Public dispersed WRITEs | Old patch-map charge | Actual moved key+value capacities | Actual ordered Vec capacity (entries) | Budget after transfer | With full index scratch | Remaining below 8 MiB |
  | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
  | 100 | 45,343 | 6,577 | 305 | 1,999,955 | 2,176,644 | 6,211,964 |
  | 512 | 228,683 | 31,754 | 1,546 | 2,264,855 | 2,625,569 | 5,763,039 |
  | 4,097 | 1,801,823 | 248,243 | 12,193 | 4,427,555 | 6,367,574 | 2,021,034 |
  | **8,192 diagnostic** | 2,458,588 | **361,851** | 16,491 | **4,824,052** | **7,427,823** | **960,785** |

  At 8,192, the measured old-to-moved charge release is **2,096,737
  bytes**, 927 fewer than the earlier 2,097,664-byte *node-only*
  estimate because the **actual** key capacities include compaction/reclaim
  additions. The existing index scratch is still **2,603,771**; the
  current transfer actually admits it, completes staging and publishes.
  These are charged-owner checkpoints, not a measured allocator RSS peak.
  No global quota reset occurs; G1 owners refund after final pins, while
  G2, metadata and old readers remain charged until their own release.
- New-source public 100/512/4,097 dispersed tests each PASS full bytes,
  Commit and clean-close. The per-WRITE disjoint roles remain those in
  iter-015, with **0/0/810/3,154** no-key normalization pages and
  **390/2,866/27,780/62,117** total index+directory page writes at
  100/512/4,097/8,192. Distinct generic split **events** are now
  **5/42/285/375**, producing **10/84/570/750** split-group pages;
  the latter **overlap** changed-key/normalization roles and must not be
  added to page totals. All covered dispersed WRITEs use the generic
  route and record **zero direct carries**, but direct/hot public controls
  separately show carries and PASS old/new bytes. Every dispersed WRITE
  records `captured=frozen_revisions=0`; C5 retains its selected pin.
  Hot/G1-G2 continuity, 32 retained generations/refund, changed closure,
  hot publication, quota refusal and no-pin quick controls PASS on the
  changed product. These functional controls are not real same-Workspace
  SDK pinned-journal performance controls.
- **Separate failure-custody control; every nonpassing attempt retained.**
  An explicit *test-only*, nonregistered 2,550,000-byte Budget first
  refused a hot-cursor **ordinary WRITE** around revision 93, **before**
  canonical Commit: **FAIL**, no post-known claim. After a test-only
  profile change to 2,650,000 bytes (never used for the default-budget
  8,192 case), all 512 WRITEs and remote canonical Commit succeeded;
  local C5 then refused `Capacity` at `active/splice.rs` with
  `installed_revision=None`. Full-byte checks of **three declared offsets**
  in the continuing Workspace and known remote root PASS, but a subsequent
  attempted G2 WRITE also refused `Capacity`: second test attempt **FAIL**
  against its stronger progress assertion, not a hidden G2 success. The
  final *assertion-only* test identity keeps the same 2,650,000-byte
  workload and explicitly checks this bounded **G2 refusal**, unchanged
  local revision/bytes, known remote bytes, retained physical charge and
  **no blind canonical retry**: both scoped controls PASS. They do **not**
  prove a *successful* new G2 mutation after a known local failure, full
  final byte oracle for this low-budget control, or a clean Workspace close;
  those are **NOT_RUN**. Both failed controls' owned containers/volumes
  were inspected/logged and removed separately; that Docker cleanup is
  not Workspace close. Fixed 8 MiB extended support is evidenced by the
  distinct successful 8,192 control, never by changing this refusal
  control's Budget.
- **Normalized ancestor boundary:** the v2 `change` route only emits an
  update-free branch after reconstructing **different** child targets/
  fences; `emit` checks kind/slot epochs, while a frozen selection retains
  its old root/directory until its selectors release. Substituting the
  old page into the **new** selected view without a new mapping would lose
  changed-child resolution or resolve a cleared/reused hot epoch. This is
  a source proof that a *naive skip* is invalid in today's grammar, **not**
  a fence/epoch/old-pin equivalence proof for a new bypass algorithm.
  No bypass was attempted; that proof remains **INCOMPLETE** before any
  future removal. Finite source counts cannot establish universal WRITE
  scaling or a numeric speed ratio.
- Verified locked release host Workspace+FUSE tests, warning-denying
  all-target Clippy/fmt, product boundary 350 files, tools 9/9, parser
  5/5, exact aarch64 release Linux active-backing **32/32** on owned ext4
  with `TMPDIR=LAYERFS_ACTIVE_TEST_ROOT=/work`, one thread and owned
  volume removed. The Linux Stage binary is rebuilt for each changed
  external test identity, while product binary/seal stays fixed after
  `ee4032e2d`. Every run uses protected closed fixture copies, one worker,
  `performance_claim=false`, `cache_claim=null`. No unchanged-arm speed
  resample or frozen-control comparison was taken. The append-only local
  gitignored `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-016/`
  retains `RESULTS.json`, per-WRITE causal rows, prior FAIL receipts,
  commands, hashes, failed owned Docker inspections and own-directory
  `SHA256SUMS`; it has **no GitHub raw-evidence URL**.
- Numeric matched control, nine registered mounted selections at this
  product/harness identity, exact three-sequential-Exec/mutation selections,
  #248 explicit C1-zero provenance, same-Workspace SDK pinned controls and
  independent cache/phase-cgroup evidence remain **NOT_RUN/INCOMPLETE/
  INELIGIBLE**. Extended 8,192 success is a scoped functional result,
  not checkpoint-5 admission or permission to close #273/merge draft PR
  #274. Next falsifiers: successful G2 edit after a known local refusal
  with sufficient admitted headroom; true allocator/phase-local peak;
  generic no-key bypass equivalence (if ever proposed); other registered
  append/repeated patterns and matched frozen control under eligible
  cache/cgroup identities.

## Iteration 017 — recoverable known-C5 failure admits a new G2 WRITE; nine public matrix cells

- **Source/status:** no product, quota, Budget, format, Workload, limit or
  page-removal algorithm change from iteration 016's `ee4032e2d` product.
  Three append-only *external-test-only* identities:
  `80dae1db092aa6f8c72ce2173c541afe4f15ef9b` (parent
  `1ec0e18d8`), `89b2caa9afff14503a0cf153811c7fce15064922`
  (parent `80dae1db0`) and `de25a10a48cde146d7ee1f25374c126b71b3b3f2`
  (parent `89b2caa9a`). Each has identical first-parent/staged/committed
  `python3 tools/production_loc.py --json --root <git archive snapshot>`:
  Core **68,022 -> 68,022 (+0)**, reference **65,417 -> 65,417 (+0)**,
  combined **133,439 -> 133,439 (+0)**. The later checkpoint harness is
  byte-identical for all nine rows, source HEAD `de25a10a4`, product seal
  pinned separately from test-binary seal. These test identities do not
  retroactively repair iteration-016's independent low-Budget or 8,192
  receipts.
- **Failure-custody hypotheses competed publicly, each attempt retained.**
  At the fixed **8 MiB** Budget/64 MiB fixture disk quota, external
  `Gate::CommitCompletionFailure` let the canonical Commit succeed, then a
  Linux file-size limit refused a physical C5 backing page. The first
  public Stage attempt `active_known_c5_g2` **FAIL** its proposed new G2
  WRITE: `PageStore::create_from` stops after the allocation error because
  physical ownership can be ambiguous; restoring the OS limit does **not**
  make the stopped backing safely writable. It nevertheless observes a
  *known* remote Commit, `installed_revision=None`, and preserved old/
  canonical bytes; its inspected failed owned Docker resources were
  removed after retaining logs. The corrected test-only oracle on the
  same physical-fault workload has two scoped PASS checks: new G2 WRITE
  explicitly returns `Busy`, old bytes persist, failed owners stay
  charged and no canonical retry occurs. **No G2-progress claim** is
  made for physical failure. Allowing writes on a stopped backing solely
  to satisfy this gate would violate custody.
- **Recoverable memory-Capacity route at the *unchanged default* Budget.**
  A separately preregistered **nonregistered** 10,240-WRITE public Stage
  control into a 20,480-byte file accepted all WRITEs; after one **known
  remote canonical Commit**, local `active_reconcile.rs:132` refused a
  **3,641,243-byte** update-map request with Budget already **4,825,906**:
  required **8,467,149 > 8,388,608 by 78,541 bytes**. No local C5
  installation (`installed_revision=None`); after unwind Budget
  **2,106,546**, Host allocated **2,867,200**, reserved **262,144**.
  This refusal precedes index/owner publication and leaves the backing
  operable; it is *not* an 8,192 regression or a disk-quota failure.
  The first test identity **FAIL** because one 20,480-byte private READ
  exceeded its unchanged 10-second per-call deadline *after* the known
  canonical result. The corrected, prospectively recorded test-only
  identity kept the exact 10,240 WRITEs, Budget, 60-second Stage command
  bound and every READ's 10-second limit, instead verifying **all**
  20,480 private bytes in contiguous 1,024-byte public READ calls. It
  **PASSes** full independently checked remote G1 and old live bytes,
  then **ACKs one new G2 WRITE on the same attached Workspace/open handle**.
  New live byte `Z` differs from the **unchanged** committed G1 byte;
  remote canonical Commit count remains exactly **two** (base + G1), with
  no resend. Backing `st_blocks*512` matches shared Host allocated
  charge after the new mutation. Complete functional command
  **50.932 s < 60 s Stage limit**, not a registered 15/25-second
  performance row; no warmup or numeric speed claim. The known failed
  owner remains charged and `close_clean` is `Busy`: clean-close refund
  **NOT_RUN** for this intentionally retained failure. This proves
  **post-known-failure G2 mutation progress for recoverable Budget
  Capacity**, not unconditional progress after every failed physical
  owner. The raw first FAIL and physical-fault FAIL remain separate.
- **One new changed-identity candidate attempt for each registered
  append/dispersed/repeated × 100/512/4,097 mounted selection** (not an
  unchanged-arm speed resample). All **nine** PASS full independent
  byte oracles, correct WRITE counts, exactly four anchored WRITE samples,
  known Commit, charged-resource and cleanup checks, each 15/25-second
  complete bound. They are all **row_status=INELIGIBLE** for numeric
  admission: no frozen control match, no runtime private container-cache/
  VM/backend/device/host proof and no phase-local cgroup fields. Host
  whole-input cache status PASS does not repair those other domains.
  Commit SaveFile pack loads reproduce source-bound operands append
  **2/7/52**, dispersed **2/7/209**, repeated **1/1/1**. New raw walls
  are in local receipts *without* ratios or a speed claim; frozen
  #271's censored 25-second rows are not phase denominators. Reused closed
  #271 masters came by verified independent byte copies, with one worker,
  exact release binaries/image seals, no mutated-sample or warm-input
  reuse. No second performance attempt was taken in any changed arm.
- **Remaining registered and admission receipts are explicit.** The
  original `issue248-separated-4097-v1` gate on this changed source
  has full-byte/cleanup/oracle PASS but `row_status=INCOMPLETE`:
  `LFS_C1_EDIT_LOAD` is absent. `LFS_C1_SAVE_COUNT nodes_read=0` and
  `LFS_FILE_INPUT` are distinct source observations, not proof the
  missing edit-load equals zero. The clean/one-edit SDK selections each
  emit a **NOT_RUN** blocker: no public same-Workspace live pinned
  4,097-record journal across required sequential Exec/Commit (a
  detached Store cannot substitute). A matched #271 control requires
  a separate *owned* checkout with unchanged frozen product and common
  observer plus identical independent cache/phase-cgroup capabilities;
  under this assigned-worktree-only instruction it is **NOT_RUN**.
  The current driver reports `container_cgroup_memory=None`, and
  requested O_DIRECT is not a complete private cache/VM/backend/device/
  host guarantee: private cache/cgroup numeric admission **INELIGIBLE**.
  Exact three-sequential-Exec/full mutations remain **NOT_RUN**.
  No 2×/10×, global scaling, numeric control or release claim; no
  normalized ancestor removal, whose alternative fence/epoch/old-pin
  equivalence remains **INCOMPLETE**. Draft PR #274 and open #273
  stay untouched pending owner ruling on SDK pin and independent
  cache/cgroup/control capability.
- **Checks and local raw evidence:** final test-identity exact aarch64
  release Linux Stage binary on owned ext4 PASSes both recovered-custody
  checks, host locked release Workspace+FUSE tests and warning-denying
  all-target Clippy/fmt PASS, product boundary 350 files, tools 9/9,
  parser 5/5, checkpoint self-check and harness 6/6 PASS. The product
  Linux `active_backing` **32/32** proof from iter-016 is reused only by
  unchanged product seal, not replayed as a new attempt. The protected
  oracle, prepare, nine candidate receipts, #248 INCOMPLETE, two SDK
  blocker files, all Stage FAIL/PASS outputs, named Docker inspections,
  phase/resource fields, exact commands, binary/image/fixture/source
  seals and per-directory/outer SHA manifests are **gitignored local**
  `benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-017/`.
  Verify each manifest from its own directory. No GitHub raw-evidence URL
  exists. The next agent must not resample this unchanged candidate or
  reissue its known canonical Commit; pursue unsupported SDK capability/
  owner ruling and matched private cache/cgroup control only under a
  prospectively authorized independent identity, keeping PR draft.

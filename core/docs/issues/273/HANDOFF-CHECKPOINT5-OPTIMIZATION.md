# #273 checkpoint 5 optimization handoff

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
>
> **Continuation update after `c113656b470998d0268e2170aaef0f501751d485`:**
> The original status and step-by-step plan below are a dated historical
> handoff, **not** the current implementation state. Read
> [§10](#10-current-continuation-mandate-all-nine-matrix-cells) and the
> [optimization log](CHECKPOINT5-OPTIMIZATION-LOG.md) first. The original
> baseline and historical receipts remain unchanged and unqualified. This
> documentation update does not change product source, rerun a performance
> arm, qualify checkpoint 5 or modify the draft PR's disposition.

> **Historical original handoff status:** This owner-authorized implementation
> task was written against `073f374300ad2f4acf2b0ab1818ad9cae4f55fba`.
> At that earlier checkpoint no optimization had been implemented and no new
> benchmark sample had been taken; §10 now supersedes those starting facts.

## 1. Execute the work; use an iterative cycle

The owner's instruction is:

> Work on the optimization and re-run the test. Work in an iterative cycle:
> optimize/fix -> run tests -> record result -> synthesize, until no bad scaling
> factor is found.

Implement the fixes below and execute that cycle. Do not stop at another audit,
a proposed plan, or a benchmark report without product changes. Continue through
the known causes and any further avoidable scaling/amplification found by the
diagnostics. Section 8 defines a reviewable stopping condition within the covered
workloads; finite tests cannot establish every possible workload.

This explicit owner instruction authorizes targeted tests after each meaningful
change and supersedes the local default against iterative verify/test loops.
It also authorizes prospective measurements of changed candidates. It does
**not** authorize resampling an unchanged performance arm to get a better number,
warm-cache credit, changing limits/workloads, or rewriting historical receipts.
Correctness tests and labelled count-driven diagnostics may repeat when a fix,
new hypothesis or expanded coverage justifies them. Keep one performance sample
per case/arm at each declared relevant product/harness/artifact identity.

This prompt replaces the execution task and stale `NOT_RUN`/keep-product-frozen
directions in the earlier checkpoint handoff. Preserve those historical
documents and their evidence. The old candidate remains an archived reference;
the owner now authorizes optimizing it and freezing a new candidate.

## 2. Checkout, authority and scope

| Item | Identity |
| --- | --- |
| Worktree | `/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs` |
| Branch | `codex/issue273-active-head` |
| Origin | `https://github.com/Ephemeral-AI-Lab/layerfs.git` |
| Draft PR / issue | [PR #274](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274), [#273](https://github.com/Ephemeral-AI-Lab/layerfs/issues/273) |
| Source inspected for this prompt | `073f374300ad2f4acf2b0ab1818ad9cae4f55fba` |
| Current product before optimization | `a2359620a7966314fbb2a96c98e8da958df72c6c` |
| Frozen functional proof source | `4ae36ad3a9c70b32b66c8280ac9496e9f1e345a9` |
| Original baseline product | `48b51e874a41b3e1e6c6661e145316df8b408f07` |
| Baseline checkout | `/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs` |

Check actual HEAD, worktree status, attached artifacts and active build/run
processes before editing. The handoff documentation commit advances HEAD without
changing product source. One writer/Cargo owner per worktree; use only its own
target directory. Do not interrupt another owner. Attach the existing PR when
continuing it in a new chat. Keep it draft; merging/closing is not authorized.

Read before edits:

- [Repository rules](../../../../AGENTS.md), [Core rules](../../../AGENTS.md),
  [benchmark rules](../../../../docs/general/benchmark_rules.md),
  [benchmark-tree rules](../../../../benchmark/AGENTS.md),
  [Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md),
  [build/reuse mechanics](../../../../benchmark/fs-bench-pro/QUICKSTART.md),
  [release policy](../../../../docs/general/release-policy.md) and
  [documentation policy](../../../../docs/general/documentation-policy.md).
- Product authority: [phase-4.5 implementation spec](PHASE4.5-IMPLEMENTATION-SPEC.md)
  -> [research audit](PHASE4.5-RESEARCH-AUDIT.md) ->
  [phase log](PHASE4.5-LOG.md) -> repository/Core rules.
  Read the [v1 format/evaluation record](ACTIVE-FORMAT-AND-EVALUATION-v1.md):
  active index/directory attachments are v2; pack pages stay v1.
- [Root-cause research](CHECKPOINT5-ROOT-CAUSE-RESEARCH.md),
  [derived arithmetic/input hashes](evidence/checkpoint5-root-cause/DERIVED.json),
  [checkpoint log, especially section 12 onward](CHECKPOINT5-LOG.md),
  [retained results](evidence/checkpoint5/RESULTS.json) and
  [execution specification](../../../../docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md).

The root-cause research qualifies earlier reports: hashing follows the last
cold-input check, private Linux files request O_DIRECT in both arms, and quick
controls did not retain the required live private journal. All campaign-3 rows
remain unqualified. Its 303 checksummed attempt files are intact. Do not use
the older report's cold-Exec claim or blanket private-page-cache explanation.

Scope is #273 active backing/public FUSE WRITE/Commit source/reconcile and the
harness needed to evaluate them. Preserve #264/PR #269 files and ownership;
do not duplicate mounted ancestry work or add a 256-node/128-dirty cap. Do not
implement concurrent SDK Exec, command leases, multiple mounts or deadline-free
commands (#249). Do not change canonical identity, protocol semantics or C1/C2
construction algorithms to disguise a backing regression.

## 3. Start here: measurement correction, then the shared reconcile helper

### 3.1 Make new observations interpretable

Start in [checkpoint5_273.py](../../../benchmark/fs-bench-pro/checkpoint5_273.py),
`cache_files`, and its caller. Current order is:

```text
evict -> residency check -> buffered digest of the same input -> launch
```

Change it to:

```text
hash/validate identities of all inputs -> evict all inputs
    -> final whole-input residency check -> launch with no intervening input read
```

Retain the existing host invalidation/residency machinery. Add a focused external
ordering regression test that would fail on the original sequence, including
the multiple-input case. Record method, identities, final residency and launch
gap. Identity validation must not reopen/read payload after the final check.

Commit a prospective correction to the execution specification and a new
receipt/schema/cache-contract version before new performance collection. Keep
the registered workloads and limits; distinguish any necessary new scenario
version rather than relabelling old rows. Check the existing CLI parser: this
checkpoint runner has no generic `--setup clone` or `--perf-fast` switch.
Implement any missing separation of exploratory performance and independent
verification in the existing runner; do not copy a second benchmark engine.

Correct required observers in the shared location:

- [benchmark_shell.rs](../../../crates/layerfs-api/sdk/examples/benchmark_shell.rs):
  a clean Commit may correctly return `UpToDate`; accept/report the appropriate
  public outcome without pretending it was `Committed`.
- [separated_writes.py](../../../benchmark/fs-bench-pro/separated_writes.py):
  a one-write command has one reachable progress checkpoint, not four.
- Handle valid zero C1 work with explicit availability/provenance; missing is
  not automatically zero. Preserve malformed/interleaved telemetry and mark
  incomplete ingestion. Use equivalent observer accounting across both arms.
- Fix the retained-journal lifecycle for clean/one-edit controls: use a real
  live/pinned private 4,097-record state in the same Workspace. A newly attached
  Workspace from a committed Store is not equivalent. Include preparation in
  complete lifecycle wall where the contract requires it; do not hide it to fit.

If a required cache/resource capability remains unsupported, keep the run
diagnostic and continue the algorithm work. Never label it qualified to unblock
the report. O_DIRECT is source evidence of requested kernel data-cache bypass,
not a complete runtime/metadata/VM/backend/device/host-cache proof.

### 3.2 First product change: remove the quadratic patch scan

Work in [backing/active/reclaim.rs](../../../crates/layerfs-workspace/src/backing/active/reclaim.rs),
`live_refs`. Trace **all callers**, including both `prune_dead` (R references)
and `prune_dead_payloads` (L references).

The current `updates.iter().any(key.starts_with(prefix))` traverses the whole
patch once per touched logical page. Construct the existing exclusive upper
bound before the test and use the existing BTreeMap prefix range:

```rust
updates
    .range(prefix.clone()..upper.clone())
    .any(|(_, value)| value.is_some())
```

This is a sketch of the local algorithm, not a complete patch. Preserve the
`u64::MAX` upper-bound case and the subsequent paged selected-index liveness
scan, including live references after the first 128 entries. Do not remove
locator/payload custody checks or retained-generation owners.

Add public-API external regression coverage for deletion-only and mixed-live
patches, shared packs/payloads across files/generations, boundary logical IDs,
quota/Budget refusal, exact selection, pinned old bytes and clean-close refund.
Use existing tests/helpers; do not expose private methods solely for tests.

Prove/count the patch-search change from O(P*M) to O(P log M + R), where R is
the relevant patch references visited. The 4,097-write deletion shape previously
required 640,614 whole-map checks. This fix occurs after SaveFile; do not credit
it with eliminating SaveFile time.

## 4. Next: fix dispersed Commit's source locality

Trace [commit/active.rs](../../../crates/layerfs-workspace/src/commit/active.rs),
[active/reader.rs](../../../crates/layerfs-workspace/src/backing/active/reader.rs),
[active/index.rs](../../../crates/layerfs-workspace/src/backing/active/index.rs)
snapshot lookup and [active/pack.rs](../../../crates/layerfs-workspace/src/backing/active/pack.rs).

Final extents are emitted in file-offset order; packs store chronological writes.
The reader retains one pack. Every different pack can cause a fresh P lookup,
full page read and allocation/decode of all its records. Add real production
telemetry or external instrumentation for pack loads/cache hits, locator seeks,
index reads, decoded records/bytes and source time. Use phase deltas, not
lifetime counters; keep baseline accounting equivalent. No test-only product
hooks, special benchmark algorithms or per-record network-frame assumption.

First validate the existing source-derived miss predictions against one labelled
causal diagnostic: 41/512/4,097 loads for 100/512/4,097 dispersed writes.

Then implement the smallest bounded grouping/scatter approach supported by the
existing upload abstraction:

1. Collect upcoming required replacement references within fixed reference
   **and byte** limits, charged before allocation. Reuse current descriptors/
   streaming helpers where possible.
2. Group references by logical pack in that window. Resolve each locator from
   the captured G1 selection; reuse that result for the group's records.
3. Read/validate one pack at a time and scatter only required bytes to their
   required positions in the bounded replacement buffer.
4. Emit the buffer in original required order, then advance to the next window.
   Preserve mixed Base/Zero/Payload coverage, source offsets and large-record
   streaming. Authenticate exact slot/inode/generation/revision identities.

An illustrative 1,024-reference window predicts loads 2/7/209 at the three
dispersed counts. This is a hypothesis, not an imposed bound or measured gain.
Choose limits from the actual charged budget and existing source contract;
freeze them before collecting evidence. Count actual loads and derive the bound
for that implementation. Loads per window depend on its distinct required packs;
do not claim globally O(P) loads for every schedule. An eight-page LRU alone
still predicts 4,097 misses on the large schedule; do not widen a cache to fit it.

Preserve <=8 resident pack pages, <=64 resident index nodes, <=8 hot cursors,
64 hot slots, the 1 MiB hot byte cap and configured overall Budget (default
8 MiB). Charge raw/decoded buffers, references, scatter bytes and old+new
capacity overlap. Retain atomic refusal. Do not introduce a file-sized spool
or read unrelated packs to improve locality.

Inspect current O(E_f) descriptor/extent materialization and repeated captured
scans too. Reuse the existing streamed approach where compatible instead of
adding more full-file clones. Bounded grouping does not itself establish
constant-RAM Commit; any retained O(E_f) term must remain charged, tested and
disclosed against the specification.

## 5. Then reduce WRITE publication cost

Use the measured regions to choose the next change. Append publication consumed
about 5.075 s of the recorded 5.964 s Exec; dispersed publication about 9.739 s
of 10.610 s. These are diagnostic observations with the campaign qualifications.

Split actual publication time/counts into:

- fit/merge/encode/cache-decode and allocation;
- create/identity/stat/preallocation;
- direct page write;
- readback/authentication/byte comparison;
- retired-owner inspection/release/unlink;
- temporary-input acquisition and publication-side reading/maintenance.

Start in [active/hot_path.rs](../../../crates/layerfs-workspace/src/backing/active/hot_path.rs)
and [active/splice.rs](../../../crates/layerfs-workspace/src/backing/active/splice.rs)
for repeated fit merge, actual merge and encode/decode-back. Reuse already
validated prepared nodes/bytes safely rather than repeating the work.

Then inspect [active/pages.rs](../../../crates/layerfs-workspace/src/backing/active/pages.rs)
and [backing/segments.rs](../../../crates/layerfs-workspace/src/backing/segments.rs)
for the dominant physical lifecycle. At sampled append WRITE 4,096, 20,768
active creations imply 81.125 MiB written and the same immediate readback,
excluding temporary inputs/fetches. Readback is absent from ordinary fetch
counters. Retained allocation and cumulative I/O are different metrics.

Only pursue preallocated physical storage/free-slot reuse when the profile
justifies it and the existing format/custody rules permit it. Never overwrite a
selected/pinned incarnation. Reuse requires final-owner release, new epochs,
checked allocation, charge/refund and unknown-outcome custody. A required format
change needs a concrete prospective design/specification revision and compatible
reader/custody proof before collection; do not silently change v2 or remove
readback/authentication to make the timer smaller.

Treat [FUSE tiny-input ownership](../../../crates/layerfs-fuse/src/adapter.rs)
and [filesystem/active_file.rs](../../../crates/layerfs-workspace/src/filesystem/active_file.rs)
as a secondary opportunity: replace unnecessary temporary-file round trips with
a bounded charged copy where the ordinary API permits it. Preserve copy-before-
reply, input lifetime, definite refusal and charge. Acquisition-only elimination
has an optimistic 1.14x append Exec model, not a 10x promise.

Dispersed WRITE had only five hot writes out of sampled 4,096, with thousands of
admissions/normalizations. Count their cost and repair unnecessary turnover in
the shared generic route if material. Preserve eligibility and exact fence/
ancestor/epoch validation. Do not make random writes falsely qualify as a
monotone frontier or increase slot/cursor limits.

## 6. Required iteration and result recording

Repeat this sequence for each cause until section 8 is met:

```text
inspect the shared cause and every caller
  -> state hypothesis, complexity/count oracle and affected coverage
  -> implement the smallest fix
  -> run the smallest meaningful correctness regression and applicable guards
  -> commit the verified candidate with exact production LOC
  -> run a labelled causal/scaling diagnostic at the committed identity
  -> record raw results, failures and derived arithmetic
  -> synthesize what improved, what remains and the next shared cause
  -> implement the next fix; repeat
```

Record the starting mechanism from retained evidence, or one declared causal
diagnostic if a required counter is missing. New instrumentation is not a licence
to take another unlabelled sample of an unchanged performance arm.

During development select affected cases; do not run every public route and the
whole 3x3 performance matrix after every small edit. Expand deterministic count
coverage to 100/512/4,097 writes, then sibling schedules and boundary/pin/G1-G2
cases when the mechanism requires them. A source change can justify another
targeted test. A docs-only commit/new output path does not justify a new sample.

Use the existing runner; add a labelled diagnostic mode if needed. It must
report `admission_eligible=false`, verifier `SKIPPED` when appropriate, exact
identity and included instrumentation overhead. New telemetry must be genuine
production telemetry or external instrumentation, not test code under src/.

Append each iteration to `CHECKPOINT5-OPTIMIZATION-LOG.md` when first needed.
Put retained artifacts under fresh worktree-local
`benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-NNN/`.
Record: cause and hypothesis; commit/tree/product/harness/workload/artifact
seals; exact commands; test status/count/coverage; phase/complete wall; raw
operation/byte/charge/resource counts; before/after operands and formula;
cleanup/interference; every FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN; synthesis and
next action. Preserve raw stdout/stderr/receipts and a checksum manifest.

Compare counts per accepted WRITE/reference/affected identity, and phase-local
increments, across the declared tiers. Explain carries, height changes, pins,
working-set eviction and canonical construction separately. Do not infer a
complexity exponent or noise distribution from one wall observation per tier.

No repeated setup: reuse protected closed masters, immutable binary archives
and images by identity, incremental own-worktree builds, independent writable
sample copies and applicable unchanged proofs. Never reuse a mutated sample,
reader warmth or prepared post-operation state inside the measured phase.

## 7. Tests, final matched collection and source custody

Focused harness checks after relevant edits:

```sh
python3 -m unittest discover -s core/benchmark/fs-bench-pro/tests -p 'test_*.py'
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py self-check
```

The existing harness test directory has substrate/Init tests, not a focused
checkpoint-5 cache-ordering regression. Add the needed small checkpoint tests
there; do not report zero discovered tests as coverage of the new behavior.
Use external Workspace tests for the product changes, including Linux-gated
backing tests and mounted proofs when host tests do not exercise them.

Once the affected implementation stabilizes, run the required checks from the
worktree root and additional checks for any other owning package changed:

```sh
cargo +1.85.1 test --release --manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace
cargo +1.85.1 clippy --release --manifest-path core/Cargo.toml --locked --offline --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked --offline --target aarch64-unknown-linux-musl -p layerfs-workspace --tests
```

Run the resulting exact Linux `active_backing-<hash>` executable once for the
changed scope, with an owned ext4 Docker volume, `TMPDIR=/work`,
`LAYERFS_ACTIVE_TEST_ROOT=/work`, and `--test-threads=1`. Use the existing
phase-log recipe; record the actual binary and command. Remove only owned
containers/volumes after preserving evidence. Host-only passes are not Linux
backing/FUSE proof. Keep historical unrelated C1 failures disclosed.

For changed mounted behavior, use registered `stage_route.py` cases with the
closed functional fixture `core/target/issue273/checkpoint3-prepared-v1/result.json`,
release service binaries and the exact release Linux stage executable. Reissue
affected receipts at the new source; preserve `performance_claim=false` and
`cache_claim=null`. Do not rename these into checkpoint performance rows.

Freeze the new product, harness, corrected contract, workload, oracle, binary,
image and report-generator identities. Commit before any receipt that records
HEAD; no dirty qualified build. If the frozen candidate then fails a required
check, return to the loop, fix it and freeze a new identity.

For the final matched comparison, keep the baseline product source unchanged.
Common harness/example corrections must be identical in both arms. If the
baseline needs common driver changes, use an available owned comparison checkout
or a managed worktree rooted at the frozen control; do not edit another owner's
active checkout. Commit only the declared common harness/driver changes, prove
the baseline product-source seal is unchanged, and record its new Git/artifact
identity plus the original product ancestry. The harness includes examples in
some seals, so driver edits are not an unchanged-build claim.

Run the nine matrix cases (append/dispersed/repeated x 100/512/4,097), clean
Commit, one-edit Commit and the original #248 gate: 12 selections x two arms,
in registered row-major control-then-candidate order. Keep 15-second complete
limits, 25 seconds for the three 4,097-write matrix cases and the #248 gate,
and the separately bounded independent verifier (9 seconds in this contract).
Use one construction worker in defaults and `LAYERFS_CONSTRUCTION_WORKERS=1`.
Do not hide required retained-state preparation to meet a limit.

The current runner command shapes are:

```sh
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py run --prepared PREPARED_JSON --selection issue273-dispersed-512-10m-v1 --output NEW_OUTPUT
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py campaign --control-prepared CONTROL_JSON --candidate-prepared CANDIDATE_JSON --retained RETAINED_JSON --output NEW_CAMPAIGN
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py report --root CAMPAIGN --route ROUTE_JSON --output NEW_REPORT
```

These are templates, not instructions to reuse old prepared metadata. Inspect
the revised parser and seals first. If retained-state/performance-verification
corrections change the interface, document and test the new grammar. Reuse the
closed 10 MiB pattern master and separate 8,194-byte #248 master through the
existing preparation logic; never substitute the 64 MiB functional fixture.

Complete the five independent registered selections: many-file-128,
multi-exec (three **sequential** 100-write Execs), G1/G2, retained-32 and mutations.
Check bytes/lengths/modes/aliases/old views, actual blocks, charges/refunds,
cursor eviction/epoch reuse, carries, continuation after Commit, refusal and
failed/unknown custody. Existing mapped coverage does not establish omitted IDs.

Report phase-local process/cgroup/Budget/backing domains accurately. Missing
memory/reset/cache capability remains a qualification gap, not an invented
zero. Old timeout rows have no completed baseline Exec/Commit durations; do
not use their 25.007-second wall as a phase speedup denominator.

## 8. Stopping condition: no unresolved bad factor in the covered scope

Treat these as bad factors to find and eliminate, not merely rename:

- whole patch/journal/tree or unrelated retained-prefix work per small affected
  operation, including the known O(P*M) patch scan;
- repeated required-pack lookup/read/decode caused by avoidable access-order
  thrashing;
- admission/normalization/retirement turnover unrelated to the actual changed
  closure;
- dominant avoidable CPU duplication or physical publication amplification;
- uncharged allocations, unbounded cache/spool growth, hidden work moved to
  setup/after acknowledgement, or a benchmark observer that conceals that work.

Completion requires all known causes above to be implemented and checked,
with before/after mechanism results. Remaining growth must be attributed to
necessary input/output, affected references, explicit carries/evictions/pins,
declared logarithmic maps or canonical work allowed by the authority documents.
A required remaining cost needs a concrete source-bound explanation; merely
calling an expensive path O(W) is not an optimization result.

Check clean/one-edit work against unrelated live/pinned state and eligible hot
WRITE against growing changed state. Preserve G1/G2 bytes and actual process
continuation. No quota/refund/custody regression, widened bound, extra worker,
shortened workload, warm-credit or postponed charged work can satisfy this gate.

Finish only when the covered operation/count/resource checks meet their declared
bounds, no identified avoidable factor remains unresolved, and every required
final selection has a retained outcome. A FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN
still blocks the corresponding admission claim; report it plainly and continue
fixable work. If an external capability or incompatible contract prevents the
remaining step, record the exact blocker and completed work without claiming
that optimization or checkpoint 5 is fully complete. Do not keep sampling an
unchanged identity while waiting for external state.

No universal constant-CPU/constant-RAM or 10x headline follows. Source-read
count reductions and Amdahl models are hypotheses until measured. A new
qualified matched comparison, independent proofs and resource/custody evidence
are needed for a speed/admission claim.

## 9. Commit and delivery rules

Production files <=999 physical lines; lib.rs/mod.rs <=200 and delegation only.
Keep tests/tools/examples outside product src/. No dependencies, third-party
patches, sync/durability calls, automatic retry or benchmark-specific product
branches. Preserve ARMv8 AEAD build inputs in root `.cargo/config.toml`.
No CI or aggregate gate exists; never run `tools/preflight.sh` or add a wrapper.

Every commit must build as applicable and be verified for its changed scope.
Prepare exact first-parent versus final staged-tree production LOC, confirm
against the committed tree, and put it in the commit message and iteration log:

```text
Production LOC: <before> -> <after> (delta <signed delta>)
Core: <before> -> <after>; reference: <before> -> <after>; combined: ...
Method: python3 tools/production_loc.py --json --root <exact snapshot>
```

Starting totals: Core 67,158; reference 65,417; combined 132,575. Recompute;
do not substitute Git diff statistics or physical line counts. Docs/tests/tools/
examples do not contribute. Update the affected architecture document in the
same commit as an algorithm/format/bound change. Keep new evidence append-only,
append the checkpoint/optimization logs and publish source-pinned results for
all required selections. Push only the assigned branch; do not merge, close or
post external issue/PR messages without authorization.

Final handoff: exact final identities; changes per cause; per-iteration tests,
mechanism counts, timing and resource results; every nonpassing/unrun item;
remaining necessary complexity terms; reproducible commands; production LOC per
commit; and whether numeric comparison and checkpoint-5 completion qualified.

## 10. Current continuation mandate: all nine matrix cells

The owner's **new instruction** is to continue the fix → targeted tests →
retained result → synthesis cycle, concentrating on time complexity and
removing *every avoidable quadratic factor*, until **append, dispersed and
repeated at 100, 512 and 4,097 writes** each have convincing source-bound
scaling, correctness, resource and matched numeric evidence. Do not pause at
another audit, at one passing small tier, at a reduced pack-load count, or at
one timing result. Follow the existing §§6–9 custody and no-resampling rules.
O(n) necessary input/output, one immutable acknowledgement per WRITE and
canonical construction cannot magically become O(log n); target O(log n)
**for unrelated lookups/scans** where the ordered index permits it, not for
reading/writing n required bytes or n affected records. Never drop identity,
authentication, charge, old pinned bytes or deadline/worker limits to make an
asymptotic claim. A finite 3×3 proves only its declared schedules, not every
possible permutation.

### 10.1 Resume from these identities and records, not the original start

| Item | Current fact at this continuation checkpoint |
| --- | --- |
| Assigned branch/worktree | `codex/issue273-active-head`, `/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs` |
| Pushed source before this **docs-only** update | `c113656b470998d0268e2170aaef0f501751d485` (tree `23263461208eafe8a268f57f9507f69436964d84`) |
| Last changed product/harness/spec source | `adbee51588d9f6efd35cab3ebb3919f5083fa6b9` (atomic, charged source telemetry and prospective attempt-v5 parser) |
| Frozen baseline product (unchanged) | `48b51e874a41b3e1e6c6661e145316df8b408f07` |
| Production LOC at last commit | Core **67,639**; reference **65,417**; combined **133,056**. Recompute for every new commit. |
| Full iteration and failures ledger | [optimization log](CHECKPOINT5-OPTIMIZATION-LOG.md), iterations 001–007 |
| Corrected registered 10 MiB causal counts | [iter-008 results](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-008/RESULTS.json) and raw [checksums](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-008/SHA256SUMS) |
| Old interleaved observations (never promote) | [iter-007 results](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-007/RESULTS-before.json) |
| Current code entrypoints | [bounded Commit grouping](../../../crates/layerfs-workspace/src/commit/active_source.rs), [upload](../../../crates/layerfs-workspace/src/commit/active.rs), [reader](../../../crates/layerfs-workspace/src/backing/active/reader.rs), [shared reconcile](../../../crates/layerfs-workspace/src/backing/active/reclaim.rs), [runner](../../../benchmark/fs-bench-pro/checkpoint5_273.py) |

Verify actual HEAD, worktree/PR status, owned binaries/volumes and running
Cargo/Docker jobs before further edits. This branch was pushed; do not edit the
other owner's baseline worktree or run another Cargo owner in this worktree.
The prior prompt's “no optimization implemented” text, old CLI templates,
`retained` preparer and old receipt schemas are historical: do **not** execute
them as the current contract. The runner now has `run --diagnostic` (verifiers
`SKIPPED`, admission false), `attempt-v5`, `report-v2` and `campaign-v2`. It
has **no** generic `--setup clone` or `--perf-fast`. `retained` now refuses
false live-journal preparation; clean/one-edit rows 10/11 record `NOT_RUN`.
The protected 10 MiB master, 8,194-byte gate master and old/updated candidate
binary/image archives exist already: reuse by matching seals, not mutated
sample directories or cache warmth.

### 10.2 What is fixed, and what is emphatically *not* fixed

- The R/L inverse-reference liveness check changed from full-patch searches
  O(P·M) to bounded BTreeMap prefix queries O(P log M + R), plus the required
  paged selected-index scan (including beyond 128 entries). The 4,097-write
  deletion-shape operand had 640,614 old whole-map checks. This is after
  SaveFile; it does **not** eliminate Commit's earlier source/construction.
- Host input SHA/size validation now precedes *all* evictions and final
  whole-input residency checks. The prospective common observer handles
  one-write progress, public clean `UpToDate`, explicit C1 zero provenance,
  malformed/interleaved telemetry and unqualified ratios. O_DIRECT is a
  requested private data-cache bypass in both arms, **not** complete
  metadata/VM/backend/device/host-cache proof.
- Fixed charged Commit grouping currently covers at most **256 references**
  and **32 KiB replacement bytes per window**; the reader retains one pack
  and authenticates exact G1 locator/slot/inode/generation/revision/offset.
  Production-captured 10 MiB dispersed pack loads were **2 / 14 / 827** at
  100 / 512 / 4,097 writes, matching that implementation's *source model*.
  Old one-pack model predictions **41 / 512 / 4,097** are not a sampled
  frozen-baseline product. The previous 100/4,097 source log was bisected by
  LFT1 and remains INCOMPLETE; changed-source iter-008 rows are complete.
- Tiny FUSE <=128-byte inputs now take a pre-acknowledgement Budget-charged
  copy through the ordinary projection permit instead of temporary `p-*`
  create/read/release. A newly published, physically verified hot Node is
  retained without an extra decode. Per-page authenticated write **and**
  readback, immutable page versions, and all other ordinary custody remain.
- Public ext4 functional tests passed: exact Linux `active_backing` 32/32,
  charged mounted tiny input at 100/512/4,097 callbacks, 100/512/4,097
  dispersed source tests, G1/G2 mounted process continuation, plus a real
  **live/pinned 4,097-record state in one Workspace**. That clean Commit read
  0 packs/6 index pages; one edit read 1 pack/33 index pages, while 52 old
  packs remained retained. These are functional/count proofs, **not** the
  public SDK retained performance fixture, which currently cannot pin an old
  journal across sequential Exec/Commit without prohibited leases or new SDK
  capability.

**Open source-locality scaling defect:** pack loads per accepted dispersed
WRITE in the registered 10 MiB cases are **0.020 / 0.0273 / 0.2019**. At
4,097 writes, 17 windows revisit the growing set of 52 packs **827 times**;
512→4,097 grows by 8× writes but 59× pack loads. For pack cardinality P that
grows with W and a fixed K-reference window, the intervening range can behave
like Θ((W/K)·P), i.e. *practically quadratic* until P approaches the fixed
window bound. “827 rather than 4,097” is **not** the stopping condition. The
hypothetical 1,024-reference window predicts 2/7/209 at these three counts,
but is **not** implemented, frozen or measured, and may still revisit packs.
Do not simply size a cache/window/spool to the 4,097 schedule or quietly
raise <=8 resident pack pages, <=64 index nodes, <=8 cursors, 64 slots, 1 MiB
hot bytes or the 8 MiB default Budget.

WRITE results also need separate treatment: candidate registered dispersed
Exec took **0.116 / 0.999 / 8.673 s** and Commit **0.026 / 0.053 / 0.739 s**.
The 100-write complete command **2.685 s** exceeded 512's **1.897 s** because
its *unattributed outside-phase* term was **2.069 s** versus **0.371 s**;
mount/Exec/Commit/cleanup were each shorter at 100. Diagnose from retained
receipts or a **labelled cause-count instrument**, not a new unchanged-arm
performance sample or a guessed warm-cache/noise story. At 4,097 public
append WRITEs, the production profile still counted 20,780 active page
creations, **85,114,880 write bytes and the same immediate readback bytes**.
An earlier profile attributed 1,731.537 ms direct write and 861.366 ms
readback of a 5,450.864 ms public write loop. The remaining work is not
proved avoidable just because it is expensive; source-bound page-version and
canonical construction floors must be separated from unnecessary duplicate
publication or per-old-record admission/normalization.

### 10.3 Next concrete optimization cycle (keep all nine cells in view)

1. **First** trace all callers of the source reader, selected index, upload
   abstraction and source telemetry. State a prospective complexity oracle
   for 100/512/4,097 across **all three** registered schedules, not just
   dispersed. Distinguish final replacement references N, distinct packs P,
   #windows and physical loaded/decoded records; count index seeks/reads and
   source wall per SaveFile. Reconcile actual 2/14/827 and 0/1 old pins against
   the formula. Identify any *avoidable* repeated packs crossing windows.
2. Implement the smallest **fixed reference-and-byte charged** locality fix
   supported by ordered SaveFile output. A justified, sealed 1,024-reference
   hypothesis can be evaluated, but it is not an arbitrary success threshold:
   prove its byte cap and old+new allocation overlap against actual Budget;
   preserve <=8 pages, exact selection/large-record streaming, atomic refusal
   and no file-sized spool. If full-file pack-once would require unbounded
   scatter or a changed C1/C2 upload protocol, derive and document that lower
   bound rather than claiming O(P). Look for a better bounded algorithm or
   data representation **without** changing protocol/canonical identity to
   hide a regression. Do not reduce an O(n) necessary stream to a fictional
   O(log n); do replace an unrelated linear lookup with a bounded/logarithmic
   query where possible.
3. After each actual product change: smallest public external regression,
   affected host/Linux ext4/mounted guards, architecture document, exact
   first-parent/staged/committed LOC, commit, **one** prospective labelled
   count diagnostic per changed case/arm/identity, fresh `iter-NNN` evidence,
   before/after operands and formula, synthesis, next cause. Expand coverage
   to append/dispersed/repeated × 100/512/4,097, G1/G2 pins, boundary logical
   IDs, shared payloads, quota/refund and continuation. The current generic
   dispersed route showed at 100/512/4,097 writes: seeks **1,519/7,699/61,327**
   (~15 per write), admissions **38/425/3,973**, normalizations
   **38/425/4,785**, index writes **390/2,866/27,780**. Its old same-inode
   cursor, bounded 64-slot closure, height change and working-set eviction
   need a **source-bound** account; avoid removing future hot eligibility to
   make the random schedule look good. Check append/repeated and quick controls
   for a newly exposed whole-prefix or large constant factor, then fix it.
4. Stabilize only when **each of the nine** has a retained functional and
   prospective count/resource result at the frozen source; compare phase-local
   counts *per WRITE, replacement reference and affected identity* at all
   three tiers, and show that changes in height, carries, pins and cache
   working set account for residual growth. A much smaller but still
   avoidably accelerating load/read/CPU curve **fails**. Do not infer an
   exponent or a noise distribution from one wall per tier. The target is a
   convincing, reviewable reduction of *avoidably* bad full-path work in
   **all** nine cells, not an invented universal 10×, constant-RAM Commit
   or merely passing a timeout.
5. Then perform the matched control comparison and independent verification
   with identical corrected common harness/example source in an **owned**
   baseline comparison worktree rooted at the frozen control. That baseline
   lacks `checkpoint5_273.py`; copying just a runner without checking all
   linked workload/driver/seal inputs is not a matched arm. Never edit the
   existing other owner's baseline checkout. Keep registered command, 15/25 s
   limits, worker count 1, row-major control→candidate order, cache contract,
   verifier and binary/image seals. The original source-product seal must be
   proved unchanged. Retain every timeout, FAIL, INCOMPLETE, INELIGIBLE and
   NOT_RUN. Baseline old 4,097 timeout has **no completed Exec/Commit** to
   use as denominator. The 12-row final matrix still includes blocked quick
   controls; do **not** revive the closed Store as a live journal. Without a
   compliant public SDK pin/contract, report an exact external blocker and
   **do not** claim checkpoint-5 completion or fabricate ratios.

The previous prompt's §§6–9 checks, specimen integrity, release discipline,
LOC accounting, append-only receipt policy and final handoff fields continue
to apply. The current **corrected** runner shape for a causal diagnostic is:

```sh
python3 core/benchmark/fs-bench-pro/checkpoint5_273.py run --diagnostic \
  --prepared NEW_PREPARED_JSON \
  --selection issue273-dispersed-512-10m-v1 --output NEW_OUTPUT
```

Its `SKIPPED` verifier and cache/runtime qualification are explicit. This is
*not* the final independently verified comparison. Reuse the [iter-008
candidate seals](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-008/RESULTS.json)
for reference, but a changed product/harness needs new prospective artifacts;
never replay an old sample, repair an interleaved receipt or rerun the same
unchanged arm to improve its number.

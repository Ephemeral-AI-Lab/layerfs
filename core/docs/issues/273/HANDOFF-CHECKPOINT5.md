# #273 checkpoint 5 execution handoff

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. Checkpoint 5 is **NOT_RUN**. Phase 4.5 implementation and focused
> functional/count/custody proof are complete at the scope below. This is a
> task prompt for the next agent, not a benchmark result or release approval.

## 1. Task and boundaries

Finish issue [#273](https://github.com/Ephemeral-AI-Lab/layerfs/issues/273)
checkpoint 5: seal the public benchmark harness, compare the frozen baseline
and candidate under the registered workload/cache/resource contract, run the
independent proofs, and report every result and limitation. The owner requested
this handoff on 2026-09-28 after reviewing architecture, future concurrency,
the baseline, the test registry and expected algorithmic benefits.

The frozen handoff requires owner review before sampling. Resolve that against
the actual owner instruction in the new session: an instruction to execute the
checkpoint after review supplies authorization; do not request it again.
Creating this prompt starts no measurement. Continue authorized harness and
preparation work while any genuinely missing sampling ruling is pending.
Keep [PR #274](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274) draft and
#273 open; merging or closing either needs a separate owner instruction.

Complete #273 before the main lane merges. #264/PR #269 integration does not
block these samples and is outside this task. Concurrent SDK Exec, multiple
daemon mounts, deadline-free command lifetime and command leases belong to
[#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249), after #248.
Do not implement them here. The registered `multi-exec` case is **sequential**.

## 2. Identity and authority

| Item | Pinned identity / location |
| --- | --- |
| Candidate worktree | `/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs` |
| Candidate branch | `codex/issue273-active-head` |
| Origin | `https://github.com/Ephemeral-AI-Lab/layerfs.git` |
| Last inspected HEAD before this handoff update | `dabaf28da116dc48348133f78b42025de40d6cea` (documentation/evidence) |
| Frozen functional proof source | `4ae36ad3a9c70b32b66c8280ac9496e9f1e345a9` |
| Last product change | `a2359620a7966314fbb2a96c98e8da958df72c6c` |
| Frozen baseline / control | #271, `48b51e874a41b3e1e6c6661e145316df8b408f07` |
| Baseline worktree | `/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs` |

Check actual HEADs, working trees, PR head and active processes first. One
writer/Cargo owner per worktree; every Cargo target stays inside its own
worktree. Preserve another owner's work and processes. Inspect attached
artifacts and attach PR #274 when continuing it in a new chat.

Read these; normative documents override this informative prompt:

- [Repository rules](../../../../AGENTS.md), [Core rules](../../../AGENTS.md),
  [benchmark rules](../../../../docs/general/benchmark_rules.md),
  [benchmark-tree rules](../../../../benchmark/AGENTS.md),
  [Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md),
  [build/run mechanics](../../../../benchmark/fs-bench-pro/QUICKSTART.md),
  [release policy](../../../../docs/general/release-policy.md) and
  [documentation policy](../../../../docs/general/documentation-policy.md).
- Product authority: [implementation spec](PHASE4.5-IMPLEMENTATION-SPEC.md)
  -> [research audit](PHASE4.5-RESEARCH-AUDIT.md) ->
  [phase log](PHASE4.5-LOG.md) -> repository/Core rules.
- [Frozen handoff](HANDOFF-PHASE45-FROZEN.md),
  [frozen identity record](evidence/phase4.5/hot-publication-20260928/FROZEN-CANDIDATE.json)
  and [evidence catalog](evidence/phase4.5/hot-publication-20260928/README.md).
- [V1 format/evaluation record](ACTIVE-FORMAT-AND-EVALUATION-v1.md), especially
  **Registered public evaluation**. New active index attachments use v2; pack
  pages remain v1. The evaluation registry and limits still apply.
- [Checkpoint-4 log](CHECKPOINT4-LOG.md) and
  [baseline causal ledger](../271/CAUSAL-DIAGNOSTIC-LEDGER.md). Preserve their
  historical qualifications; do not restart the superseded phase-4.5 prompt.

Receipts must name the **actual committed source/tree** and artifact identities.
Docs/harness commits may advance HEAD without changing the frozen product;
prove that through product/compilation seals. Do not relabel a new row as
`4ae36ad3a` because the product matches. Existing frozen evidence retains its
original pending-review status and identities.

## 3. Completed proof and known gaps

Phase 4.5 implemented v2 fences/tagged targets, selected hot directory, charged
EOF and Base/Zero cursors, balanced carries, normalization, slot epoch reuse,
selecting-revision retirement and precharged G1/G2 reconcile. Hot state belongs
to Workspace inode/view identities, not Exec invocations. Capture pins G1 while
accepted later edits enter G2; ordinary mutations remain ordered.

At `4ae36ad3a`, **32 backing tests** and **17 public functional cases / 42 named
checks PASS**. A real mounted process kept its PID, fd and inode, acknowledged
edits during SaveFile/C5 and continued after Commit. Scoped host release tests,
Clippy/fmt, boundary guard and nine self-tests, native builds and Linux suites
are recorded in the frozen handoff. Reuse applicable identity-matched proof;
do not routinely rerun the passing suite. All these public rows have
`performance_claim=false`, `cache_claim=null`.

No checkpoint-5 speed sample, enforced cold comparison, or RSS/cgroup bound
exists. There is **no qualified checkpoint-5 baseline timing**. Historical
#271 timings have uncontrolled cache and different workload/identity scopes;
do not use them as new speedup denominators. The old #248 timeout stays FAIL.
The two historical C1 `filesystem_ordering` failures (19 objects against 18)
and two ignored mounted-only readable tests remain disclosed.

Measure known limits honestly: eight hot cursors, 64 slots, 1 MiB current hot
reservations; generic fallback/admission; temporary `p-*` acquisition even for
tiny FUSE input; physical registry/Node lookup costs; charged O(E_f) per-file
Commit upload scratch. Affected reconcile index preparation/installation still
runs under the Workspace state gate. No full CPU O(1), constant-RAM Commit,
zero callback blocking or linear parallel-write throughput claim follows.

## 4. First work: seal the harness and reuse setup

Inspect/reuse `core/benchmark/fs-bench-pro/write_patterns.py`,
`separated_writes.py`, `shell_package.py`, their C writers, SDK benchmark
examples, verifier and existing cold-cache helpers. `write_patterns.py` still
encodes the earlier **100-write** diagnostic/scenario; the full checkpoint
harness is not already implemented. Inspect parsers before issuing commands.
Do not rename old receipts or pretend changing a constant registers a family.

Commit the prospective execution specification/registry and timer boundaries
under `docs/roadmap/0.1/0.1.7/` before implementing a changed family or sampling,
as benchmark rules require.
Preserve the IDs, schedules, source arms, limits and oracles below. Freeze
harness/verifier/workload hashes, included/excluded work, cache procedure,
metrics and result layout. Run focused harness checks at final identity, then
commit before receipts pin `git rev-parse HEAD`.

Reuse the closed, validated **10 MiB `data.bin` filled with `A`** master. The
historical [prepared-patterns metadata](../271/evidence/fourhop-v1/prepared-patterns-public.json)
points to
`/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs/benchmark-results/fs-bench-pro/issue271/fourhop-patterns-prepared-v1/master`.
Check existence, Store/history/proof hashes and exact fixture contents before
reuse. It is not a new build/cache qualification. Reuse matching setup/proof
identities where allowed; if missing or incompatible, record the precise
prerequisite failure. Never regenerate a protected master before every row.

The phase-4.5 fixture at
`core/target/issue273/checkpoint3-prepared-v1/result.json` is a separate closed
functional fixture, not this 10 MiB master. Keep the original **8,194-byte #248
gate** fixture separate as well. Every arm uses an independent writable
`shutil.copyfile` Store/history clone; a clone is never a cold claim.

Build only needed locked **release** binaries in each arm's own `core/target`,
from that repository root. Reuse immutable archives/images only after seals and
hashes match. `FROZEN-CANDIDATE.json` lists candidate functional archives under
`core/target/issue273/binary-archive/`; they are not automatically the SDK
benchmark driver or daemon image. Record root `.cargo/config.toml` ARMv8 flags,
lockfile, features, target, toolchain, image ID and dependency/compilation seals.
No debug arms, dependencies or third-party patches. No build overlaps a timed
phase in this worktree; record observed competing work from other worktrees.

## 5. Performance registry: 12 selections, 24 source-arm attempts

Run this **exact row-major order**, control then candidate for each case.
One SDK Mount, one generic Exec launching the ordinary C writer, one explicit
Commit, independent old/new-head oracle and clean close are mandatory. The
driver cannot create changes via internal Workspace methods, direct FUSE
callback invocation, Bridge/service/C1 calls or special edit APIs.

| Order | Case ID | Work | Complete-command limit |
| ---: | --- | --- | ---: |
| 1 | `issue273-append-100-10m-v1` | 100 appended one-byte writes | 15 s |
| 2 | `issue273-append-512-10m-v1` | 512 appended one-byte writes | 15 s |
| 3 | `issue273-append-4097-10m-v1` | 4,097 appended one-byte writes | 25 s |
| 4 | `issue273-dispersed-100-10m-v1` | 100 dispersed one-byte pwrite calls | 15 s |
| 5 | `issue273-dispersed-512-10m-v1` | 512 dispersed one-byte pwrite calls | 15 s |
| 6 | `issue273-dispersed-4097-10m-v1` | 4,097 dispersed one-byte pwrite calls | 25 s |
| 7 | `issue273-repeated-100-10m-v1` | 100 same-offset one-byte pwrite calls | 15 s |
| 8 | `issue273-repeated-512-10m-v1` | 512 same-offset one-byte pwrite calls | 15 s |
| 9 | `issue273-repeated-4097-10m-v1` | 4,097 same-offset one-byte pwrite calls | 25 s |
| 10 | `issue273-clean-commit-v1` | No new mutations after retained old state | 15 s |
| 11 | `issue273-one-edit-commit-v1` | One new byte after unrelated retained 4,097-record generation | 15 s |
| 12 | Original #248 `gate` | 4,097 separated writes in the 8,194-byte file | 25 s, unchanged |

Matrix byte `i` is `B + (i % 24)`, starting at zero. Append uses `O_APPEND`;
dispersed offset is `(104729 + i*2654435761) % 10485760`; repeated offset is
`5242880`. One fd, one mounted write/pwrite per byte. The independent oracle
derives every byte/length from this schedule and checks the unchanged old head.

Quick-Commit cases retain old state as specified in the evaluation contract.
Their preparation/cleanup remain visible and included in complete lifecycle
wall; neither Commit may scan unrelated journal records. Do not hide the
preparation to fit a limit. The original #248 gate keeps its own writer
schedule and FAIL history; the dispersed matrix does not replace it.

## 6. Independent correctness and space selections

| Case ID | Required scope |
| --- | --- |
| `issue273-many-file-128-v1` | 128 one-byte files; exact bytes/identities, shared backing and refunds |
| `issue273-multi-exec-v1` | Three sequential 100-write Execs before one Commit; incremental backing across commands |
| `issue273-g1-g2-v1` | Three consecutive edited generations; old reader held across successors |
| `issue273-retained-32-v1` | One edit and pin in each of 32 generations; retention and physical release |
| `issue273-mutations-v1` | Truncate/hole, rename/unlink, aliases, open-unlinked handles, quota refusal, failed/uncertain backing and clean close |

Existing `active_*` rows prove named components, not these registered IDs.
Reuse proof only where exact identity/coverage satisfy the current contract;
record reuse and omissions without renaming it. Every test execution is capped
at 60 s. Each performance row has a separate independent verifier capped at
**9 s**, outside speed timing. Verify full old/new-head bytes, lengths, modes,
aliases/identity and cleanup as applicable; no sampled byte oracle.

## 7. Cache, resources and receipts

Before each timed arm, apply the same declared Darwin mmap/mincore invalidation
and whole-input Store/history residency check. Record file identities, page
counts, method and launch gap. Also prevent Exec's recent private-backing writes
from giving measured Commit free cached reads. Validate the host/daemon boundary:
host source invalidation alone does not prove Linux private-volume cache state.
Do not add durability calls to Workspace backing to manufacture qualification.

If equal required cache state cannot be established, retain raw timing and
functional evidence but mark both numeric speed rows **INELIGIBLE**, with the
actual reason. No cache-qualified speedup follows. No priming, warm expected
ranges, pooling warm/cold rows or relaxing the cache contract.

Fresh append-only outputs; **one performance sample per case/arm**. No n3,
best-of, unchanged-arm rerun or warm replacement. Started timeout = FAIL;
missing prerequisite = NOT_RUN, with measured preparation/attempt wall when
available and exact missing scope; incomplete instrumentation = INCOMPLETE.
Preserve every attempt. Diagnose from receipts or separately labelled
count-driven causal diagnostics, never a second sample of the same arm.

Record actual FUSE callbacks/accepted bytes and Service/public-call counts;
Exec, capture/freezing, Commit, cleanup and complete-command walls; index page
writes/fetches, pack writes/fetches, `Q_fetch`; admissions, ordinary hot rows,
carries, eviction/normalization and retirement inspections. Count temporary
`p-*` acquisition and legacy maintenance separately. Attribute time/counters
to phases; a lifetime memory counter is not a phase peak.

Report live/dead/pinned/candidate pages, compaction slots/inverse references
moved, actual `st_blocks * 512`, Host quota charge and refunds; charged Budget,
process memory and container anonymous/file-cache memory with actual boundaries.
Unknown allocation/unlink/outcome retains custody and charge. Missing counters
or bounds remain explicitly unmeasured.

Use one construction worker in default product wiring and
`LAYERFS_CONSTRUCTION_WORKERS=1`; namespace Init is the only exception and is
not this workload. Do not raise workers, timeouts or quotas, or shrink cases.
Remove only owned containers/volumes after preserving evidence; report failed
product cleanup separately from forced environment removal. Preserve foreign
resources and processes.

## 8. Expectations and interpretation

Eligible resident append/advancing frontiers should remove ordinary root seeks
and ancestor copies: approximately O(W*log_B E) structural work becomes O(W),
with carries amortized and admission/normalization/general work explicit. Both
formats have finite height bounds; this is not an O(W^2)-to-O(W) rewrite.
Dispersed/repeated overwrite remain generic. Full CPU retains encoding,
registry/Node lookups, I/O and lifecycle work.

Packing reduces a linear-space constant. The **all-charged-backing <=3 MiB**
target applies only to the named one-file, one-generation **4,096 separated
WRITE checkpoint**, not every matrix, many-file or pinned-generation row.
Historical 17.88 MiB and phase-4.5 functional space observations are not matched
speed evidence. Include pins, directories, metadata, candidates and dead-slot
slack; payload-only packing arithmetic is not total Workspace reduction. Do
not claim constant memory for Commit's O(E_f) scratch.

Calculate baseline/candidate ratios per matched row and declared metric: Exec,
Commit, complete command and backing space. Two times faster means half the
time for the stated metric. There is no invented requirement that every row
improve 2x; a general 2x claim needs complete cache-qualified matched evidence,
independent oracles and accountable resource gates. Counts can fail even when
a timer passes; fixed overhead and generic work may reduce/reverse time gains.

## 9. Verification, source custody and finish

Keep product frozen while implementing measurement. If a genuine defect needs
repair, preserve the failed row, diagnose the shared cause, verify the changed
path, record LOC, refreeze actual source and apply matched-identity rules.
Do not reuse old proof/numbers for changed product or harness identities, or
repeat unaffected passing checks. Product changes use the scoped Core locked
release checks below, plus checks for the changed package/example. Run them
from the repository root, once at the final relevant source; applicable
unchanged proof may be reused under the identity rules.

```sh
cargo +1.85.1 test --release --manifest-path core/Cargo.toml --locked --offline -p layerfs-workspace
cargo +1.85.1 clippy --release --manifest-path core/Cargo.toml --locked --offline --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Host tests do not substitute for Linux-gated backing/mounted proof. Reuse the
frozen Linux artifacts where identity-matched; if rebuilding, use the locked
release `aarch64-unknown-linux-musl` zigbuild recipe and owned ext4/Docker
resources from the phase log. Do not regenerate the closed functional fixture.

No CI exists. `tools/preflight.sh` is permanently retired; no replacement
aggregate wrapper/workflow. Production files stay <=999 physical lines;
`lib.rs`/`mod.rs` stay <=200 physical lines and declaration/delegation only. Tests/drivers stay
outside product `src/`. Do not change #264 files, dependencies, third-party
source, AEAD profile, canonical format or no-sync/no-retry rules to pass.

Every commit records exact first-parent -> staged/committed production LOC with
`python3 tools/production_loc.py --json`, Core/reference/combined subtotals and
signed delta. Handoff-start totals: Core **67,158**, reference **65,417**,
combined **132,575**; recompute exact snapshots before committing. Docs, tests
and harness do not add production LOC; no legacy retirement occurred. Commit
and verify identities before receipts; push only `codex/issue273-active-head`.

Publish raw append-only receipts/logs/checksums and source-pinned tables for all
12 performance and five correctness/space selections. Report every
PASS/FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN, exact reproduction commands, limits,
arithmetic, reuse, exclusions, checks and outstanding failures. Append the
active #273 evidence/log entry; update architecture/status only to what results
establish. Update #273 and draft PR #274 when the owner's execution task
authorizes those external updates. Preserve all archived receipts. State which
count/space bounds held, which speed comparisons qualified, whether the original
#248 gate met 25 s, and what still blocks completion.

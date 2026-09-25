# #232 Phase 2: complete and optimize the Exec/FUSE edit families

> **Status:** Current planning checklist; no release candidate exists.
> The all-ioctl route is prospective; no #232 performance admission exists.

Tracking: [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232).
The [#241 four-case report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/87b10ab1b2144f1d7fb54f1be9419095f891fea0/core/docs/issues/241/evidence/phase4-v4-admission/REPORT.md)
records four completed, independently verified public SDK Exec → FUSE ioctl →
Commit middle inserts and a separate 264/264 position proof. Its raw
Edit→Commit times were 47.632, 48.413, 59.284 and 81.680 ms at 1, 10, 100
and capped 500 MiB. The original performance receipts remain `INCOMPLETE`
after a parser error; a retained-raw-log recheck derives `INELIGIBLE` for all
four under the frozen Linux FUSE backing-cache contract without resampling.
These cases do not complete the #232 parent.
The [#232 specification](SPEC.md),
[Workspace edit workflow](WORKSPACE_EDIT_WORKFLOW.md) and
[benchmark rules](../../../../docs/general/benchmark_rules.md) remain
normative; this plan does not revise the existing 56-row registry, targets or
receipts. Historical v0.1.6 G2 used direct SDK range Edit; its 5–13 ms range
is context, not a matched gate for the process-launching Exec route.

## Prospective optimization target

The immediate optimization is to keep the 4 KiB structural edit bounded as
file size grows and to identify the measured Commit tail. For a **future
diagnostic with an equal, enforced cache state** using the same Exec/FUSE ioctl route,
the planning bands are raw Edit→Commit ≤55 ms at 1 and 10 MiB, ≤60 ms at
100 MiB, and ≤70 ms at capped 500 MiB. The 500 MiB cap asks for about 12 ms
below #241's observed 81.680 ms while allowing shell and daemon overhead.
Target `service.finish` 500 MiB minus 1 MiB growth ≤15 ms rather than the
observed 31.81 ms; ≤65 ms total at 500 MiB is a stretch goal, not an
admission gate. The other 52 cases, including 4 KiB overwrites, append,
truncate, zero extension and 64 KiB overwrites, use the **same semantic
ioctl route** under a new scenario identity. They need prospective per-case
targets; do not assign the four-case insert band to them.

These are planning thresholds, **not** retroactive PASS thresholds or cold
cache predictions. Freeze a new #232 scenario version, enforceable cache
contract and per-case latency targets in a committed [SPEC.md](SPEC.md)
amendment before candidate optimization or sampling. An untouched same-route
baseline may set a performance target only before candidate work, per the
benchmark rules. Never apply new targets to historical v2 or #241 v4
receipts, or report their raw ratios as a matched G2 speedup. The functional
goal remains 56/56 correct and verified routes; the hard complete-command
budget remains 15 s and the independent verifier must finish under 10 s
under the repository measurement rules.

## Decision before the full run

1. Carry #241 Phase 1A's completed mounted range operation and four-case
   functional proof into the integrated #232 source. Before the full 56-case
   campaign, finish [Phase 1B](../241/ROLLOUT_PHASE1.md#phase-1a-exit-and-phase-1b-work):
   retain the cause of `service.finish` growth and apply a narrow fix only if the
   evidence supports it. A fix gets a new-source four-case SDK result with
   verification, cleanup and zero suffix I/O; a no-change decision retains the
   existing receipts without resampling. Preserve content/size/open-handle
   coherence, old-Commit and Branch proof, and position coverage. A
   cache-ineligible raw timing is not a latency PASS for either issue.
2. Complete the [Phase 1C complexity screen](ROLLOUT_PHASE1C.md). Its
   repeated-edit, Chunked-locality and cutoff decisions identify any
   additional product change justified before the full campaign; it does not
   turn diagnostic wall times into performance PASS results.
3. Commit the [unified ioctl contract](UNIFIED_IOCTL_IMPLEMENTATION_PLAN.md)
   and a #232 specification amendment for the changed mounted editor route,
   then freeze a **new, prospective 56-case registry and scenario version**
   before benchmark implementation or sampling. Pin each case's editor operation,
   command and tool hash, offset, delete length, literal/zero payload,
   source/build/image/cache identities, expected callbacks and byte totals,
   result oracle, and target. The current [scenario-v2 baseline](exec-fuse-edit-v2-baseline.md)
   stays append-only and is not pooled with the changed route. Use the next
   unused **#232** version (v3 if no intervening #232 registry exists);
   #241's v4 insert registry is separate and cannot supply 56 new rows.
   Never relabel either set of receipts.
4. First prove the versioned BEGIN/DATA/APPLY carrier with unmodified
   published fuser and the target Docker kernel. A 64 KiB replacement must
   remain private until one APPLY and one Workspace revision. Preserve the
   current 4 KiB inline LFE2 carrier. Record a NO-GO if the carrier cannot
   uphold the atomic and cleanup contract; no POSIX substitute can fill a
   registered row.
5. Freeze an enforceable Linux FUSE/backing cache contract before a numeric
   latency claim. The v2 completed rows were `INELIGIBLE` because Commit could
   consume Edit's resident backing pages. Prepared-master byte copies do not
   establish cold cache. If the relevant domains remain uncheckable or resident,
   run a labelled diagnostic campaign and report `INELIGIBLE`/`INCOMPLETE`;
   do not mark a fast row `GOAL_MET` or silently change the cache policy.

## Route selection for all 56 cases

Every product operation, including setup and cleanup, uses the public SDK. The
timed operation is one `WorkspaceApi::exec` command on the mounted Workspace
followed immediately by one `WorkspaceApi::commit`. Keep the same Edit→Commit
boundary and independent verifier as the [#232 specification](SPEC.md#1-claim-and-timing-boundary).
This is the separately registered **Workspace Exec/FUSE workflow** allowed by
[benchmark_rules.md §2](../../../../docs/general/benchmark_rules.md#2-measure-the-authentic-operation),
not a direct `Client::edit_workspace_file_range(s)` claim. The direct-SDK
range-edit invariant remains unchanged. The performance driver calls only
public `layerfs-sdk` product APIs and `layerfs-telemetry` observation; Server
assembly is permitted, but it never calls private Service, Bridge, Store,
Workspace or FUSE methods. No private method is exposed, no benchmark-only
product hook is added, and the measured mutation cannot use a host path or
`docker exec`.

The command's actual filesystem calls define its route. The current
`WorkspaceApi::exec` implementation launches `/bin/sh -c`; this plan does not
claim an arbitrary-interpreter Exec API. **The range carrier itself is
shell-syntax agnostic:** a cooperating executable opens the mounted file and
issues the ioctl on that file descriptor, so LayerFS handles the kernel FUSE
request rather than parsing Bash, `/bin/sh` or other command text. An
unmodified editor's ordinary WRITE is not transparently converted into a
range EDIT, and a tool that copies a suffix still pays for that work.
Membership stays 12 length-preserving, 32 length-changing, and 12 canonical
chunk-count cases.

| Family / shape | Cases | Parameters to one mounted range replacement |
| --- | ---: | --- |
| `edit_length_preserving`: head/middle/tail 4 KiB overwrite | 12 | Delete 4 KiB at the selected offset and insert 4 KiB literal bytes through the current inline carrier. |
| `edit_length_changing`: middle insert/delete, prepend, middle grow/shrink replacement | 20 | Vary offset, deleted length and replacement stream. Zero-length delete or replacement expresses insertion or deletion. Use the inline carrier where the replacement is ≤4 KiB. |
| `edit_length_changing`: append, tail truncate, zero extend | 12 | Append at EOF; delete the tail with an empty stream; or append a semantic Zero run through the new carrier. One checked range operation per case. |
| `edit_canonical_chunk_count`: fixed 64 KiB overwrite, preserving/increasing/decreasing canonical chunk count | 12 | Delete and replace 64 KiB at the frozen offset. Versioned DATA fragments stage one payload; one APPLY publishes one edit and revision. |
| **Total** | **56** | One semantic ioctl route, no automatic fallback. |

If an operation cannot use its declared route, keep the case registered and
report `FAIL` or `NOT_RUN` with the reason. The 8 MiB replacement cap is
distinct from the 4 GiB file/result cap. An oversized ioctl request returns
Capacity before mutation; a caller-selected POSIX workflow is a separate
operation and cannot fill a registered row. The 64 KiB staged carrier must be
proven before any of its twelve cases run.

## Algorithm and resource model

Let `N` be total file bytes, `k` replacement bytes, `d` deleted bytes, `M`
untouched suffix bytes, `P` live Workspace overlay pieces, `E` canonical
content extents, `C` actual FUSE mutation callbacks and `A` affected CDC
windows/tree paths. Let `F(S)` denote the still-uncharacterized CAS/SQLite
finish work for Store state `S`. These are source-derived terms and open
costs, **not** a proven wall-time law.

| Route | Time work | Incremental space and I/O |
| --- | --- | --- |
| Old v2 structural shift | At least `O(M)` suffix read/write bytes; repeated `O(P)` rebuilds over `C` callbacks can reach `O(C²)` metadata work as piece count grows. | `O(M)` data movement through FUSE; cache/spool residency can grow with file size. This is the route the ioctl removes. |
| One inline ioctl replacement | Fixed `B = 4,192`-byte frame validation/transfer `O(B)` and `O(k)` literal-byte copy; current vector splice and Commit lowering each `O(P)`; Chunked C1 processes replacement and affected paths, approximately `O(k + A log E)` when the affected window is bounded, plus `F(S)` and fixed Exec/transport. `d` is a range length, not copied suffix bytes. | `O(B + k + P + A)` logical temporary work and expected zero suffix read/write callbacks; actual backing/page-cache residency remains an evidence question. |
| One staged ioctl replacement | `O(ceil(k/b))` DATA calls with proven bounded payload `b`, `O(k)` total transfer/validation, then **one** `O(P)` semantic splice and `O(P)` Commit lowering; Chunked C1 and `F(S)` remain. A Zero run carries its length, not dense zero bytes. | Bounded aggregate stage custody up to 8 MiB per operation, plus explicit concurrency budget and measured backing/page-cache cost. A 64 KiB edit is not sixteen visible splices. |
| Repeated edits before one Commit | Vector splice/lowering depends on rising `P`; repeated rebuilds can approach quadratic work in edit count. | `O(P)` piece metadata plus accepted payloads; assess separately before introducing a piece tree. |

The extent tree reuses unchanged canonical subtrees. CAS reuses unchanged
objects, and delta encoding chooses a physical representation. None removes
shell launch, FUSE protocol, Workspace `O(P)` lowering or `F(S)`. The current
single-splice cases have few pieces, so a persistent piece tree is not
justified by the four-case result. Neither whole Edit→Commit `O(1)` nor
whole-route `O(log N)` follows from the ioctl. Four file sizes and one sample
per size also cannot establish an asymptotic wall-time bound.

## Expected file and folder changes

Reuse the #241 v4 layout; this is an **edit map**, not an instruction to add
empty directories or duplicate #241's implementation. The ioctl carrier,
splice tool and v4 telemetry checker are on the separate
`codex/issue241-phase4-admission` branch, not yet in this `bb50` planning
checkout. Integrate that reviewed source before Phase 2 implementation.
Files marked diagnostic change only if retained counts implicate them.

```text
core/
  docs/issues/232/
    SPEC.md                            # prospective 56-case scenario amendment
    UNIFIED_IOCTL_IMPLEMENTATION_PLAN.md # semantic ABI and checkpoints
    ROLLOUT_PHASE2.md                  # this plan; link final report later
  benchmark/fs-bench-pro/
    registry/workspace-exec-edit-v3.json # new immutable registry if v3 free
    families/edit_length_preserving.py
    families/edit_length_changing.py
    families/edit_canonical_chunk_count.py
    shared/edit_contract.py            # route, identity and callback bounds
    shared/edit_route.py               # public SDK driver/receipt handling
    shared/edit_cache.py               # declared cache eligibility, no warm-up
    shared/telemetry.py                 # existing LFT1 parsing/coverage checks
    shared/edit_telemetry_v4.py         # reuse v4 operation/resource checks
    runner.py                          # existing release run/verify/report entrypoint
    image/build.py                     # sealed release daemon and tool image
    workload/src/main.rs               # frozen case arguments and tool dispatch
    workload/src/splice.rs             # one generic mounted-file ioctl editor
    tests/test_exec_edit_{registry,route}.py # focused guard checks
  crates/layerfs-api/sdk/examples/benchmark_edit.rs
                                       # public SDK performance driver
  crates/layerfs-server/examples/verify_edit.rs
                                       # independent oracle
  crates/layerfs-fuse/src/{adapter,range_ioctl}.rs
                                       # inline and staged Linux carrier/validation
  crates/layerfs-workspace/src/overlay/pieces.rs
  crates/layerfs-workspace/src/commit/lower.rs
                                       # existing algorithms; diagnostic changes only
  crates/layerfs-server/src/service/save/content.rs
  crates/layerfs-storage/src/cas/{lifecycle,store,placement}.rs
                                       # LFT1 finish substeps; optimize measured cause
  docs/architecture/                    # same-commit update if algorithm/bound changes
```

`core/crates/layerfs-api/sdk/src/{project,sandbox,workspace}.rs` remains the
public operation surface; change it only for a demonstrated API defect. There
is no new SDK range-edit API, no `host.rs`, no public internal counter accessor
and no test-only product feature. Keep Linux ioctl specifics in the existing
FUSE adapter with platform guards and the semantic range splice in Workspace,
so a future macOS or Windows projection can choose its own carrier without
changing the edit algorithm. Do not patch or fork third-party FUSE code.

## Fast lane, measurement and release gate

Use the completed #241 four-case and position evidence to justify expanding
the range route; its receipts are not Phase 2 rows. Before the full campaign, use
focused functional checks and **count-driven diagnostics** for any unresolved
failure: request/reply boundary, FUSE callbacks and bytes, piece count,
replacement bytes, CDC bytes, CAS object counts, and `service.finish` scopes.
Diagnose any five-second `Unknown` or a new failure from retained
traces; do not lengthen a timeout, add heartbeats, shrink a fixture, or retry a
case to obtain a convenient result. A root-cause fix gets a new source identity
and the previous receipts remain visible.

Specifically, add **product `layerfs-telemetry` LFT1 child scopes** inside
existing `service.finish` for batch drain, pack seal, index/signature flush,
publication and SQLite transaction, with object, byte, page and statement
counts. The #241 v4 span grew from 1.34 ms at 1 MiB to 33.15 ms at capped
500 MiB. This scope includes Store finish and index work, **not owner
release/drop**; do not charge its growth to drop. A count-driven 1 versus
500 MiB diagnostic should locate the cause before any CAS, CDC or index
redesign. Inspect Exec spawn/output waits separately for its observed fixed
25–28 ms Edit time. Diagnostic times do not replace registered samples.

At the frozen final identity, run the **full 56 selections once each** through
the public SDK and locked **release** host driver, verifier, Linux daemon and
tool. Reuse only compatible sealed prepared masters, immutable builds and image
layers outside the operation timer, without warming measured paths or moving
Edit/Commit work into setup. Give each attempt a fresh append-only
output path. The Edit→Commit root and its Edit/Commit child wall, CPU and
sampled RSS come **only** from `core/crates/layerfs-telemetry` LFT1; retain raw
caller, Service and daemon records, label cardinality, producer identities,
resource coverage and dropped-record counts. Do not substitute Python clocks,
cgroup lifetime peaks or a missing resource sample with zero. Enforce one
construction worker for Commit.

The #241 v4 host and daemon CPU/RSS sample windows did not enclose both scope
boundaries. Report those as partial process-window coverage, not exclusive
Edit/Commit CPU or exact peak memory. If a short child span has no covered
resource samples, report `UNAVAILABLE`; do not insert a sleep or extend the
operation. External complete-command/preparation wall and cache checks are
labelled supervision, never substituted for operation telemetry.

The **complete performance command**, including Sandbox lifecycle and cleanup,
must be at most **15 seconds**; preparation and cleanup have separately reported
walls. A separate identity-matched verifier reopens published and old versions
and checks final bytes/size, canonical root and chunk count where applicable,
Branch head, unchanged pristine root and cleanup. The independent verifier
must finish in **under 10 seconds** under the repository measurement rules.
An existing identity-matched qualifying **verification** receipt may be reused only if the
Core runner implements and records that exact reuse contract; it is never
presented as a new performance sample. The current Core runner has no
`--reuse-pass` option, so run the separate verifier. Record every
`GOAL_MET`, `TARGET_MISS`, `FAIL`, `INELIGIBLE`, `INCOMPLETE` and `NOT_RUN` cell,
including verification `NOT_RUN` when Exec never produced a Commit. No subset,
best-of selection, resampling at one identity or dropped failure can close #232.

The v0.1.6 G2 direct-SDK numbers remain historical aspirational targets,
not a matched speed ratio or realistic gate for this Exec/FUSE route. A new
per-case target requires the prospective spec/scenario amendment above.
Report raw phase times and each applicable target comparison with the
operation and cache differences attached. Phase 2 closes only when all 56
have an authentic completed route, eligible Edit→Commit evidence, independent
verification and confirmed cleanup under the frozen
[#232 owner route](SPEC.md) and
new scenario contract.

## Conditional repeated-edit optimization

If full-campaign evidence implicates repeated piece rebuilding, register a
separate, prospective **1 / 32 / 128 edit-count diagnostic** at one fixed file
size. Hold total replacement bytes, a frozen offset-generation rule, cache
state and route fixed. Count piece visits/rebuilds and Commit lowering
separately from FUSE and C1 work. Adopt a persistent length-indexed piece
tree only if those counts show that `O(P)` work limits the route. Any product
fix preserves canonical roots, save atomicity and the one-worker policy, uses
a new source identity, and receives focused correctness proof followed by a
final full 56-case qualification. See the [edit-complexity study](edit-complexity-v016-v017.md)
and [generic route design](generic-shell-edit-and-commit-design.md).

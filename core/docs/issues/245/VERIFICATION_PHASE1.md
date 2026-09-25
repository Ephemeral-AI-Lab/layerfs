# #245 Phase 1 verification: repair the observed ordinary-shell path

> **Status:** Current planning checklist; no release candidate exists.

No #245 candidate, new measurement, or admission result exists. Phase 1 targets
the retained failures and growing work below; larger package and file workloads
belong to the separate load-bearing plan. The public route is
`WorkspaceApi::exec(command)` → `/bin/sh -c` → ordinary POSIX calls on the
mounted FUSE Workspace, followed by an explicit `WorkspaceApi::commit()` only
when the command succeeds. “Workspace shell” is the product concept, not a
separate SDK method. No LayerFS edit tool, range ioctl, or command-text
classifier may supply a result for this route.

## Retained evidence and its limits

| Retained selection | Observed fact | What Phase 1 must establish |
| --- | --- | --- |
| [#243 four-case ordinary-shell attempt](../243/evidence/phase2-ordinary-shell-v1/REPORT.md) | The mixed three-package refresh failed before Commit when `mv -f package-lock.json.next package-lock.json` returned `EIO`; an earlier temp-file replacement succeeded. A read-only post-attempt diagnostic found the old head/tree intact. | Identify the Workspace error and failing stage beneath FUSE's `EIO`, repair the shared rename path, and verify the full package command, replacement, and old/new heads. The retained failed row stays failed. |
| Same attempt | The 4 KiB overwrite and sixteen one-byte writes passed full-tree verification. Raw Exec observations were 17.845 ms and 208.898 ms. Piece and Commit diagnostic counters were **UNAVAILABLE**; all latency rows were cache-**INELIGIBLE**. | Preserve byte correctness while proving which per-callback piece/index work the candidate removes. A new eligible control is required for a speed comparison. |
| Same attempt | The deliberate failed command made no Commit and left the old head intact. It and mixed refresh spent about 5.5 s each in cleanup/log capture; shutdown logged `Busy`. | Attribute shutdown time and retained state to an exact operation before changing cleanup. Keep the failed-command no-publication behavior. |
| [#232 POSIX v2 baseline](../232/exec-fuse-edit-v2-baseline.md) | Eleven structural shifts returned silent Exec `Unknown` at about 5 s before verification; 45 other shapes verified. Every latency row was cache-**INELIGIBLE**. | Instrument progress and first failure without raising the progress deadline. These eleven did **not** observe a Capacity error. |
| [#232 limit derivation](../232/SHELL_ROUTE_CORRECTION.md) and [one-attempt count diagnostic](../232/evidence/posix-count-diagnostic/REPORT.md) | Source/registry analysis deduced that those eleven shifts also exceed the 8 MiB existing-file replay bound. The diagnostic counted 9 versus 81 FUSE WRITEs, 51 versus 3,363 cumulative old-piece loads, and 10 versus 144 piece-index pages written for 1 versus 10 MiB middle shifts. | Remove repeated whole-list/index work and provide a bounded Commit route past replay limits. The deduction is **not** the observed failure code; diagnostic wall times are not eligible speed evidence. |

The historical [#232 ioctl campaign](../232/evidence/phase2-all-ioctl/REPORT.md) proves an explicit cooperating route only. Its results and every retained receipt remain append-only; none becomes a generic-shell control or PASS.

## Diagnostic work before a candidate

1. **Rename error:** trace FUSE rename → Workspace rename → backing/namespace publication at the root lockfile replacement. Use the retained command and state to choose one newly labelled mounted diagnostic identity if source inspection cannot isolate the cause. Record the underlying error variant, stage, source/destination identities, whether the replaced target is base or upper, and which candidate root was published. Do not infer the variant from `EIO`; several variants map there. Fix the shared path after checking its callers, not a package-specific name.
2. **Write/index cost:** expose operation-specific FUSE READ, WRITE, SETATTR, CREATE, MKDIR, RENAME, and UNLINK calls and bytes. Status `write` aggregates namespace work and is not the FUSE WRITE count. For each write/resize, retain old/new piece count, affected pieces, pieces/index pages visited and rewritten, private bytes allocated/reclaimed, and full-rebuild/fallback count. Separate successful callback cost from shell process launches and Store work.
3. **Exec and shutdown:** capture progress of the in-flight POSIX callback and daemon child when Exec is silent near five seconds; separately time Unmount, container stop, log collection, and Sandbox Delete, with the exact `Busy` cause. Do not make a passing row by increasing timeouts or dropping shutdown from complete-command wall.
4. **Commit:** record streamed replacement/Store bytes, candidate and reused objects, and distinct capture, lowering, construction, final-drain, and publication spans. Existing `LFS_FINISH_SUBSTEP` totals are save-wide and can overlap earlier waves; do not add them or call them final-drain time without a batch-scoped event. Missing counters are **UNAVAILABLE**, never zero.

These are **labelled cause-finding diagnostics**, with their own source, binary, image, harness and workload identities and fresh output paths. They are not repeat performance samples of an unchanged arm. Preserve raw errors, traces, complete command, and cleanup for every failed diagnostic.

## Prospective Phase 1 candidate selection

Keep the four exact #243 commands, fixtures and full-tree oracles from its
[frozen v1 contract](../243/PHASE1_CONTRACT.md) and
[command specification](../243/TEST_WORKSPACE_AND_COMMANDS.md). A command or
fixture change requires a new scenario identity and cannot be paired as the
same operation. Add only the smallest newly specified ordinary-shell shift
selection needed to check the earlier mechanism: one 10 MiB middle shift for
count growth and one shape at a historically failing size (for example a
10 MiB prepend). Use ordinary POSIX shell utilities, never the historical
LayerFS edit tool. Freeze their **actual command bytes and algorithm**,
prepared masters, full-file oracle, limits and identities before running.
The retained v2 rows are mechanism evidence, not matched controls for these
new commands. The 56-shape parent gate remains open.

| Case | Required functional result | Mechanism evidence |
| --- | --- | --- |
| Mixed package refresh | One Exec exits zero, one explicit Commit publishes the exact nine-file final tree. The old Commit remains readable; both temp-file replacements, removal and creation are correct. | First failing rename stage resolved; actual callback counts and bytes, changed-file/index/Commit counters retained. |
| 4 KiB positional overwrite | Exactly the declared 4 KiB changes in the 10 MiB file; full published tree and old head verified. | Affected piece/path work recorded independently of total file size; no full-list rebuild or undeclared fallback. |
| Sixteen one-byte writes | Exact 64 KiB file and full tree; later reads observe successful earlier writes. | Per-callback and aggregate piece/index visits, pages and allocations; shell launch time reported separately. |
| Deliberate command failure | Definite nonzero Exec, **zero Commit calls**, old head/tree unchanged, cleanup succeeds. Private mutation is not misreported as rollback. | Exact shutdown stage and retained-state reason; complete wall includes cleanup. |
| Selected POSIX shifts | Exact final file and old Commit under independent full-file oracle; no silent `Unknown` or hidden Capacity failure. | Actual shifted FUSE bytes, callback count, piece/index growth, replay/Store bytes, and Exec progress. POSIX suffix movement remains real work. |

If the implementation changes generation capture or mutable-root ownership, include a focused public behavior check that pins a Commit snapshot while a later write proceeds, then verifies both versions. This protects the continuously writable Workspace contract; it does not expand Phase 1 into a large-load performance campaign.

## Timing, admission, and custody

- Use the [repository measurement rules](../../../../AGENTS.md), [Core rules](../../../AGENTS.md), [Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md), and [general benchmark rules](../../../../docs/general/benchmark_rules.md). One sample per case per arm, no best-of or unchanged-arm resampling. Run construction with one worker in product wiring and `LAYERFS_CONSTRUCTION_WORKERS=1`.
- Prepare a closed, validated master once, then give each attempt an independent writable byte copy and record its copy method. Clone is setup reuse, not a cold claim. Pin source/tree, compilation and dependency seals, binary/image, harness, exact command, fixtures, oracle, report generator, host and cache state; use a fresh append-only evidence path.
- Before any performance run, freeze a complete prospective receipt schema, identical cache contract and numeric aims for both ordinary-POSIX arms. The old #243 and #232 raw times are **not eligible latency controls**. If backing-page residency cannot be controlled and checked equally—including bytes just written then read by Commit—report numeric latency `INELIGIBLE`; keep functional and count evidence. No cache priming, arm-specific invalidation, or moving measured work into setup.
- The complete command includes product timers, SDK/container lifecycle, Status, Unmount, log capture, and Delete: **≤15 s** ordinarily. The already declared #243 mixed-refresh exception is **≤25 s**. A new shift exception, if needed, must be declared **before** its attempt and stay within the small 25 s exception allowance. Independent verification is outside that timer and **under 10 s** (the frozen #243 selection used **9 s**). Do not raise the five-second Exec progress threshold, deadlines, worker count, or cache allowance to turn a miss into a pass.
- The identity-matched, read-only verifier reopens Store/History, checks every path, type, portable mode, size and SHA-256, rejects extra/missing paths, confirms the new Branch head and intact old Commit, and checks absence of publication in the failure case. Verify at frozen final source with the covering commands. Record every `PASS`, `FAIL`, `INELIGIBLE`, `INCOMPLETE`, and `NOT_RUN` line plus cleanup, resource scope and receipt completeness. A functional PASS or a fast raw timer alone is not admission.
- Require the general rules' mandatory row/campaign identity, provenance, route, failure-class and completeness fields. Preserve raw stdout/stderr, exact exit/timeout, counters, LFT1, hashes, manifests and reproduction commands. Report absent fields as absent; never backfill old raw receipts. No claim of #232 completion follows from this focused Phase 1 selection.

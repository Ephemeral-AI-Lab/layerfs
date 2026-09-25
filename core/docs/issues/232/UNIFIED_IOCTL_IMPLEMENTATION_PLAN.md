# #232 unified mounted range edit: implementation plan

> **Status: Implementation and functional campaign complete; no release candidate.**
> The 56-case v3 campaign is independently verified but cache-ineligible for
> cold latency; the capped-500 MiB raw insert aim also misses.

The owner selected **one ioctl range-replace route for all 56 registered
#232 edits**. One replacement is limited to **8 MiB**, matching the current
Workspace, Commit and Bridge replay contract; a file and its result may be
as large as the existing **4 GiB MAX_FILE**. The largest registered
replacement is **64 KiB**: twelve canonical-chunk-count cases use three
different 65,536-byte payloads across four file sizes. This 64 KiB value is
a benchmark selection, not an API limit. The [frozen v2 registry](../../../benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json)
and [baseline](exec-fuse-edit-v2-baseline.md) retain the original POSIX route
and results; the new ioctl route needs a prospective scenario identity.

Every product lifecycle operation remains on public layerfs-sdk. The measured
edit enters through WorkspaceApi::exec, whose cooperating command opens a
mounted file and issues ioctls; WorkspaceApi::commit is the publication
boundary. No direct SDK range-edit method, private benchmark entrypoint,
Service/Store call, shell-text parser or third-party patch is added. The
current Exec launcher uses /bin/sh -c. The **mounted request** is independent
of the issuing program's language; an unmodified editor does not issue this
ioctl automatically.

Read with the [Workspace edit workflow](WORKSPACE_EDIT_WORKFLOW.md), the
[#241 Phase 1B handoff](../241/IMPLEMENTATION_PLAN.md), the
[Phase 1C count screen](ROLLOUT_PHASE1C.md), the [56-case rollout](ROLLOUT_PHASE2.md),
the [#232 specification](SPEC.md), the [benchmark rules](../../../../docs/general/benchmark_rules.md)
and [Core rules](../../../AGENTS.md). This plan is a draft; commit its
contract/scenario amendment before candidate implementation or sampling.

## One semantic edit, two bounded transports

The product's single semantic operation is:

    replace(open_handle, expected_stamp, offset, delete_length,
            replacement_stream)

The stream may contain literal Bytes and a Zero(length) run. Insert, delete,
overwrite, append, truncate, zero extension and grow/shrink replacement are
different parameters to this operation, **not family-specific opcodes**.
Preserve sparse zero-piece semantics where the existing size operation uses
them. Validation uses checked arithmetic:

    offset + delete_length <= current_file_length
    current_file_length - delete_length + replacement_length <= MAX_FILE
    logical_replacement_length <= MAX_REPLAY (8 MiB)

The logical replacement length includes both literal Bytes and Zero runs;
the limit cannot be bypassed by encoding zeros compactly. The existing LFE2
inline EDIT remains the small byte-payload carrier. A versioned BEGIN /
bounded DATA / APPLY carrier moves larger payloads and represents Zero runs.
Both carriers reach the same semantic Workspace operation. Every DATA
fragment is private staging: it changes no
file bytes, inode revision or mtime. APPLY validates complete length/digest,
current writable handle, daemon/Workspace incarnation, expected stamp,
quota and result length under one mutation permit, then calls the existing
projected Workspace range mutation **once**. A 64 KiB overwrite may use many
DATA ioctls, but must yield one visible edit and one revision. Do not model it
as sixteen published 4 KiB edits.

The stage is bound to an open descriptor and single-use token. BEGIN/DATA
must not hold a Workspace mutation permit while a caller streams. Reject
malformed, duplicate, out-of-order, over-quota and stale input before
publication. ABORT, descriptor close, unmount, deadline and daemon shutdown
free an unfinished stage. A lost APPLY reply after publication is UNKNOWN:
never reapply automatically. An outcome query is optional and may only read
a retained same-incarnation receipt. Freeze exact errno and typed SDK
classification before product code.

An ioctl replacement **above 8 MiB returns a definite Capacity error before
mutation**. The adapter never silently switches to POSIX writes, a full-file
rewrite or multiple revisions. A caller may deliberately choose an ordinary
POSIX workflow through public SDK Exec; its atomicity, suffix I/O and
performance are different and it is **outside the all-ioctl 56-case
registry**. This is a clear capacity boundary, not a hidden fallback.

## Why a carrier experiment is first

The current LFE2 request contains 4,192 bytes and at most 4 KiB of payload.
The ordinary Linux ioctl command size is below 16 KiB, so one 64 KiB inline
request is unavailable. A user pointer inside a small request does not make
its referent available to the FUSE daemon. The target Linux/FUSE path and
published fuser must therefore prove ordered bounded requests before code
depends on a new carrier.

Freeze a standalone release-built Docker/FUSE probe. Test 4, 8, 12 and
near-16 KiB **total ioctl frames** on the target kernel; record actual
callback length, flags, reply and errno. Then prove a staged 64 KiB
replacement stays invisible until one APPLY, and test bad order, bad
digest, stale stamp, close, unmount and lost acknowledgement. This is a
functional ABI experiment, not a performance sample. If the published
provider cannot support it, record NO-GO; do not patch fuser or substitute
ordinary writes.

## Architecture and complexity target

    public SDK Project/Branch/Sandbox/Workspace setup
       -> WorkspaceApi::exec(opaque command)
       -> cooperating tool opens mounted file
       -> STATE, inline EDIT or BEGIN/DATA/APPLY, STATE/readback
       -> Linux FUSE ioctl adapter: checked, quota-bound private stage
       -> platform-neutral Workspace: one piece splice and revision
       -> WorkspaceApi::commit()
       -> C1 affected extent paths -> C2 CAS save -> History publication
       -> public SDK Status/Unmount/Sandbox Delete
       -> separate identity-matched verifier

Let N be file bytes, k literal replacement bytes, b the proven DATA payload
per ioctl, P live Workspace pieces, E canonical extents, A affected paths
and R logical edits. The carrier requires O(ceil(k/b)) DATA callbacks and
O(k) literal-byte work; a Zero run carries its length instead of k zero
bytes. The current Workspace splice and Commit lowering each scan O(P)
pieces. Chunked C1 processes replacement and affected extent paths,
approximately O(k + A log E) for a localized edit. C2 finalization still
has an unknown Store-size term until Phase 1B attributes it. A local edit
must cause **zero suffix-proportional FUSE reads/writes** as N grows; this
does not claim that the whole Exec/Commit route is O(1) or O(log N).

One stored 8 MiB replacement may use several carrier calls, but staging
must stay within a declared aggregate budget across open handles and
concurrent operations. Reuse existing Workspace payload custody with a
bounded resident window or a proven bounded in-memory stage; account for
temporary duplicate buffers, physical backing and page cache. Do not move
DATA transfer into benchmark preparation. Existing MAX_REPLAY, 256-edit,
1,024-piece, Bridge and Service limits remain enforced; **the 8 MiB
boundary is not removed in this plan**. A 4 KiB local edit in a 4 GiB file
still needs a file-size locality proof; a dense 4 GiB replacement is not
supported by this operation.

The v0.1.6 G2 implementation supplies a useful algorithmic precedent:
one compact first overwrite, persistent piece split/merge for repeated
edits, local C1 extent changes, and prepare-then-publish-once semantics.
Its 5–13 ms timings used a specialized direct SDK edit without shell or
FUSE and are not an Exec target. v0.1.7 already has local C1 extent
editing; do not port it again. Copy the **locality and atomic publication
principles**, not v0.1.6's cached-inode suffix reconciliation or private
SDK edit route. [Source study](edit-complexity-v016-v017.md).

## Roundtrips and realistic targets

The retained 1 MiB ioctl insert made one public Exec and one public Commit,
with Hello before each retained control operation. Inside Exec there was
one Service Inspect and four ReadFile calls; Commit made EditFile,
UpdatePortableMetadata and HistoryCommand: **eight Service calls total**.
The authenticated connection was already reused. Phase 1B's Store fix
and Phase 1C's piece/C1 fixes are expected to cut **local work, zero
transport calls**. A separate merged content+metadata save can remove
one Service call, but its one cache-ineligible observation proved no
speed gain; leave it outside the 1B/1C acceptance path. Do not remove
Hello, STATE, readback or History publication without their own proof.

For the existing four middle inserts, prospective raw engineering guards
are ≤55 ms at 1/10 MiB, ≤60 ms at 100 MiB and ≤70 ms at capped 500 MiB
(≤65 ms stretch), with 500-minus-1 MiB service.finish growth ≤15 ms.
Only the capped-500 MiB duration requires an observed raw reduction; all
four retained timings are cache-ineligible and cannot be speed PASS.
The changed **all-ioctl** route needs its own frozen per-case targets and
enforceable cache contract before a numerical #232 admission claim. The
56 registered cases retain the ≤15-second complete-command limit and a
separate identity-matched verifier. The 8 MiB capacity boundary is checked
for correctness, not introduced as another timed performance case.

## Exact file and folder map

| Owner | Expected paths and scope |
| --- | --- |
| Linux carrier | core/crates/layerfs-fuse/src/range_ioctl.rs; focused range_ioctl/wire.rs and range_ioctl/staging.rs only if the probe justifies splitting; adapter.rs and mount.rs only for descriptor/unmount cleanup. Keep adapter.rs below 999 physical lines. |
| Workspace semantics | core/crates/layerfs-workspace/src/types.rs, filesystem/write.rs, overlay/pieces.rs and existing backing/payload.rs; extend the projected range path for one checked Bytes/Zero replacement, preserving MAX_REPLAY. |
| Phase 1B attribution | core/crates/layerfs-storage/src/cas/{store,lifecycle,placement}.rs; LFT1 children for drain, seal, candidate flush, publication, SQL commit, and **separate** PoolIndex/Candidates post-commit copies. |
| Public benchmark | core/benchmark/fs-bench-pro/workload/src/splice.rs, existing families/edit_*.py, shared/edit_*.py, runner.py, image/build.py, new immutable 56-case registry and core/crates/layerfs-api/sdk/examples/benchmark_edit.rs. No SDK src edit method. |
| Independent proof | Existing external Workspace/FUSE tests, public SDK route tests and core/crates/layerfs-server/examples/verify_edit.rs. Update the affected architecture document in the same commit as a changed algorithm or format. |

The #241 v4 product source is on codex/issue241-phase4-admission, not the
dirty bb50 planning checkout. Implement in a separate clean worktree based
on that source. Do not modify, vendor or fork third-party packages.

## Commit checkpoints: one decision per commit

Every commit records first-parent production LOC before, after and signed
delta, with Core, reference and combined totals. Focused checks accompany
each changed product boundary; run the full locked Core tests, examples,
Clippy, formatting and product-boundary guard once at the frozen final
source. Keep all failures and receipts append-only, with no unchanged
performance-arm rerun.

- [x] **0 — Contract:** commit the all-ioctl route amendment, one semantic
  operation, 8 MiB/4 GiB limits, no automatic fallback, cache policy,
  versioned ABI, new scenario IDs and provisional targets. Documentation
  only; production LOC delta 0.
- [x] **1 — Kernel carrier proof:** commit the focused unmodified-fuser
  experiment and its raw GO/NO-GO result. Test bounded frames, 64 KiB
  ordering, no partial publication and cleanup. No benchmark speed claim.
- [x] **2 — Phase 1B attribution:** in one product commit add bounded LFT1
  children and counts without optimizing; in a separate evidence commit
  collect one labelled 1 MiB and capped-500 MiB diagnostic at the new
  source. Use only layerfs-telemetry for reported wall/CPU/RSS.
- [x] **3 — Phase 1B causal decision: no product change.** The retained
  diagnostic located growth at batch drain but did not isolate a safe
  substep fix; unchanged #241 arms were not repeated.
- [x] **4 — Versioned staged carrier:** commit wire validation and private
  stage lifecycle separately from one-APPLY Workspace integration.
  Prove 4 KiB inline, 64 KiB staged and 8 MiB boundary operations,
  definite refusal above 8 MiB, one revision, open-FD/fstat/read/EOF
  coherence, unknown APPLY custody and no silent POSIX fallback.
- [x] **5 — Generic tool and new 56-case registry:** use only offset,
  delete length and replacement stream in the tool; freeze exact command,
  tool/image/source hashes, callback bounds, oracle and cache treatment for
  each row. Every product operation uses public SDK; every measured edit
  uses mounted ioctl. Keep v2 evidence unchanged.
- [x] **6 — Phase 1C count decision:** one frozen 1/32/128 semantic-edit
  diagnostic, C1 locality and cutoff edges. A piece-index change needs
  superlinear visits **and** material LFT1 wall share; keep product,
  architecture, focused proof and evidence in separate commits. Do not
  rewrite C1 merely because v0.1.6 was fast.
- [ ] **7 — Final 56-case latency qualification remains open.** At the frozen clean source,
  attempt each locked-release SDK Exec→ioctl→Commit selection once;
  enforce complete-command ≤15 s, one construction worker, LFT1-only
  operation wall/CPU/RSS, declared cache state, independent verification
  and confirmed Sandbox Delete. Report every GOAL_MET, TARGET_MISS, FAIL,
  INELIGIBLE, INCOMPLETE and NOT_RUN cell, without a G2 speed ratio.
  The [retained v3 campaign](evidence/phase2-all-ioctl/REPORT.md) completed
  all 56 functional and verifier routes, but all 56 cold latency rows are
  `INELIGIBLE` and the capped-500 MiB raw insert aim missed.

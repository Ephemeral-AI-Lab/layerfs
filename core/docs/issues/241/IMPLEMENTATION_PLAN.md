# #241 implementation plan: projected FUSE range edit

> **Status:** Current planning checklist; no release candidate exists.

Tracking: [child #241](https://github.com/Ephemeral-AI-Lab/layerfs/issues/241)
of [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232). The
[owner's position-sweep clarification](https://github.com/Ephemeral-AI-Lab/layerfs/issues/241#issuecomment-5811888164)
is part of this plan. Read the [#232 specification](../232/SPEC.md),
[baseline](../232/exec-fuse-edit-v2-baseline.md), and
[source study](../232/generic-shell-edit-and-commit-design.md) with the
[benchmark rules](../../../../docs/general/benchmark_rules.md) and
[Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md).

This is an implementation sequence, not a claim that a FUSE ioctl, low latency,
or cold-cache admission already works. The four retained scenario-v2 middle
insert results are 155.01 ms (1 MiB), 2,905.70 ms (10 MiB), and Exec errors at
about 5 s (100 and capped 500 MiB). The latter two have **no completed Commit
time**. All completed scenario-v2 rows are cache-ineligible. Keep their
registry and receipts append-only.

## Intended route and boundaries

```text
public SDK WorkspaceApi::exec(opaque command)
  -> process launched in mounted Workspace
  -> cooperating program opens file and issues versioned range request
  -> kernel FUSE -> projected Workspace range mutation
  -> Core's existing RangeEdit/piece splice -> coherent mounted inode
public SDK WorkspaceApi::commit() -> ordinary canonical save and Branch publication
```

The product selects behavior from filesystem operations, never command text.
The current SDK Exec launcher uses `/bin/sh -c`, but the FUSE operation must
also work for a binary launched by another interpreter or a future direct-argv
Exec route. Ordinary `pwrite`, append and truncate continue through their
normal FUSE callbacks. A program that rewrites an entire suffix still pays for
those bytes; the new request lets a cooperating program express a local insert
without doing that rewrite. Do not add a benchmark-only Service/Store shortcut.

## Expected file layout and responsibility

Use existing modules before adding files. The list below is a review map, not
permission to create every optional file.

| Path | Required change |
| --- | --- |
| `core/crates/layerfs-fuse/src/adapter.rs` | Add the narrow FUSE request callback, projected handle check, reply and callback count. At 938 physical lines now, delegate substantial parsing and ABI logic to a small new module to stay under the 999-line production limit. |
| `core/crates/layerfs-fuse/src/range_ioctl.rs` **if selected** | Own one versioned Linux request format, strict bounds/flag validation and response encoding. Reuse this product-owned ABI in the cooperating tool or assert byte-for-byte conformance; do not create a second edit engine. |
| `core/crates/layerfs-workspace/src/filesystem/write.rs` | Extend the existing `RangeEdit` mutation path for projected, writable-handle origin with owned payload, exact inode/version checks, mutation permit and completion/coherence. Preserve the local API. |
| `core/crates/layerfs-workspace/src/filesystem/projection_counters.rs` | Expose a distinct bounded range callback count and accepted payload/shifted-byte evidence through the existing public status route. Count actual requests, including refusals, separately from accepted operations. |
| `core/crates/layerfs-bridge/src/contract/control.rs`, `core/crates/layerfs-bridge/src/adapters/native/protocol/response.rs`, `core/crates/layerfs-daemon/src/lifecycle.rs` | Extend the existing bounded projection-count wire only as needed to carry the new class into `WorkspaceApi::status`; preserve its versioned decoding rules. |
| `core/crates/layerfs-workspace/src/overlay/pieces.rs` | Change only if one-splice profiling proves that all-piece rebuild is still material. A new tree/index is not a prerequisite for the first insert proof. |
| `core/crates/layerfs-fuse/tests/kernel_write.rs` or a focused sibling test | Test actual mounted Linux file descriptor, alias, size, read and EOF behavior. Keep the existing Linux FUSE test harness. |
| `core/benchmark/fs-bench-pro/workload/src/main.rs` | Add one `splice` command that opens the mounted file and issues the frozen request with exact 4 KiB owned replacement bytes. Never fall back silently to suffix shifting. |
| `core/benchmark/fs-bench-pro/shared/edit_contract.py`, `registry/workspace-exec-insert-v3.json`, `runner.py` | Freeze only the four #241 middle-insert cases with new scenario identity, exact command/tool/image/cache hashes and one-case selection through the existing runner. Do not rewrite the 56-row v2 registry. |
| `core/benchmark/fs-bench-pro/tests/` | Focused request, route, registry and failure-receipt checks; reuse existing test modules where clear. |
| `core/docs/issues/241/` | Freeze ABI and position manifest before evidence; retain the four-case report and links to append-only raw receipts. |

Production LOC is a **planning estimate, not a measured commit comparison**:
FUSE ABI/callback ~100–220 nonblank non-comment lines; projected Workspace
mutation/coherence ~100–250; status wire/counters ~30–90. Thus initial product
scope is roughly **230–560 production LOC**. The workload, tests, registry and
docs are outside production LOC. Do not add a new UAPI crate, general command
parser, or piece-tree replacement merely to meet this estimate. Every actual
commit needs the AGENTS.md before/after production-LOC count for its exact staged
tree, with Core/legacy/combined subtotals.

## Phases and exit gates

### 0. Diagnose without changing the frozen arm

From retained #232 receipts and source, tabulate suffix bytes, FUSE callbacks,
piece counts, the 5 s progress error and edit/Commit timing for the four middle
inserts. Inspect the actual `fuser` callback support and Linux mount behavior.
State which observations are measured and which are source-derived. **Gate:** a
short diagnosis identifies the suffix-copy mechanism and the exact current
failure boundary; no rerun of scenario v2 is used to select a nicer result.

### 1. Freeze the range-operation contract

Choose the smallest Linux FUSE interface that reaches userspace for a
byte-granular insert. `fallocate` insert/collapse does not reach this FUSE
daemon on the relevant kernel; a versioned ioctl is the leading candidate,
subject to a live-kernel proof. Specify command number/version, little-endian
field layout, offset, deletion length, replacement length and bytes, maximum
request, response, unsupported version/flags, overflow, stale handle, read-only
mount, quota, deadline and partial-publication errors. The 4 KiB replacement
must fit the validated request path. Reject malformed or oversized requests
before ownership/publication; never interpret arbitrary ioctl bytes as a
Workspace edit.

Specify the atomic visibility point, size/mtime update, revision/generation
stamp and FUSE invalidation/reply order. In particular, ioctl completion alone
must not be assumed to refresh Linux inode size. Existing open descriptors,
hard-link aliases, `fstat`, reads around the insertion, and EOF must observe a
coherent result before the tool reports success. If publication happened but a
reply or invalidation fails, return/retain an **uncertain** outcome with custody
evidence; never retry an unknown mutation automatically. If validation or
quota fails before publication, preserve bytes, length, metadata and Branch
head. **Gate:** ABI and observable semantics are reviewed and frozen before
tool, registry or timed run changes; a mounted-kernel prototype proves the
chosen request actually reaches FUSE and can make size/old-FD state coherent.

### 2. Implement one projected mutation

Route the FUSE callback through the same projection ingress, writable handle
authorization, `OwnedPayload`, mutation permit, exact version check, piece
splice and publication accounting used by ordinary projected writes. Reuse
Core's `RangeEdit`; do not call its local-path API directly from the callback
and bypass projection admission. Keep the payload bounded and owned before
publication. Count the one range request and prove the 500 MiB untouched suffix
is neither read nor written by the tool/FUSE route. Preserve rollback on
pre-publication errors and explicit uncertain/retained state on post-publication
delivery errors. **Gate:** native unit tests and actual Linux mounted tests show
exact bytes/size/mtime, open-FD and alias coherence, repeated edits, valid
zero-length insert/delete variants, refusal paths, Commit, retained old Commit
and fresh reopen. No direct Store or private Workspace API is used by the
benchmark or live SDK end-to-end tests; those routes use the public SDK for
product setup/edit/Commit/cleanup. Focused FUSE and Workspace package tests
may exercise their owning lower-layer APIs directly.

### 3. Prove position generality, outside the performance timer

Use the pinned SHA-256 per-band offset rule in [SPEC.md](SPEC.md) and publish
**all exact offsets before running**. For each pristine 1/10/100/500 MiB fixture and each
of 4 KiB insert, overwrite and delete, partition the legal inclusive offset
interval into 16 equal-width integer bands and draw one byte-granular offset
per band. Insert permits `0..=N`; overwrite/delete permit `0..=N-4096`.
Do not align, adjust or choose offsets from observed results. Add separately
named exact head, midpoint, last legal offset and representative canonical
chunk-boundary checks. This is at least 4 × 3 × 16 = **192 functional checks**,
plus edges, not 192 performance samples.

Reuse one validated, closed prepared master per fixture size. For functional
checks, make **one private writable Store copy per size**, keep its pristine
source Branch unchanged, and fork a fresh sibling Branch plus mount a fresh
Workspace for each offset through the public SDK. This gives each check
pristine input without retaining 192 full Store copies. Unmount every
Workspace and confirm Sandbox deletion; the private per-size Store can be
released after successful compact receipts are sealed, while failed cases
retain their diagnostic state. Timed benchmark cases later get separate
independent writable copies and fresh sandboxes. No previously mutated Branch
or Workspace is a fixture.

The independent oracle checks exact changed bytes, size, untouched prefix and
suffix, old Commit, pristine source Branch, new Branch head and cleanup for
every offset. Record every offset and outcome, including failures, in compact
append-only evidence. **Gate:** all declared positions pass on a real mounted
Linux route. A latency-by-position claim requires a separately registered
prospective campaign; this functional sweep cannot be pooled with four timed
middle inserts.

### 4. Freeze and run the four-case release selection

Build the SDK driver, verifier, daemon and tool with locked **release** builds;
seal source/product/harness/tool/ABI, binary hashes, image digest, fixture and
registry. The #236 debug Init profile remains separate. Keep the four #241
middle-insert shapes and input sizes unchanged under new scenario IDs (v3 or
later if v3 is already used). Run public SDK Project/Branch/Sandbox/Workspace
setup, `WorkspaceApi::exec` then explicit `WorkspaceApi::commit`, status,
unmount and Sandbox delete. Time **only Edit→Commit** with
`core/crates/layerfs-telemetry` LFT1 wall/CPU/RSS. Retain producer identities:
caller and Service share one host-process CPU/RSS window; daemon is a separate
process, and its samples exclude the shell child. No Python timer becomes operation
telemetry. Mount, preparation, status, cleanup and separate verification stay
outside that operation timer and are reported.

Acquire each timed case from its own independent writable byte copy of a
validated closed master. Reuse the six #232 pristine masters only when their
fixture/format/preparation compatibility is proved; a harness-only change must
not force expensive re-Init when it leaves every preparation input unchanged;
unknown compatibility must fail closed. Record the
clone method, build/image reuse and cache state. No warm-up, read-ahead, or
benchmark-only eviction between Edit and Commit. Recent-write page cache
cannot credit an eligible cold PASS; a row exposed to it remains `INELIGIBLE`
even if fast. One
sample per case at each frozen identity, fresh append-only output, and a
separate identity-matched verifier (target <10 s, **hard ≤15 s**). The full
performance command, including Sandbox lifecycle and cleanup, is **≤15 s**;
retain every timeout or failure. Keep one construction worker and all resource
limits. **Gate:** all four cases complete Exec and Commit, pass independent
content/root/Branch/old-Commit verification and confirmed cleanup; route counts
show one bounded range request and no suffix-copy callbacks for the 500 MiB
case. Report raw latency beside historical G2 targets as aspirational context,
not a matched regression ratio. `GOAL_MET` additionally needs the frozen cache
admission and telemetry gates; this child can establish completion without
claiming an eligible 5.30–7.76 ms result.

### 5. Diagnose Commit and liveness as separate follow-ups

The projected splice should remove the size-proportional suffix movement and
is the likely fix for the 100/500 MiB **Exec completion** failures. It does not
by itself prove a 5.5 ms Edit→Commit route or repair cold-cache admission.
From retained LFT1, split `service.finish` with bounded child scopes and
count diagnostics before choosing any Store change. Preserve canonical roots,
save atomicity and one construction worker. Change Exec progress framing only
if the new operation still fails because a healthy silent command exceeds the
product's five-second progress window; retain the real overall deadline and
classify unknown publication correctly. Neither an enlarged benchmark timeout
nor synthetic heartbeat counts as an optimization. **Gate:** each separate
change has its own identity and focused proof; #232's remaining 56-case
performance and cache-admission work is reported under #232, not silently
credited to this child.

## Final handoff

Provide the frozen ABI and offset manifest, source/build/image/master seals,
exact commands, all four append-only performance and independent-verification
receipts, the per-offset functional matrix, raw LFT1, bounded FUSE counts,
cache status, full-command/verification walls and every nonpassing line.
Run owning Core boundary, test, Clippy and formatting checks once at final
source identity and state exactly which checks ran. Close #241 only when its
four completion, correctness, cleanup and route-custody gates hold; #232 stays
open for its full 56-case eligible performance decision.

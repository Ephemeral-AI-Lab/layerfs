# #241 specification: generic mounted-file insert and four-case proof

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.

Tracking: [#241](https://github.com/Ephemeral-AI-Lab/layerfs/issues/241),
child of [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232).
Read with the [parent specification](../232/SPEC.md),
[scenario-v2 baseline](../232/exec-fuse-edit-v2-baseline.md),
[generic edit and Commit study](../232/generic-shell-edit-and-commit-design.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md), and
[Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md).
This document is a prospective implementation and proof contract. It does not
change the retained scenario-v2 receipts or admit their timings.

## Claim and four selected cases

Implement a reusable, byte-granular edit on a **mounted FUSE file**. A program
started by public `WorkspaceApi::exec` requests one logical range replacement;
public `WorkspaceApi::commit` then publishes it. The command string remains
opaque to LayerFS. A shell, Python program, or compiled tool can request the
same operation; a program that rewrites the whole file still pays for its I/O.
The four performance selections remain the existing middle 4 KiB insert
shapes, with a **new scenario and algorithm identity** before candidate
measurement. Register only these four in prospective
`core/benchmark/fs-bench-pro/registry/workspace-exec-insert-v3.json`, with
scenario version 3 and IDs formed by replacing the final `-exec-v2` suffix
below with `-exec-v3`. This four-row child registry does not replace or edit
the frozen 56-row scenario-v2 parent registry. The v2 IDs and receipts are
historical comparators only.

| Historical v2 selection suffix | Pristine bytes `N` | Insert offset | Final bytes | v2 raw result | C1/G2 context target |
| --- | ---: | ---: | ---: | --- | ---: |
| `insert-middle-4k-on-1mib-ops-1-exec-v2` | 1,048,576 | 524,288 | 1,052,672 | 155.01 ms, completed, verifier PASS, cache INELIGIBLE | 5.30 ms |
| `insert-middle-4k-on-10mib-ops-1-exec-v2` | 10,485,760 | 5,242,880 | 10,489,856 | 2,905.70 ms, completed, verifier PASS, cache INELIGIBLE | 6.40 ms |
| `insert-middle-4k-on-100mib-ops-1-exec-v2` | 104,857,600 | 52,428,800 | 104,861,696 | Exec `Unknown` after 5.003 s; Commit not called | 6.25 ms |
| `insert-middle-4k-on-500mib-result-capped-v2-ops-1-exec-v2` | 524,283,904 | 262,141,952 | 524,288,000 | Exec `Unknown` after 5.003 s; Commit not called | 7.76 ms |

Every insert has `delete_len=0`, `replacement_len=4,096`, the existing sealed
payload and fixture seed from the [v2 registry](../../../benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json).
The 500 MiB result uses a capped input, not a 500 MiB input plus 4 KiB.
The G2 values come from the [#152 C1/G2 candidate report](../../../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md),
which timed **direct SDK range Edit → Commit** at commit `8b5e0955e`.
They are aspirational per-case engineering targets, not matched ratios or
inherited PASS values. The two failed v2 rows have no completed Edit→Commit
duration. Do not compare their five-second Exec error with a G2 completion.

## Root causes to address and work that remains separate

The v2 tool grows the file, then copies the untouched suffix backward through
128 KiB FUSE read/write blocks. Moving that suffix is proportional to file size;
each projected write also rebuilds a growing overlay-piece vector, introducing
an `O(callbacks²)` metadata-work mechanism. The 100/500 MiB requests exceeded
the product's five-second **silent Exec progress window**, although their
complete benchmark commands stayed under the 15-second outer cap. Current
8 MiB non-base replay and 1,024-piece bounds make merely extending that wait
an inadequate fix. These are source mechanisms and observed outcomes, not a
proved asymptotic bound for every edit.

`service.finish` also grows in the retained 4 KiB **overwrite** Commit rows;
its storage subcause remains unknown. Diagnose it with bounded nested
`layerfs-telemetry` scopes and count evidence before changing Store behavior.
Do not fold a speculative storage change into the range-edit mechanism or claim
that an insert fix resolves the other #232 families. This child must report
its own Commit result even if the separate Commit optimization is still open.

## Product capability and pre-implementation decision

The product must accept a checked byte range `(offset, delete_len,
replacement)` on an authorized writable handle of the **live projected
Workspace** and apply one logical splice, reusing the semantic behavior of
Core `Workspace::edit_file_range`. The product path must own replacement
bytes, admit the mutation against quotas and replay limits, validate the exact
inode/incarnation/version stamp, and preserve failure, rollback and uncertain
publication rules. On success, file size, mtime, `fstat`, reads through existing
open descriptors, EOF, aliases, subsequent writes, Commit and reopened Branch
readback must agree. A stale handle, unsupported operation or failed admission
must return a typed failure, with no silent suffix-copy fallback.

The [Linux v6.12 FUSE path](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/file.c#L2904-L2924)
rejects `fallocate(INSERT_RANGE/COLLAPSE_RANGE)` before userspace, so adding
only those callbacks does not supply this primitive. Confirm the actual target
kernel profile before freezing the interface. A versioned FUSE
ioctl on an open file is the leading candidate for a cooperating program; a
4 KiB replacement fits its ordinary encoded argument bound. **Before product
implementation**, freeze and review the actual interface and ABI: request
version and layout, bounded payload transfer and ownership, exact authorization
and validation, kernel and inode attribute invalidation, open-FD coherence,
unsupported-kernel behavior, error and partial-publication semantics, and
route/callback counters. Prove the selected mechanism on the live Linux FUSE
kernel, not only against Workspace unit tests. The candidate is not a decision
to expose a 64 KiB atomic replacement; that requires a separate payload design.

The benchmark tool may issue the chosen public mounted-file operation. It may
not call LayerFS internals, write a host path, recognize a benchmark-only
command string in product code, or choose a different algorithm on error.
Ordinary localized `pwrite` and truncate behavior must continue to work for
programs that do not request a splice. The route is independent of the outer
shell: `/bin/sh -c` is today's `WorkspaceApi::exec` launcher, while FUSE sees
filesystem calls from any process under the mount.

## Portable semantics and platform adapters

The range replacement is a **LayerFS Workspace semantic operation**: byte
offset, deleted length, replacement bytes, version/handle authority, result
size, and typed failure/uncertainty. Its definition and Commit lowering must
contain no Linux ioctl number, `fuser` type, macFUSE type, WinFsp type, or
platform-specific cache assumption. Platform adapters translate their native
filesystem requests into that same checked projected mutation; none gets a
second edit algorithm. Ordinary WRITE/SETATTR semantics remain the default
for programs that do not request a range operation.

The current product projection is explicitly Linux-only
([mount implementation](../../../crates/layerfs-fuse/src/mount.rs)); #241's
live Docker and four-case performance gate is Linux. A Linux FUSE ioctl is
one candidate **carrier**, not the universal product API. [macFUSE documents
FUSE_IOCTL support](https://github.com/macfuse/macfuse/wiki/FUSE-Features),
but its ABI, payload and kernel invalidation must be proved on macOS before
that adapter advertises range replacement. [WinFsp distinguishes its FUSE
compatibility layer from the native Windows filesystem API](https://github.com/winfsp/winfsp/wiki/Native-API-vs-FUSE);
a Windows adapter must select and prove a supported request mechanism and
Windows open-handle/size semantics rather than assume a Linux ioctl works.
Future support also needs native mount and process-launch integration for the
same public SDK behavior; adding a range callback alone is insufficient.
Unsupported platforms return an explicit typed `Unsupported` result before
mutation. A platform is not called supported until its mounted end-to-end
tests prove the same bytes, metadata, coherence, Commit, and failure behavior.
Do not pool latency or cache claims across platforms.

Use published third-party packages unchanged. Do not patch, fork, vendor,
`[patch]`/`[replace]`, or edit a package registry or installed macFUSE/WinFsp
files to obtain an ioctl, notification or mount behavior. If an unmodified
provider cannot deliver a required operation, retain that platform as
unsupported and report the blocker; keep builds locked. This requirement
applies to the Linux implementation now and future macOS/Windows adapters.

## Functional position proof (not performance samples)

For each of the four pristine input sizes above, exercise **insert, overwrite
and delete of 4 KiB** at 16 positions spread across each operation's legal
range: 4 sizes × 3 operations × 16 bands = 192 named functional checks. The
legal inclusive interval is `0..=N` for insert and `0..=N-4096` for overwrite
and delete. For operation `op`, size `N`, and zero-based band `b` in `0..15`,
let `M = max_offset + 1`, `lo = floor(b*M/16)`, and
`hi = floor((b+1)*M/16)-1`. Choose
`lo + (SHA256(UTF8("issue241-position-v1|" + op + "|" + N + "|" + b)) mod
(hi-lo+1))`, interpreting the digest as one unsigned big-endian integer.
This pinned hash seed and rule choose a reproducible, approximately uniform
offset within each equal-width band. No alignment or selection after seeing
outcomes is allowed. **Materialize and publish all 192 exact offsets and IDs
in an identity-hashed manifest before running any check.**

Add separately named exact-edge checks at offset 0, midpoint, last legal
offset, and selected actual pristine canonical-chunk boundaries (including
adjacent byte offsets where legal). Freeze those offsets in the same manifest
before execution. Each check starts from independent pristine file/Workspace
state derived from one validated, closed master per size; do not regenerate or
reimport a 500 MiB fixture for each position, and never use a prior check's
mutated file. The sweep is extended **functional** qualification, outside the
four selected performance samples. Validate final bytes and size, untouched
prefix and suffix, open-descriptor visibility, canonical root/count, Branch
head, retained old Commit and cleanup for each offset. Report every failure
with its exact offset. Position-independent latency is **not** established by
these functional checks; that claim needs a separately preregistered timed
position sweep with its own identities and cache contract.

## SDK route, measurement and custody

All product setup and teardown use public `layerfs-sdk`: Project/Branch,
Sandbox Create, Workspace Mount, Exec, Commit, Status, Unmount and Sandbox
Delete. The host `Server` may assemble the route; the benchmark must not
directly mutate Service, Store or Workspace. Reuse one sealed prepared master
per size via an independent writable byte copy for each selected case. This
reuse is outside the timer and is **not** a cold-cache claim.

```text
untimed, reported: prepare/acquire master → copy → SDK Branch/Sandbox/Mount
layerfs-telemetry edit_commit root starts
  edit child:   WorkspaceApi::exec(command) → tool → mounted FUSE splice
  commit child: WorkspaceApi::commit(workspace_id)
root stops on typed Commit result
untimed, reported: SDK Status → Unmount → Sandbox Delete; retain cleanup proof
separate command: independent verifier reopens published and old versions
```

The Edit→Commit root starts immediately before Exec and ends after the typed
Commit result. A failed Exec, or an Exec whose output is truncated, is a
benchmark error under this contract: the driver stops before Commit and cannot be
reported as a completed Edit→Commit time. No status query, readback, assertion
command or telemetry publication goes between Edit and Commit. Use only
`layerfs-telemetry` LFT1 for operation wall, CPU and sampled RSS; retain raw
caller, Service and daemon records, producer identities, scope completeness,
resource coverage and zero dropped records. Host caller/Service CPU and RSS
share one process window, daemon has another, and shell child usage is not
included in daemon RSS. `UNAVAILABLE` resource samples are not zero. Report
complete-command wall, preparation, verifier and cleanup separately.

Freeze and commit a **new scenario version**, tool command and digest, source,
product/harness/compilation/dependency seals, release binaries, image digest,
fixture/payload hashes, callback expectations, oracle and cache contract before
sampling. Use `--locked` release builds for host driver, verifier, daemon and
tool, and one construction worker. One sample per case and identity; each gets
a fresh output path. Keep failures and ineligible receipts append-only. Do not
rerun for a better number, warm or pre-touch measured paths, raise the product
progress or benchmark deadlines to turn a miss into a pass, add workers, shrink
fixtures, or move product work into setup. The complete performance command
must fit 15 s; the independent verifier has the existing #232 15 s hard cap
and is expected to finish well under 10 s. A timeout remains a failure.

The frozen #232 cache contract currently marks the Linux FUSE backing domain
`INELIGIBLE` when Commit can consume Edit's resident writes. Preserve that
classification unless an authentic, prospective cache contract can prove
equal eligible states; do not call an ineligible fast timing `GOAL_MET`.
V2 and new-scenario rows cannot be pooled as one arm because the editor
algorithm, command and cache state may differ. Show each new raw duration beside
the old diagnostic and G2 context with that limitation explicit.

## Acceptance and parent boundary

Each of the four cases must complete Exec **and** Commit, pass the separate
identity-matched oracle, meet the command/verifier budgets, prove mounted-FUSE
range calls with bounded bytes/callbacks independent of untouched suffix size,
and confirm Sandbox deletion. The oracle checks exact bytes/size, metadata,
canonical root/count, Branch head, retained older Commit and fresh reopen.
The functional position sweep and live-kernel open-FD/coherence/error checks
must pass. Report raw LFT1 Edit and Commit time/CPU/RSS coverage, the cache
classification and each per-case G2 target result without relabeling a miss.
These gates permit a child implementation-complete result even when cold-cache
eligibility or the aspirational G2 latency remains unmet; that result must say
so explicitly. **#232 retains its separate 56-case eligible performance and
admission gate** and cannot be closed by four functional completions.

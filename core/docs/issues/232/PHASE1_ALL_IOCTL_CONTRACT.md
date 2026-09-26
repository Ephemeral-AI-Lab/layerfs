# #232 Phase 1 all-ioctl contract and selection

> **Status:** Current planning checklist; no release candidate exists.
> Frozen before staged-carrier product work or a new timed sample. A failed
> provider probe requires a new contract identity; it cannot silently alter this one.

> **Historical scope:** This frozen v3 selection measures a cooperating
> program that explicitly issues mounted-file ioctls. Its 56 completed rows
> remain [opt-in evidence](evidence/phase2-all-ioctl/REPORT.md); the owner's
> arbitrary-shell route is [separately assessed](SHELL_ROUTE_CORRECTION.md).

This is the prospective `workspace-exec-edit-v3` parent scenario. The
[immutable registry](../../../benchmark/fs-bench-pro/registry/workspace-exec-edit-v3.json)
inherits the 56 fixture, replacement and oracle identities from the untouched
[v2 registry](../../../benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json).
The historical #241 four-case v4 selection is separate. The implementation
[plan](UNIFIED_IOCTL_IMPLEMENTATION_PLAN.md), [#232 specification](SPEC.md),
and [benchmark rules](../../../../docs/general/benchmark_rules.md) apply.

## Semantic and SDK boundary

The one platform-neutral operation is
`replace(open_handle, expected_stamp, offset, delete_length, replacement_stream)`.
The stream is an ordered sequence of `Bytes` and `Zero(length)` elements; an
empty stream deletes. Each element's logical length contributes to the **8 MiB
maximum replacement length**, including zeros. With checked `u64` arithmetic,
require `offset + delete_length <= old_length` and
`old_length - delete_length + stream_length <= 4 GiB` before any Workspace
mutation. Overwrite, insert, delete, prepend, grow, shrink, append, tail
truncate and zero extension are parameter choices, never separate mutation
opcodes. One accepted replacement publishes one private Workspace revision and
one mtime; staging changes neither. Preserve the current edit-count, piece,
replay, Bridge, Service, quota and failure limits. An oversized replacement or
result returns definite Capacity/`ENOSPC` before mutation. No POSIX fallback.

The benchmark host uses public `ProjectApi`, `SandboxApi` and `WorkspaceApi`
only. Its one timed command is public `WorkspaceApi::exec` running a cooperating
editor on the mounted file, followed by public `WorkspaceApi::commit`. The
editor obtains `STATE` on that open descriptor, issues the mounted FUSE ioctl,
then confirms a revision increase of exactly one, size, mtime, open-FD bytes and
EOF. Command text is opaque to LayerFS. Linux FUSE is the current carrier;
macFUSE and Windows require separate capability proofs.

## Prospective Linux wire version 3

Retain frozen LFS2 `STATE` and LFE2 inline `EDIT` for a byte stream of at most
4,096 bytes. The staged commands use `_IOC` type `0xf5`, version `3`, little
endian integers and zero reserved/tail bytes. Command numbers `0x42` through
`0x45` are respectively BEGIN, DATA, APPLY and ABORT. BEGIN, APPLY and ABORT
have **128-byte total frames**; DATA has a **4,224-byte total frame**, a
128-byte header and at most 4,096 literal bytes. The live provider probe
records actual input/output lengths and flags before this ABI is implemented.

BEGIN carries the LFS2 serial/incarnation/generation/revision stamp, offset,
delete length, declared logical and literal lengths, and SHA-256 of the
logical replacement bytes. It returns a 128-bit unpredictable, single-use
stage token. DATA carries that token, the next logical stream offset, an
element kind (`Bytes` or `Zero`), its length and, for `Bytes`, the literal
payload. The semantic SHA-256 hashes zeros as zeros using a bounded scratch
buffer. DATA offsets must be contiguous and strictly ordered. APPLY carries
the token and validates complete length and digest before taking one Workspace
mutation permit and calling the semantic splice once. ABORT consumes the token
without publication. An inline request reaches the same semantic splice.

The token is bound to the FUSE open handle and Workspace incarnation. BEGIN
reserves its entire declared logical length under an **8 MiB aggregate stage
quota**, so concurrent BEGIN calls cannot overcommit. Stage literal custody is
bounded by the same quota; an implementation must account for its transient
duplicate buffers and backing page cache. Release of the descriptor, ABORT,
unmount, daemon shutdown or a 30-second monotonic stage deadline discards an
unfinished stage and releases its reservation. BEGIN/DATA hold no Workspace
mutation permit. Malformed frames, bounds and overflow: `EINVAL`; unknown
command: `ENOTTY`; unsupported version/flags: `EOPNOTSUPP`; wrong FD or
incarnation: `EBADF`; stale stamp: `ESTALE`; concurrent mutation permit:
`EBUSY`; quota/capacity refusal: `ENOSPC`. The editor maps a definite
prepublication `ENOSPC` to Capacity and stops. A lost APPLY reply, transport
loss, or postpublication notification failure is **UNKNOWN** (editor exit 75),
never automatic replay. The SDK Exec result is accepted for Commit only for
exit status zero, untruncated output and the exact confirmed editor PASS line;
other outcomes stop the automatic workflow. The public SDK does not claim to
resolve a lost ioctl acknowledgement into a definite rollback.

## Frozen cases and custody

The [registry](../../../benchmark/fs-bench-pro/registry/workspace-exec-edit-v3.json)
has 12 preserving, 32 changing and 12 canonical chunk-count rows, one attempt
per frozen source/case/arm. The largest replacement is 64 KiB in exactly 12
rows. A Bytes payload of 4 KiB or less uses inline EDIT (two STATE and one
EDIT callback); deletion also uses inline EDIT. A 64 KiB Bytes replacement
uses BEGIN, sixteen ordered 4 KiB DATA callbacks and APPLY, plus two STATE.
A Zero run uses one DATA descriptor with no literal bytes. All 56 operations yield
one revision. The generic tool command and independent final-file digest,
bounded window, canonical root/count and old-Commit oracle are pinned per row.

The source baseline is `87b10ab1b2144f1d7fb54f1be9419095f891fea0`;
the reviewed planning import is `8d50a7279330cdb57960c84419e69abdc78d1d9b`.
The **executed** source tree, tool binary SHA-256, locked release compilation
seal, immutable Docker image ID, kernel/fuser identity, harness/registry SHA-256
and fixture/payload seals must be recorded in a separate append-only pre-run
manifest before the first attempt. A changed product or tool source gets a new
manifest and cannot reuse an arm or verifier from another identity. No image
or tool binary exists yet for this prospective v3 scenario; an invented digest
would not freeze an executable.

The minimal Phase 1 performance selection is the four 4 KiB middle inserts at
1/10/100/capped-500 MiB, three 64 KiB 1 MiB chunk-count outcomes, and one
capped-500 MiB 64 KiB locality case. The four insert raw engineering aims are
respectively 55/55/60/70 ms, and service.finish 500-minus-1 MiB growth aims
at most 15 ms. Other raw targets are set **before** their first candidate
performance attempt by an append-only target manifest from an untouched
same-route baseline; until then they have no numeric latency PASS. The
complete-command gate is at most 15 s and the independent verifier is under
10 s for every attempted row.

The Edit→Commit operation wall/CPU/RSS is LFT1 only. Setup uses sealed,
independent writable master byte copies outside the timer; this does not
establish a cold cache. The macOS Store residency and Linux FUSE backing
domains are checked and classified separately. If Edit's resident backing
bytes can serve Commit and cannot be invalidated and checked equally, the
row's raw time is `INELIGIBLE` for a cold latency claim. Unknown cache state,
missing telemetry/resource coverage, identity or route evidence is
`INCOMPLETE` or `INELIGIBLE`, never PASS. Each row retains callback counts,
LFT1 raw records, complete-command wall, an independent identity-matched
verifier and public `SandboxApi::delete` cleanup confirmation. Receipts are
append-only, including failure and unrun rows.

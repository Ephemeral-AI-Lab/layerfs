# R1 supervision and bounded root-binding checkpoint

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This continues the active full-cluster-two goal on primary local `main`, starting
from `da331dfb607f5939b3e150c82480faabf6fb258c`, tree
`29ed5e63eb40baad5db196ac970be1ff35d93bf9`. The starting product subtree
`e67f06e72239363ffb8614ed5cecc08d1e906474` is the restored incumbent. The
[initial reconciliation](checks/continuation-20261007-r1-e1/initial.json) records
the tracker boundary, preserved notes/containers and unchanged prior verdicts.

## Behavior and ownership

R1 composes the existing initialized Runtime, borrowed Sessions, fair Service,
authenticated native connections and independent socket input/output owners.
Each connection serves one bounded original exchange; other connections continue
service when its header/body/output is blocked. Provider work runs on the serving
thread without holding a lock over socket I/O or a complete Save/Commit lifetime.

Reply capacity is reserved before provider submission and encoding. Original
requests, partial bytes, typed results, unsent replies and terminal send permits
retain their charged owners through exact service/input/output completion fences.
Caller-held Delivery/Fence values keep their credits. Local socket shutdown/write
is neither a history-publication outcome nor proof of remote consumption. No
request, failed transfer, Finish, Stage, transition or discard is replayed.

Consumer Attachment supplies the ordinary existing object/length/serial ports and
an independent close handle. It refuses a wrong-kind reply budget before moving
the connection, and separates socket close from completion of its original call.
No provider, namespace, dependency restoration or whole-Save connection checkout
is added. [The architecture](../../architecture/45-runtime-supervision.md) owns
the public types, custody boundaries, actual counters and cost model.

The focused R2 binding slice validates the actual root directory inode along its
canonical inode path, its zero-reference invariant, directory root grammar and
required portable mode/mtime. Every demanded child is authorized before provider
acquisition. The first original authority or Storage failure survives Content's
narrower provider interface. It performs no whole-tree scan. Full direct-child,
saved-closure, membership, alias/cycle and incremental topology acceptance remains
open; [root binding](../../architecture/22-sdk-runtime.md#r2-root-binding-context)
states this scope.

## Review and checks

Independent source review covered the runtime/credit/fence and root context
changes. It corrected unusable metadata readiness, wrong-kind consumer admission,
terminal permit reuse, allocation-capacity accounting and overstated retained-
delivery progress. Six supervisor tests use actual macOS Stores and authenticated
sockets. The blocked-output selection demands 65 valid WholeFile objects of
128 KiB minus one byte, preserving a reply larger than 8 MiB.

| Check | Outcome and exact scope |
| --- | --- |
| Host locked SDK all-target no-run | PASS at final source in 18; earlier PASS 04 and first test-API compile FAIL 03 retained |
| Host public SDK bodies | All 43 PASS at final source in 19: attachment 2, runtime 19, root binding 7, supervisor 6 and wire 9; earlier scoped results 05/07 retained |
| Initial supervisor run | FAIL in 05: invalid oversized fixture and assertion while holding the fence's actual completion credit; wire NOT_RUN in that invocation |
| Linux ARM64 locked SDK no-run | PASS in 09; global Store remains macOS-only |
| Linux portable public bodies | 11 PASS in 10: attachment 2 and wire 9; runtime/root-binding/supervisor targets execute zero bodies there |
| Host/Linux SDK all-target Clippy `-D warnings` | PASS in 11/12; original external-test assertion-style FAIL retained in 08 |
| Core workspace format | PASS in 14; coordinated entry-module order FAIL retained in 13 |
| Product boundary guard | PASS in 15, 669 production Rust/SQL files; a source guard is not runtime proof |
| Core tooling/provenance/guard self-tests | 40 PASS in 16 |
| Performance, phase residency, physical I/O, sustained service | NOT_RUN; no numerical qualification inferred from these functional checks |

Every test invocation had an explicit outer ceiling at most 120 s. Docker commands
add a 110 s inner timeout and a one-second termination escalation; no ceiling was
reached. All commands, raw stdout/stderr, wall values and failures are
[append-only](checks/continuation-20261007-r1-e1/), including the
[failure diagnoses](checks/continuation-20261007-r1-e1/FAILURES.md). Caches are
uncontrolled, construction environment is `LAYERFS_CONSTRUCTION_WORKERS=1`, and
repository ARM64 flags remain in effect without an override. The pinned Docker
image is `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Only task-owned transient containers were created and removed. FUSE is excluded
from this dependency graph; these checks do not build or qualify native FUSE.

The final thin-module format changes order declarations/reexports only. Earlier
functional results retain their original receipts/source scope. A final covering
build/proof at the [final SDK source manifest](checks/continuation-20261007-r1-e1/r1-final-source.json)
ran once after those changes in 18/19. No receipt is relabeled as a newer
performance observation.

## Complexity and remaining work

For configured concurrent connections C and service jobs J, startup registry work
is O(C + J); attachment and dispatched-ticket matching cost O(C). A turn polls
one attachment and invokes at most one existing bounded provider unit. Across M
calls, routing is O(M*C), with C a declared simultaneous admission window, plus
the actual bodies, provider and native work. Input/encoding/copies follow bounded
bytes B and error fields E. No resident namespace or lifetime-call collection
is introduced. Root validation follows one inode path, one directory page, two
attribute-key paths and bounded 4/12-byte mappings, rather than base population.

No SQL, schema, global provider algorithm or canonical format changes. Existing
SQL plans/profiles retain their owning identities; complete attribution of the
newly composed route is still E2. Source bounds are not phase RSS, pager/journal,
device-byte, service-rate or debt measurements. E3/E4 and native attribution stay
explicit. The entire daemon reservation remains an independent required charge.

S0/S7/S8/S9/S10/S11/S12/S13 remain incomplete. R3 must retain terminal-unknown
and known-failure custody through explicit scope/process fences without inventing
crash recovery; Durable Store contents alone do not persist session knowledge.
R2 direct-child admission and K2 topology, R4 real macOS-host/Docker assembly,
E1 complete route/observer/numerical registration, E2–E4 and Q1 full/huge/dense
root proofs remain ready work. E1 tooling is being independently reviewed; all 27
samples remain NOT_RUN. The active goal continues beyond this local checkpoint.

The [restoration verdicts](INCUMBENT-RESTORATION-RESULTS-20261007.md) remain:
all eight Init speed/strict allocation FAIL and all six approved history pairs
PASS their own bypass-acquisition scope. The correct Init control is public
Project `197d2fb7d`; the older Service `7edddbdb8` comparison stays distinct. No
withdrawn payload/streaming experiment, resampling, push, release or deployment
was performed. Reference retirement still requires actual S12 acceptance.

## Exact source-size comparison

Production LOC: **160473 -> 161586 (delta +1113)**.
Core **95056 -> 96169 (+1113)**; reference **65417 -> 65417 (+0)**.
This adds SDK supervision/attachment/root-context implementation; it is neither
relocation nor reference retirement. Production includes shipped SQL and retained
excluded predecessors; tests, inline tests, docs, tools, harnesses and third-party
source are excluded. Exact first-parent/staged archives use unchanged
`tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
[The source comparison](checks/continuation-20261007-r1-e1/production-loc-source.json)
retains per-crate counts and the method. The final staged and committed tree
comparison is confirmed before advancing the tracker; unstaged E1 work and the
two owner notes are excluded from this commit snapshot.

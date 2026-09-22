# Authenticated daemon Unmount

> **Status: implemented and verified through the actual authenticated daemon/Linux mount route; target v0.1.7, not released.**
> Implementation parent: `451a1f6bdda00482a528659489c8e4d053677e01`.
> Product input seal: `8d74caf369eaa1f104e761f2bb4f51b406b5da655cb8934d87c841717421ef68`.
> Selected 2026-09-22 as one R1-C lifecycle operation after mounted resize.

## Exact operation and authority

`Operation::WorkspaceUnmount { workspace, incarnation }` uses the existing
profile 3 control route and opcode 10. Request ID is positive; Store, generation
and response-data allowance are zero. Workspace ID uses the existing ASCII
managed-name/63-byte bound and incarnation is nonzero and exact. Logical input
is empty but authenticated END_INPUT must still arrive. The request is at most
124 bytes and its declared deadline is 1..5000 ms. No new client, protocol, Source
path or per-canonical-object request is introduced.

Control grants preserve Status bit 1 and add Unmount bit 2. Mask 0 is a valid peer
with no operation permission; bits outside 3 are invalid. Keys/expiry are daemon
control authority, independent of service grants. The daemon checks request,
operation grant, exact launched target/incarnation and complete empty input, then
rechecks deadline/expiry/stopping before acquiring native operation ownership.
Service owners explicitly reject both daemon controls before Store lookup or
admission; opcode 10 gains no Store permission bit.

`Response::WorkspaceUnmount(Box<WorkspaceUnmountWire>)`, response tag 12, echoes
exact identity and carries `WorkspaceUnmountOutcome::Unmounted` or
`Retained(Code)`. The latter accepts only Deadline, Io, Busy or Unsupported.
The maximum success/retained encoded sizes are 99/100 bytes, with zero ResultData.
The native Client verifies exact requested identity and data count. Unmount is
read_only=false, content_mutation=false, metadata_mutation=false and mutation=true:
this lifecycle effect must participate in transport uncertainty classification.

An ordinary checked Failure is refusal **before native Unmount admission**.
Retained is a checked result **after native Unmount was entered**, with its owner
still held. It is not success or unchanged-state confirmation: admission may have
stopped, or detach may have happened before a later drain/join/absence check failed.
Unmounted means the native operation completed drain, detach, join, checked mount
absence and lease completion. It does not clean-close, Commit, discard, delete
backing, refund the Workspace registry slot, remount or exit the daemon.

## One native owner and bounded completion

Main assembly and control share one `Arc<Mutex<MountHandle>>`. Control takes it
with immediate try_lock; contention is a pre-admission Busy refusal, never a
waiting queue. This mutex owns lifecycle exclusion, not Workspace state; FUSE
callbacks and Status do not acquire it. The final stopping check under this owner
is the admission linearization point. A request admitted just before shutdown is
joined as accepted work, rather than being silently reclassified as unstarted.

The result identity/box is prepared before native admission. Control reserves
100 ms for terminal construction/delivery **inside** the original deadline:
native Unmount receives `deadline - 100ms`. If that point has already arrived,
the request is refused before entry. This neither renews the deadline nor promises
preemption of mount/unmount/notification syscalls. A late syscall or delivery can
still prevent the result frame from arriving, leaving the client Unknown.

The owner mutex is released before terminal encoding/sending. Known success
remains in MountHandle even if transport delivery fails. Following a checked
Retained timeout, a later explicit request may continue cleanup of that same
retained owner/incarnation; the handle remembers whether detach was already
attempted. A terminal cleanup failure is not blindly retried. After missing,
malformed or mismatched delivery, Unknown does not authorize automatic replay or
a new identity. Status is a fresh observation, never a replacement receipt for
the missing operation.

The existing single control session/Q=0 policy remains. An in-progress Unmount
occupies that session; a second network Status connection is refused while it
remains live/closing. No extra worker, queue, helper lane or control retry is added.
The 100 ms allowance is selected completion headroom, not a measured latency claim.

## Explicit signal cleanup and retained runtime

Signal shutdown first stops control admission and shuts down/joins its session,
then accesses the same native mount owner, unmounts and clean-closes. It never
starts competing native cleanup when control stop has not completed. One explicit
signal has the existing 10-second cleanup budget. If cleanup is incomplete, the
process and owners remain alive and log the retained outcome; another explicit
termination signal can continue checked cleanup. There is no automatic renewed
budget, retry, remount or restart recovery. An unmount success by the control
route is preserved when later signal cleanup uses the same finished handle.

Startup remains `--mount-readonly`. This operation does not add writable startup,
Attach, CloseClean, SDK edit or Commit controls, and does not qualify those
management routes. Local mounted writable proofs retain their prior separate
scope. Broader namespace/new-inode/npm work and R6 remain open.

## Source/resource and verification scope

Bridge owns the operation/result contract, bounded codecs and exact native Client
matching. Daemon owns grants, endpoint dispatch and the shared MountHandle. Service
only gains explicit early refusal. Workspace/FUSE algorithms and library dependency
direction remain unchanged. ControlGrant's operation byte replaces its former
Status bool without another grant entry. The new native-owner Arc/Mutex and bounded
references belong to the daemon's fixed lifecycle account; no Workspace working
budget, payload window, FD or thread count is expanded. The fixed allocation formula is the aligned combination of a two-usize Arc
header with `Mutex<MountHandle>`, plus bounded Arc references in the existing
main/acceptor/session owners. The former MountHandle value is moved into that
allocation. It does not create a second handle or native device FD. No measured
heap/RSS total is inferred from this formula; the maximum new response 100 bytes
remains below the existing Status response 139-byte envelope.

Actual native codec/client, service refusal, mounted control and existing Status
results follow. Every check remained within its declared route; no CI/preflight,
performance campaign or issue closure occurred.

## Actual verification

All eight new actual daemon-control selections and the original Status route pass:
**9 PASS, zero functional failures**, one selection per declared case. Every new
case creates a real Linux mount through the production daemon binary, targets its
independently authenticated endpoint through the existing public native Client,
and verifies ordinary daemon shutdown/Workspace cleanup afterward.

| Selection | Complete command seconds |
| --- | ---: |
| success | 3.138212791 |
| authority | 2.001261500 |
| expiry | 10.026888250 |
| timeout | 1.420619458 |
| loss | 1.100682292 |
| partial_input | 1.390457708 |
| signal | 1.087355167 |
| shutdown_retained | 11.100452667 |
| existing Status regression | 24.882436125 |

The success selection confirms service SIGSTOP before Unmount and completes both
Unmount and subsequent Status before service resume, establishing the local
lifecycle route. The Workspace remains open/accounted, its mount directory exists,
and Status is available after checked detach. Only a later explicit signal closes
it. Status-only/no-operation peers, wrong target/incarnation and expired established
peers are refused without stopping the mount. An Unmount-only peer cannot read
Status but can unmount. The actual service endpoint refuses control despite all
Store grant bits.

With one actual projection FD held, the 500 ms request returns checked
Retained(Deadline), with mounted=true/stopping=true/handles=1. After that reader
explicitly releases, a new request on the same target continues teardown and
returns Unmounted. The allowance is not promoted to a delivery guarantee: an
opaque authenticated-channel proxy separately drops the terminal after native
completion, yielding Unknown. Only one Unmount request is submitted; subsequent
Status observes absence without being treated as the missing receipt.

An incomplete authenticated input never admits native Unmount. A concurrent
control/signal schedule first observes kernel admission stopped while a real FD
remains held, then receives client Unknown from socket closure before releasing
that FD. Native control completion and main shutdown use the same retained owner,
and the daemon closes normally. A separate signal-only attempt with a held FD
exhausts its original 10-second budget and logs retained Deadline while the process
stays alive. Releasing the FD and sending another explicit signal completes cleanup;
no automatic renewed budget or retry turns the first attempt into success.

Bridge tests use authenticated sockets for exact Unmounted/all four Retained
outcomes and pre-admission failures. Ten malformed/mismatched/lost/illegal-data
variants return Unknown with empty output and one received request. Codec tests
exercise exact bounds, profile/identity, truncation/trailing data and all 256 encoded
retained-code values. The public service test refuses both controls before reading
input/admitting a Store, including through the native transport.

Locked Rust 1.85.1 host whole-core tests pass 657, zero failures/three ignored.
Linux bridge/daemon/service tests pass 84, zero failures/ignored. Whole-core
host/Linux Clippy with all targets/-D warnings, examples/binaries, fmt, the 241-file
boundary guard and all six self-tests pass. No compile, unit, lint or functional
failure occurred in this round. Workspace/FUSE algorithms are unchanged; their
older writable proofs were not rerun or relabeled. The existing actual daemon
Status regression was rerun because dispatch/grants/shutdown ownership changed.

Every complete command fits the 60-second functional budget. These wall values are
functional budget observations, not performance evidence. Source stayed frozen;
no same-worktree build overlapped selections. Each receipt retains the actual
interference snapshot and original immutable binary/source identities. Fixtures
reuse independent byte copies of the closed read-only Store/history, with no
claim of restarted writable history authority, cold state or RSS/cgroup limits.

[Raw receipts and remaining rows](evidence/control-unmount/functional-index.json),
[compiled/executed identities](evidence/control-unmount/control-unmount-inputs-01.json)
and [implementation review](evidence/control-unmount/source-review.json) retain
these scopes separately. No issue is closed.

Reproduce the locked host commands from round 26. Linux selects
`-p layerfs-bridge -p layerfs-daemon -p layerfs-service` for tests and uses
whole-core all-target Clippy and examples/binaries, with the same tool image,
read-only registry and owned target directories. Then select each case once at a
fresh output:

```sh
python3 core/crates/layerfs-daemon/tests/control_unmount.py \
  --fixture core/target/pair1-evidence/mounted-07/result.json \
  --binaries <host_binaries-from-inputs-01> \
  --linux-daemon <linux_daemon_binary-from-inputs-01> \
  --case timeout --output <fresh-owned-output>
```

ARM build config remains SHA256
3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9;
construction workers remain 1. The next lifecycle operation is separately declared
CloseClean of an already unmounted clean target, preserving dirty/held/failed
ownership and exact outcome reporting. Writable controls, namespace/new-inode/
larger-input work, declared npm and R6 remain open.

## Exact production LOC and changed files

First parent `451a1f6bdda00482a528659489c8e4d053677e01`; counted staged tree
`a768a20ad3bd62ed025ca19f791af59632738c68`. Production LOC: **108284 → 108474
(delta +190)**. Reference 65417 → 65417 (+0); core 42867 → 43057 (+190).
Bridge 4183 → 4288 (+105), daemon 710 → 790 (+80), service 2024 → 2029 (+5).
Workspace and FUSE implementations are unchanged. Growth adds this authenticated
lifecycle operation and ownership/error handling; no reference retirement or
relocation is claimed. [Exact changed source/test paths](evidence/control-unmount/changed-files.json).

Method: `git archive <revision> crates core/crates`, then identical
`python3 tools/production_loc.py --root <archive> --json`; counter blob
`b5b9617d08204977176302311e0b2c72a811b420`. Nonblank/noncomment production
Rust/runtime SQL, excluding inline/external tests, fixtures, examples, docs, tools,
manifests and generated output.
[Machine-readable comparison](evidence/control-unmount/production-loc.json).
Final receipt/doc additions do not alter the counted production tree.

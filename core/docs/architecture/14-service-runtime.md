# Service, bridge, daemon and hosted telemetry

> **Status:** Current general guide.

This describes the issue #192 implementation candidate for v0.1.7; it is not a
released contract or performance qualification.

Source basis: optimization work after documentation checkpoint
`0819f3f39833d477d9ed6d878a50691c3c046a83`, preserving the selected C1 production
checkpoint and dependency lock. The
[C2 schema-7 checkpoint](proposal/service-daemon-transport/implementation/evidence/optimization-c2-checkpoint-20260920.json)
binds the tested storage prerequisites; source inventories in each new command
receipt bind the subsequent service integration. The original implementation seal
and image do not identify this revision. The
[optimization decisions](proposal/service-daemon-transport/implementation/09-optimization-decisions.md)
and [acceptance record](proposal/service-daemon-transport/implementation/07-acceptance.md)
state exact selections and incomplete proofs. These changes do not rewrite Stage 6
results or qualify #193 performance work.
The native Init progress-record update describes product commit
`1f74d80be12ad19de1335d39fad0688ecc48c0f8`; the older sections retain
their separate source bases.
The Init ordering-backing correction describes product commit
`0042a909ac3f16a5041aa51d76f96522a58352c8`; older sections retain
their separate source bases.
The #243 range-ioctl retirement describes the FUSE source in the same commit
as this note. Historical range-ioctl paragraphs below describe the earlier
opt-in carrier; the current Linux adapter returns `ENOTTY` for ioctl, and
ordinary Workspace writes continue through FUSE WRITE and the internal
Workspace piece operations.
The #232 Exec progress rule below describes the bridge and daemon source in
the same commit as that rule; earlier sections retain their stated bases.

The agent-facing project Init route is based on `main` at
`7df25f9790996cf83232782c7b35f7c26fcc3252` plus the #236 source change.
The host Service accepts a checked, request-scoped directory binding for its
existing native importer. `layerfs-sdk::Client::init_project(name, path)` calls
that Service directly. The Service generates a stack body and scope seed for
each call, runs its normal authorization and writer admission, and returns the
published genesis record. The native daemon request retains its startup-bound
root. Invalid sources are refused before the import call; name collisions are
refused by C5 publication. The later agent SDK implementation below adds
Workspace lifecycle and Exec through the distinct daemon control authority.
This route has no benchmark qualification from older daemon-host receipts.

The follow-on agent route is described against worktree base
`13773c5896c30c19047bdfabced4aa25efb27034` plus the product changes in
this commit. `ProjectApi::init` retains the host-direct Service import;
`layerfs-sdk::Client::init_project` remains a compatibility delegation.
`layerfs-sandbox` owns one concrete Docker deployment profile and an in-memory
Sandbox ID to daemon registry. Each created container starts its daemon idle,
reports an assigned Sandbox ID and fresh daemon instance over authenticated
control, and receives no Workspace at creation. The owner checks that identity
at creation and each routing lookup. Workspace IDs route through an owner
binding to one Sandbox ID, daemon instance and Workspace incarnation. An owner
process restart loses the in-memory registry and requires external lifecycle
reconciliation; there is no persistent sandbox directory in this revision.
The control-session refinement after source commit
`f61f575f4c5355fefdde347e68294329bc28545c` retains the authenticated
connection used by checked `SandboxHello` and sends the Workspace operation as
the next request on that connection. It still checked the Docker-published port
on every lookup at that revision. The original fixed-port refinement after
source commit `c45e93d4a7eb2a8f41d1803f704a881f41fe5282` selected a free
loopback port at Create, released its reservation, and then requested that
mapping from Docker. The #241 correction based on source
`a23507d1303d82c4a96595c4f1db01a113ee84ea` lets Docker allocate a loopback
port with container launch. The owner retains the assigned Sandbox ID before
launch, records the endpoint only after one checked `docker port` query, and
refuses routes while that endpoint is pending. Workspace calls use the recorded
endpoint and still authenticate Hello and check the daemon instance before
mutation. Docker may assign a different port after container restart; if the
recorded endpoint fails, a changed published mapping classifies the old route
as stale without replaying its operation or changing the recorded endpoint.
A failed launch or port query retains the Sandbox ID for cleanup; it does not
retry or choose another route.
The fixed deployment profile uses two CPUs, 512 MiB memory/swap, 64 PIDs,
a read-only image root with a 16 MiB `/tmp`, `/dev/fuse` and `SYS_ADMIN`, and
a separate writable Workspace volume. The image digest must contain both the
daemon binary and `/bin/sh`; readiness checks the authenticated daemon and shell.

Control profile 3 adds Hello (opcode 17), selected Workspace Open (18), and
Exec (19) to the existing authenticated codec. Open carries a Project ID,
Branch ID, optional exact Commit ID and expected daemon instance; the daemon
rejects an instance mismatch before attaching, resolves and checks the
selected history before writable attachment, and mounts the resulting FUSE
Workspace. Entered failed or uncertain attachment retains custody and never silently
changes the Branch. Exec runs `/bin/sh -c` in the mounted directory, caps each
output stream at 8,192 bytes, returns truncation flags and exit status, and
kills its process group at its 30-second deadline. Exec never publishes; only
the existing explicit Workspace Commit operation does. Unmount detaches FUSE
but keeps dirty Workspace ownership. A daemon instance change invalidates old
Workspace bindings; a Sandbox ID alone grants no control authority. These
operations have no benchmark receipt or performance qualification here.
The Exec readiness refinement after source commit
`61431f3e0` polls the child stdout/stderr pipes for output or closure rather
than parking for a fixed 5 ms while either pipe remains open. The deadline,
8 KiB stream caps and process-group cleanup stay the same. If both pipes close
before the child exits, the bounded 5 ms process-status wait remains.
While waiting for a mounted Exec child, the daemon checks the Workspace's
accepted revision at most once per second. An increase sends one authenticated
`ResultData [0]` progress record on that Exec response. The client accepts that
exact marker for Exec without counting logical result bytes; unrelated operations
still reject it. A marker acknowledges published Workspace mutation, not child
completion or Commit. No marker is sent for an unchanged revision. The existing
five-second no-wire-progress limit, 30-second Exec request deadline and bounded
response-frame count remain in force.
The control acceptor revision after source commit
`00e7374ff1e6d86d176aab90379bf040a3cf036f` waits for listener readiness
instead of sleeping for a fixed 10 ms when no connection is queued. Its poll
still checks stop admission within 10 ms, and it retains the single live or
closing session limit and immediate refusal of excess connections.

The #236 route speed alignment is based on product checkpoint
`bad49805cb4cacdd29ed15ed5f44e20274e63a3a` plus the changes in this commit. It
changes transport lifetimes, adds bounded diagnostic spans and counts, and
alters no format, bound or authorization rule.

**Daemon-to-host transport reuse.** The daemon delivery closure previously
opened a fresh authenticated Service connection per upstream request, so a
Mount that issues `HistoryQuery` and `Inspect`, and a Commit that issues
`EditFile`, `UpdatePortableMetadata` and `HistoryCommand`, each paid a TCP
connect and a Noise handshake. The native server already admits many successful
requests per connection. `layerfs-daemon::transport` retains one session across
delivery threads and serves their consecutive upstream calls on it. The daemon
serializes those calls under one mutex. Reuse is bounded and failure-closed: a
session is dropped on every error except a definite missing-name Inspect
refusal, so an uncertain mutation is never resent; a session idle beyond two
seconds is closed and replaced, staying well inside the server's five-second
idle limit. Request IDs
are allocated before the mutex, so a lower ID can arrive after a higher one;
the daemon closes the session before such a call and starts a fresh authenticated
connection. This preserves the native `Client`'s per-session increasing-ID rule
without rewriting request identities. Authorization, deadlines, response bounds
and existing operation counts are unchanged. This shared-session correction is
based on source `50414c386a4f6b71dfc074dee685ca21d63f1eb0` plus the run.rs
change recorded with this document; it has no mounted qualification yet.

**Daemon and owner spans.** Bounded child timing scopes now divide the real
route: `daemon.workspace_attach` and `daemon.fuse_mount` inside selected
Workspace Open, `daemon.exec_spawn` and `daemon.exec_output` inside Exec, and
`daemon.commit` around native Commit. Daemon-to-Service calls record
`daemon.service_connect` for a fresh TCP/Noise connection,
`daemon.service_hello` for its checked protocol Hello, and
`daemon.service_call` through request delivery and terminal receipt. Reused
sessions omit the first two children. The owner records
`owner.docker_launch`, `owner.docker_port`, `owner.daemon_ready` and
`owner.shell_ready` inside Create, which previously reported one undivided
window. These are ordinary product telemetry: they add no test-only branch and
change no result.
The #241 liveness and capacity text uses source basis
`31083d4316905cdb055349ffa07107f9a5a0c8d0` and the shared capacity
correction committed with this document; it carries no performance or release
qualification claim.
The #241 connection diagnosis uses source basis `a8fb5697e` plus the
observation change in this commit. The bridge exposes the existing bounded TCP
connect and Noise authentication steps separately; the daemon records them as
children of `daemon.service_connect` under the same absolute deadline. The
acceptor emits at most eight immediate capacity-drop lines and one shutdown
summary with accepted, admitted, reaped, live, peak-live and dropped counts,
plus the acceptor's terminal error code when it exits unexpectedly.
Missing shutdown summary makes that diagnostic incomplete. These observations
do not classify the historical `Unknown` cause or change admission behavior.
The subsequent #241 missing-path correction uses source basis `9a377c85f`
plus the coordinated client, server and daemon change in this commit. After
a complete zero-input Inspect refusal with `PathNotFound` or `NotFound`, the
server drains the input and sends the exact failure frame, then both sides
retain the authenticated session for the next increasing request ID. Every
other failure, including uncertain delivery, closes it. This removes the
confirmed reconnect between missing-path lookup and inode reservation;
it does not establish why one historical connection stalled.
The first mounted SDK check at `dde88f114` showed that the daemon's outer
delivery closure still discarded the synchronized session on a known Inspect
refusal. The follow-up source change in this document's commit applies the
same refusal classification there and advances its retained request ID.

**Bounded Workspace control session.** `layerfs-sandbox::session` retains at
most one authenticated control connection across rapid Workspace calls. A
retained socket is handed back only after a Hello on that same socket confirms
the live daemon still reports the instance the caller validated, so a restarted
daemon cannot answer an operation addressed to its predecessor; a socket the
restart closed fails that check and the caller performs a fresh checked lookup.
Only a successful operation retains the session, so a broken or uncertain
operation is never resent, and the idle bound stays inside the control server's
five-second idle timeout and one-session admission. A refused operation is
reported as `Stale` only when the live daemon is reachable and reports a
different instance than the route was bound to; every other refusal keeps its
own cause. The public SDK surface is unchanged.

**Projection counts.** `layerfs-workspace::filesystem::projection_counters`
holds fixed-size saturating counts of projection callbacks (`lookup`, `getattr`,
`read`, `write`, `readdir`, `open`, `setattr`, `rename`, `other`, `range_state`,
`range_edit`) and of upstream
host Service calls issued by that Workspace. The FUSE adapter records each
callback at its single entry point, `setattr` and `rename` included, and
`Workspace::remote_call` counts each upstream call. The counts appear in
`WorkspaceStatus` as `projection_calls` and `upstream_calls`. They are
diagnostic product telemetry and never gate an operation; no cache, backing or
write path is altered by counting. Published range
edits also saturating-count their accepted replacement payload bytes and
physical suffix payload bytes copied; a piece splice records zero shifted
bytes even when the logical suffix is large. An edit that publishes but later
fails notification remains counted as accepted.

The Linux projected range EDIT checks the held descriptor's writable,
nonappend state and current stamp under a mutation permit before acquiring
private replacement payload bytes. Workspace repeats the stamp and handle
checks at preparation and final publication, so the early refusal avoids
backing work for an already stale request without weakening the final CAS.
Source basis for these corrections: `11e08d9ac` plus the same-commit
`layerfs-fuse/src/range_ioctl.rs` and `layerfs-workspace/src/filesystem/write.rs`
changes.
Projected range edits use the writable descriptor's admitted rights and repeat
its handle/stamp checks at publication; a mode change does not revoke an
already-open writable descriptor. Path-based edits and handleless size changes
retain their separate permission check.

The Linux adapter validates LFB3/LFD3/LFA3/LFX3 version-3 ioctl frames.
BEGIN reserves declared logical bytes under an 8 MiB per-mount aggregate
budget and a 32-stage cap, binds a random token to the descriptor and exact
Workspace stamp, and changes no Workspace bytes. Ordered DATA stores at most
the declared literal bytes and hashes Zero runs through a fixed scratch
buffer; ABORT, descriptor release, mount stop, destroy and a 30-second
deadline discard private stages. A mount-owned sweeper enforces deadline
cleanup while idle and is joined during unmount. APPLY validates complete
length, digest and stamp, consumes the token, obtains one projection mutation
permit, owns only the literal bytes as one Workspace payload and submits one
ordered Bytes/Zero splice. Workspace validates the combined logical length
against 8 MiB and the result against 4 GiB; Zero parts become sparse Zero
pieces, while Bytes parts address successive offsets in that one payload.
The final exact-stamp publication advances one revision; no DATA call
publishes. LFS2/LFE2 inline behavior is unchanged. Source basis: parent
`ae11f56e9739bcf1a0e4e603978afe32c79511e1` plus the same-commit
FUSE and Workspace range-stream changes.

The daemon status wire carries the same counts in the fixed
`layerfs_bridge::contract::PROJECTION_CLASS_LABELS` order plus `upstream_calls`,
`range_accepted_payload_bytes` and `range_shifted_suffix_bytes`,
so a caller reads bounded classes rather than a variable-length map. A Workspace
that does not report exactly those classes is refused as `Integrity` instead of
being read as zeros. The public SDK exposes one read-only route for them,
`WorkspaceApi::status`, which accepts either the plain status response or the
local-edit writable observation for the same request. The counts describe what
the kernel asked the mounted projection for; the two separate range fields
describe accepted physical byte work. Status profile 4 fails closed on profile
3 requests because the fixed response grew from 219 to 251 bytes (writable
status 405 to 437). The status call is never part of an acknowledgement
boundary.

The sandbox owner now accepts an optional telemetry run identity at assembly.
When present, it forwards the existing daemon telemetry stream and supplies a
10 ms monitor interval and a sandbox-specific namespace. The daemon's one
runtime records authenticated control operations and upstream Service calls;
the host Service records its own operations when its application assembly
enables that recorder. CPU and RSS windows are process-shared samples, not
exclusive call costs or exact memory peaks. The opt-in functional diagnostic
and its non-admission cache/resource limits are declared in
[`telemetry-diagnostic.md`](../issues/236/telemetry-diagnostic.md).
After the one-connection diagnostic at source commit
`0edc58ce8d4f21115a1eb27e2964290426f418bc`, the host owner accepts the
application's existing telemetry runtime as well. Each checked lookup records
Docker port discovery and authenticated Hello as separate local-process LFT1
operations at that source. With the fixed-port refinement, only authenticated
Hello remains on the per-call route; Docker port verification occurs at Create.
Disabled telemetry performs no observation work; the owner shares the host SDK
and Service recorder when enabled. These substeps remain diagnostic and do not
change cache state or deadlines.

The SDK-owned host setup extension is based on source commit
`611620360261a2195b21dd178753572ffe2164be`. `layerfs-sdk::Host::create`
owns fresh Store/history creation, credential parsing and the local Service
grant, then lends an ordinary `Client` for `init_project`. Benchmark drivers
need no direct backend package imports. Host setup remains outside the timed
Init call; the Service continues to own source validation, C1/C2/C5 work and
genesis publication. It does not make the separate full verifier an SDK
operation or qualify a cold-cache performance claim.

The #237 Service layout and import-batch description below is pinned to source
commit `bc944fe6347f640b6d4877f69f2d98b464c4d0ad`. It changes source organization
and native Init's bounded producer-to-save messages; it does not change the
bridge wire format, the C2 save owner, or SQLite's physical format. The measured
prototype, integrated diagnostic and their qualifications are in
[`#237`](../issues/237/service-layout-and-import-batch.md).
The merged source keeps the SDK's request-scoped Project Init in `project.rs`
and routes it through the reorganized `service.rs` admission and dispatch path.
The #237 daemon-host timings remain historical; the SDK Init benchmark has a
different operation surface and its own evidence.

The optimization revision uses ordinary `TcpListener` and one
`TcpStream::connect_timeout` attempt, with TCP_NODELAY and explicit blocking mode
on accepted sockets. Socket option sizes are not admission criteria. The failed
preconnect experiment and original observations remain archived in the handoff.
The [C2 checkpoint](proposal/service-daemon-transport/implementation/evidence/optimization-c2-checkpoint-20260920.json)
qualifies the storage prerequisite on macOS; the wider native/Docker profile still
requires its own current-source evidence.

Pair 1 readable prerequisite, 2026-09-21: the additions below are based on
`0749180db34d1cdc57f905806a17e3f3f48ec2bc`. They add complete logical read
attributes, export the existing cooperative input contract without native
dependencies, and propagate the caller's deadline through connection setup.
They do not establish mounted verification or measured performance.

The R1-C Status extension is based on the R1 commit
`598d8405f1168f5f8409807c4f3b6e95d1a110c9`. Its shared bridge/service changes
define one authenticated daemon-targeted local observation. They do not add
network attach, edit, mount management or Commit controls.

The R2 portable metadata extension is based on
`4d6f5cd0fa4d21afe51fb1dda01db0d4a0095c88`, including the attribute hierarchy
prerequisite `d3d767393`. It adds one typed C1/C2 metadata save, with no Workspace
mutation, automatic inode attachment or Branch publication.

The native symlink addition described below is based on parent
`2fc2e8d7a100b812a46753c4b35d383bedac448d` and frozen product input seal
`af105d8725996152b1b94082f82ad2d0d5aaf57fe846ccb24c2303bd15f9501d`.
Its verification and commit `9060c26bcc3e905e031415da20cec54352b94192` are recorded in
[Round47](proposal/fuse-workspace-snapshot-overlay/47-native-symlink.md). The mounted
extension is based on that commit and frozen seal
`e5589871b9e32f54fbccd458fccdc187302b99dc1f7873f51f65a039721df963`;
its actual checks are recorded in [Round48](proposal/fuse-workspace-snapshot-overlay/48-mounted-symlink.md).

## Boundaries and public calls

`layerfs-bridge::contract` owns the closed content/history operation union and typed results.
Building bridge without its default `native` feature gives the portable contract
without Snow/nix or socket code. The native adapter authenticates and delivers
frames; it has no C1/C2, SQL or service dependency. `Client::call` consumes a
cooperative deadline-aware `Source` and bounded output. The real stdin source uses
poll and the same frame/input-state codec as the network endpoint. `Source` is
exported by the portable contract; `adapters::native::client::Source` remains a
compatible reexport. `Client::call` and `call_until` accept a dynamic Source,
so an embedding can bind logical delivery without importing socket ownership.
One scoped
upload thread permits concurrent early-response handling. It closes the upload
half on malformed input, allowing an authoritative service failure to arrive.
Only a returned terminal success validates provisional read bytes.

`layerfs-service::Service::handle` is the same direct and remote entry point. It
records around authorization/admission, validates the request, binds authenticated
public-key possession to configured Store/op grants, and then acquires one permit
without waiting: a logical mutation takes one writer permit of the Store it
targets, whose budget is that Store's persisted `max_concurrent_writes`, and a
read-only operation takes one of the process's `MAX_READ_OPERATIONS` read permits
and no writer permit at all. One operation takes exactly one permit, however many
saves or statements it performs. Native configuration selects one Store (logical ID 1);
the service facade admits at most four explicitly configured Store mappings.
No request carries a native Store path or independent construction capacities.
`VerifiedPeer` belongs to the portable bridge contract and only trusted native
entry code constructs it. Direct callers use `VerifiedPeer::from_private` using the same authorized
private key; a caller-chosen numeric principal is insufficient.

The Service source follows the same request flow inside the
`layerfs-server` package: `service/handler.rs` owns admission and dispatch;
`service/read/` holds content and catalog queries; `service/save/` holds content
and catalog mutations plus shared filesystem construction;
`service/save/import/` owns native scanning and namespace construction, with
bounded producer messages in `service/save/import/batch/`;
`service/init_project.rs` exposes request-scoped Project Init to the SDK. Native
assembly is grouped separately: `host/config.rs` reads operator configuration,
`host/store.rs` builds Stores, catalogs, grants and the acceptor's session bound,
`host/acceptor.rs` is the single bounded connection acceptor used by both entry
points, and `host/assembly.rs` is the one concrete `Server` that binds a Store,
its history catalog, the authorized `Service`, the loopback listener and the
sandbox owner together. `src/bin/layerfs-server.rs` is the only process
entrypoint: it calls `host::run`, which is the operator-configured assembly over
the same acceptor. The package was renamed from `layerfs-service` and its
listener moved from `server/` to `host/` as a source relocation; no second
acceptor exists.
`records.rs` converts catalog identities and wire records. The
`lib.rs` and `mod.rs` files only declare or export these modules.

A local `StoreProvider`, C1 call and `SaveHandoff` implement each operation. File
construction uses exact-length streaming; edits acquire at most 8 MiB of separate
replacement parts before the save, preserving current-result coordinates. Known
failures retain the available typed cause and checked cleanup disposition. Unknown
C2 outcomes are not aborted or replayed on a guess. A successful content-object or filesystem
root is returned only after validated input finality and successful C2 `finish`.

For a save's existing `service.finish` LFT1 span, the Store records one
`storage.finish.drain` child and one `storage.finish.owner` child. The owner
records bounded children for pack seal, transaction begin when needed,
candidate flush, ownership publication, ordinal/watermark work, SQLite
commit, and the separate postcommit PoolIndex and Candidates clones. A
disabled recorder leaves those nodes absent without changing the save's
result; failed scopes retain their completed parents and children. The
`SaveProfile` diagnostic carries pending-batch object/byte counts and each
index's entry/live-byte count or a skipped-lock marker. With
`LAYERFS_FINISH_DIAGNOSTIC` set, the Service writes one fixed count line after
successful finish, including existing object, pack, statement and pooled
fetch counts. Exact per-finish SQLite page and OS physical-read counts are
unavailable in the safe product API; any external process I/O/page reading
must keep its broader attribution. Source basis: parent
`501addcd1693f6e2afb15599a7db966a200ca5ab` plus the same-commit
storage/Service instrumentation.

Prepared updates verify the original scope/root serial, existing identities and
retained references. They send final bindings only for changed names. Directory
content cannot be swapped through an inode value; it uses directory changes.
`FilesystemSaved` is a separate result from content-object `Saved`. The latter
length is the file logical length or exact symlink-target byte count, according
to the operation; a tree result has no such length. Content saves followed by
attachment remain separate saves;
no atomic composite, history publication or crash-durability promise exists.

Inspect supports File, combined Stat (identity, roots, count, mode and full mtime),
List with an explicit C1 continuation name, and Readlink. Paths use C1's canonical
relative UTF-8 grammar; the empty path is root. `/f` is invalid; `f` is valid.
Provider failures already collapsed by `StoreProvider` remain Provider failures;
no error-string parsing reconstructs unavailable native categories.

`Inspect::Attributes` supplies the complete Stat fields plus `size: u64` in one
logical response. The service obtains a regular file's exact length from C1's
FileView, a symlink's length from its checked stored target, and explicitly
projects directory size as zero. It uses the existing Inspect grant/read permit
and no input body. Subtag 4 and result tag 9 append to the existing wire unions;
older tags retain their exact encoding. The bridge validates supported kinds,
positive representable serials, namespace reference counts, mode masks,
nanoseconds and kind-specific size bounds. Native request matching additionally
checks that root attributes have directory kind/reference count zero and
descendants have positive references. Roots remain opaque 32-byte identities;
the service's C1 reader authenticates their bytes and logical role. Signed mtime
seconds remain exact, including values before the Unix epoch.

List still returns names/serials rather than complete child attributes. A consumer
requiring per-entry kinds issues bounded Attributes calls for its page under one
remaining callback deadline. This establishes no batching or request-count gain.

`WorkspaceStatus` is a separate daemon-control operation: current profile 4, opcode 8,
positive request ID, zero Store/generation/result-body fields and at most 5,000 ms.
It supplies a 1–63-byte managed ID and nonzero 32-byte producer incarnation. Its
metadata is at most 124 bytes and it accepts no input or ResultData body. The
existing content/history service explicitly refuses it before Store lookup or
read/write admission; opcode 8 has no Store permission bit, even in an all-bits
grant. The daemon independently authorizes the authenticated peer and expiry for
the exact Workspace/incarnation. Service grants never confer that authority.

Response tag 10 encodes the echoed ID/incarnation, three state flags and five
u64 observations: active operations, nodes, handles, cookies and aggregate
`consumer_accounted_bytes`, followed by the fixed projection/upstream counts
and two physical range-byte totals described above. Its current maximum is
251 bytes including the tag. Reserved
flag bits and inconsistent closed-state counts are rejected. Native matching
requires the exact request ID/incarnation binding and no output body. This is
current local state, never an earlier edit/Commit receipt or recovery protocol.
The existing non-history three-byte failure representation applies; transport
loss is Io with no unknown-mutation claim. Existing framing, authentication,
server input completion and failure/session-closure algorithms are unchanged.

`UpdatePortableMetadata` uses content profile 1/opcode 9 and explicit Store grant
bit `0x80`. Legacy grants 31/127 do not confer it; opcode 8 remains reserved to
daemon control with no Store grant. The request is exactly 76 bytes, including
the common envelope and existing attribute-tree base root, inode kind, mode,
signed mtime seconds and nanoseconds. Input and ResultData budgets are zero.
Mode/kind/nanosecond checks reuse the complete-attributes validator's portable
rules: regular `0777`, directory `01777`, symlink exactly `0777`, and nanoseconds
less than one billion. The kind is the caller's typed construction context; an
attribute root does not by itself prove membership in a particular inode.

The existing mutation owner validates empty input, takes one writer permit and
opens one C2 save. `save/metadata.rs` reads the existing typed fields through
C1, applies the two sorted portable patches and preserves every generic value
root through the existing streaming patch builder. No full attribute map, object
RPC, new storage implementation or arbitrary generic mutation operation is added.
The grouped read and patch cursor enforce the corrected hierarchy described in
[12](12-attributes.md). Metadata-only attachment later retains the original
content root and uses the separate prepared filesystem operation.

An operation-local provider/consumer delegation checks the absolute deadline
before and after each bounded read wave/object handoff. C1 has no deadline error,
so a typed local expiration flag distinguishes this cause without parsing text.
An in-progress storage call is not preempted. The existing save owner gives a
retained C2 failure precedence, aborts definite pre-finish failure once, and
checks the deadline before finish. A successfully acknowledged finish remains
success even if time expires immediately afterward; lost mutation delivery is
unknown and never automatically replayed.

Result tag 11 is exactly 98 bytes: base root, kind, mode, signed seconds,
nanoseconds, saved metadata root, inserted and reused counts. Native matching
requires every echoed input field and zero ResultData. This result means one
metadata tree has been saved, not that an inode, filesystem, stage, Commit or
Branch was updated. A no-op preserves the metadata root and still reports the
actual save result. The operation inherits current Store/C1 limits and introduces
no larger prepared-update or EditFile input envelope.

## Native protocol and ownership

The carrier is `Noise_KK_25519_ChaChaPoly_BLAKE2s` over ordered TCP, provided by
published Snow 0.9.6. Both public keys are pinned in operator configuration. The
four-byte principal selector is only a key lookup hint and is bound into the Noise
prologue. Every operation is authorized again, including credential expiry. Key
revocation changes configuration and restarts the process. No replay/resume exists.

Each encrypted record has a checked big-endian u32 ciphertext length. Nonces are
strictly increasing per direction and never supplied by a peer. The decrypted
frame is exactly `LFB1 | kind:u8 | flags:u8=0 | reserved:u16=0 | id:u64 | len:u32 |
bytes`, all multibyte numbers big endian. HELLO=1, BEGIN=2, BODY=3, END_INPUT=4,
RESULT_DATA=5, SUCCESS=6 and FAILURE=7. END_INPUT carries the exact total as u64.
Body/data frames contain 1..=16384 bytes; metadata is <=32768 bytes. No empty-body
frame, in-band cancellation, per-chunk ACK or object RPC is accepted. Local stdin
uses the same plaintext frame schema; credentials never come from those frames.

The host acceptor uses the bridge contract's
`session_capacity(budget) = budget + MAX_READ_OPERATIONS` for
persistent/handshaking/closing sessions, plus one synchronous accept/refusal
socket slot. At the default budget of two that is four sessions and **five
application connection resources in total**, as before; a raised writer budget
raises the session space with it, so the transport is never a lower ceiling than
the configured budget. The acceptor's extra slot is counted, not hidden behind
the session limit. At most that many 2 MiB service thread stacks exist, and at
most `budget` admitted handlers can construct, each with one producer. Client upload uses one explicit 2 MiB stack. Closing owners retain their
slots until their threads finish or the executable exits. A session retains a
socket shutdown clone with its worker. Shutdown stops admission, shuts down all
live sockets, and collects completed workers for at most two seconds. If core work
is still running, explicit process exit preserves an unresolved outcome; detached
workers are not labelled cleanup. Kernel socket state, backlog, stack mappings
and RSS are separate observed domains.
The composed Server's flag-driven acceptor polls only its listener; stdin is
polled solely by the operator `Stop::Stdin` mode. A closed stdin therefore
cannot turn an idle composed host into a polling loop.

The [resource profile](proposal/service-daemon-transport/implementation/10-resource-profile.md)
records the aggregate byte/count ownership vector. At the default budget it
admits four session owners and two complete operation owners, preserves 16 KiB
body and 32 KiB metadata limits, and selects a 4 GiB construct/read-range limit.
Those counts are the default-budget instance of the configured one; see the
[concurrency controls](../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md). Edit replay remains 8 MiB.
Frame work is bounded by `ceil(declared_bytes / 1024) + 257`, including boundary
slack and a terminal frame. File size grows total work and persisted bytes, not a
whole-file transport buffer.

Requests select an overall budget up to 600 seconds and I/O also observes a
five-second no-progress limit; handshake/idle is five seconds. Each process shares
one connection activity timestamp between upload and response consumption, so
productive upload traffic keeps its concurrent response wait alive. A blocked
socket read may continue waiting when the other direction made progress; this
does not reconnect, resend frames or replay a mutation. Complete silence remains
bounded by five seconds, and the overall deadline never extends.
The public native-directory import sends a checked one-byte `ResultData`
progress marker at most once per second while it constructs the source tree.
The native client consumes that marker without delivering logical result bytes;
it does not reset the absolute request deadline or change the five-second
no-progress rule. Other operations still reject unexpected `ResultData`.
The shared namespace builder supplies C1's existing file-backed ordering
reducer with a private directory beside the Store. The default pending-row
and ordering-byte budgets remain unchanged; C1 spills sorted rows when the
in-memory budget fills and checks release before the Service publishes a root.
The Service removes the private directory on success or failure and reports
failed cleanup. No file or entry-count threshold is inferred from that budget.
Each process shares
one absolute operation deadline through its layers. Only a remaining duration
crosses the network, never an Instant. Synchronous
core calls cannot universally be preempted mid-hash or mid-commit. A late known
C2 success is not reclassified as an abort; a lost response remains unknown.
The daemon observes stdin/stdout deadlines with poll and closes unsynchronized
sessions rather than draining attacker-declared input. Native macOS accepted
sockets are explicitly switched to blocking mode because the nonblocking listener
otherwise passes O_NONBLOCK to accepted sockets.

`connect_until` shares a caller-local deadline across one TCP connect attempt,
authentication and Client HELLO; each connection phase still has at most five
seconds. The existing `connect` retains its separate five-second connect and
authentication/HELLO caps. A consumer using `connect_until` also passes the same
deadline to `Client::call_until`, so setup cannot grant a fresh operation budget.
Connection errors retain their existing typed mapping; socket timeouts can still
be reported as Io. A complete missing-name Inspect refusal is the one
synchronized error that permits session reuse. Every other failed Client
operation closes the session; consumers must not replay a failed operation
implicitly.

## Single-crate telemetry

Portable `Timing`, `RecordingLimits`, `OperationRecorder`, `Observation` and
`Window` remain std-only and forbid unsafe code. Timing's existing Result/panic,
child and attachment behavior remains. Owned labels are normalized to a new
bounded allocation on every public/import path, including short strings with
excess capacity. Clipping cannot be cleared with `with_incomplete(false)`.
`TimingNode::retained_bytes` reports Vec/String capacities; the conservative node
charge covers arena/tree conversion overlap. Caller-owned imported/cloned reports
remain caller-owned until transferred; arbitrary external clones are not a global
process-memory guarantee.

Native runtime reserves eight 256-node recordings under a 4 MiB pool; report owners
hold reservations until drop. Encoding preflights a valid whole or clipped timer
projection and accounts simultaneous prefix/suffix/timing/output buffers. It does
not truncate JSON. An independently recorded product-success bit survives timing
deselection. Off performs no telemetry allocation/probe/worker/file/export after
normal application configuration is supplied, verified by an external allocator.

One explicit monitor uses safe macOS nix `getrusage` for CPU and libproc 0.14.11
for RSS/start identity, or bounded Linux procfs plus safe nix sysconf. POSIX CPU
microseconds are checked and converted to nanoseconds; RSS is bytes. Raw macOS
libproc CPU fields are Mach ticks and are not used as nanoseconds. Reports include
the collection source and measured local probe duration. The 100 ms
selection keeps 600 recent slots and 32 fixed summaries. Live summaries survive
raw-ring wrap, use generation-checked nonblocking access, and release atomically
on return/error/unwind. Window opening/closing and observation times are local to
one monitor; reports include run, configured host/container namespace, PID and
native incarnation. CPU deltas need actual covered samples and are shared process
windows, never exclusive operation cost. RSS maxima are sampled maxima. Missing
capabilities or insufficient samples are unavailable. Cgroup fields are not yet
collected; process RSS must not be relabelled as cgroup/anonymous/file/socket memory.

`OutputConfig` bounds encoded bytes, queue count/capacity, aggregate rate/burst and
segments. `Output` handles share one owner whose final destruction stops its
writer exactly once, including concurrent final-clone drops; the writer retains
queue state but never its owner. Submissions recheck closure under the queue lock.
Current writes retain their queue charge. `Collector` has <=16 producer
slots; queued/in-flight records retain closing producer registrations. Fixed loss
counters saturate and expose overflow. Actual acceptance coordinators separately
bound their Python pipe decoders and retained diagnostic bytes.

Ordinary native startup/configuration/error diagnostics use one best-effort write
of at most 512 bytes and leave their dedicated stderr descriptor nonblocking.
Both CLIs return explicit exit codes so Rust's default `Result` termination cannot
block error cleanup behind an unread diagnostic pipe. Stderr must remain separate
from protocol stdout. Regular-file OS calls are not claimed preemptible.

Forward is structured `LFT1 ` JSON lines on stderr, independent of framed stdout.
The Docker host coordinator must attach product stdout and diagnostics through
separate Engine connections: the ordinary Docker CLI attachment multiplexes them
and can block stdout when its stderr consumer stalls. The external acceptance
coordinator uses stdout/stdin-only `docker run` plus a bounded stderr-only Engine
attachment. It discards excess diagnostics and never writes a retry spool.
Resource records include `selected` bits (CPU=1, RSS=2), so deliberately disabled
fields differ from unavailable selected fields. All process samples name their
local clock/source; run summaries carry namespace and counter-overflow status.
Local/both use an exclusively locked, marked operational namespace; permissions
are 0700 for new directories and 0600 for files. Fixed segment names, retained
length validation and profile markers allow bounded restart. Active segments count
against capacity. Expiry is checked at least once per second while the writer runs;
local wall-clock reversal delays age expiry. Unknown directories, links, conflicting
writers and changed retention profiles fail closed. Only owned segment paths can
be retired; benchmark receipts beside the namespace are untouched. There are no
sync calls or durable-output claims. Local filesystem syscalls are not preemptible;
shutdown attempts a bounded drain, then releases rather than indefinitely joining
a stuck writer. Expected telemetry loss cannot replace a known product result.

Native environment configuration: service uses `LAYERFS_LISTEN`, `LAYERFS_STORE`,
`LAYERFS_PRIVATE_KEY`, and `LAYERFS_PEERS` (`selector,public-key,expiry,op-mask` entries
separated by semicolons). Daemon uses `LAYERFS_ENDPOINT`, `LAYERFS_SELECTOR`,
`LAYERFS_PRIVATE_KEY` and `LAYERFS_SERVER_KEY`. Keys are exactly 32 bytes represented
by 64 hex digits and are never printed. Telemetry uses `LAYERFS_TELEMETRY` (off,
forward, local, both); enabled collection requires `LAYERFS_RUN_ID` and
`LAYERFS_NAMESPACE`. Local/both also use `LAYERFS_TELEMETRY_DIRECTORY`. Numeric
runtime/monitor/output configuration is exposed through the public Rust structs;
these executables select the documented initial profile. Invalid optional native
setup emits a separate bounded initialization diagnostic and remains disabled.

## Portability and remaining qualification

Service keeps native Store paths, `rusqlite::Error`, connection/session lifetimes,
SQLite transaction/ownership, codecs, packs and mutable caches local. A managed
provider would replace C2's connection and grouped read/write execution and qualify
lifetime/transaction/resource semantics. It would preserve logical requests,
canonical identities, input finality and outcome distinctions. No managed-provider registry or cloud adapter is implemented. Pair 1 now has
separate Workspace/FUSE owners and the existing C5 history owner; their operation
and verification scopes are recorded in the linked round records. API and
canonical compatibility remain separate from persisted-schema compatibility.
The schema-7 checkpoint above retains its historical scope; the integrated tree
uses the [configured-concurrency/schema-8 contract](../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md). Independent algorithm substitution remains #172.

The acceptance record must distinguish deterministic tests, real macOS/Linux
processes, OS faults, optional local/both output and unrun platform/resource cases.
Functional evidence does not qualify overhead, cold storage, bandwidth or #193.

Native early-refusal teardown correction, 2026-09-21, based on
`4d6f5cd0fa4d21afe51fb1dda01db0d4a0095c88`: after sending a decoded request's
handler failure, the server half-closes output and discards remaining input
through the existing bounded adapter before closing both directions. The
original length/frame/progress/absolute deadline remains in force. This avoids
resetting a terminal frame while upload bytes are unread. The handler is not
reentered and the original failure is preserved; malformed BEGIN metadata keeps
its immediate best-effort refusal. See the [observed failure and correction](proposal/fuse-workspace-snapshot-overlay/12-early-refusal.md).

The R3a Workspace input primitive, based on `d555c8bef`, is described in
[the owned-payload implementation](proposal/fuse-workspace-snapshot-overlay/14-owned-payload.md).
It uses the existing portable Source contract and introduces no service operation
or daemon input control. Read-only daemon startup keeps disk backing disabled;
its shutdown and failed-control cleanup now pass the existing absolute deadline
through `close_clean_until`. FUSE preserves typed local backing error causes in
its errno mapping, without adding a writable callback.

R3b, implemented from `4629b8d62de1e0df8a7bd9808b59a86d7c6669f3`, adds one
explicit unmounted `LocalEdit` capability to Branch-backed Workspaces. The
[local range-edit record](proposal/fuse-workspace-snapshot-overlay/15-local-range-edit.md)
describes the maintained private page index, ownership ledger, generation
completion reserve and exact public operation. It retains the same logical
delivery boundary and makes no service mutation during a local edit. Daemon
read-only launch selects `ReadOnly` explicitly; the existing authenticated Status
wire remains its readable scope. LocalEdit projection reservation is refused
until the writable kernel coherence binding exists.


The next Pair 1 operation, implemented from
`788a63950500e6ba79c6a55dc07f7b84ca0fde89`, is
[Workspace Stage](proposal/fuse-workspace-snapshot-overlay/16-stage-capture.md).
It captures one immutable local generation with a live successor, streams only
its changed file spans, saves changed portable metadata and invokes the existing
StageChanges once. There is no added service opcode or service/storage/history
implementation dependency in Workspace. Disk completion associations use the
private index; the ordinary candidate reserves 137 pages and each dirty generation
reserves 208 pages for completion. The latter accounts for interleaved live slot
allocation and is an explicit disk-admission increase, with unchanged RAM/window/
FD limits. The real native save and failure proofs, narrower scope and pending
CommitStaged/reconciliation are recorded in 16. FUSE's error conversion accepts
the new Stage failure as EIO; no writable callback or daemon edit control is added.


The next public-operation round, based on
`0b2c729bdb3f026f12beb667ccdc15c52853280f`, adds
[Workspace CommitStaged and reconciliation](proposal/fuse-workspace-snapshot-overlay/17-commit-staged.md)
through the existing C5 command. It reserves 64 completion-fund pages, 26 metadata
slot credits and ledger replacement capacity before remote publication. A bounded
streaming builder compacts only the current D1 frontier and reuses piece trees;
the exact captured-version/result association governs rebasing. Local state owns
a changing canonical base/Branch context and baseline epoch, allowing lazy
canonical cache refresh while reads retain selected roots. A validated own result
survives later local failure; unknown results remain unknown. Exact staged replies,
local pre-admission, failure retention, repeated commits and current-source
regressions are recorded in 17. FUSE maps Commit errors to EIO; no new wire opcode,
service algorithm, writable callback or daemon Commit control is added here.


From parent `6702e31e629ada5e78981b6854e36721e619e65b`,
[ordinary Workspace Commit](proposal/fuse-workspace-snapshot-overlay/18-composite-commit.md)
shares Stage's file/metadata preparation and CommitStaged's known-result
reconciliation, using one existing HistoryCommand::Commit after preparation.
Clean Commit pre-reserves an empty capture descriptor and 208-page escrow and
submits empty PreparedChanges for actual C5 UpToDate. Pre-admission holds the
remote permit only through the first logical request. Composite outcome lacks a
token, so local Commit report/selector associations are optional and native stage
observations remain separate. The existing shared operation, wire profile and
one-construction-worker default are unchanged. Actual native scope and remaining
mounted/control/failure-disposition dependencies are recorded in 18.


From parent `573b4bbd35bdcd5d8fe55c8fc107f8c31cd791c5`,
[Workspace resize](proposal/fuse-workspace-snapshot-overlay/19-resize-zero-ranges.md)
adds set_len for existing cached regular inodes through the same mutation and
Commit owners. The64-byte private piece value gains strict tag2 Zero with no
payload/custody/offset; reads and Source synthesize only bounded requested spans.
Zero bytes count toward the8 MiB replay envelope for existing or captured-base
files; fresh complete construction has the separate profile described below. In-memory Piece
is48 bytes; the conservative596,448-byte transient accounting remains below the
existing640 KiB reservation, with unchanged windows, FD and disk reserves.
No writable mount, shared service operation or reference change is introduced;
actual native results, retained failures and next dependencies are recorded in19.


From parent `a5bdc9f1e7e0fa4815ae356a7c5d783315fac649`,
[portable Workspace open](proposal/fuse-workspace-snapshot-overlay/20-portable-open.md)
adds FileAccess/FileOpenOptions and open_file, preserving legacy read-only open.
A charged pending handle pins its inode before truncate preparation; shared
mutation publication installs inode state and READY together. All consumers
reject pending IDs, and WriteOnly handles reject read even at EOF. The128-slot
24-byte table remains3,072 bytes, with56 bytes of pending control charged per
truncating open. No write API, FUSE option, daemon opcode or dependency is added;
actual native results and narrower compatibility claims are recorded in20.

From parent `1f9cceb73ba0ede11c86120b73b2015f900d1dd8`,
[routine healthy-owner reclamation](proposal/fuse-workspace-snapshot-overlay/21-routine-reclamation.md)
runs before input, mutation and submission admission. It retires registry-only
healthy metadata roots, then payloads released from their last custody, across
the same consumer. Existing writer/window/deadline bounds apply, without state
locks across I/O. Earlier failed/partial owners remain charged and are excluded;
a root marker preserves cleanup failure even before DFS starts. Explicit cleanup
keeps its deliberate repair semantics. The former post-install Commit sweep and
proposal CommitPhase::Cleanup are removed; the next pre-capture pass handles
healthy retirement between repeated Commits. No public operation, dependency,
kernel callback, worker or service opcode is added.

From exact parent `c4f965357381870b3784e3423e75783496c0b7c7`, the focused
[native stream-fragmentation correction](proposal/fuse-workspace-snapshot-overlay/23-native-stream-fragmentation.md)
coalesces short local Source/Write fragments at the existing 1,024-byte frame
budget quantum. Output owns a fixed 1 KiB pending tail and the original absolute
deadline; successful completion flushes before Success, while failure discards
unsent bytes and latches refusal without Drop flushing or replay. Upload uses its
existing 16 KiB array. Large-frame batching, wire bounds, receiver anti-abuse
checks and service algorithms remain unchanged. Output's selected fixed layout
grows 40 to 1,096 bytes; upload adds an eight-byte cursor. The source-pinned failure,
read-only discriminator and exact verification scope are recorded in 23.

From parent `1343b00accf2277085410c2fd84144b2432f6222`,
[native handle write](proposal/fuse-workspace-snapshot-overlay/22-handle-write.md)
adds write_file over borrowed OwnedPayload. The shared mutation body validates
READY writable Local handles, selects live EOF for native append, and uses one
two-piece splice for positional Zero gaps and Local bytes while preserving tails.
Handle release is checked again before publication; zero input is a validated
no-op. Existing capture/lowering/reconciliation and limits remain, with a
conservative 596,616-byte working allowance below 640 KiB. The FUSE adapter stays
read-only; kernel append positioning and reply/invalidation coherence are separate
required projection work. The original frontier failure and shared transport
prerequisite retain their source identities in 23.

From parent `cb9d5a8249602e77a454672de290f6358e04c23b`, the
[mounted SDK-coherence binding](proposal/fuse-workspace-snapshot-overlay/24-mounted-sdk-coherence.md)
admits at most two projection observations through their reply attempts and
excludes SDK publication while an older observation exists. One charged callback
and pending receipt carry checked inode invalidation outside all state/backing
locks. Applied failures retain the receipt and any READY truncating-open handle;
fresh observations and cleanup remain available. Mount finish drops completed
failed bindings only after checked detach/join. LocalEdit can use the existing RO
kernel projection, with WRITE/SETATTR and write access still explicitly refused.
Workspace has no fuser dependency. Six real mounted cases and two native completion
API subsets are recorded separately in 24; writable kernel and remote SDK controls
remain open. The internal per-Workspace control account grows 8,192 to 8,200 bytes;
mount control, both permit slots and the concrete callback Arc are separately charged.

The next operation is documented against parent
`d770f5d10bf160b57a90b212102e7960148dba18` in
[existing-file mounted WRITE](proposal/fuse-workspace-snapshot-overlay/25-mounted-write.md).
The public `mount_writable` explicitly chooses a LocalEdit direct-I/O projection;
`mount` and daemon `--mount-readonly` retain the cached RO profile. One
ProjectionWritePermit owns one of the existing two reply slots and excludes SDK
publication through WRITE's send attempt. It makes one write attempt using the
current kernel append flag and original deadline ceiling, shares the native
payload/splice pipeline, validates append at live EOF, and invalidates after
publication outside state/metadata locks. A second observation can progress during
notification; publication refuses outstanding old replies. Matching-inode flush
reports the existing retained coherence failure; release stays available. The
on-demand reservation now funds one 40-byte write permit plus one 16-byte observer
and the actual boxed-state layout, without another worker/window/FD/queue.
Kernel SETATTR/truncate and daemon writable controls remain open. The operation
record retains exact RWF append observability and concurrent SDK-size cached-read
limits; no universal writable or performance qualification is asserted.

The following size operation is implemented against parent
`4d5443c1239722c2ed57f0428ad2c32bdbb3d941` in
[size SETATTR and truncating OPEN](proposal/fuse-workspace-snapshot-overlay/26-mounted-resize.md).
ProjectionMutationPermit replaces the proposal's write-only token name and owns
one WRITE or size attempt. Its size method shares native SetLen/Zero publication,
validates an optional writable Projection handle and returns exact published
NodeAttributes. The explicit size origin skips userspace invalidation while the
kernel caller owns NOWRITE; kernel post-reply completion installs size and
invalidates pages. TTL stays zero, SDK publication remains excluded through reply
attempt, and unsupported non-size fields are refused. ATOMIC_O_TRUNC remains off,
so the existing OPEN refusal checks precede the separate truncate request.
Actual verification/accounting status is recorded in26; daemon writable controls
and the previous cached-size/RWF limitations remain separate.

The R1-C lifecycle extension is implemented against parent
`451a1f6bdda00482a528659489c8e4d053677e01` in
[authenticated Unmount](proposal/fuse-workspace-snapshot-overlay/27-control-unmount.md).
Bridge owns WorkspaceUnmount opcode10/profile3, a bounded100-byte tag12 result,
exact identity validation and lifecycle mutation/Unknown classification. The
result distinguishes Unmounted and entered-but-Retained; pre-admission refusal
uses the existing Failure terminal. Service explicitly rejects it before Store
lookup/admission and assigns it no Store permission bit. Daemon grants preserve
Status bit1 and add Unmount bit2; keys and expiry remain independent of service
authority. One Arc<Mutex<MountHandle>> shares native lifecycle ownership with
main; control uses immediate try_lock, keeps Status independent and reserves100ms
inside the original deadline for terminal delivery. No queue, worker, Source
protocol or Workspace algorithm changes. Signal cleanup joins control first;
incomplete cleanup retains the process/owners until another explicit signal.
The startup profile stays read-only; wider Attach/Close/edit/Commit controls are
not implemented by this extension. Actual results and qualifications are in27.

The next R1-C operation is based on parent
`dab1751312adecdc57d073145582a9a702449744`:
[CloseClean](proposal/fuse-workspace-snapshot-overlay/28-control-close-clean.md).
Bridge adds opcode11/tag13 with the same124/100-byte request/result bounds and
renames the proposal DTO to WorkspaceLifecycleWire/Outcome. Unmount opcode10/tag12
bytes are unchanged; operation-specific Response variants prevent cross-operation
completion. Daemon adds grant bit4 (valid mask0..7), calls existing native clean
closure under the same lifecycle try_lock and100ms completion headroom, and retains
checked failures. Service refuses all three controls before Store admission. The
closed target remains observable and a later signal skips an already-completed
close. No Workspace/FUSE algorithm, resource limit or dependency changes. Actual
verification status and remaining writable control prerequisites are in28.

Native Mount failure ownership is corrected against parent
`3c227266b6740cb9c50f63f0ba27a38d1b8b9a00` in
[the retained-mount prerequisite](proposal/fuse-workspace-snapshot-overlay/29-mount-failure-ownership.md).
FUSE mount functions return boxed MountFailure with exact phase/cause and a retained
MountHandle after every lease reservation failure. One temporary Session transfer
cell prevents failed worker creation from dropping the only owner. No LayerFS
constructor cleanup or renewed deadline occurs; explicit unmount owns cleanup.
Daemon startup keeps the original mount deadline, closes its unserved listener on
mount failure, and retains incomplete cleanup until a new explicit signal. Checked
cleanup preserves the original startup error exit. This does not add remote Mount,
Attach or writable management; exact route evidence and limits belong to29.

Authenticated Mount continues from documentation checkpoint
`3e5d6a66a9f4e4df4a8a19087e63909a18046c6d` in
[round32](proposal/fuse-workspace-snapshot-overlay/32-control-mount.md).
It adds WorkspaceMount opcode12/profile3/tag14 and independent daemon grant bit8;
valid masks are0..15. Exact target/incarnation, empty authenticated input and
124/100-byte request/result bounds reuse the lifecycle contract. The existing
native client requires the specific Mount response; service rejects it before
Store admission. Daemon shares one Arc<Mutex<Option<MountHandle>>> with main:
only checked successful Unmount clears it, and Mount refuses occupied, mounted,
closed or stopping state. It installs a returned partial owner before Retained;
ownerless admission refusal remains Failure. Original-deadline100ms headroom,
Unknown/no replay and the read-only projection remain. Round32 records exact
source identity, checks and limits; remote Attach/writable control remain separate.

The shared [pipe-cancellation prerequisite](proposal/fuse-workspace-snapshot-overlay/33-cancelled-pipe.md)
records an observed upload-worker busy loop caused by permanent cancellation being
reported as Interrupted to standard read_exact/write_all loops. Cancellation must
be terminal at the Pipe boundary; ordinary operating-system EINTR remains distinct.
This correction adds no queue, retry, worker or replay authority. Its source and
regression evidence are recorded with the operation round that exposed it.


From parent`0d220870175abb6e3f162cb164dba7c49bc98f9d`, the
[failed-Attach prerequisite](proposal/fuse-workspace-snapshot-overlay/35-failed-attachment-ownership.md)
keeps Attaching/Attached/Failed custody in the existing WorkspaceHost registry.
Public exact-identity observation returns the existing Workspace or bounded failed
resource progress; explicit cleanup retains cause/progress and removes only a fully
released failed entry. BranchContext allocation precedes resource acquisition,
and an acquired arena survives later failure. No daemon/Bridge control is added by
this prerequisite. Runtime locks end before backing I/O; short final custody locking
and syscalls have deadline observation points, not preemption. Native managed roots
are exclusively host-owned; static replacement checks do not claim atomic defense
against hostile concurrent root-level renames. Exact resource/count/verification
scope is recorded in35. The prepared DSH workload is pinned in[34](proposal/fuse-workspace-snapshot-overlay/34-preinstalled-dsh-workload.md).

From parent `3e92fe277a0379fd85f158b858085af3ecb2e2d5`,
[authenticated Attach](proposal/fuse-workspace-snapshot-overlay/36-control-attach.md)
adds identity-only opcode13/profile3 and grant16 (masks0..31). The startup RO profile
remains immutable. Daemon control and signal shutdown share one current selector,
Workspace capability and MountHandle; the native registry retains all failed
resources. An attempted selector is installed before Attach and restored to the
prior closed owner only on exact confirmed absence. Tags15/16 carry the distinct
Attach result and failed-attachment Status; existing healthy/lifecycle results
keep their validators. Failed Status observes native custody; CloseClean disposes
it explicitly. Startup attachment failure retains the same pending-selector
cleanup route. Round36 records source identity, bounds, checks and qualifications.

From parent `74e6fbd2d23cb7519ea63c275f42283ce3c79d17`,
[authenticated Workspace Commit](proposal/fuse-workspace-snapshot-overlay/37-control-commit.md)
adds opcode 14/profile 3 and independent grant 32 (masks 0..63). It invokes the
existing native capture/preparation/composite Commit/reconciliation operation.
Tag 17 preserves typed results and known/observed failure distinctions; tag 18
adds bounded writable Status while existing RO/failed-attachment forms remain.
Writable CLI entry requires Branch, explicit disk quota and current Commit grant.
Control starts under lifecycle exclusion before writable mount publication. Signal
cleanup retains control on dirty/refused closure, and only ends admission after
successful close while still holding the same owner slot. This is process assembly,
not a second Workspace or Commit algorithm; round37 records qualification.


The prepared-directory extension after source commit
`8e01d28a1c8b7708f5d319990c1440ae682f8b44` shares one typed directory metadata
record across fresh declarations and existing-directory patches in both prepared
filesystem routes. New declarations use the existing C1 directory builder under
the same C2 save; existing patches preserve generic attributes through the common
portable patcher. Their combined inode budget remains 128 and the complete metadata
envelope remains 32768 bytes. Empty extensions preserve legacy bytes.
[Round 38](proposal/fuse-workspace-snapshot-overlay/38-prepared-directories.md)
records the exact encoding, ownership, verification state and remaining namespace
work. This extends the earlier existing-identity-only prepared surface.


The native mkdir extension after source commit
`85582e1ac2fb75761897115ec9679c59439b1efe` adds backed generation-local directory
records and name deltas to Workspace's existing COW arena. Lookup consults that
state before immutable Service data; directory handles pin their view. Creation
uses one exact-scope C5 reservation, then atomically publishes child, binding,
parent metadata and generation accounting. Capture/lowering/reconciliation use
the existing prepared Service operation and retain D1 entry roots. Files and
directories share complete-request admission; namespace scratch remains charged
while remote admission is released between RPCs. [Round39](proposal/fuse-workspace-snapshot-overlay/39-native-mkdir.md)
records checked bounds and native proofs. That checkpoint refused mounted mutation.

The mounted mkdir extension after source commit
`e95c90d757d246d66ac96d2b4fa7e1d6fcbb11ca` adds a single-use projection mkdir permit
and a FUSE callback delegating to the same native publication. SDK creation shares
checked mutation completion with a borrowed parent/name notification target;
the adapter invalidates parent attributes then the entry outside Workspace locks.
Projected creation sends no reverse notification while the kernel parent lock is
held, relying on its normal entry reply. Notification failure retains the applied
namespace and receipt, releases the withheld Local return reference and refuses
further mutation until checked unmount/remount. No retained name queue or new
persistent layout is added. [Round40](proposal/fuse-workspace-snapshot-overlay/40-mounted-mkdir.md)
records real mounted proofs and the two corrected caller failures.

The portable metadata constructor after source commit
`3b34e3002b4a7abfd205b68bc94de9a4af22b572` adds operation15/profile1 under the
existing metadata grant128. It saves mode/mtime for a stated kind without a prior
metadata root, through the same deadline-aware metadata helper and C2 save owner.
The helper selects the existing portable patcher for update or the existing C1
builder for construction. Request44/result66 bytes and typed result tag19 carry
only the portable fields, saved root and save counts; no inode allocation,
filesystem publication or history authority is implied. [Round41](proposal/fuse-workspace-snapshot-overlay/41-construct-portable-metadata.md)
records the implementation and verification state. Fresh regular-file declarations
in a prepared filesystem remain a separate prerequisite.

The prepared fresh-file extension after source commit
`82c87d70a1f3e723b627bf1d0777305c9eb393ea` adds sorted fresh regular-file IDs as a
subset of the final inode rows in both direct and C5 prepared changes. Empty and
v1 directory trailers remain byte-identical; v2 adds the nonempty file-ID list.
Direct fresh rows and all C5 rows receive semantic content/metadata role checks.
The shared filesystem handler verifies base absence exactly for fresh declarations,
then lets C1 derive new file references from retained bindings. Unbound fresh
regular files fail; directory omission is a separate rule. The128 combined inode
and32768-byte request limits remain. [Round42](proposal/fuse-workspace-snapshot-overlay/42-prepared-files.md)
records the implementation, resource delta and verification state. Workspace still
supplies an empty fresh-file list until its native create operation is implemented.

The native regular-file creation extension after source commit
`ab473145a606a71327d55d10f12205edb1803946` adds Workspace::create_file with
atomic name, inode, lookup-reference and ready-handle publication. It shares
child creation with mkdir and opens existing regular files through the existing
admitted open path. Initial handles retain their admitted rights independently
of the created mode; later opens and handleless edits still check mode. Fresh
identity uses reserved inode-record byte25, and fresh/captured local originals
resolve before absent base paths. Fresh content and metadata save through the
existing ConstructFile/ConstructPortableMetadata operations; captured F IDs enter
the prepared request, and own-result reconciliation replaces both saved roots
while preserving D1. At that checkpoint,128-row/name,32768-byte prepared request
and8-MiB replacement bounds were unchanged; the fresh streaming extension below
separates complete construction from existing-file replay. [Round43](proposal/fuse-workspace-snapshot-overlay/43-native-create.md)
records the exact evidence, including the unresolved capacity gate failure.
Mounted CREATE and full preinstalled-workload admission remain subsequent work.

The mounted CREATE extension after source commit
`7eb46ef0766eae0d6ccfb894d07919a39c69ef38` reuses child creation through a
single-use ProjectionMutationPermit. Kernel creation atomically acquires a
Projection lookup reference and ready handle; the permit remains held through
ReplyCreate. CREATE validates/removes S_IFREG, accepts its selected creation flags
through the existing access/append parser, and uses already-masked portable mode
because DONT_MASK remains off. Existing-name truncation carries its projection
origin through the existing open reservation and publication path. Kernel CREATE
sends no reverse notification while the parent lock is held. Native mounted
creation uses the existing parent/entry invalidator, records the published Local
handle in Pending/Failed custody, and releases only the withheld lookup reference
on notification failure. No backing format, count/byte budget, queue or worker
changes. [Round44](proposal/fuse-workspace-snapshot-overlay/44-mounted-create.md)
records verification and preserves Round43's open capacity failure.

The shared symlink-content constructor after source commit
`5ed91aaca38dc54e145753a6844b12e6baf30abd` adds ConstructSymlink with0..4096
opaque non-NUL target bytes. Opcode16/profile1 carries the target in29+L bytes of
request metadata and has no input body. It reuses Saved tag2 (57 bytes) with
length==target.len and no ResultData, through an explicit client result matcher.
Grant0x04 now explicitly authorizes file or symlink content construction, including
legacy mask31; no other authority follows from that bit. Service validates empty
input before acquiring its one save, then calls the existing C1 symlink builder
through SaveHandoff and the common finish/retained-failure/abort path. No inode,
namespace, Stage or Commit is created. Empty object targets do not relax the
history manifest's nonempty rule. [Round45](proposal/fuse-workspace-snapshot-overlay/45-construct-symlink.md)
records exact checks and qualifications; fresh symlink admission remains separate.

The prepared fresh-symlink extension after source commit
`521bcb304c382e53f454eb3eb9007a010c1de486` adds sorted new_symlink_serials (S)
as a kind3 subset of inode rows, separate from kind1 fresh-file IDs (F). The shared
row budget remains I+N+P<=128 and the combined fresh lists satisfy F+S<=I. Only
nonempty S selects trailer v3, sized9+24(N+P)+8(F+S); S0 preserves older bytes.
Decode checks the remaining fresh-ID budget before allocating S. Shared Service
base checks recognize declared N/F/S absence; fresh file/symlink values start at
reference count0, and C1 derives their final topology. Direct new-S rows use the
existing canonical symlink/portable metadata validator; C5 keeps its single
all-inode validation pass. No C1 builder, allocator or ownership path changes.
Workspace supplied an empty S in that round.
[Round46](proposal/fuse-workspace-snapshot-overlay/46-prepared-symlinks.md)
records its shared prepared-update verification.

The native symlink extension after source commit
`2fc2e8d7a100b812a46753c4b35d383bedac448d` adds
`Workspace::symlink(parent, name, target, deadline)`. It shares child publication,
atomically installing one Local lookup reference, a kind3 binding/inode, parent
mtime and generation accounting, with no file handle. The mounted extension below
uses the same ownership path. One exact-scope C5 reservation precedes
bounded payload acquisition. Empty targets use no payload; nonempty targets use
one existing Local piece and custody, with byte26 of the160-byte I record marking
the kind and the16-byte E record accepting kind3.

Lookup and Readlink consult pinned local state before a canonical Service call,
so forget/relookup and reads during captured-G delivery retain exact target bytes.
Known own completion replaces the filesystem base, removes G-only records and
preserves D1-born symlinks and directory deltas. Commit saves target and kind3
portable metadata through the existing shared constructors, records both roots in
R, then lowers kind3 I and S through the prepared update. No canonical object
construction moves into Workspace. The existing public FileSave phase and
saved_files counter cover regular-file and symlink content saves; no status field
or wire tag is added. Local payload reads preserve typed BackingFailure details
through Stage source-failure observations and unknown-outcome classification.

The fresh-symlink count participates in current/captured generation accounting and
the same complete-request admission as files and directories. Existing128-row/name,
32768-byte request, backing, worker and scratch limits remain.
[Round47](proposal/fuse-workspace-snapshot-overlay/47-native-symlink.md) records the
implementation, source-derived resource arithmetic and completed native checks.
Full preinstalled DSH admission and Round43's native-capacity failure remain open.

The mounted symlink extension after `9060c26bcc3e905e031415da20cec54352b94192`
adds ProjectionMutationPermit::symlink and a kernel SYMLINK callback. The single
attempt uses the earlier permit/call deadline, publishes one Projection reference
and no handle, and holds the permit through the entry reply. The kernel owns
parent/name invalidation after its reply, so this path sends no reverse notification.
Pre-reply attribute conversion failure releases the withheld reference. An entry
reply has no observed delivery result; no guessed rollback follows its send.

Native SDK creation while mounted uses the existing parent-attribute/name invalidator.
It publishes a Pending receipt with no handle; a failed notifier retains the name,
target and Failed state and releases only its unreturned Local reference. Checked
Unmount/Mount recovers projection through the existing lifecycle path.

The native target grammar remains0..4096 opaque non-NUL bytes. Actual Linux syscall
creation accepts1..4095, rejecting empty/4096 before FUSE. Shared FUSE Readlink
returns ENAMETOOLONG for a target at least Linux PATH_MAX bytes, on both local and
canonical paths, preserving exact native4096-byte reads without a truncated kernel
success. Native empty targets remain available for mounted SDK qualification.
[Round48](proposal/fuse-workspace-snapshot-overlay/48-mounted-symlink.md) records
sources, selected kernel proofs and exact verification.

The fresh-file streaming extension is based on commit
`74d4a2ace173d09689b8fbb42953658e94277bab` and frozen product seal
`ffa9fa10899063601d7520581f7db932644a9c45a34b2123fa895f012320da08`.
For an inode that is fresh, noncaptured and regular, write normalization admits
complete retained contents under MAX_FILE instead of the EditFile8-MiB replay
bound. The sole splice caller chooses this only after any inherited-G conversion.
Parsing and lowering require empty canonical base, no Base pieces, and the complete
Local/Zero sum equal to both logical length and replacement count. Existing and
captured-G edits continue counting Local and Zero bytes against8 MiB. Known G
completion clears fresh/captured state as it substitutes saved canonical roots.

The existing ReplacementSource streams one Local payload reader or Zero span at
a time through ConstructFile; no whole-file buffer or new Service operation appears.
Its chunk bounds are clamped in u64 before conversion to usize to preserve progress
at the existing4-GiB MAX_FILE on32-bit platforms. Actual32-bit execution is unrun.
Per-payload8 MiB,128-KiB read/FUSE-write windows,1024 pieces,256 edits, quotas,
4096 payload records, worker count and deadlines stay unchanged. See
[Round49](proposal/fuse-workspace-snapshot-overlay/49-fresh-file-streaming.md) for
selected actual-file proofs and remaining full-corpus admission prerequisites.

The first live attempts of that extension are now recorded. The largest already
preinstalled DSH file18,259,144 bytes streamed through one mounted writable
Workspace in140 caller buffers of131072 bytes with SHA256 verified during the
upload, then one complete ConstructFile/Commit and one4-byte EditFile/Commit,
with native close and teardown clean: **PASS** in40.81s complete. The captured-G
replay selection **FAILED** at its post-rebase `refuse_extra` assertion: the +1
write returned `Capacity` and changed no observed state, but one native
`Inspect` preceded the refusal, so the case is open and neither product nor
caller source was changed on that evidence. Four registered regressions against
the same frozen product passed. These are functional receipts with an undeclared
cache state; the full prepared tree, its one complete upload Commit, incremental
Commits and matched R6 remain unqualified, and Round43's capacity failure stays
open. [Round49](proposal/fuse-workspace-snapshot-overlay/49-fresh-file-streaming.md)
records the selectors, the diagnostic trace and the open failure.

## 14.4 SDK-only product route assembly

Source basis: the commit that introduces this section
(`git log -1 --format=%H -- core/docs/architecture/14-service-runtime.md`).

The replacement-product package `layerfs-server` now owns both the authorized
operation code under `src/service/` and native assembly under `src/host/`.
`layerfs-sdk` depends on it and never the reverse, so the dependency direction
stays acyclic. `Server::create` prepares a fresh Store for a first Init and
`Server::open` opens a prepared Store clone; both build the same authority from
an explicit `ServerConfig`: the Store, its history catalog, the authorized
`Service` with host and daemon peer grants, the loopback acceptor the sandbox
calls back on, and `Server::owner()`, which assembles the `SandboxOwner` bound to
that listener. The SDK constructs `ProjectApi` from a borrowed `Server`,
`ProjectApi::fork` publishes a Branch through the ordinary authorized Service,
and `SandboxApi`/`WorkspaceApi` are constructed from the borrowed `SandboxOwner`.
The former SDK `Host` and forwarding `Client` are retired; every Init caller now
calls `ProjectApi::init` directly. Existing #236 receipts keep their original
source and build identity because this is a relocation and a new composition
surface, not a measured change.

Four product surfaces complete that route:

* `ProjectApi::fork` publishes a named Branch from a project's genesis Layer as
  an ordinary authorized history command, returning a typed `Branch` with its
  base Layer, head Commit, resolved roots and allocation scope.
* `Server::open` reopens a prepared Store clone together with its history
  catalog. `HistoryMode` makes the ownership explicit: `Create` is a fresh
  catalog and `OpenWritable` is one existing, closed catalog taken into this
  process's continuity (`layerfs_history::sqlite::open_writable`), validated for
  application identity, schema version, table set, metadata row and
  binding-derived identity exactly as the read-only open does. Nothing is
  migrated, repaired or promoted; a mismatch is refused.
* `SandboxApi::delete` delegates to `SandboxOwner::delete`, which stops the
  owned daemon within a bounded grace period, then removes the owned container
  and its named Workspace volume, confirming each before it drops the matching
  registry bindings. `DeleteError` keeps the Sandbox ID, the cause and which
  resources still exist, so a partial outcome is recorded and retried
  explicitly. An unknown ID is refused and can never name an arbitrary
  container; a retained `CreateError.sandbox` ID is deletable even when
  readiness never completed. Dropping a `Server` or an owner is not a cleanup
  receipt.
  The #241 diagnostic `delete_with_logs` uses the same owned route. After a
  successful bounded stop and before container removal it streams up to 8 MiB
  of raw daemon stderr into the caller's sink, with a separate capture result
  for command/write failure or truncation. Log collection has a 3 s command
  bound and cannot skip container or volume cleanup; ordinary `delete` does
  not collect logs. This addition is based on the #241 Phase 4 source change
  after `cb1bdb70e97628c2c38055ff2600e070e742010e`.
* `WorkspaceApi::status` exposes the bounded projection and upstream counts
  described above after the acknowledgement, never between Edit and Commit.

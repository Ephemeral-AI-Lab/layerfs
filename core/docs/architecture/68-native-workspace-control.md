# Authenticated Workspace control over the direct Store

> **Status:** Implemented pre-S8 control composition after `9041f338d`; functional
> evidence is recorded in [F13](../issues/307/PRE-S8-F13-20261007.md). Kernel mount
> readiness and Bash Exec remain S8; live namespace normalization remains S10.

Bridge control records carry typed Content/History identities and one original
correlation. They support mount binding, Store-half Commit, status, terminal
unmount, fork and bounded Commit-history pages. A history window holds at most32
records inside one8192-byte control record and resumes using the existing
160-byte authenticated cursor. These are processing windows, not ancestry/file
caps. There is no object, file-length, inode-reservation or Save data RPC.

SDK Control owns one authenticated Connection. A call encodes once, sends once
and receives the original reply once. Before-effect encoding refusal leaves the
channel alone. Failed delivery or malformed/cross-correlated replies retain the
request, attempt phase, original received answer and failed fence independently;
they never reconnect, resend, refresh history or infer rollback. Remote Busy and
other typed refusals leave the channel usable for a later explicitly requested
operation. Concurrent callers can own separate control channels.

Daemon Service uses the existing Owner's configured Workspace capacity, currently
16 by default. Brief registry admission selects an exact Workspace incarnation
and the original positive i64 namespace. A stale token never redirects to another
namespace. The mutex never spans Store reads, construction, publication or queue
waits. Commit and normal unmount arbitrate through explicit local activity state;
one wins and the other returns a typed refusal. Ordinary local projection obtains
a fresh scoped StoreOperation rather than access to bypass control Commit admission.

Mount runs the existing coherent snapshot/bounded root check/local Open and returns
Reply::Bound with the original token and binding. This is the Store/engine half of
mount; it does not claim a kernel attachment or execution directory. S8 attaches
FUSE and supplies full mount readiness. No whole-root qualification occurs here.

R8b update, 2026-10-10 (specification 4.2, D-13): registry admission of a new
incarnation, after the duplicate and capacity checks and before the entry is
created, reads the owner's retained first maintenance failure
(`OwnerClient::maintenance_failure`, a mutex read and no owner job). While one
is retained, Mount is refused `Capacity` with phase `mount:debt` and the
original error as bounded detail; no entry is created, no History or Store
read is made and no owner job is submitted. A stopped or poisoned owner keeps
its existing answer. Already bound Workspaces are not touched by this check.
Not implemented: a declared debt headroom, and maintenance failure in Status.
Proof and its scope: `layerfs-daemon/tests/mount_debt.rs`, where the retained
failure is `Quarantined` after a foreground write met a changed backing-file
identity; a maintenance turn that is itself first to meet a failing resource
is not staged.

Commit forwards the existing Content producer callback over the original capture
and saved base. No test-selected product constructor or Init cleanup exists.
Before S8, an application can build the intended changes directly through Content;
S10 supplies live Workspace normalization. The original capture/Save/history/local
install completions remain in Success or Failure independently of reply delivery.
Known publication stays known after failed local install, and unknown custody
blocks another Commit and normal unmount. An observer never resolves an unknown.

Status copies binding, control activity, known publication and diagnostic epoch
coherently under the registry owner. It then obtains one indexed engine State row,
with its own revision, generation and maintained counters. These observations
have separate scopes; the result is not an atomic snapshot of the whole daemon.
The row's values are copied and its completion is dropped before the reply is
sent, so an answered Status holds no Lifecycle slot (R6); only a failed
observation keeps its original completion with the reply.
If the engine is unavailable, its fields are absent with the original typed
observation refusal, while known control publication remains visible. The server
keeps the original failed observation receipt. There is no Store statement,
Branch refresh, payload read, checkpoint or cleanup hidden in status. Saturating
diagnostic epochs do not refuse operations.

Normal unmount refuses active or unresolved Commit custody. Otherwise it submits
one terminal Close and removes routing only after original known success. The
existing engine owns automatic bounded cleanup. The result does not claim that
all physical rows are gone or that the overlay file shrank. It never deletes
shared history. For an attached Workspace the same unmount first runs the
reversible kernel probe, connection drain and revocation described in
[native mount session](76-native-mount-session.md#normal-unmount). The explicit
force policy is a separate request, `ForceUnmount`, described in
[forced teardown](80-forced-teardown.md); normal unmount never falls back to it.

serve_one executes one authenticated request and sends one result. Served keeps
the original product outcome; ServeFailure retains it when encoding/delivery
fails. The embedding daemon owns connection admission and serving lifetimes.
No host application assembly, restart custody or remote Save runtime is added.

Concrete bootstrap exposes cumulative diagnostics for its already-open writer
and fixed readers, using Persistence's existing session counters without SQL.
This remains outside provider-independent store/. It lets the status proof count
all Store statements, including history, rather than only object demands.

The current Branch publication policy is overwrite-only: two completed candidates
from the same captured head may both publish, with the last database effect
defining the head. Each candidate retains its captured ancestry; displaced
Commits remain immutable. Native replies preserve that known result or original
unknown independently. See the [owner decision and proofs](../issues/307/BRANCH-OVERWRITE-DECISION-20261007.md);
the earlier F13 HeadMoved receipts are historical evidence only.

## Next native integration boundary

At verified R1 product tree e3a61dfd814a579b58f56b63754ec3dbce83d67e, this control
service still supplies logical Bound and the Store-half Commit composition. The
[reviewed R2–R5 target](../issues/307/R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md)
assigns the full native connection/request service to Fuse and leaves this daemon
registry/control owner responsible for overall Ready/terminal unmount and existing
Commit admission. Attach, Locate, Ready and per-Workspace normal drain are now
implemented in that owner; see [native mount session](76-native-mount-session.md).
No daemon-wide aggregate drain exists.

## Forced unmount records

R6 adds these control records. Every earlier request and reply keeps its tag
and bytes; an unknown tag, value or boolean is refused as before.

| Record | Encoding | Sent when |
| --- | --- | --- |
| `Request::ForceUnmount { token, relinquish_unknown }` | request tag 11: token, one strict boolean byte | the caller forces one attached Workspace |
| `Reply::ForceUnmounted { token, outcome }` | reply tag 14: token, forced facts, `NativeWork`, cleanup byte | the forced teardown completed and the entry left routing |
| `Reply::Retained` with `forced: Some(ForcedFacts)` | reply tag 15: the tag 12 body, then the facts | a forced teardown stopped after an effect, or a later Unmount, ForceUnmount or Attach met that stored custody |
| `Reply::Retained` with `forced: None` | reply tag 12, unchanged | a normal unmount or an Attach stopped after an effect |
| `ForcedFacts { abort, detach, commit, fenced }` | abort byte, detach byte, Commit knowledge (`Absent`; `Published` with the existing outcome encoding; `Unknown`), `u64` | inside tags 14 and 15 |
| `NativePhase::Stopping` | value 7 in the native status block | status while a forced unmount owns the session |
| `TeardownStage::Abort` | value 9, valid only with forced facts | the abort write was short or failed |

A refusal before any effect is the existing `Reply::Refused` with phase
`force:admission`, `force:custody` or `force:capability`. The daemon routes
the request in [`control/operations.rs`](../../crates/layerfs-daemon/src/control/operations.rs)
to [`control/force.rs`](../../crates/layerfs-daemon/src/control/force.rs); the
SDK operation is `WorkspaceApi::force_unmount(token, relinquish_unknown)`.

A Commit failure that leaves the registry `Uncertain` always answers `Unknown`:
besides an unreleased reader or owner, that now includes a definite failure
with no publication whose one local resolution of the capture is not known
done (for example `Busy` when the Save begins, then a resolution that could
not be submitted). A failed install after a known publication (`LocalFailure`)
keeps answering its own code with the publication attached.

R5, after `bb5b3c220`: `execute_control` no longer refuses Commit. It runs the
captured namespace producer through the unchanged driver
([product Commit](79-product-commit.md)). The paragraph above that says no
product constructor exists describes the earlier source.

## Explicit operator observations

The R7 source amendment after planning identity `980c169e6` adds request tag 12,
`Request::Observed { scope, request }`. It carries a caller-selected 32-byte
nonzero public scope followed by exactly one ordinary request tag and body.
Encoding and decoding refuse a zero scope, another Observed layer, Hello or
EndSession before parsing or allocating an inner wrapper. Every earlier control
record keeps its original bytes and the observed operation returns its ordinary
existing reply. Application owns the diagnostic sink; a direct Service call
refuses an unprocessed wrapper. The operation executes once and its original
known result, failure or unknown survives diagnostic transfer failure.

`WorkspaceApi::with_observations(&mut Control, &scope)` selects the wrapper through the
same private exchange used by ordinary facade methods. Its one caller-owned
scope reference lives only in that borrowed facade alongside its Control
reference; the facade remains two pointers. `WorkspaceApi::new` retains the
ordinary request encoding. The daemon retains no observation preference; a later
ordinary request on the same channel has ordinary behavior. SDK reply matching
validates one checked layer and preserves exact token/correlation checks. The
caller supplies a fresh scope for each observed channel/run. The daemon stores
no used-scope registry or connection nonce. Evidence groups use existing daemon
instance plus caller scope plus call as identity; slot is attribution only and
may be reused. An evidence consumer rejects duplicate complete identities.

Cleanup is a separate explicit observation, `WorkspaceApi::cleanup(token)`:
request tag 13 carries the existing WorkspaceToken, and reply tag 16 carries the
same token followed by one strict state byte (1 Live, 2 Held, 3 Queued, 4 Gone).
It observes indexed existing namespace/incarnation and terminal-queue state in
the current daemon's allocated namespace domain. It does not recreate a Route,
retain the removed control binding, advance maintenance, repeat Unmount or
resolve an original unknown. A live row requires exact incarnation agreement;
an absent valid allocated row is Gone. Invalid/unallocated namespaces retain a
typed refusal. Logical Unmounted and physical Gone remain distinct outcomes.

Existing Store SQL observations can now be copied one writer/reader at a time
without constructing an additional reader vector. The application observation
schema and its separate epochs are described in
[daemon application](70-daemon-application-startup-control.md#numeric-observation-schema).
This instrumentation is source behavior, not qualification or a measured
optimization result. Its owning external tests include additive literal bytes,
malicious nesting, observed facade scope, and authentic Linux binary observations.

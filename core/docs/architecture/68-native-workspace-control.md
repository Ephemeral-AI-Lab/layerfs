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
If the engine is unavailable, its fields are absent with the original typed
observation refusal, while known control publication remains visible. The server
keeps the original failed observation receipt. There is no Store statement,
Branch refresh, payload read, checkpoint or cleanup hidden in status. Saturating
diagnostic epochs do not refuse operations.

Normal unmount refuses active or unresolved Commit custody. Otherwise it submits
one terminal Close and removes routing only after original known success. The
existing engine owns automatic bounded cleanup. The result does not claim that
all physical rows are gone or that the overlay file shrank. It never deletes
shared history. Native activity/FUSE fences and any explicit force policy are S8
integration work, not a silent success path in this pre-S8 binding service.

serve_one executes one authenticated request and sends one result. Served keeps
the original product outcome; ServeFailure retains it when encoding/delivery
fails. The embedding daemon owns connection admission and serving lifetimes.
No host application assembly, restart custody or remote Save runtime is added.

Concrete bootstrap exposes cumulative diagnostics for its already-open writer
and fixed readers, using Persistence's existing session counters without SQL.
This remains outside provider-independent store/. It lets the status proof count
all Store statements, including history, rather than only object demands.

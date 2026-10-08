# Observation review amendment

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Fresh-context review became available after the earlier temporary thread-limit
refusal. It inspected the uncommitted Stage 0 telemetry and found two concrete
coverage gaps before any product timing or instrumentation acceptance.

1. Two authenticated connections can both send observed Status at call 1 for
   the same namespace. All 54+R groups currently have zero connection-scope and
   zero daemon-instance fields. Contiguous stderr groups do not identify their
   original client. Slot reuse also prevents slot alone being an identity.
2. A successful native drain followed by either Revoke or Close refusal retains
   its original receipt behind generic failure evidence; section 25 cannot
   deliver its 13 MountWork numbers after the dispatcher lane has been released.
   Custody remains exact. This is a failed-prefix observation limit.

Taken under the owner's direction of 2026-10-09: fix item 1 using caller-supplied
public observation scope in the new, not-yet-committed Observed wrapper. Encode
32 nonzero scope bytes beside its one additive tag. Old ordinary request/reply
bytes stay unchanged. `WorkspaceApi::with_observations` borrows that scope
beside the existing Control pointer; it stores neither a daemon preference nor
a new per-connection nonce. The caller supplies a fresh scope for each control
channel/run; the receiver emits the scope, existing daemon instance and existing
admitted slot beside call correlation. No registry of used scopes is retained.

The envelope's identity is daemon instance + caller scope + call. An evidence
consumer rejects duplicate groups with that identity and records connection
scope setup. Duplicate caller scopes are an explicit operator-input error, not
permission to invent attribution. All record/byte/admission allowances remain
unchanged. Formatter references existing request/instance bytes and streams the
identity fields directly; no whole JSON buffer or persistent state is added.
The four-pointer/array serialization changes only this new diagnostic wire.

Proof: two channels with equal call numbers and equal namespace, plus a later
channel reusing a slot, produce distinct caller-qualified complete groups and
unchanged ordinary outcomes. Retain literal old-byte tests. Update SDK/harness
API use and fixed-record bound tests before baseline.

Item 2 remains explicitly UNAVAILABLE at these two failed terminal boundaries.
No success/full-failure accounting claim is made for it. Benchmark receipts
retain original failure, native opcode/join facts and missing fields with this
exact source cause. A future bounded typed view of retained generic evidence is
a permitted instrumentation candidate, but no copied retained snapshot, replay
or guessed typed result is added here. This gap does not establish a failed
operation's success, nor remove its original receipt from registry custody.

Other verified review facts: one original operation and one reply attempt,
gated ordinary zero observation work, locks released before output, fixed
54+R records under 64+R and 4096 bytes, one allocated-domain cleanup query with
no mutable Route, original typed successful drain receipt and unchanged cache/
thread/queue/database schema allowances. These are source findings, not runtime
proof or a measured optimization.

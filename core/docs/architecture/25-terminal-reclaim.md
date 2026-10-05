# Exact terminal ownership and bounded automatic cleanup

> **Status:** Current general guide; initial S6 slice, not complete lifetime/pressure qualification.

The #307 checkpoint after `c6df039e6` adds overlay logical close and daemon-owned
terminal maintenance. It does not implement live-generation retirement, effective
open-unlinked filesystem semantics, repeated failed-capture composition or physical
headroom admission. Complete S6/S8 exits remain open.

[Close](../../crates/layerfs-overlay/src/lifetime/close.rs) revokes new mutations/acquisitions
in one short transaction. Existing exact releases, published reply-send-attempt
tickets and already allocated captures remain owned. Closed captures stay readable;
known install can finish an existing capture, or an explicit closed-capture release
requires the trusted caller to fence/resolve all construction/history custody.
Unknown history cannot be inferred from absence or released automatically. These
engine primitives do not themselves resolve upstream outcomes.

A namespace enters the ready index only when closed, with no capture, exact owner
lease or pending publication ticket. Eligibility uses primary-key point/EXISTS
queries, without COUNT or owner sweeps. Release/reply/capture transitions maintain
that ready row in the same transaction. Existing held namespaces never enter the
ready scan; many long-lived references cannot enlarge another namespace's cleanup
query. The ready key uses reserved i64 maximum, which capture cannot allocate
because it must first allocate the next active generation.

[Reclaim](../../crates/layerfs-overlay/src/maintenance/reclaim.rs) rotates ready namespaces by
keyset, then deletes fixed windows from payload, names, inodes, scratch and old
retirement rows. It holds at most 64 selected metadata keys. Payload selects at
most 14 fixed cells (64,512 declared BLOB bytes); scratch selects length metadata
and deletes at most 65,536 BLOB bytes per step. Names are bounded by format width
and the 64-row window. Scratch values/payload are not copied to plan deletion.
Index/journal/page work costs real I/O and is not proven bounded resident memory
by declared-byte counts. Final foreign-key-checked deletion removes ready/workspace
rows after child tables are empty. Freelist space is reusable; the file does not
necessarily shrink.

Schema v4 adds reclaim_ready(queue_key,ns) and payload_namespace_row(ns,rowid).
Actual ready/payload SQL explicitly uses them. EXPLAIN plus Reclaim-family runtime
counters are observed from the first path. Every statement retains namespace
predicates, including rowid deletion. One connection/profile remains initialized
before readiness; no sync call or Store profile change is introduced.

[Daemon](../../crates/layerfs-daemon/src/overlay/owner.rs) gives maintenance one short turn
after at most eight dispatched jobs and runs remaining ready work while idle.
Namespace rotation keeps large closed work from monopolizing cleanup. Idle waiting
checks an event revision under the queue mutex before sleeping, so admission/
progress cannot be lost between a DB-ready check and wait. No lock spans SQL,
result delivery or construction. Stop checks held-but-unattempted jobs after a
maintenance turn; parked capture retains its original stop fence.

The first automatic maintenance error retains its original nested cause and stops
further automatic attempts. It does not retry, drop the target or infer success.
Bounded diagnostics expose jobs/rows/declared data bytes/service wall and retained
error. Ordinary jobs keep actual engine results, including shared-engine quarantine
when applicable. Restart inspection and actual ENOSPC/uncertainty proof remain open.

Worst/cumulative cleanup is O(D log N) indexed deletion work for D retired rows,
plus fixed table/ready transitions, with O(window) selected keys. Each row is
deleted once. Live folds, growing generation chains, the prototype's max-page
admission, headroom and aggregate pager/journal/socket/process residency remain
required resource criteria; this slice does not solve them.

Public proofs cover exact owners/replies, capture retention, a 1,024-row held
namespace beside ready work, 65,536-byte scratch windows, actual plans/counters,
unrelated live progress and idle completion without a reclaim/status job. Native
FUSE/unmount owner drain is unimplemented; these proofs do not establish kernel
reference or full teardown qualification.

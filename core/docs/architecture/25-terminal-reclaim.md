# Exact terminal ownership and bounded automatic cleanup

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> **Status:** Current general guide; initial S6 slice, not complete lifetime/pressure qualification.

The #307 checkpoint after `c6df039e6` adds overlay logical close and daemon-owned
terminal maintenance. Subsequent [live composition](32-live-composition.md) and
[independent custody](33-independent-custody.md) add live generation/orphan
cleanup and repeated definite-failure composition. Physical headroom admission
and complete S6/S8 exits remain open.

R7 update, 2026-10-09 (reclamation by key range, decision U4). Implemented:
a step of a closed namespace reads one page of one terminal table in
primary-key order and deletes it with ONE statement,
`DELETE ... WHERE ns=?1 AND (key columns)<=(last key of the page)`, in place
of one delete per row. The pages keep their bounds: 64 rows for every
metadata table, the 65,536-byte value window for operation records, and for
payload one maintenance page of 14 cells of bytes (a row of several cells
counts as its cells, so one 32 KiB row is a step while the next row is
another). The payload page and delete use the unique key
`(ns,serial,gen,cell_offset)`; no statement of the engine names or plans
`payload_namespace_row` any more. It still has one reader (checked
2026-10-09 under the bundled SQLite, receipt
`353-cleanup-payload-plans.txt`): with `foreign_keys=ON`, the final
`DELETE FROM workspace WHERE ns=?1 AND lifecycle=1` checks every child
table by `ns`, and its program opens `payload_namespace_row` for that scan
of `payload`. Without the index the same scan opens `payload_generation`.
The index is therefore kept; dropping it is a decision about that scan,
not the removal of an unread index. A table that holds nothing for the namespace is
passed in the same step: a step deletes one page of the first table that
still holds rows, or the final ready and workspace rows, so an empty table
costs one page read and no transaction of its own (at most 12 empty page
reads in a step). Indexed operation records are deleted the same way, also
after an operation's last owner releases. Each further step is three
statements whatever its rows: the ready queue, the page, the delete.
Exact counts at two sizes beside a live namespace are in
[`reclaim_cost.rs`](../../crates/layerfs-overlay/tests/reclaim_cost.rs).
Since overlay schema 28 the payload delete trigger accounts from the
stored lengths and the delete does not load the row's bytes
([daemon overlay](19-daemon-overlay.md)).
The paragraphs below that describe a delete per row and "Actual
ready/payload SQL explicitly uses them" for `payload_namespace_row` are
historical.

[Close](../../crates/layerfs-overlay/src/lifetime/close.rs) revokes new mutations/acquisitions
in one short transaction. Existing exact releases, published reply-send-attempt
tickets and already allocated captures remain owned. Closed captures stay readable;
known install can finish an existing capture, or an explicit closed-capture release
requires the trusted caller to fence/resolve all construction/history custody.
Unknown history cannot be inferred from absence or released automatically. These
engine primitives do not themselves resolve upstream outcomes.

R7 update, 2026-10-09 (overlay schema 24): "exact owner lease" below is every
exact owner. A descriptor and a lookup reference no longer have a `lease`
row, so the eligibility statement names their tables (`file_handle`,
`lookup_owner`, `native_lookup`) beside `lease`, `native_mount` and
`native_directory`, each by a key probe. See the
[overlay note](19-daemon-overlay.md).

A namespace enters the ready index only when closed, with no capture, exact owner
lease or pending publication ticket. Eligibility uses primary-key point/EXISTS
queries, without COUNT or owner sweeps. Release/reply/capture transitions maintain
that ready row in the same transaction. Existing held namespaces never enter the
ready scan; many long-lived references cannot enlarge another namespace's cleanup
query. The ready key uses reserved i64 maximum, which capture cannot allocate
because it must first allocate the next active generation.

[Reclaim](../../crates/layerfs-overlay/src/maintenance/reclaim.rs) rotates ready namespaces by
keyset, then deletes fixed windows from payload, names, inodes, operation records and old
retirement rows. It holds at most 64 selected metadata keys. Payload selects at
most 14 fixed cells (64,512 declared BLOB bytes); operation records selects length metadata
and deletes at most 65,536 BLOB bytes per step. Names are bounded by format width
and the 64-row window. OperationRecord values/payload are not copied to plan deletion.
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
namespace beside ready work, 65,536-byte operation records windows, actual plans/counters,
unrelated live progress and idle completion without a reclaim/status job. Native
FUSE/unmount owner drain is unimplemented; these proofs do not establish kernel
reference or full teardown qualification.

S6 completion update after `be651a048`: schema14 now includes backed resource
accounting and indexed source waits; physical reservation, cleanup headroom,
bounded generation wakes and nonduplicating orphan migration are implemented.
See [shared physical capacity](35-shared-physical-capacity.md) and the
[S6 exit audit](../issues/307/S6-EXIT-AUDIT.md) for current scope/evidence. Earlier
checkpoint limitations and numbers above retain their original source identity.
Native/runtime/kernel and integrated qualification remain later milestones.

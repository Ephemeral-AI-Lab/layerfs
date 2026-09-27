# Issue 271: indexed active head with a charged edit journal

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.

The [research](RESEARCH-OWNERSHIP-EXEC.md), [original reform proposal](../../../../docs/roadmap/0.1/0.1.7/issue271-causal-telemetry-and-reform-proposal.md),
and [append-only causal ledger](CAUSAL-DIAGNOSTIC-LEDGER.md) show where this
work is paid. The second diagnostic has a complete Commit cause record and a
full oracle, but its ordinary cache is uncontrolled. Its raw latency is useful
for locating work within that run; it is `INELIGIBLE` for comparative speed or
release admission. The earlier 25 s public gate remains FAIL.

## Cost and crossing model

Let `W` be acknowledged one-byte WRITEs, `E` final extents, `H` extent-tree
height, `Δ` copied ownership edges, `P` private Local payload files, `S`
replacement bytes, and `F` bounded Bridge body-frame capacity. In this
workload `W=4,097`, `E=8,194`, `H=2`, `P=S=4,097`. The current path creates
an immutable candidate extent root for each WRITE, charges its owned edges,
and reclaims an eligible previous root before the next payload acquisition.
An abstract bounded-fanout tree suggests `O(W log_B E + Δ)` construction and
reclamation work, with a large finite coefficient. It does not imply a
global `O(W²)` scan. The retained per-512 ledger reads rise from 19,426 to
about 30,500 around the height-two transition, then roughly level off;
each splice visits one old leaf and at most two branch pages in this range.
Through WRITE 4,096, **220,377 ledger reads**, **119,492 writes** and
**15,419 page creations** accompany 409 live pages at that checkpoint.

The independent timing split places **10.912 s** in acquisition metadata
maintenance, primarily old-root reclamation, and **14.853 s** in
publication core through WRITE 4,096. Those non-overlapping children total
**25.765 s**, 97.2% of the **26.497 s** Exec call in the same run. Open and
identity validation for 274,418 ledger-file calls took **0.521 s**; page
creation took **1.955 s**. Both are nested cost centers, and deleting only
them cannot explain a 2× Exec target. The old-root charge/release cycle is
the larger algorithmic opportunity.

| Crossing or local call | Observed count | Implication |
| --- | ---: | --- |
| Public SDK Exec / Commit | one each | No per-WRITE SDK round trip to remove. |
| Upstream Service calls | four total | Control traffic is constant for this one-file route. |
| FUSE WRITE request/reply | 4,097 | One per declared `pwrite`; batching them would change the workload and acknowledgement contract. |
| Checked kernel notifier | 4,096 through checkpoint; 0.009 s | Coherence requires it; even ideal elision cannot supply a multi-second gain here. |
| Local ledger reads/writes | 220,377 / 119,492 through checkpoint | Authenticated 4 KiB ownership work, not Service RPCs. |
| Commit descriptor/replacement source calls | 13 / 2 | Internal bounded stream reads. Actual Bridge frame count is unavailable. |

The Commit stream is separately `O(E+S+P)`: 8,194 descriptors, 4,097 edit
records, 4,097 Local payload opens and 4,097 authenticated aligned 4 KiB
reads for 4,097 useful replacement bytes. Its daemon replacement source took
**0.308 s**, including **0.254 s** in aligned reads; the host pre-save span
was **0.372 s**, including **0.028 s** of spool writes. The full SDK Commit
was **0.449 s**. A one-write-per-edit spool change or tiny-payload pack can
help Commit, but neither directly removes the roughly 25.765 s of Exec
publication and reclamation. Halving Commit alone saves at most 0.225 s of
this 31.391 s complete command if all else is fixed.

For scale, this raw diagnostic would need **more than 6.391 s** removed to
fall under the unchanged 25 s complete-command limit, and a 2× Exec target
would need **13.248 s** removed. That means cutting over half of the measured
publication-plus-reclamation pool if all other work stays fixed. A 2×
complete-command target is harder because lifecycle and Commit still cost
time. These are arithmetic opportunity thresholds, not qualified speed
predictions; a new representation must pay any deferred seal or cleanup
inside the appropriate measured phase.

## Proposed representation

The [Workspace-scoped architecture and complexity proposal](WORKSPACE-SCOPED-ACTIVE-HEAD.md)
specifies one active generation spanning inode attributes, namespace deltas,
dirty membership and file extents; one pack shared across files; repeated
Exec and Commit behavior; and the current single-selected-Workspace daemon
limit. Its implementation and proof are tracked in [#273](https://github.com/Ephemeral-AI-Lab/layerfs/issues/273),
a sub-issue of #271. These boundaries are required to remove keyed-root publication as
well as extent path copying. Its 64/128-byte records, 3 MiB one-file budget,
amortized constant hot-right-edge updates and timing examples are design
targets or sensitivity calculations, not measured after results.

The detailed proposal's [implementation rollout](WORKSPACE-SCOPED-ACTIVE-HEAD.md)
freezes the format and public workload contract first, switches one complete
Workspace incarnation, then proves capture, cleanup and performance before
default selection. The existing Service `SaveFile` and C1 `apply_edits`
localized construction stay in place: Commit lowers the **final indexed
view**, not the historical journal records. The C writer exercises ordinary
mounted `write`/`pwrite` calls through FUSE; it is an application-level
workload, while its uncontrolled-cache diagnostics remain speed-ineligible.

Use a **versioned, disk-indexed active head** for Workspace edits. Each
accepted WRITE appends a charged edit record and updates a mutable ordered
index of the current file view under the existing writer gate. A bounded
in-memory right spine can make the separated, increasing-offset case an
amortized `O(1)` page-update route; arbitrary overlapping writes remain
`O(log_B E + K)` for `K` affected extents. Reads use the index in
`O(log_B E + output)` rather than scanning an append log. The format must
charge disk and resident index/journal bytes before acknowledging work.

An immutable generation is pinned when a snapshot, captured reader or Commit
needs it. The design target is a bounded root/watermark pin and pack-tail
seal, then page copying on the next generation's first touch. A later WRITE
advances the new active head; G1/G2 and other retained roots keep their exact
older bytes. Commit validates and streams the dirty files' ordered final
sequence through the existing bounded SaveFile route. A full `O(E)` index
rebuild on **every** small Commit would defeat the quick-operation goal; if
the implementation requires construction, that work belongs inside the
Commit or snapshot timer and must pass the separately specified quick-Commit
case. Delaying work until after measurement, letting unbounded garbage
accumulate, or using a warm cache to hide the seal is not acceptable.

Publication still has to make the new bytes and attributes visible before
the FUSE reply, including aliases and concurrent readers. Each journal/index
change needs a bounded in-process transaction: either the new revision is
published exactly once or its partial state is rolled back or quarantined.
An uncertain reply keeps the existing typed unknown-outcome behavior; no
blind retry may duplicate an acknowledged edit. This is a storage and
ordering proposal, not a new durability promise: Workspace backing still
does not call `fsync`, `fdatasync`, `sync_data` or `sync_all`.

## Proof obligations before replacing the format

1. Specify the versioned journal/index bytes and a read-only migration path.
   Preserve all old-root and frozen generation readers, including G1/G2 and
   a reader pinned during a later WRITE.
2. Prove overwrite, overlapping ranges, append, truncate, holes, zero-fill,
   size/mtime, read-after-WRITE and concurrent lookup semantics through the
   public FUSE route. Keep one acknowledgement per original syscall.
3. Bound and charge journal, index, payload, cleanup and resident windows at
   every stage. Exact quota refunds and candidate abandonment must work after
   failures; cleanup time stays visible in the complete command.
4. Preserve one construction worker, checked kernel coherence, one SaveFile
   stream, the old/new-head full-byte oracle, typed unknown outcomes and
   clean close. No warm cache, changed worker count or larger gate timeout.
5. Freeze a new source/format and workload specification before measurement.
   Use one public sample per arm, a matched independent verifier and the
   unchanged 25 s gate. Any general 2× claim additionally needs separately
   declared cache-qualified matched workloads beyond this one-byte extreme.

## Smaller experiments and decision rule

A candidate-scoped ownership delta can coalesce repeated ledger-page RMWs
across new-root charge and old-root release. It needs an exact count of
identical PageRefs added and removed and distinct ledger pages touched per
candidate; aggregate edge totals are insufficient. It also needs an
authenticated partial-write recovery proof. This is a narrower experiment,
but it retains per-WRITE immutable root creation and may leave the 10.912 s
reclamation bucket largely intact. Four-hop sponsorship already avoids many
copied-edge charges; increasing its depth again has no measured route to 2×.

The active-head design is the recommended architecture experiment because it
targets **both** dominant buckets. Commit's aligned reads and spool calls
should be addressed only after the new format makes them material to the
total. Neither a source read nor a ledger open is an upstream round trip;
the next count instruments should track candidate ledger-page revisit and
freeze/cleanup work, rather than adding another per-WRITE SDK callback.

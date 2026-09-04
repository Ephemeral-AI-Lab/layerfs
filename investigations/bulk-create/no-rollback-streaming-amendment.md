# Phase 1 amendment: streaming Commit without object rollback

2026-09-04. This amendment records the user's explicit clarification that a failed
Commit need not roll back newly admitted CAS objects. It supersedes the earlier
complete-preflight-before-insertion and owned-insert-undo proposals for the isolated
Phase 1 implementation. It does not authorize merging, publishing, changing frozen
release evidence, or weakening the invariants below. No further clarification is
needed to implement this bounded interpretation.

## Revised contract

A construction, validation, collision, resource or expected-head failure may leave
earlier successfully committed admission batches in SQLite. Report the error,
keep the published branch head unchanged unless publication actually succeeded,
preserve every preexisting object/root, and stop/join workers and clean private
resources. Retain usable Workspace/recovery state for retry or Discard. A retry
may reuse objects left by an earlier failed attempt; those rows count against
normal Store limits and must be observable in failure diagnostics.

Do not implement an owned-insert rollback journal or delete those rows to restore
the original object inventory. This waiver does not disable SQLite rollback of
an unsuccessful individual transaction: final commit-record/head publication
remains one conditional atomic transaction. After publication, preserve the exact
published outcome through installation/cleanup failure and never duplicate Commit.

Normal filesystem delete still changes root reachability and never removes
historical CAS chunks. No admission moves into Exec or arbitrary live writes.
The waiver covers failed Commit attempts, not abandoned local sub-attempts inside
an ultimately successful Commit: successful candidates must not silently retain
unselected reconciliation output, superseded content or tree-retry intermediates.
No-op/repeated Commit behavior, hard links, stable handles, collision validation,
resource limits and private cleanup remain mandatory.

## Continue the existing implementation

Current implementation HEAD inspected: `69bca3950`. Latest measured/verified
candidate 7 is source `8165519b337bc5eaa3e28d9ae0842fc9d2f5272e`:
100k create Commit **10.420664 s / 11.21 CPU-seconds**, in a two-CPU container;
canonical and fresh-FUSE proofs passed separately. Generic delete has an earlier
**0.379839-s** sample; neither result is a completed three-seed assessment.

Pool reuse and 64-KiB handoff buffering are already implemented. Candidate 7 used
one pool/two workers, 140 admission transactions, up to 8,191 objects per batch,
less than 4 MiB payload, and zero recorded admission payload copies. Preserve
these, the generic tree merger, reference ledger and checked handoff. Do not
restart them as new optimizations or introduce a create-direct/delete-special
route.

The current slab pipeline overlaps **reading a completed private candidate** with
SQLite insertion. It still rereads **565,781,158 canonical payload bytes** and
performs a separate membership plan. The new target is construction-to-admission
overlap, eliminating that payload reread and corresponding private write for
streamed final output—approximately **1.13 GB of temporary traffic**. Original
mutable Workspace backing still remains in Phase 1 for failure/retry/handle use.

## Recommended generic architecture

```
freeze final Workspace generation and acquire Store operation ownership
  → existing bounded content workers / generic tree constructors
  → selected final canonical objects in existing owned slabs
  → one SQLite owner: carried batches, INSERT or exact conflict comparison
  → compact identity/receipt accounting and final selected-root validation
  → conditional atomic publication
  → existing checked identity installation and private cleanup
```

Introduce the smallest Store-owned admission session spanning construction through
publication under one existing operation permit. Avoid recursive acquisition of
the nonreentrant Store gate. Cheap early branch/base checks may avoid wasted work;
retain the final conditional publication check.

Reuse `InitializationSlabWriter`, `InitializationObjectSlab`, the current bounded
pool and large-object adoption path. Extract the checked insertion primitive
from `insert_initialization_segment_batch`; do not invoke the empty-Store wrapper
or initialization cleanup. Replace global membership planning with actual checked
insert/conflict outcomes where those determine the same fact. IDs and lengths
alone do not replace exact collision/authentication checks.

One producer output must represent a selected final version per inode, not per
alias. The generic builder's finality/selection boundary governs all operations.
Keep bounded readable private state for intermediate rope/structural construction
until its selected output is known. In particular, directory memory-pressure
attempts can emit before retry; those objects must not flow blindly into CAS if
the parent Commit later succeeds. Reconciliation must select its actual root and
content before authorizing its alternatives for persistent admission. Prove these
boundaries rather than setting an unchecked `all_reachable` flag.

Retain a compact exact candidate identity ledger, not a canonical payload spool
merely for accounting. SQLite conflicts can represent either preexisting data or
duplicates inserted earlier in the same attempt. Reuse bounded sort/merge/index
machinery to distinguish unique candidate IDs, actual newly committed inserts,
preexisting reuse and duplicate submissions. A 40-byte ID/length record for about
412k unique objects is approximately 16.5 MB on private disk, consumed through
bounded buffers. Charge repeated emissions and spill growth; never use repeated
linear journal scans. On success preserve candidate equations; on failure report
successfully admitted objects/bytes and retained-row behavior without inventing a
successful receipt.

The sole writer must not wait for producer input while holding the SQLite
connection/transaction. Readers and writers share the same connection mutex.
Likewise a producer must release DB read guards before sending to a bounded queue.
Form a bounded batch, acquire/insert/commit/release the writer, then receive again.
Dependent object reads must resolve from a bounded readable pending buffer or a
safely acknowledged batch; avoid per-object commits and queue/mutex deadlocks.

## Resource and performance budget

Keep two-CPU results separate from the authorized eight-CPU profile. The primary
new experiment target is **complete create Commit below five seconds on eight
available CPUs**, while reducing total CPU work. This is a research target, not
a promise or a change to frozen release gates. Two-CPU **5–7 s** is a first
planning range; retain and report the original two-CPU three-seed assessment.
Keep generic delete around **0.3–0.5 s**, with its prescribed proof/median gates.

| Eight-CPU critical-path allocation | Budget |
| --- | ---: |
| Concurrent construction, checked admission and most object accounting | 3.00 s |
| Remaining closure, namespace and publication | 0.55 s |
| Checked installation and required cleanup | 1.10 s |
| Contingency | 0.35 s |
| Total | 5.00 s |

Current candidate 7: content 3.252 s, closure 0.807 s, membership 1.679 s,
admission 3.018 s, installation/cleanup 1.266 s. Merely overlapping the current
content and membership+admission work gives about **7.17 s**, before contention;
sub-five needs real removal of staging, index and repeated checking work.
Admission's 1.166-s insertion plus 0.648-s transaction-commit attribution is nested
inside 3.018 s, which also includes 1.128-s consumer idle. It supports testing a
better-fed writer; it does not guarantee 1.814-s standalone admission.

Target **8–9 CPU-seconds** initially, about 20–29% less work than 11.21. With that
work a five-second two-CPU run would require 80–90% aggregate utilization and a
small serial tail. Eight available CPUs do not require eight active producers:
choose concurrency from the existing shared memory/CPU allowance.

Construction and admission now overlap, so recompute simultaneous ownership;
formerly non-overlapping peak reservations cannot simply be reused independently.
Retain the 8-MiB final-delta and shared file-construction allowances, existing
candidate/index and Store/spool limits, four ≤256-KiB/512-object queue slabs,
blocked-sender/in-flight ownership, one <4-MiB/≤8,191-object batch, thread stacks,
large-object adoption, identity-sort buffers and SQLite memory. Prefer fewer
workers over raised limits. Reserve before growth and count failed-attempt CAS
rows toward the normal Store bound.

## Comparators and smallest next experiments

The historical 2.766-s / 12.968-CPU-s initializer remains a mechanism reference.
The exact-tree two-CPU initializer now measured **15.838443 s / ~13.31 CPU-s**, but
selected **fast_path=0**. Its direct-pipeline zero fields are unavailable for the
fallback, not evidence of no work. Diagnose fallback and actual occupancy; neither
claim optimized-initializer parity from that slow result nor reshape paths to
activate another route. Preserve and extend the matched two-/eight-CPU evidence.

1. One focused mixed fixture: create/update/delete, hard links, preexisting and
   repeated content; more than one admission batch; late collision/source/resource
   error. Verify unchanged old head/rows, permitted retained new rows, precise
   errors, joined workers, private cleanup and successful retry/reuse.
2. Force partial structural emission followed by memory-pressure retry, plus
   write/revert, create/delete and reconciliation selection. Successful Commit
   must not persist objects belonging only to discarded local alternatives.
3. Exercise final head conflict, postpublication installation failure and saturated
   queues with reads. Preserve atomic publication/outcome and prove no deadlock.
4. One selected 2k diagnostic: demonstrate producer→writer overlap and removal of
   canonical spool bytes on finalized streamed output with identical successful
   root semantics, receipt equations and bounded simultaneous ownership.
5. One seed-1 100k eight-CPU sample plus separate canonical/FUSE verification;
   record complete wall/CPU, temporary payload bytes, owner/producer idle, object
   accounting, full tail, peaks and cleanup. Then one comparable two-CPU sample
   distinguishes eliminated work from extra parallel capacity. Reuse existing
   generic-delete evidence/tests where compatible and recollect invalidated proof.

Finish the currently running measurement/cleanup before changing source or profile.
Do not rerun already-passing families indiscriminately. Preserve all old evidence
under its old source/contract. Implement this as an incremental successor to the
current generic engine, not a restart. No merge, push, publication or Phase 2
live-operation optimization is authorized by this amendment.

## Review synthesis

Three read-only subagents reviewed pipeline construction/admission, revised
semantics, and performance/resources. They agreed that the waiver removes the
complete-preflight barrier and rollback journal, and that the remaining critical
proof is final-only successful output plus exact accounting and shared-budget
ownership. They also verified that pool reuse, buffered handoff and carried large
batches are already present, and updated the quantitative baseline from 10.70 to
10.42 seconds. No benchmark was run by the reviewing agents.

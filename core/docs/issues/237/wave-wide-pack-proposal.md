# #237: exact pack-boundary buffering across one C2 wave

> **Status:** Read-only research proposal. No product edit, test, public
> sample or speed result belongs to this page. The isolated experiment's
> seven-COMMIT source remains `3e321080a`; SQLite pages stay **4,096 B** and
> the C1 cutoff **128 KiB**.

## Why a second proposal is needed

The rejected [per-lane queue iteration](per-lane-pack-experiment.md) would
remove lane-switch drains but leave the queue's 256-KiB encoded-byte flush
rule. The only detailed hot-path count available is an
[adjacent 4-MiB-wave diagnostic](evidence/per-lane-pack/adjacent-4mib-profile-receipt.json)
at source `a9f8b7de4` with temporary instrumentation: **1,153
byte-bound drains, 118 lane-switch, 75 wave-end and one row-bound**, plus
**1,089 pack appends**. It is not a matched count on the current 64-MiB-wave
source. Its 300,630,888 queued encoded body bytes require at least
`ceil(300,630,888 / 262,144) = 1,147` pack-sized portions, so its 1,153
byte-bound drains are already close to that byte floor. Eliminating those
*drains* is neither possible at the same queue byte bound nor the right
mechanism. The question is why a drain often leaves an open pack that must
later receive an **append**.

Current `LanePlacement::select_many` makes one BLOB increment per receiving
pack per call. The queue flushes on aggregate encoded bytes, without asking
whether its next group fits the **exact receiving pack** after the reserved
directory, header, group count and any earlier groups. A flush can therefore
write a partial pack, and a subsequent flush appends to it. The source's
`append_fits` predicate already knows the exact physical fit; this proposal
uses that predicate to decide *when* to materialize a pending pack.

## Candidate shape, if selected later

Keep at most one pending pack worth of encoded groups per eligible lane:
ordinary, native and whole-file. Total retained encoded group bodies stay
within **3 × 256 KiB**, plus **3 × 512** bounded member records and the
existing open unframed group per lane. A lane switch does not itself flush.
For the next group of lane `L`:

1. Project the lane's current open pack plus its queued groups through
   `pack::layout::append_fits`, using the same assembled-byte and group-count
   rules as `LanePlacement`. If the group fits, retain it in the bounded queue.
2. If it does not fit, place and write that lane's pending groups **once** as
   the completed receiving pack, then start the next pending pack. A pack
   already materialized at a previous wave boundary can require one append
   when its first new groups arrive; this is a declared cross-wave limit.
3. Before any exact reuse, same-save read, advisory or winner-cache base read
   that demands a pending identity, materialize its pending pack and locator
   rows. Direct logical references may still use the bounded pending-identity
   fact. A same-save read never sees a locator before its BLOB bytes.
4. Drain every lane before the wave's collision validation, content-signature
   flush and COMMIT. Each wave still owns Store arbitration for its whole
   bounded transaction and commits before unlock. Definite failure rolls back
   placed packs and locators together; unknown COMMIT remains unknown.

This is a **physical write schedule** change. It retains schema 10, the pack
framing, canonical bytes/IDs, 128-KiB file cutoff and 4-KiB SQLite pages.
Pack IDs and dense Store space may change because lane drains can reorder
physical allocation. Exact root/object-ID equality, full reopened manifest
readback, Store pack capacity/use, #229 sparse-history space and multiwriter
checks would be required before adoption.

The tempting alternative is to retain all encoded groups of a 64-MiB
preparation wave and call `select_many` once per lane at the end. It would
hold roughly a wave of encoded bytes **in addition to** the wave's canonical
objects; `select_many` then builds selected body vectors and locator rows
while both inputs remain live. Its actual RSS bound is not established and it
could worsen the already observed **247-MB sampled Service RSS**. The
one-pending-pack-per-lane form uses existing fit logic and a sub-megabyte
encoded queue; choose a larger buffer only if a count diagnostic proves this
form still issues substantial avoidable appends.

## Evidence needed before coding or timing

- Obtain one **count-driven, clearly labelled diagnostic** on the current
  64-MiB seven-COMMIT source for exact file-Save pack appends, queue flush
  causes, pack-write calls/time and largest queued bytes. The adjacent 4-MiB
  profile cannot answer that baseline question.
- Trace every read path that can demand a pending ID (`seal_pending`, exact
  reuse, whole-file predecessor/winner selection, same-save `read_batch` and
  final collision validation) and prove the materialization barrier covers it.
  The immediate SQLite foreign key from `objects.pack_id` to `object_packs`
  means locators must still be inserted **after** their pack row exists.
- Prospectively declare a distinct candidate source identity and one matched
  public 10k control/candidate pair only if the diagnostic shows a material
  avoidable-append population. Each arm needs an independent byte copy,
  zero-resident source payload checks, fixed stack/scope, fresh Store, no warm
  cache from prior runs, one performance-only public sample and a separate
  full reopened readback. Metadata cache remains unqualified, so such rows
  stay exploratory. Keep failed attempts and report RSS/lock/space regressions.

No exact percentage of appends is projected here. The 4-MiB adjacent counts
identify a likely scheduling issue; they do not prove an opportunity at the
current 64-MiB source or a throughput gain toward the historical 518.8 MB/s.

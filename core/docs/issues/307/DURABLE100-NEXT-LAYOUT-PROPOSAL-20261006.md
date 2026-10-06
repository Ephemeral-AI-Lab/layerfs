# Durable100: proposed durable payload layout

> **Status:** Reviewable proposal following current VFS/placement investigation.
> Not implemented, selected, benchmarked or a replacement for retained v2 FAIL.

The current trace shows9,410,416 B submitted by five Storage publications for a
5-MB namespace;4,161,536 main-file B are checkpointed inside one publication's
COMMIT. Smaller acknowledgement count does not remove the large immutable body
path through WAL and main. A new physical payload layout is the next substantial
engineering candidate. Speed improvement and parity are unproved.

## Candidate behavior

Keep SQLite as the sole authority for policy, pack/catalogue metadata, object
locators, acquisition and history. Keep the existing canonical objects and packed
body grammars byte-compatible. For the new explicitly selected layout, place
sealed payload bodies into immutable filesystem segments rather than SQLite
payload BLOB pages. Keep small pooled/namespace metadata in SQLite initially, to
avoid a separate durable file for every tiny metadata publication.

One bounded publication writes at most its existing charged payload/row window.
An existing permitted singleton retains its own bound; total file/namespace size
is streamed without a new total cap. Segment preparation is one producer and
uses already sealed immutable carriers, not another construction lane. Include
segment/header/index overhead in physical charges and final allocation.

The publication protocol must be:

1. Reserve globally unique physical IDs through the existing acknowledged owner.
   Never recycle consumed IDs or guess a new range after an uncertain reply.
2. Exclusively create a Store-owned segment under checked path/device/inode/link
   custody, outside the acquired source. Stream final body bytes and retain exact
   offset/length/key records in bounded memory.
3. Make immutable bodies and their directory entries durable under the selected
   macOS full-flush guarantee before SQLite can publish locators naming them.
   No workspace-disposable sync rule is repurposed; these are global Store bodies.
   Required file/directory synchronization must be proved on the supported platform.
4. Atomically publish segment locations, pack metadata and object/value/signature
   rows through the same bounded SQLite acknowledgement. Preserve first-wins
   conflicts, reference closure and exact typed refusal/uncertainty.
5. Keep acknowledged bodies on lost replies. Retain precise unacknowledged or
   uncertain physical custody; never delete by timeout, guessed rollback, retry or
   process exit. Recovery/reclamation requires authoritative reference checks and
   explicit fencing. A file without a catalogue row is not sufficient proof that
   its producer is finished or its outcome known.

Readers must validate metadata, segment bounds, existing packed headers and keys,
and perform the existing scoped immutable acquisitions with bounded windows.
No file-sized mmap/backing cache is presented as a bounded heap. Old layouts remain
readable through their exact version-specific implementations. New stores select
the new version explicitly; there is no implicit migration, fallback or second
acquisition algorithm. Public content/Storage/history APIs stay compatible.

## Scope kept fixed initially

Retain A1 host-global acquisition and its current Durable guarantees for the first
payload-layout experiment. Its eight acknowledged writes remain real paid work.
Do not combine two new architectures in one speed treatment. The separately
completed placement review describes a possible later operational owner, including
crash loss, never-reissued epochs/incarnations, authenticated fencing and cleanup.
That needs a separate explicit A1 supersession, not an unsynchronized table toggle.

Do not tune the1000-page automatic checkpoint threshold, WAL/FULL/fullfsync,
construction workers, input workload, cold rules, transaction caps or1.10x gate
to make the result pass. Do not exclude segment creation/sync/close or fresh Store
bootstrap from the whole product clock.

## Qualification and owner decision

This changes physical layout/schema/backing, so the frozen Monolithic schema4
acquisition-v2 case cannot silently measure it. Preserve every v2 FAIL and register
one new candidate identity with the same100-file seed1 input, original reference,
durability guarantee, complete30s/build30s/proof19s bounds, cold-content attestation,
four constructors,1.10x speed gate and final-allocation gate. Count main/WAL and
segment traffic/syncs separately; current VFS counters do not see direct segment
I/O. No device-byte or phase-memory claim follows from submitted bytes/lifetime RSS.

Before measurement, implement exact version selection/creation/reopen/old-format
compatibility, bounded publisher/readers, physical ownership and error custody,
then verify canonical namespace bytes, concurrent conflict outcomes, malformed/
missing/tampered segment refusal, definite and uncertain failure, read-only
authority and capacity behavior. Use external public tests and explicit bounded
invocations. No third-party edits/new dependencies or aggregate CI gate applies.

The requested owner decision is whether to select this new physical layout and
candidate identity while retaining A1, or require all subsequent work to stay on
the existing Monolithic schema4 vehicle. Approval would select an engineering
experiment with unchanged durability/workload/cache/gates; it would not declare
success or authorize release/push/deployment. No code for this layout is included
in the current checkpoint.

Owner supersession: the payload format is withdrawn; active behavior and the new
full same-profile regression screen are described in [Monolithic restoration](MONOLITHIC-RESTORATION-20261006.md).
Historical proposal/measurements keep their original identity and verdict.

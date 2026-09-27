# Issue 265: balanced packing of a touched extent leaf

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Prospective algorithm, resource and evidence treatment, committed before the
> product correction or any corrected-source sample.

The one-attempt public rows at source `4f644e950` retain `INCOMPLETE` receipt
status because their first diagnostic parser refused `spool_resident=true`.
Their raw logs and independent verifiers are retained; the postprocessor
derives counts without rewriting a receipt. All three rows proved 100 mounted
WRITE callbacks, public Exec/Commit, full old/new bytes and clean custody.
Raw latency remains cache-`INELIGIBLE` and supplies no speed comparison.

The dispersed raw work has 131 extent leaf writes, 39 branch writes, 699 child
and 3,859 custody edge additions, plus 636 child and 3,697 custody edge
decrements through write 100. The aggregate live metadata-page count is 37.
An in-memory run of the same 100 offsets through the production page codec
shows 32 reachable extent leaves and one branch at write 100, with leaf
occupancies mostly 2–4 records (range 1–124) for 201 final extents. This is
the missing page-kind evidence; it is a source-equivalent structural diagnostic,
not another public performance sample. The old 50-write root remains readable.

## Selected correction

The current fold writes a leaf immediately when its 124-record buffer fills.
Inserting two records into a full leaf therefore leaves a full leaf and a new
1–2-record leaf. Keep at most **248 touched records** in the existing fold
buffer, then partition those records evenly into `ceil(n/124)` leaves when a
fold boundary closes. Thus 126 records become 63/63 rather than 124/2. A
stream that reaches 248 flushes at that bound and continues; a broad edit may
still leave a sparse final tail, but it never holds an unbounded sequence.
This changes private page packing only: same 32-byte extent records, child
format, path-local reads, immutable old roots, atomic publication, exact
quota/ledger identity/checksum/quarantine, G1/G2 custody and one worker. No
untouched sibling is read for balancing, and no historical write is replayed.

The touched-record scratch ceiling rises from 124 × 32 = 3,968 bytes to
248 × 32 = **7,936 bytes**, within the existing 640 KiB metadata writer
allowance. Page emission still uses the existing `RootOwner::write_raw_page`
and temporary-root seal: each new leaf gains exact Local custody edges; any
superseded temporary page is released by the existing seal. No persistent
index, new format, dependency, timeout, cache policy or worker is introduced.

## Frozen decision and proof

Run the same append/dispersed/repeated public cohort once at one frozen
corrected release source with its own local validated master copies and fresh
receipts. Keep the 15 s complete command, 9 s verifier, 25/50/75/100
checkpoints, uncontrolled cache and `INELIGIBLE` latency classification.
The old rows stay `INCOMPLETE` and are not resampled. Compare page/edge/ledger
counts as diagnostics, not raw time. The correction is worth retaining only if
the dispersed reachable leaf count and aggregate live metadata pages both
fall, extra leaf writes and branch-child edge updates fall, and ownership
ledger reads/writes do not exceed the retained 3,328/1,929 counts. Append and
repeated must not acquire extra pages or ledger I/O. A count tradeoff that
fails these conditions is reported and the correction is reverted without
dropping any row.

The focused external page-codec proof checks 25/50/75/100 occupancies, exact
201 extents, pinned old-root bytes, page-level uniformity and the 248/249
buffer boundary in under 30 s. The three independent public verifiers check
full old/new heads, parents, exact changed runs (1/100/1), resource/quota and
clean G1/G2 custody. Core boundary, focused tests, Clippy and formatting
gaps are reported at final source. A #248 4,097 public gate remains separate
and is attempted once only if #266 resolves its FUSE risk and the frozen
source is otherwise reviewable.

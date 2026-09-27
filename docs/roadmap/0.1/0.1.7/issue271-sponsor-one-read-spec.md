# Issue 271: one-read sponsor authentication correction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Committed before the source correction or new sample. Starting source
`78c4a2fd350d7aa7010ef689155f30685664acdb` retains the first
sponsorship 100/512 count misses. Its authenticated old-page load reads the
same sponsor owner twice per attempted sponsor: once for eligibility and once
inside `load_raw`. Both reads validate the same 4 KiB ledger record under the
same writer gate. The existing `load_raw_with_owner` reads the authenticated
page body using the already checked owner without another direct ledger read;
use it here. Keep the same owner reference, identity, checksum, sponsor-chain
bound, edge difference, publication order, failure/quarantine and cleanup
rules. This changes the redundant I/O path, not the ownership graph.

Take one new-source 100 and one 512 public separated-write row, using the
same release SDK/verifier, prepared master, writable byte-copy clones,
one-process/one-fd writer, one Mount → Exec → Commit, one construction worker,
cache policy, 15 s command bound and separate 9 s verifier as before. Capture
exact FUSE callbacks, old/new-head oracle, charged child/Local/sponsor edges,
4 KiB ledger reads/writes, within-run progress, resources and clean close.
Both reads and writes must fall below the unchanged combined baseline of
2,905/1,511 at 100 and 20,953/12,150 at 512; an uncontrolled-cache raw wall
remains INELIGIBLE. Keep every receipt, including a miss. If both count rows,
native G1/G2 and failed-candidate custody checks and the 100-write pattern
siblings pass, attempt the 4,097 gate once at the final source under its
unchanged 25 s complete-command exception and independent verifier. A gate
timeout or incomplete evidence remains FAIL, never a retry or inferred PASS.

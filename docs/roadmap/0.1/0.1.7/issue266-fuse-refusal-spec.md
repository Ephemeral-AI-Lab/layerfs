# Issue 266: mounted WRITE refusal diagnosis and treatment

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This count and evidence selection is frozen before changing the diagnostic runner
or taking a changed-source sample. The original
[`issue261-separated-512-v1` FAIL](../../../../core/docs/issues/261/evidence/512-v1-fail/receipt.json)
remains append-only: callback 258 returned `EBUSY` after 257 accepted writes,
without a Commit. Its post-reply optional snapshot and later shutdown `Busy`
do not identify the WRITE refusal site.

## Diagnostic selection

`issue266-fuse-512-v1` keeps the existing 8,194-byte old head, one shell-launched
generic writer, one fd, and 512 one-byte positional writes at offsets
`0,2,...,1022`. It uses one public `WorkspaceApi::mount`, one
`WorkspaceApi::exec`, one explicit `WorkspaceApi::commit` only if Exec succeeds,
and the independent old/new-head oracle. The writer, fixture, deadlines,
construction worker count, and cache policy are unchanged. A fresh writable
byte copy of the validated prepared master supplies each run. Progress remains
at 128, 256, 384, and 512 accepted writes; FUSE's actual WRITE callback count
is separately captured. Optional backing snapshots run before their WRITE
replies and remain diagnostic overhead.

Audit every Busy exit reachable from WRITE before changing admission semantics.
On failure, emit a bounded, ordinary daemon diagnostic naming the callback
stage or exact coherence refusal condition, the projection status, mutation
and reply permit state, revision, and the remaining callback deadline. Record
whether an earlier reply was attempted and whether notification completed when
that state is observable. Do not emit a record per successful write or make a
per-write SDK, control, or Service call. The diagnosis must reconcile the
recorded refusal with source; a possible reply race alone is not proof.

Take exactly one labelled 512-write diagnostic per frozen source identity.
Retain all FAIL and INELIGIBLE outputs. If the proven cause is a healthy prior
permit, a later treatment may wait for its release only inside the existing
10-second callback deadline. Failed coherence must still refuse immediately.
If another branch is proven, fix that branch instead. No unconditional retry,
new worker, benchmark-only mutation path, or changed time limit is permitted.

## Treatment and gate evidence

At a fixed treatment source, run one 512-write public case and independent
oracle. Check the unchanged old head, exact new head and parent, all file bytes,
exactly 512 separated changed runs, 512 replacement bytes, and 1,024 final
extents. Cover one-fd sequential writes, overlapping FUSE workers, known
notification failure/unknown reply outcome, and clean close after Commit with
focused commands under 30 seconds. Preserve atomic per-write publication,
read-your-writes, and G1/G2 custody. Retain daemon close/retained status, not
just successful Docker deletion.

Once the refusal is localized and a frozen source makes it meaningful, attempt
the already declared #248 4,097-write public Exec/Commit gate once for that
source identity. Its independent oracle checks exact old/new heads, all bytes,
4,097 changed runs and 8,194 extents. Record actual callback count, Exec,
Commit and complete command wall, private payload/ownership and page-index
counts, Store work, resource charges and cleanup outcome. The gate's complete
command exception remains 25 seconds; the #249 30-second product Exec timer
remains separate. No row with uncontrolled cache becomes a latency PASS.

Every row pins source/tree, product and harness seals, image and release binary
hashes, workload hash, prepared-master and clone identities, cache contract,
interference, and all non-passing outcomes. No arm is resampled to replace a
number. Keep #265 payload/index changes and its writer and C1 source outside
this issue's changes.

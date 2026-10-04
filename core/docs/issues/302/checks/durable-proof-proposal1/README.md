# Reviewable proposal: closed zero-frame WAL proof, prospective v2

Status: **inactive prototype, not admission**. The complete Disposable 10 → 3
→ 1 ladder is qualified at its recorded product/harness identities. Durable
stride 10 performance completed, but the existing verifier refused retained
sidecars before the independent namespace proof. Durable stride 3 / 1 are
NOT_RUN pending this review. Original failed receipts and prototype attempts
are immutable and remain non-passing where recorded.

## Cause and current boundary

The active `shared/phase7_history_proof.py::owner` rejects any `-wal`, `-shm` or
`-journal` file: `history proof refuses outstanding database sidecars`.
The local stock system SQLite reports `SQLITE_FCNTL_PERSIST_WAL=1` and retains
an empty WAL plus 32,768-byte SHM after a successful checkpoint/close. Four tiny
public-API diagnostics show identical behavior with/without the observer and
with/without an extra raw DB descriptor. This is a verifier/profile mismatch,
not evidence of an observer mutation. See the retained
[stock cause](../durable-sidecar-failure10/stock-sqlite-cause.json).

No product dependency, unsafe policy or persistence profile is changed here.
The current persistence crate forbids unsafe code; rusqlite 0.32.1 exposes the
raw handle as unsafe and has no safe file-control setter in its public source.
No dependency/registry patch or weakening of that source boundary is proposed.

## Exact proposed treatment

The inactive [prototype](proof_prototype.py) retains the exclusive regular
single-link main-file check, schema/application/version checks, quick/foreign
key integrity, canonical inventory, C5 roots/counts, independent reference pins,
all-state structure and the existing selected-content/authentication bounds.
It permits a sidecar pair only when the main header declares WAL, WAL length is
exactly zero and SHM length is exactly 32,768 bytes. Both sidecars must be regular,
single-link, nonsymlink files. Every journal, nonempty WAL, incomplete pair,
invalid SHM size, alias or changed original is refused. The main and both
auxiliary hashes/identities are recorded before/after. SHM is not treated as
application data; an empty WAL contains no pending data frames.

The original main file is censused with `mode=ro&immutable=1`. The normal public
native verifier runs on an independent read/write byte copy of main/WAL/SHM,
never on the original. Copy hashes must match; copy buffer is 65,536 bytes, total copy bytes cannot
exceed the already registered Store ceiling. macOS F_NOCACHE is required on
copy descriptors and copy checksum reads; it is a hint, not a memory claim. A
sealed native helper invalidates/checks all copied input pages before native
verification and fails closed if any remain resident.
Copy bytes, method and time are charged inside the existing separate proof
command and recorded separately from native selected-content acquisition bytes.
The concrete prototype enforces the existing Store copy-size ceiling and its
whole-copy residency contract. No total heap/RSS bound follows from a bounded
heap buffer. Original allocation includes
main/WAL/SHM exactly as the performance receipt does. No original file is deleted,
checkpointed, rewritten or relabeled after measurement.

## Concrete validation and remaining integration

On the retained Durable 17-state output, the prototype performs the full
independent namespace/custody/root proof in **3,318,778,667 ns** under the existing
12,000,000,000 ns bound: 101,477 paths / 17 states / 67 selected content paths,
970,326 authenticated bytes / 3,301,773 acquired content bytes. All original
hashes remain unchanged. This diagnostic cannot promote the original failed
receipt or serve as release admission. Its main/WAL/SHM byte copy is 50,724,864
bytes, separate from the native bounded content-acquisition counter.

Six rejection/custody/copy tests and the five existing proof-helper tests against
the prototype pass. [Custody](custody.json) retains all failed prototype attempts:
first an incorrect main-header byte offset, then a copy missing WAL read-side
auxiliaries, followed by the complete byte-copy treatment. A subsequent bounded
copy found that normal checksum reads warmed all destination pages; checksum
reads now use F_NOCACHE and attestation records zero initial/final residency
across 3,096 copied pages before native verification.

Approval is requested because this extends the previously reviewed closed-owner
admission treatment from no sidecar files to an independently proven zero-frame
pair and introduces a verifier copy. After approval: integrate this narrowly
scoped treatment, retain the tested copy bound/residency contract, register prospective
Durable v2 case IDs retaining historical v1 verdicts, freeze the new harness,
and take fresh matched Durable 10 → 3 → 1 pairs. Keep 60/170/300 s performance,
12/12/30 s proof and 54,278,964 / 70,427,034 / 92,342,273 byte ceilings unchanged.
No prior pair is rewritten or promoted; a harness change requires new matched
arms. The existing Disposable receipts retain their original qualified identities.

## Follow-up audit — growing source, checkpoint11

The inactive copy now counts every chunk and refuses excess bytes before writing
when the source grows after its initial stat. No output can exceed the registered
Store ceiling. The new source-growth rejection test and the changed positive
exact-copy test pass; unchanged rejection tests are carried from their previous
run. The complete diagnostic is repeated only for this changed copy algorithm,
with a fresh output and source SHA; all previous results remain immutable.
Active product, verifier and case registry are unchanged pending approval.

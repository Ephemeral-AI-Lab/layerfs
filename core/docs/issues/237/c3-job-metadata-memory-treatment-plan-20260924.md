# #237: prospective compact native-import Job metadata treatment

> **Status: Research; informative and not a product contract.** Freeze
> before candidate source or measurement. This is one memory treatment,
> not a new speed arm or a change to the registered #236 debug selection.

## Cause and treatment

The retained C3 release phase diagnostic on the seed-1 SHAKE 100k/500-MB
manifest reports `Job` vector reservation **23,068,672 B** for
100,000 jobs while all 101,001 `PreparedEntry` values are also live.
`Job` clones the full `std::fs::Metadata`, but its only post-scan reads
are `dev`, `ino`, `len`, `mtime` and `mtime_nsec`. Replace that clone
with those five captured values; retain the same `PathBuf`, entry index,
open-time device/inode check, and post-read length/mtime checks. This
does not alter the public SDK, source traversal, C1/C2/C5 format,
SQLite policy, four-worker count, or the Save/handoff order.

The single candidate's release diagnostic reuses the *same* temporary
phase-marker logic, benchmark driver, runner, manifest and 0-resident
payload-page contract as the retained C3 phase diagnostic, with a fresh
independent byte copy and fresh Store/History. The control receipt is
`evidence/c3-phase-memory-20260924/`; do not rerun it. Pin product,
instrument, harness, binary, fixture and cache identities, then run
one candidate. The instrumented binary's call time remains diagnostic;
no raw time is promoted to a speed comparison.

## Decision and proof

- Structural check: `size_of::<Job>()` and `jobs_reserved` must fall;
  100,000 jobs must still finish, read 500,000,000 bytes and yield the
  same 112,424 stored object count. The control `jobs_reserved` was
  23,068,672 B.
- Outcome check: record exact scan-end and public-call high-water RSS,
  current RSS and namespace-end high-water. An ≥8-MiB scan-end RSS
  reduction is a meaningful local result; less is recorded plainly.
  The final public-call high-water is separately reported even if
  namespace construction remains the maximum. One observation is not
  a distribution or a release memory guarantee.
- Correctness: a separate release verifier must reopen Store/History
  and check all 101,001 paths, portable metadata, every file size and
  SHA-256, and 500,000,000 bytes. Source copy, 0/126,206 resident
  payload pages at launch, cleanup and evidence hash manifest must
  pass. Failures, over-budget work and unmet criteria remain visible.
- Verification: run the owning Core locked tests, warning-denying
  Clippy, examples, formatting, boundary guard and its self-tests once
  at final product identity. Update the C5 architecture description in
  the same commit. Production LOC is counted per repository policy.

If the treatment changes only vector reservation and not process
high-water, do not claim the 59.47-MiB old-to-C3 gap is fixed. The
separate namespace peak and #229 sparse-history gate remain open.

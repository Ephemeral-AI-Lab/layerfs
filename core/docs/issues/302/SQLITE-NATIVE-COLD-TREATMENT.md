# Native cold-contract treatment

Status: Cold qualification passes; competitive Init outcome unmeasured.
Based on947c0ef49. Full100000-file Python count diagnostic took11970979333ns
on already-nonresident contents; residency200000/path-stat200000 and198000
mmap/mincore/munmap sequences. No product speed claim. The repeated traversal,
Python/ctypes/object work and page-size queries are avoidable mechanism overhead.

A first-party C harness helper performs the same mincore->conditional msync
invalidation->immediate mincore, followed by a separate whole-input mincore pass.
It obtains host page size once, walks deterministically without following links,
keeps one payload descriptor/mapping at a time and reuses only the residency byte vector.
It never reads/touches payload pages. Inode/device/size and final count/byte/page
checks refuse inventory changes; input is the closed identity-pinned fixture.
Vector and helper child RSS are counted separately from product memory. No claim
about cold filesystem metadata or full concurrent-mutation safety is introduced.

Native build is outside the performance envelope, sealed by source/flags/clang
identity/platform/architecture and binary SHA; immutable archive is reused through
its seal. Both arms run identical helper binary/cache rules. Its source is part
of harness identity. All invalidation and attestation remains inside the complete
performance envelope. The product gets only the remaining15s deadline. No silent
fallback or enlarged deadline. Independent verifier remains separate<=9.5s.

Actual tests: warm invalidation matches existing primitive, already-nonresident
pass, empty/non-directory/symlink/FIFO refusal and source/binary seal refusal.
Three native testsPASS; four registry/arithmetic/wait4 testsPASS.
C builds with Wall/Wextra/Werror. No product source changes; prior product owning
checks are retained, not repeated. Core fmt/boundary/23 self-tests checked.

Prospective next: one count-only full100000 fixture diagnostic, no product sample,
<=15s; then one new matched100000-file Init-v2 pair if the measured mechanism
leaves budget for the product. Do not substitute this diagnostic for speed proof,
credit its cache state to a later sample, or rerun an unchanged failed arm. Other
cases/history remain required; older receipts retain original identities/status.


## Full100000-file count diagnostic atf5abcee18

Count-only, zero product samples.100000files/500000000B/126206pages, initial/
final residents0, invalidations0. Native firstpass3492563000ns, finalattestation
3177878000ns; complete diagnostic6675567875ns under15s. Same198000mmap/mincore/
munmap checks;200000opens/fstats,204004inventory entries; vector peak6104B.
Cold helper child CPU6184813000ns, lifetimeRSS1916928B; this is separate from
product-child memory. Generic invocation scope text is corrected in the next
harness identity to describe the recorded command, not assume every child is
Init (build/verifier/helper are separate invocations).

The older Python diagnostic11970979333ns includes observer wrappers; this new
count-driven diagnostic does not establish a controlled numeric product speed
delta. No source pages are primed, and the next speed pair repeats the complete
invalidation/attestation contract inside its own envelope. Both matched arms use
the same new helper/harness identity; no historical arm is re-labelled.
Raw issue302-nativecold100000-counts-treatment1. Largest-tier matched pair is
now prospectively selected, no buffer/worker/deadline/profile changes.


## First largest-tier pair at2e6c0edd0: timeout retained

Reference product5421641709ns; complete11845503875ns, final allocation518029312B,
proof1657169250nsPASS, cold6410521833ns, residents0 before andafter. Candidate
cold12993975333ns:109495resident pages,82296invalidated files,362592mappings,
364592opens/fstats. Extra164592mappings result from reopening/remapping around
invalidation's immediate check. Product only receives2005504333ns remaining;
watchdog kills it (-9), product comparison unavailable, proofNOT_RUN, statusFAIL,
command15011762583ns exceeds15s. Allocation189906944B is partial, not an
admitted storage outcome. This arm is never retried or reported as competitive.
Raw issue302-nativecold100000-{baseline,candidate}-treatment1.

## Qualified warm-state mechanism change

Reuse the initially unfaulted read-only shared mapping for conditional invalidation
and its immediate mincore check; retain the writable capability/size check before
invalidation and the separate final traversal. This reduces mapping/open/stat
multiplicity, never drops attestation. Descriptor fstat supplies actual metadata;
FTS_NOSTAT removes redundant traversal stats. A bounded streaming pathname/device/
inode/size FNV consistency fingerprint compares the two traversals, alongside
file/byte/page counts. It is not a cryptographic content/tamper proof; the closed
identity-pinned fixture and independent product root/proof remain required.
Nonblocking/nofollow open plus descriptor type checks refuse nonregular entries.

Native qualification4PASS (warm mapping reuse, cold equivalence, read-only cold
acceptance/warm writable refusal, symlink/FIFO/empty/root and seal refusal).
Wall/Wextra/Werror compilationPASS. Installed macOS msync(2)/fts(3) documentation
checked; no statp dereference in FTS_NOSTAT paths. Intermediate test/code revisions
are retained under target/phase7-agent, not speed receipts. No product edits or
unchanged speed-arm reruns. Prospectively select one fresh largest-tier pair at
this changed mechanism/harness identity, same15s/9.5s/profile/workload/worker/cache
and allocation gates. Other required rows/history remain unqualified.


## Fused mapping pair at2aaf6c7d1: second timeout retained

Reference product5376246166ns; envelope11622127959ns; final allocation518029312B;
proof1623258833nsPASS; cold6233551000ns; initially/finally nonresident.
Candidate initially122558 resident pages,95358invalidated files, final0.
Firstpass5666985000ns and finalattestation3231373000ns; mappings198000 (removes
extra maps), mincore293358,opens/fstats295358 (capability checks retained).
Complete cold child8904244459ns. Product receives
6095223542ns, times out(-9); comparison unavailable,
proofNOT_RUN, complete15015172917ns exceeds15s. Ordering scratch cleanupPASS.
Partial allocation476270592B is not storage admission. GateINCOMPLETE;
no numeric relative speed, root equivalence or final-memory verdict is possible.

Raw issue302-fusedcold100000-{baseline,candidate}-treatment1; comparison JSON
issue302-fusedcold100000-comparison-treatment1. Reproduction runner.py run --case
phase7-sqlite-init-100000-v2 --arm baseline --baseline-root
target/phase7-baseline/layerfs --out <fresh-owned-output>, then --arm candidate,
reading benchmark_agent_report.md before each invocation. No resample of this
identity. Both previous and this timeout remain intact. Other three Init rows and
three history rows are NOT_RUN at this harness identity, not dropped or PASS.

The warm-state mechanism removes redundant mappings and reduces work even with
more resident files than the previous window. Different windows are not a
controlled timing delta. This still does not leave enough envelope for current
product work. Keep15s/9.5s/profile/cache/worker/byte bounds fixed; next product
focus is durable publication composition and redundant full-body validation,
with immutable proof/atomicity preserved. History driver/proof/budget binding
remains required. All-seven goal active. No further unchanged-arm retry or deadline
extension. No product source edits this round; every commit LOC137505 unchanged,
reference65417/core72088,active28044/inactive44044; exact snapshot methods in
f5abcee18/2e6c0edd0/2aaf6c7d1 commit bodies.

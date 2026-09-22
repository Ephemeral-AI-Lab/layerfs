# Shared symlink-content construction

> **Status: implemented and functionally verified for the declared content-object scope.**
> Implementation parent: `5ed91aaca38dc54e145753a6844b12e6baf30abd`.
> Exact implementation commit: `521bcb304c382e53f454eb3eb9007a010c1de486`; [confirmation](evidence/construct-symlink/commit-confirmed.json).
> Product input seal: `ab247a3da4597e0429239b2b6b8a912d4455e83d380172300fabe947c913a80a`.

This round adds one shared no-base symlink-content save. It reuses C1's existing
SymlinkTarget/emit_symlink builder and the normal Service save owner, finish and
abort handling. It allocates no inode, attaches no name, and publishes no Stage or
Commit. Portable metadata remains the existing kind3/mode0777 constructor.

## Selected contract

ConstructSymlink carries0..4096 opaque target bytes without NUL, preserving exact
bytes rather than resolving paths, validating UTF-8 or normalizing separators.
Empty is valid for the C1 object primitive. The stricter nonempty history-manifest
rule remains unchanged; mounted syscall semantics are a later operation.

Opcode16 uses profile1. Its request metadata is29+L bytes (maximum4125), with no
input body. It reuses Saved tag2 (57 bytes), whose length equals the target byte
count, and permits no ResultData. Authenticated request-ID and explicit operation/
result validation provide correlation; no target echo or new response type is
introduced. A lost or invalid mutation terminal remains Unknown without replay.

Store grant4 explicitly becomes content construction for file or symlink content.
Existing holders, including legacy mask31, gain that symlink-object construction
capability. It does not grant metadata construction128, history64, namespace
attachment or daemon controls. The mapping remains explicit with no unchecked
opcode shift and no grant-width change.

## Actual proof and qualifications

Two authenticated host-native selectors passed on their first attempts:

| Selection | Status | Driver seconds | Complete command seconds |
| --- | --- | ---: | ---: |
| construct | PASS | 2.692872750 | 2.835509291 |
| refusals | PASS | 0.267746417 | 0.350547083 |

The successful selector constructs relative, absolute, non-UTF8, empty and exactly
4096-byte targets. Each first save inserts1 object/reuses0; its repeat returns the
same root with inserted0/reused1. Five explicit prepared candidate saves replace
only an existing fixture symlink inode's content, enabling exact InspectReadlink
readback without a fresh declaration. The old root, Branch and history remain
unchanged. No workload Commit or Reserve is attempted; the fixture Commit is
recorded as setup. Constructor direct tests also use no HistoryCatalog, so an
empty attempted-request list is not the only no-history boundary evidence.

The native refusal selector distinguishes NUL/4097 local NOT_SUBMITTED validation
from actual authenticated grant failures. Masks128 and64 cannot construct a
symlink; grant4 cannot construct portable metadata or query history. Masks4/31
are exercised successfully. No failing refusal mutates content or history. Both
commands reuse independent byte copies of the closed master, create fresh live C5
authority, close all clients/Service normally and verify the master is unchanged.
They have a60-second complete budget and one construction worker. They run on the
host, so Docker CPU/RSS isolation is not inferred.

Three Bridge Rust tests check exact wire sizes and framing, target/body/allowance
bounds and eight real authenticated peer schedules. A wrong result, wrong length,
ResultData, wrong request ID, malformed/error/lost terminal yields Unknown as
applicable; the peer observes exactly one BEGIN/EndInput and no replay frame or
replacement connection. Three direct Service tests check actual C1 canonical
bytes/root identity, repeat counts, grants, unexpected input, deadlines and
occupied save ownership. The occupied-owner test aborts only its explicitly held
owner. Late constructor-save SQL failure and native daemon terminal corruption/
loss are not claimed from those narrower routes.

The [functional index](evidence/construct-symlink/functional-index.json.gz),
[caller review](evidence/construct-symlink/caller-review.json.gz) and
[archive manifest](evidence/construct-symlink/archive-manifest.json) retain exact
commands, identities, attempted request logs, ownership journals and all output.
No passing selection was rerun and no fixture was regenerated. These are
functional results, not performance, cold-cache or release qualification.

Locked/offline Rust1.85.1 checks passed: host697 tests/3 ignored, Linux695 tests/149
ignored, both all-target Clippy commands with warnings denied, fmt, host binaries/
examples, both platform product checks, the254-file boundary guard and six guard
self-tests. Docker checks used the pinned tool image, two CPUs and Cargo jobs2;
all builds/checks/samples were serial. The [check index](evidence/construct-symlink/checks-index.json.gz)
links commands and logs. CI and retired aggregate preflight did not run.

## Resources and source size

The request owns at most4096 target bytes; its encoder is bounded by4125 bytes.
The existing C1 constructor clones the target and uses its existing value encoder
(up to4110 bytes) and canonical object (up to4123 bytes). These are individual
logical-buffer bounds, not an asserted sum of peak heap or allocator capacity.
There is no response target copy, new response type, worker, queue or persistent
state. [Linux compiler layout](evidence/construct-symlink/layout/layout-01.json.gz)
confirms Operation328, Request368 and Response112 bytes with alignment8 before
and after. Fixed type sizes are not RSS/cgroup or stack-peak measurements.

Production LOC: **112470 ->112510 (delta +40)**; core47053->47093 (+40),
reference65417->65417 (+0). Count exact first-parent/final-staged product snapshots
with unchanged tools/production_loc.py, blob b5b9617d08204977176302311e0b2c72a811b420,
using identical production Rust/runtime SQL scope and exclusions.
[Per-file/folder/crate inventory](evidence/construct-symlink/source-loc.json)
retains the complete254-core/193-reference-file scope. No legacy retirement or
canonical builder duplication is credited.

The prepared DSH tree is unchanged. Fresh symlink attachment, full-input admission
and upload followed by one explicit full Commit, subsequent incremental Commits,
hard memory qualification and matched R6 remain open, together with the Round43
native capacity failure.

Additional retained client/fixture output is indexed in the
[archive supplement](evidence/construct-symlink/archive-supplement-01.json).
Original receipts and result classifications are unchanged.

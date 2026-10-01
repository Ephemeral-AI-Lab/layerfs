# V4c2a authenticated immutable file-graph certification

> Status: Prospective research dependency; no speed/resource/release admission.

Parent `57e3f88d1fe0bae101280aeb23375b1f35d1dc89`. V4c1 has actual indexed
construction but publication still walks every live inode and rereads every file.
Split V4c2 into this file-graph dependency and V4c2b namespace transitions/import/
retirement. This gate removes payload-size-proportional repeated validation;
it does not remove the population-proportional namespace walk.

## Exact authority and representation

Use the existing lifetime global locator SQLite connection and2MiB cache, MEMORY
journal/syncOFF/tempMEMORY. Add paged immutable facts and immediate child constraints
only for registered WholeFile, Chunk, ExtentLeaf, ExtentBranch and FileState roles.
Derive facts from the exact canonical bytes authenticated by actual MinIO/C2
registration, using existing public C1 codecs; never trust transmitted summaries,
ObjectRole declarations alone, candidate output or authentication as namespace
certification. Registration and facts are one bounded SQLite transaction; preserve
same-page and existing exact CAS collision checks and normalized physical locators.
No new SQL engine per Commit, payload persistence in SQL or resident whole graph.

Facts record actual role, logical bytes, extent count, level, actual non-root fill
validity and graph-certified flag. Child rows record exact expected role/level/
byte/extent summaries for mapping and FileState children; leaf slices instead
require a Chunk whose actual length covers source_offset+logical_length. Validate
root/non-root partition, profile/framing/EOF, slice bounds and cumulative summaries
from the existing C1 grammar. Ignore unsupported roles for this file-only catalog;
a file root must actually be WholeFile or FileState. Symlinks remain explicitly
unsupported until their own declared gate; no fallback to full reads on a failure.

Physical pack ordering need not match child-before-parent C1 emission, so facts
can be registered before children. Before publication, certify a selected file root
by indexed queries over facts/child ordinals. Only newly uncertified reachable
nodes are processed. Previously certified immutable summaries are reused exactly.
A bounded recursive path is safe because every mapping edge decreases the checked
level, leaves reference only chunks and FileState references only a mapping root:
maximum31mapping levels plus root/leaf/payload, not a population-sized stack.
Query one child row at a time; no whole-frontier, OFFSET, historical scan or
SQL copy of payloads. A node is marked certified only after all exact child facts
pass; failure retains registered objects and accepted workspace data. Unreachable
registered objects are not automatically certified or adopted.

READY still requires actual uploaded pack ACKs, registered exact locators and the
existing complete namespace/portable-metadata validation before conditional C5
publication. Replace only audit's full payload read with this typed file-graph
proof. Authentication alone never certifies names, aliases, reference counts,
cycles or orphan reachability. V4c2b must provide a separate incremental namespace
transition proof against a certified exact base before removing the walk.

The selected experimental provider assumes acknowledged CAS objects stay immutable
and retained; no GC/delete/durability or stronger credential-confinement claim is
introduced. Missing/corrupt objects detected during registration fail explicitly.
Full storage rereads in the separate post-daemon semantic proof remain actual.

## Owning checks and collection

External real-SQLite/public-C1 fixture checks cover deferred child registration,
exact EOF/type, malformed summaries, missing child, actual short payload slices,
root versus non-root fill, changed-file subtree reuse and cached certificate work
counts. Literal expected bytes and rejected forged inputs remain independent of
candidate construction. Publish fixed visited/reused/edge counters; physical
resources/cache observations remain separate and unqualified.

On a frozen source, run one affected declared two-head4KiB SDK/FUSE/SQL/C1/C2/
MinIO/C5 correctness treatment within existing15s child/9.5s proof; preserve all
failures. Canonical reference and physical qualification remain NOT_RUN and cache
unknown INELIGIBLE. Larger/provider file-size and namespace-scale cases are later
prospective gates; do not resample an unchanged arm or call this full V4c COMPLETE.

Own new experimental file facts/certification module, locator registration and
audit caller, plus external tests. No shipped product or third-party source change,
new dependency, retry, hidden quota/worker/deadline increase or benchmark mutation
path. Update #294 every checkpoint and #293 for major results. Next after this
dependency: V4c2b exact incremental namespace certification and admitted inherited
import/mount/retirement, followed by the still-open named/family/DeepSeek cases.

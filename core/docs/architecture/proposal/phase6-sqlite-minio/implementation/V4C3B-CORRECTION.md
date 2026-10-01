# V4c3b mutation prefix correction and retained oracle failure

> Status: Research; informative and not a product contract.
> PARTIAL; corrected live sixth-root gate next.

Parentfb16b6c907700361f680ee0f7ed4fb5b281ccddb.
[Initial actual failed cohort](v4c3b-initial-failed-cohort/),
[corrected owning evidence](v4c3b-mutation-checks/).

Initial runtime performance child7.963867792s <15 completed all6known publications,
cleanup PASS. Separate proof failed exit1 in4.187174042s: first5roots pass, mode
mismatch spans before sixth data/history completion. New preparation wrote0644
into a decimal mode field, expecting644 rather than independent0666maskedbyumask022=420.
Parser uses decimal u32 and owning manifest recipes emit decimal. Corrected source
preparation to420; all commands/bytes/other expectations preserved. Old receipt
remains failed/incomplete; no mode/byte/history PASS inferred from it, no relabeling.

Source review also found write left/right end-filtered predecessor queries scan
old earlier spans when no boundary overlaps; DELETE lacked a lower start bound.
Actual nearest predecessor now seeks once and checks overlap outside SQL. Delete
uses exact lower start and end range; split source bytes/ref counts remain correct.
Real64/4097span count proof: new boundary+delete50VMsteps each, old predicate270/
16402. Existing read38VMsteps each vs275/16407 stays. No speed sample selecting a
nicer number; corrected runtime changes affect sparse writes and truncate/churn.

Truncate now atomically publishes logical size/base visibility first, updates only
its actual crossing span, deletes tail rows in64row transactions, then retires at
most64files per callback. After accepted visibility, partial cleanup failure keeps
that exact logical intent and quarantines future writes/construction/install; reads
are clipped to accepted size, so removed tail is never exposed as old visible data.
No rollback/guessed adoption/automatic retry, WAL/fsync/new connection/profile change.
Physical journal/cache/resource proof remains unrun; touched write spans are bounded
by the existing128KiBcallback body, tail work by64rows per transaction. No unrelated
source or earlier immutable C1head retired.

Changed-source17covering engine/source/construction/span/SQL checks PASS; unchanged
namespace/index7checks retained by scope. Host all-target locked Clippy/fmt/native
release/Linux musl release/Pythonprep compile PASS. Old failing proof and raw outputs
retained; only demonstrated source/fixture correction gets covering run. Full Core
unchanged suites/examples/boundary/LinuxClippy unrun. Canonical/physical NOT_RUN.

Corrected case8c702ca7d4af806e5725ceb3d60b8e168bde2ab8bb835d9e70d17022b690e136
is same original full128five steps and sixth sparse64dd command, with corrected
independent decimal mode expectation. Runtime now has actual bounded mutation
algorithm and cleanup change, not an unchanged passing arm. Freeze then one covering
15s performance/9.5s readonly6root proof, cacheINELIGIBLE. No timeout/worker/input
shrink or benchmark/release admission claim. Current512/256scope and full seven
families/DeepSeek/admission/import/canonical/physical/Unknown/concurrency goal open.

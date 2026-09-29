# #273 lowering: ordinary Exec/FUSE route and C1 ordering obstacle

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Source review at clean `05ac5de1c8d105e6b391c91cfbf6e74f00e54aba`;
> **all experimental product edits were reverted** after the failures below.
> The branch remains NO MERGE; the registered 64 MiB lowering remains **FAIL**
> at its original source. A dirty diagnostic PASS is not a sealed product fix.

## Owner boundary, definitive

The owner's follow-up explicitly forbids restoring **either** the direct
Workspace range-edit entrypoint **or** the FUSE ioctl. They must remain
removed, as [#252's cutover](../252/REPORT.md) required. `WorkspaceApi::exec`
executes any caller command using `/bin/sh -c` at the mounted directory;
ordinary FUSE `READ`/`WRITE`/`SETATTR` reach the same product implementation
regardless of command text. The lowering optimization must live in that
**generic read/write and canonical SaveFile** path. No command-name parsing,
SDK range-edit substitute, benchmark fixture selector, test-only hook, raised
64 MiB quota, backing fsync, extra construction worker or duration increase.
The earlier [public-shift proposal](LOWERING-PUBLIC-SHIFT-DECISION-20260929.md)
is superseded and its Option A is explicitly **rejected**; do not implement it.

## A real generic read-provenance experiment and its limits

An unmerged, local-only patch at
`core/target/issue273-generic-experiment-unmerged.patch` (SHA256
`f7f4c5089ebf1a2d061899da6b9e16e58e0a75781468c4ad005d62fe7b0c1ec7`)
prototyped a **single charged ≤128 KiB recent canonical-read record per
Workspace**. Ordinary `Workspace::read` records the exact returned Base
bytes and source offset; ordinary `write_file` (including projected FUSE
WRITE) compares *every* incoming byte to that record. Equal bytes can name
an internal Base extent with a distinct logical destination and unchanged
immutable C1 source offset. Other writes retain the ordinary pack/payload
route; the daemon never inspects the shell command. No direct range-edit
or ioctl was added. This is conditional reuse for any command that actually
reads/copies matching bytes, **not** a promise that arbitrary new,
incompressible writes can fit a full 64 MiB private quota.

The unchanged original registered `stage_lowering` selection was run on the
*dirty experimental tree* using the closed 67,108,864-byte prepared fixture,
byte-copy setup, locked Linux test binary, single worker, unchanged 64 MiB
private quota, 10 s Stage deadline and 60 s command limit. The original
three edits `[100,110) → abc`, insertion of `12345` at 200, and deletion
`[500,700)` remained; **no changed oracle or limit**. A new *independent*
bounded-window oracle compared all **67,108,662** final saved C1 bytes to
the literal three-edit transformation, not only the original prefix and
far-offset windows. Both original Stage checks passed, full-byte oracle
passed; charged private physical blocks were **180,224** versus the
**67,108,864** quota; command wall **35.529 s** within its original 60 s.
Raw local-only `core/target/issue273-generic-lowering-full-diagnostic-02/result.json`
SHA256 `69aeda51d7f8ef403ccca7313359d98ee551a3722bc71cf29ab3d9fab2f36946`.
An earlier dirty diagnostic passed the two original *partial-window* checks
without the full-file verifier. Neither diagnostic has a clean committed
product/test seal. Their statuses are not promoted to registered PASS, numeric
admission or release proof. The original FAIL remains unchanged.

The attempted **public SDK Exec** falsifier used a caller-supplied `dd`
read/copy/write through the real FUSE mount, read and write callback counters,
read-only held view, full 512 KiB byte oracle and normal SDK Commit. Its
single *out-of-order* copy wrote original canonical bytes from a later offset
into an earlier interval. After exact live byte checks, Commit failed
`Preparing/KnownBeforeCommit/InvalidInput` on the first product iteration;
local-only raw dirty test stderr is under
`core/target/issue273-generic-sdk-diagnostic-05.stderr`. This is a
**correctness counterexample** to promoting that product iteration: C1's
`SaveFile` parser in
[`file_stream.rs`](../../../crates/layerfs-server/src/service/save/file_stream.rs)
accepts retained Base descriptors only when each source offset is **≥ the
previous Base end**. Ordinary POSIX copies can duplicate/reorder immutable
base bytes and still have a perfectly valid final file. A one-command copy
must not turn an accepted WRITE into an unstageable Workspace.

The next dirty experiment tried turning backward Base references into streamed
replacement bytes *inside the daemon's SaveFile upload*. SDK Commit instead
returned `Unknown` after the existing Stage bound; it was **not** a PASS.
The reason is source-checkable: the daemon
[`run.rs`](../../../crates/layerfs-daemon/src/run.rs) keeps a mutex over its
single authenticated Service `Transport::call` while consuming the upload
source, so that source cannot recursively issue a `ReadFile` over the same
delivery without waiting for itself. No retry or increased timeout is
acceptable. Local-only raw `core/target/issue273-generic-sdk-diagnostic-07.stderr`
retains this failure. The experimental patch includes both the early
read-provenance concept and this **unsafe, reverted** nested-call attempt;
never apply it as a product fix. The direct SDK view additionally returned
`Io` at 32 KiB on an unmodified 512 KiB base while 16 KiB succeeded; that
separate read-size diagnostic is retained at
`core/target/issue273-generic-sdk-diagnostic-04.stderr`. It cannot be
quietly hidden by calling an SDK 128 KiB read proven.

## Next reviewed design decision (internal only, never a new edit API)

A complete generic solution must preserve both cases: the full unaligned
64 MiB read/copy/shift without 64 MiB private duplication, **and** a single
ordinary `dd` copy that reorders Base bytes before Commit. There are two
honest candidates. Neither is approved or implemented:

1. **Internal authenticated SaveFile grammar for backward/duplicated Base.**
   Prospectively specify a validated *server-side* source descriptor, not a
   direct edit operation: the daemon declares a canonical Base root and an
   exact `(source,length)` per reordered run. The Service resolves those
   bytes from its already authenticated C1 base during its own SaveFile
   construction, not through a recursive daemon RPC or a file-size spool.
   Validate source bounds, replacement totals, framing, failure/unknown
   custody, schema/peer compatibility and exact canonical identity against
   an independent byte oracle. This changes the internal C1/Bridge SaveFile
   contract; it needs an explicit owner format/compatibility decision before
   editing it. #252's retired SDK edit/ioctl stays absent.
2. **Generic charged boundary escrow.** Keep the current forward-only C1
   grammar, but preserve one or more materializable temporary boundary
   owners in private backing while the shell writes, promoting a displaced
   chunk to a Base reference only when its publication can be staged in
   source order. The hard part is proving a bound for *every* arbitrary
   read/write schedule, old pinned G1 selectors, later overlapping edits,
   quota refusal and exact physical refunds. A speculative one-boundary
   shortcut cannot be called a general solution without that proof.

Both candidates run through the same ordinary Exec/FUSE path, never select a
specific command. A test that only performs a completed left shift is not
sufficient: the out-of-order one-copy falsifier must also reach known
canonical Commit with full exact bytes. Preserve all original and dirty FAIL
attempts separately, keep current limits, and only then take new clean-source
Linux Stage + public SDK Exec receipts. The fixed 16 MiB response Budget and
held-G1/C5 proofs from the previous source do not prove this changed product.
Numeric comparison remains INELIGIBLE, control NOT_RUN; no PR merge.

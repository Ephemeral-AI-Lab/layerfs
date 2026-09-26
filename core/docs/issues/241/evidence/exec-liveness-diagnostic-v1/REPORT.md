# #241 baseline Exec liveness diagnostic v1 result

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. This is a functional diagnostic, not a performance sample.

The single frozen [attempt](attempt-01/receipt.tsv) at source
`105360cd90346d7037316de2bd421c68c8fb4d45` passed. Public SDK Exec of
`printf baseline > .position-baseline` returned exit 0 with empty,
untruncated output in 23 ms of host context wall. SDK unmount, Sandbox delete
and final absence all passed. The test command completed in 5.77 s, including
container lifecycle. No range EDIT or Commit was attempted. The qualified
pristine 10 MiB master was validated and independently byte-copied outside
the call. Cache state was uncontrolled; neither wall is a latency gate.

The [identity](attempt-01/IDENTITY.json) pins the clean source tree, host test
binary SHA-256 `e009c0b196e78161ff879eda4449e6bab3739c9099e7005f15af832711d48f1b`,
immutable Linux image `sha256:03d791bd1c133c438f621eb713ef933a33adf00e0ac5c79eb37c9436bfbc1a27`,
master receipt and both copied database hashes. The [raw console](attempt-01/console.txt.gz)
is stored as a lossless gzip stream and contains host telemetry with zero
dropped/failed records; no daemon operation
record is present there. This version saved Docker inspect/top/logs only on an
Exec failure, so the passing attempt has no retained daemon or FUSE event
vector. The [SHA manifest](attempt-01/SHA256SUMS) verifies all three raw files.

**Interpretation:** a fresh qualified 10 MiB copy did not reproduce the
historical failure. This does not identify whether the failed `printf` reached
FUSE, why its response was lost, or whether the five-second native progress
clock expired. The earlier [integrated campaign](../position-sweep-integrated/REPORT.md)
remains 243 PASS / 1 FAIL / 20 NOT_RUN, and Phase 3 remains incomplete.
Do not rerun this diagnostic to select a failure or pass. A next diagnostic
needs request-scoped frame, child and FUSE events at a newly frozen identity;
the [plan](../../EXEC_LIVENESS_DIAGNOSTIC.md) lists those discriminating events.

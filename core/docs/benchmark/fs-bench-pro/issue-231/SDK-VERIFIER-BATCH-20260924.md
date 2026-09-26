# #231: batch authenticated file-root reads in the full verifier

> **Superseded before implementation.** Owner direction selected the
> [lightweight sampled verifier](SDK-VERIFIER-LITE-20260924.md) instead.
> No batch-root candidate was built or measured; this plan is retained to
> explain the decision sequence.

> Frozen before implementation or a new sample. The preceding
> [traversal-overlap attempt](SDK-VERIFIER-PIPELINE-RESULTS-20260924.md)
> missed its 9.5-second verifier gate and remains a failed receipt. Do not
> rerun that arm or increase its bound. This is a distinct verifier source
> and release-binary identity under the same **9.5-second** limit.

The full verifier currently asks `StoreProvider` for one authenticated
file root on each of 100,000 public `read_all` calls. Every read wave has
its own Store session/locator work. This treatment returns to the prior
four-worker, post-traversal job queue, then gives each worker at most
**64 roots** at a time through the existing public authenticated
`read_canonical_batch` provider API. It classifies each authenticated
canonical root with public C1 `file::classify`. For a whole-file root it
obtains the logical payload with public C1 `whole_file_payload` and feeds
**all** those bytes to SHA-256. For chunked/empty file-state roots it
uses the existing public C1 `read_all` path. It still checks each file's
kind, portable mode/mtime, exact byte count and digest against the sealed
manifest. A missing root, wrong role, cardinality error or read failure
fails the verifier. No raw SQL, object-ID-as-content shortcut or sampled
byte check is allowed.

The two 100-MB anchors in the 100k fixture are scheduled as separate
jobs when their sealed expected size exceeds 1 MB, so they can occupy
different existing workers. That scheduling uses the manifest only to
decide a batch boundary, never to supply observed content. Keep exactly
four workers and the existing Store read policy. The 64-root batch is a
fixed verifier resource ceiling, not a new product constructor policy.
The full C5 root check, complete C1 namespace traversal and distinct
actual-path count remain unchanged. No new dependency is needed.

Build the SDK driver and verifier with locked Cargo `--release` from the
sole runner. Mark the changed verifier and runner as append-only release
v5 receipts at a new source/build/harness identity. Keep the 15-second
public command, 9.5-second independent verifier and 30-second build
limits. Run the 100k selection once first; if it passes, run 100, 1k
and 10k once each at that same identity. Every fresh Store uses the
seed-1 `core-sdk-init-fixture-v2` case and a separate full oracle. Preserve
any failure and do not take a second sample of an unchanged arm.

This remains functional verification under
`source-cache-uncontrolled-v1`, with no approved numeric SDK latency gate.
Even four full verifier PASS rows do not alone satisfy #231's eligible
performance requirement or authorize main merge/issue closure.

# Pooled demand reconstruction treatment

Status: concrete changed treatment; retained-history qualification pending.

The prior matched17 diagnostic showed candidate filesystem wall6.751541250s
versus reference4.505912250s, despite SQL step0.422399s versus1.937591s.
Inspection traced two calls to the same pooled-body reconstruction: the C2
wave-wide ordinal prefetch and the canonical Resolver. A public64-row FULL-leaf
fixture confirms4 physical-record extractions before the treatment, versus the
2 required by the existing chain walk/decode. The pre-treatment regression failed
with actual4/expected2; this is mechanism evidence, not a speed sample.

Treatment: remove whole-wave pooled-body prefetch; prepare bounded catalogue rows
from the already checked body inside canonical resolution. Existing scalar sources
explicitly do no advisory prefetch and retain their required value lookups. The
active Fetch source delegates to the existing bounded set-query implementation.
No chain, catalogue, digest, length, canonical authentication or cache limit is
removed. The temporary ordinal vector is at most MAXIMUM_LEAF_ROWS, released at
the prefetch return, replacing the old whole-wave ordinal set/vector. No cache or
buffer bound increases. Required unsupported capabilities still fail; advisory
prefetch is not required for correctness and has no error-driven fallback.

Actual counters are exposed on the public operation Reader; benchmark stages log
those counters in both arms with the same operation-reader-only scope, including
prefetch. They exclude save-owner reconstruction and physical device I/O. A
fixture passes with one leaf request/two physical record calls/one physical group
decode; the next demand yields cumulative2/4/1 and identical canonical bytes.
Missing catalogue rows still refuse the read. Partial-hit locator regression PASS.
Owning storage/project Clippy all-targets warning-denying PASS; boundary442 PASS,
23 guard self-tests PASS. Focused persistence/public Init integration checks are
recorded separately. No full-workspace test claim.

Prospective qualification: one new history-stride10-v2 arm each, same release
harness/source/corpus/cache/observer/workers and restored60s command/9.5s proof
bounds, fresh outputs, reference pins derived only from this qualified baseline.
This is a changed-source ordinary gate, not another sample of a prior treatment.
All previous Init PASS artifacts, history failures and diagnostic receipts remain
unchanged.53/157/Durable remain NOT_RUN until evidence supports moving forward.

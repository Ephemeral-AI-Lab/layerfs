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


## Ordinary17v2 result at1ca759559

Both fresh arms completed with source/state cold checks PASS, matching full17root
vectors, canonical51689objects/380559460B, storage and cleanup PASS. Original raw
runs are `issue302-history17-pooled-{baseline,candidate}1`; compact evidence under
`checks/history17-pooled-demand1` omits database bodies, build logs and other large
raw artifacts. Raw manifests apply to original runs, not the compact copies.

| Scope | Phase4.5 | Candidate Disposable |
| --- | ---: | ---: |
| Complete product lifecycle |34.377794000s|36.518927083s|
| Complete performance command |52.082604750s|53.494491208s|
| Independent verification |4.754689625s PASS|9.507012333s TIMEOUT|
| Allocated closed Store/history |52,473,856B|49,594,368B|
| Acquisition |16.408217916s|16.513819001s|
| Construction |1.672465043s|1.641716501s|
| Filesystem |4.563421792s|3.718593293s|
| Save/custody |11.514611042s|14.391946960s|
| Processing subtotal |17.750497877s|19.752256754s|

Time gate PASS: ratio1.062282445552; allowed37.815573400s under integer
10*candidate<=11*baseline. Strict allocation<54,278,964B PASS, both complete
performance commands<60s. Joint gate INCOMPLETE because candidate proof times out;
no budget increase or original-receipt promotion. Producer root/canonical match
and closed census do not replace required independent namespace/custody proof.
The contextual processing target19.3213359625s remains unmet by0.4309207915s;
processing is not substituted for the complete lifecycle gate.

Comparable operation-reader filesystem counters summed across all17readers now
agree:11926leaf requests,13373chain edges,50598physical-record calls,2633physical
group decodes/119505866decodedB,47965physical group cache hits,33834value-group
decodes. Reference384pack fetches/15365529B, candidate331/16011390B; lower fetch
count is not fewer copied bytes. Counters exclude save-owner work. Construction
pooled counters are zero; save snapshots do not add work on these same readers.

Full observed SQLite trace (includes setup/finalization, not only stage work):
reference524829statements/24421375VM; candidate193438/9339792VM, omitted0both.
Candidate native51374transactions/724writes =>50650read transactions, compared
with53350in the original v2 candidate; identity differs and no speed attribution
is inferred from that historical comparison. Save/custody remains slower and
provider body reads remain1.702GBpayload+1.051GBmetadata-pack returned bytes,
with distinct overlapping work scopes. Investigate save read-lifetime and native
proof steps next.53/157/Durable remain NOT_RUN; no all-seven or Durable claim.

Validation: count regression PASS; missing catalogue refusal PASS; locator custody
PASS; sqlite_publication13PASS/sqlite_transaction_units9PASS; init_sqlite1PASS
(fullnamespace100/1000both profiles). Storage/project all-target Clippy -Dwarnings,
package formatting, boundary442 and23guard self-tests PASS. Full workspace tests
were not run; these checks cover changed reader, persistence and public Init paths.

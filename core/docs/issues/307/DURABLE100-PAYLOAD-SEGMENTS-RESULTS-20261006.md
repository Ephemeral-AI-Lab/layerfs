# Durable100 payload segments: retained first candidate

> **Status:** Implemented candidate; first speed gate FAIL, storage/proof PASS.
> Monolithic v2 failures remain retained. No release or S7/S9 closure follows.

The owner selected the [payload proposal](DURABLE100-NEXT-LAYOUT-PROPOSAL-20261006.md)
after reviewing both VFS cost and acquisition placement. Implementation `fb7f3f47734eb8cd4d68b8e965a24929c8332785`
adds explicitly selected schema7/10, unchanged canonical grammars, immutable
large-payload segments, exact directory/file custody, bounded strict/scoped readers,
retained definite/uncertain physical files and separate direct-I/O observations.
A1 acquisition stays on the same Durable Session. Default/old layouts remain.

| Exact case / source | Product ns | Complete command /30s | Independent proof /19s | Final allocated B | Speed / storage / joint |
| --- | ---: | ---: | ---: | ---: | --- |
| Retained original acquisition-v2 reference /7edddbdb8e | 41,992,917 | original receipt | original receipt | 7,372,800 | reference |
| acquisition-payload-segments-v1 /fb7f3f477 | 114,541,166 | 1,817,386,042 | 653,297,875 | 5,304,320 | FAIL / PASS / FAIL |

The reference is the competitive Phase4.5 MEMORY/OFF public Service route, not a
matched Durable physical layout. The new case retains the original100-file,
5,000,000B seed1 fixture and manifest, four constructors, environment workers1,
30s build/command and19s proof caps, cold helper/attestation and exact gates.
It changes case/harness identity explicitly rather than relabeling Monolithic.
`10*114,541,166=1,145,411,660 > 11*41,992,917=461,922,087`.
Final allocation includes315,392B DB,32,768B SHM,0B WAL/directory and4,956,160B
segments. It is2,068,480B below the reference. Raw metrics and original reference
hash are in the [ledger](checks/durable100-segments-results-20261006/ledger.json).

Bootstrap is28,365,875ns, Init83,524,208ns, checkpoint2,199,375ns and
close219,500ns; these are nested attribution, not operands added to fabricate a
second product clock. Cold content residency is zero after attestation; canonical
root matches. Independent proof checks all102 paths/kinds/directory metadata and
53 sampled files /3,354,003B. This is sampled content, not a full-payload oracle.
The command, build, proof and cleanup pass; lifetime RSS is not a phase peak.
The four other containers remain declared interference and unchanged.

The same-source count diagnostic uses the public observed Init path, instrumented
VFS and direct counters; every clock is ineligible for speed admission. It observes:

| Count scope | Original Monolithic diagnostic | Payload diagnostic |
| --- | ---: | ---: |
| SQLite submitted writes B | 11,051,708 | 1,036,756 |
| Direct segment writes B | absent | 4,921,726 |
| Combined submitted writes B | 11,051,708 | 5,958,482 |
| SQLite sync calls | 26 | 23 |
| Direct full-flush calls | absent | 8:2 bootstrap +3 file +3 directory |

This demonstrates less submitted I/O, while the first candidate introduces five
net additional synchronization calls. It does not prove a device-byte delta or a
causal latency increase from a single noisy sample. The diagnostic's physical
packing differs slightly from the eligible arm; keep its byte/count observations
separate. Publication profiling records58 statements,28,674 actual VM steps,
zero full scans/sorts,5,612,838ns SQLite COMMIT wall and13,871,960ns direct sync
wall. Native steps, port spans and whole clocks are nested and never added.
The paired EXPLAIN programs use the same linked SQLite3.51.0, immutable read-only
open, primary-key pack/segment lookup, and actual external INSERT/extent statements.
The count diagnostic's producer files, including SHM, were copied before independent
proof; its complete final manifest and independent byte copies match. Raw/analysis
are retained [here](checks/durable100-segments-vfs-20261006/analysis.json).

Checks cover269 host test bodies through the failed covering invocation followed
only by repaired/unrun checkpoints,128 Linux portable bodies, host/Linux Clippy
with warnings denied, formatting,658-file boundary guard,40 tooling tests and16
owning harness tests. Native SIGKILL preserves acknowledged body/catalogue/ID;
external checked-close uncertainty quarantines, retains the exact file and does
not reissue its ID. Physical power-loss testing is NOT_RUN. Initial fixture errors,
an example launch failure and an ineligible fcntl refusal observer remain retained;
the fixed-signature close observer delegates real native operations. See the
[check summary](checks/durable100-segments-20261006/check-summary.json).

Production LOC per commit, exact first-parent comparison with unchanged
`tools/production_loc.py` SHA256c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb:

- `7399fac994`: core94,499→94,499, reference65,417→65,417,
  combined159,916→159,916 (delta+0), diagnosis/proposal docs only.
- `fb7f3f477`: core94,499→95,150, reference65,417→65,417,
  combined159,916→160,567 (delta+651), new physical layout/custody implementation.
  Shipped SQL included; tests/inline tests/examples/docs/tooling excluded. No
  relocation, duplicate product or reference retirement. Committed tree equals
  the [prepared staged comparison](checks/durable100-segments-results-20261006/implementation-committed-loc.json).

## Next bounded correction

The next selected correction coalesces duplicate full-device barriers without
changing durability or catalogue publication: stage directory metadata with
`fsync`, then full-synchronize the same-device body file; at creation, stage the
payload directory then full-synchronize its same-device parent. Apple's full-sync
contract drains prior device-buffered writes. All bodies and directory entries
still become durable before any SQLite segment/pack INSERT. A failed fsync/full
flush/checked close remains uncertain and retains custody; no retry or weaker
acknowledgement applies. This reduces direct full-device barriers from8 to4 for
this shape, while staging calls remain real paid work. Prove native support/order,
reopen/crash/uncertainty, then take one new source sample under the unchanged gates.
No unchanged performance arm is replayed and no acquisition relocation is selected.

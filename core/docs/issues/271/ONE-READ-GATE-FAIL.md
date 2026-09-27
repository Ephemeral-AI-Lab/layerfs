# Issue 271: one-read custody source and retained 4,097 failure

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective one-read correction](../../../../docs/roadmap/0.1/0.1.7/issue271-sponsor-one-read-spec.md)
preceded frozen source `fd407a0370828efac4a3e44a4edff5b1704cf77b`.
It reuses one authenticated sponsor-owner ledger read rather than reading the
same record again while loading the old page body. The ownership graph,
one-hop limit and page format remain as specified in the
[custody trial](../../../../docs/roadmap/0.1/0.1.7/issue271-sponsored-page-treatment-spec.md).
The [append-only raw evidence](evidence/one-read-v1/) pins product/harness,
source tree, release binaries, image, writer, master, clone, oracle, cache and
command identities. Each public selection ran once. Ordinary host/container
cache was uncontrolled: raw latency is **INELIGIBLE** throughout.

| Public selection | Actual WRITEs | Ledger 4 KiB reads / writes | Exec / Commit / complete command | Full oracle | Product cleanup | Row |
| --- | ---: | ---: | ---: | --- | --- | --- |
| separated 100 | 100 | 2,866 / 1,446 | 0.372032 / 0.036252 / 1.331315 s | PASS | PASS | INELIGIBLE |
| separated 512 | 512 | 20,573 / 10,988 | 2.501532 / 0.077527 / 3.828853 s | PASS | PASS | INELIGIBLE |
| 10 MiB append 100 | 100 | 2,459 / 1,235 | 0.351285 / 0.021854 / 1.959451 s | PASS | PASS | INELIGIBLE |
| 10 MiB dispersed 100 | 100 | 2,858 / 1,441 | 0.412543 / 0.038172 / 1.439296 s | PASS | PASS | INELIGIBLE |
| 10 MiB repeated 100 | 100 | 2,663 / 1,289 | 0.366516 / 0.026254 / 1.327033 s | PASS | PASS | INELIGIBLE |
| separated 4,097 gate | unavailable | unavailable | unavailable / unavailable / **25.004969 s** | NOT_RUN | FAIL | **FAIL** |
| separate 2,048 count diagnostic | unavailable in host receipt | postmortem 113,143 / 67,771 | unavailable / unavailable / **15.004927 s** | NOT_RUN | FAIL in host receipt | **FAIL** |

The 100 and 512 separated rows satisfy the frozen count predicate against the
combined baseline: 2,905/1,511 → **2,866/1,446** (−39/−65), and
20,953/12,150 → **20,573/10,988** (−380/−1,162). Both had four upstream
calls, exact old-head parent relation, 100/512 changed runs, full byte oracles,
unmount and sandbox deletion. Their new heads are
`12c0ec902b8f3c247b19812061cf02f09164bbdcd3909de5c54dfdb638d1718116`
and `123d462af71f9335992947dd5ca616ee876676b97a478d25cbd9ea63eb1a06a258`;
the old head is
`12ef44464e764a34399e5be721a9d14ab714813471949c06bf20413d2ca6f9a098`.
The three 10 MiB sibling receipts pin their separate old/new heads and full
oracles. Their verifier walls were under 9 s; the separated 100/512 verifier
walls and commands are in their receipts.

At 512, the 128-WRITE ledger checkpoints were 3,882/1,964, 9,173/4,701,
14,724/7,696 and 20,573/10,988 reads/writes. The consecutive blocks cost
3,882, 5,291, 5,551 and 5,849 reads; writes cost 1,964, 2,737, 2,995
and 3,292. The charged page-owner counters distinguish child, Local and
sponsor edges from encoded page fields. The last WRITE charged 2,223 child,
11,809 Local and 457 sponsor additions; old-root cleanup had released
2,206 child, 11,057 Local and 447 sponsor edges by that checkpoint.
The root remained height 1 with 16 children. Sponsorship reduced ledger
work but did not flatten the within-run count slope.

The **one 4,097 gate** expired at its unchanged 25 s complete-command bound
before a driver receipt, actual WRITE count, Commit, verifier or clean close.
The Store/history clones remained 618,496/86,016 bytes. The owned daemon's
postmortem log records `WorkspaceExec` **error** after **29.101309 s**, with
`daemon.exec_output` at 29.083922 s, followed by `sandbox shutdown retained:
Busy`. The 30-second product timer belongs to #249 and was not treated as a
benchmark allowance. Its container stayed alive after the host timeout;
deliberate stop ended with exit 137. A read-only postmortem found 17,772 KiB
and 4,375 files in the private volume. The owned container and volume were
then removed, but forced removal is **not** product cleanup PASS. The sampled
daemon RSS reached 43,098,112 bytes before shutdown; neither that lifetime
observation nor the volume size is a phase-local memory peak. Gate custody,
callback count, Commit root and quota refund are unavailable, not zero.

The separate 2,048 **count diagnostic** retained its unchanged ordinary
15 s host timeout, so its row is also FAIL with no driver callback, Commit or
verifier receipt. Its daemon later recorded all 2,048 write-class callbacks
and a successful Exec at 13.019009 s, but that postmortem cannot promote the
timed-out host row. The daemon's four 512-WRITE ledger checkpoints were
20,573/10,988, 46,724/26,916, 79,051/46,494 and
**113,143/67,771**. Their increments were 20,573, 26,151, 32,327 and
34,092 reads; 10,988, 15,928, 19,578 and 21,277 writes. The root reached
height 2, with 106 live metadata pages, 577,536 metadata bytes and 8,966,144
backing bytes at the last WRITE. The daemon later exited 0 and its private
volume had no files, but the host did not observe a clean product close; the
host receipt remains cleanup FAIL. This diagnostic measures cause only and is
not a replacement 4,097 sample.

The algorithm is correct in the selected passing rows and native custody
proof, yet #271's public 4,097 gate remains **FAIL**. No wall-time, memory or
count extrapolation from 100/512 changes that outcome. A further structural
source must reduce the increasing per-block ownership cost, keep old G1/G2
roots and exact refunds, and receive its own prospective identity before any
new gate attempt.

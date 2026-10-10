# Full mounted oracle timeout (receipt 053): counted diagnosis

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Written 2026-10-10 at `496bb5643` (product tree `9a76077239b4`, unchanged). Every
number below is exploratory and ineligible: one run each, natural caches with
setup-copy warmth, no cold claim, no verdict, no timing admission. Receipt 053
keeps its verdict: FAIL, comparator exit 124 at its registered 85 s stop. The
unchanged full proof was not invoked again.

## What was run

| Receipt | What | Outcome |
| --- | --- | --- |
| [001](001-comparator-bookkeeping-diagnostic/) | The registered comparator's expectation preparation, unchanged, on the sealed inventory; then the 71,300 retained observed rows of 053 replayed through the same per-entry `SELECT`/`UPDATE` and JSONL emission; then SHA-256 over the same byte count from memory. No mount, no Store. Host and pinned Linux image | Completed on both |
| [002](002-full-mount-probe-prepare/) | New owned volume `layerfs-r8-full-proof-20261010-2cd69f68db34`: streamed byte copy of the retained sealed Store by the declared preparation route; digest `0c836c88…cf05`, mode 0600, root-owned, 1,417,285,632 bytes | Prepared |
| [003](003-full-mount-probe/) with [tools](002-full-mount-probe-tools/) | Registered runtime and daemon binaries (SHA-256 equal to registration v2), one mount, four bounded read-only commands using the comparator's system-call pattern, a daemon snapshot around each, terminal unmount | COMPLETE: Unmounted, Gone observed, stop acknowledged |
| [005](005-walker-probe/) with [tools](004-walker-probe-tools/) | Same, new daemon and mount: 0, 4, 8 and 16 concurrent directory walkers over disjoint quarters of `node_modules/.pnpm` | COMPLETE: Unmounted, Gone observed, stop acknowledged |

## Comparator bookkeeping (001, pinned Linux image)

| Part | Time | Count |
| --- | --- | --- |
| Expectation preparation before the first mount observation (two inventory digests, load, projected export and its digest) | 4.43 s | 130,046 rows |
| Per-entry `SELECT` + autocommit `UPDATE` + JSONL emission | 1.65 s | 71,300 rows, 23 µs each; 3.0 s projected for 130,046 |
| SHA-256 of 2,824,199,089 bytes from memory | 1.44 s | — |

The comparator's own bookkeeping accounts for about 7.5 s of the 85 s that
expired in 053. It is not the main cost. The host run agrees (5.07 s, 2.30 s,
1.43 s).

## Through-mount service (003 and 005)

| Phase | Wall | Entries | Regular bytes | Rate | Daemon CPU in the phase |
| --- | --- | --- | --- | --- | --- |
| Serial walk, files up to 4 MiB (003) | 7.00 s | 5,358 | 56,654,224 | 765 entries/s | workers 3.42 s, receive loop 1.62 s, overlay owner 0.003 s |
| Serial walk, 8 reader threads (003) | 7.01 s | 9,526 | 105,452,126 | 1,358 entries/s | workers 5.24 s, receive loop 2.83 s, owner 0.010 s |
| One 220,584,000-byte file, serial (003) | 2.30 s | 1 | 220,584,000 | 96 MB/s | workers 1.53 s, receive loop 0.13 s |
| Four large files, four readers, stopped at 5 s (003) | 5.00 s | 4 partial | 557,318,144 | 111 MB/s in total | workers 3.75 s, receive loop 0.31 s |
| Serial walk (005) | 6.00 s | 5,825 | 57,840,870 | 970 entries/s | workers 3.33 s, receive loop 1.56 s, owner 0.001 s |
| 4 walkers (005) | 6.00 s | 14,578 | 153,813,956 | 2,429 entries/s | workers 8.60 s, receive loop 3.55 s, owner 0.91 s |
| 8 walkers (005) | 6.00 s | 17,285 | 161,227,702 | 2,880 entries/s | workers 10.52 s, receive loop 2.13 s, owner 4.06 s |
| 16 walkers (005) | 6.00 s | 17,744 | 130,245,072 | 2,955 entries/s | workers 11.53 s, receive loop 1.36 s, owner 5.33 s |

CPU is from `/proc/1/task/*/schedstat` in the runtime's own snapshot, summed by
thread name (four `layerfs-fuse` workers, one `fuser` receive loop, one
`layerfs-overlay` owner). The phases read different names, so their entry mixes
differ; the rates are indications, not a comparison between treatments.

## What the counts establish

1. **The serial arrangement cannot finish inside the fence.** 053 reached
   71,300 of 130,046 entries and 2.82 of 3.48 GB when 85 s expired; 58,746
   entries and 651,577,060 bytes remained, 31,528 of those entries under
   `node_modules/.pnpm` and 24,587 under `packages`. At the serial rates seen
   here (765 to 970 entries/s in `.pnpm`; about 1,330 entries/s averaged over
   053's own prefix after subtracting its large payloads at 96 MB/s) the
   remainder alone needs roughly 45 to 75 s more. A serial full traversal is
   therefore about 130 s or more: above the 85 s comparator stop, the 100 s
   command stop and the 120 s test ceiling.
2. **No daemon thread is saturated by one serial client.** In the serial walks
   the four workers use about 0.5 of one core in total and the receive loop
   about 0.25; the overlay owner is idle. The time is spent per entry (about
   1.0 to 1.3 ms of wall, about 0.9 ms of daemon CPU), one request at a time.
3. **Concurrent walkers are served concurrently, up to a plateau.** 4 walkers
   gave 2.5 times the serial entry rate and 8 walkers 3.0 times; 16 walkers add
   nothing. At the plateau the overlay owner thread is busy for 4.1 to 5.3 s of
   6 s, while it is idle under a serial client. Why concurrent base-only reads
   reach the owner at all was not investigated here; it is a count to explain
   under gate G08, not an optimization assignment.
4. **Large cached reads do not scale with readers.** One reader gets 96 MB/s,
   four get 111 MB/s in total, with the same worker CPU per byte (about
   145 MB per CPU-second). This is consistent with the promoted profile's
   background depth of 1, under which readahead READs of one mount are served
   one at a time. 3.48 GB at about 100 MB/s is about 35 s whatever the
   comparator does with page-cache reads.
5. **The full fixture has no regular alias class.** 0 of 103,108 regular files
   share a source inode, so the alias-class assertion is exercised on this
   fixture only as "every link count is 1 and no two names share an inode".

## Consequence for the comparator

A change to the comparator's SQL (one transaction, deferred indexes) would
recover at most a few seconds and cannot complete the proof. The correction
that the evidence supports is concurrency in the verifier: several directory
walkers that observe names, metadata and bytes with the unchanged per-entry
system calls and stability checks, and one collector that performs the
unchanged comparison against the independently sealed expectation. Projected
from the rows above: 130,046 entries at 2,400 to 4,000 entries/s is 33 to 54 s,
large payloads about 26 to 35 s partly overlapping with it, preparation 4.4 s.
That is 55 to 80 s against an 85 s stop: it fits on the evidence, without a
wide margin, and it is not proven until it runs once at a registered identity.

Nothing here shows a product contract defect. No product change is proposed
from this diagnosis, and no limit, workload, cache state or oracle assertion
is changed by it.

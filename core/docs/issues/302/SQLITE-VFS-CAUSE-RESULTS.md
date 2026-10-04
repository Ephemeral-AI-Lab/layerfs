# Delegated SQLite VFS comparison, revision 4

> **Status:** Research; informative and not a product contract.

The qualified paired cause diagnostic points to write and acknowledgement work as
the largest remaining measured opportunity. It does not establish a speed PASS.
The active product uses embedded SQLite; the earlier PostgreSQL/MinIO ranking
applies to the retired implementation, not this candidate.

## Comparable work and qualification

One diagnostic child per arm at frozen observer source
`890ecb2d4b90bcca684a5e03a63f11708ed06ca9`; reference product remains
`7edddbdb8e8512627aed0ed42533ef099d802384`. Release, locked builds and immutable
binary archives are identified in the [comparison receipt](checks/paired-cause1000-comparison4/comparison.json).
Both public C1/C2/C5 callers produced root
`a71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c`,
2,003 canonical IDs and 20,187,652 canonical bytes, inventory SHA256
`b997719ad382df32b053decb160b8531093fcf8685c48ad4c058ef86ed9f9b99`.
These match both frozen production roots. Both independent sampled content/tree
proofs, source-content cold checks (whole-input resident pages zero), cleanup and
observer qualification passed. Filesystem metadata residency remains unobserved.

Prepared fixtures were reused outside timing. Complete cold-plus-child commands
were 3.773767625 s / 3.100976375 s, below 15 s; independent proof commands were
0.053785417 s / 0.049201333 s, below 9.5 s. Four Init construction workers and the
single C2 consumer/bounded channels are unchanged. Neither arm is a new admission
sample. No unchanged speed arm was rerun.

## Observed operations

| Whole diagnostic lifecycle | Phase 4.5 | Candidate |
| --- | ---: | ---: |
| Init wall | 131.673875 ms | 211.486250 ms |
| Bootstrap wall, separate | 5.108250 ms | 15.828500 ms |
| Final explicit checkpoint wall, separate | 0 | 1.502250 ms |
| Traced statements | 3,640 | 305 |
| Trace VM steps | 326,442 | 203,586 |
| COMMIT sqlite3_step calls | 27 | 32 |
| COMMIT sqlite3_step wall | 34.544209 ms | 117.368291 ms |
| COMMIT sqlite3_reset calls / wall | 0 / 0 | 32 / 0.258706 ms |
| VFS xRead calls / requested bytes | 380 / 513,363 | 5,327 / 21,803,148 |
| VFS xRead wall | 0.417994 ms | 5.083195 ms |
| VFS xWrite calls / submitted bytes | 10,499 / 21,416,960 | 10,288 / 42,235,980 |
| VFS xWrite wall | 32.745796 ms | 63.789514 ms |
| VFS xSync calls / wall | 0 / 0 | 34 / 43.442045 ms |

| File class | Arm | Read calls / requested bytes / ms | Write calls / submitted bytes / ms | Sync calls / ms |
| --- | --- | ---: | ---: | ---: |
| Main | Phase 4.5 | 380 / 513,363 / 0.417994 | 10,499 / 21,416,960 / 32.745796 | 0 / 0 |
| Main | Candidate | 135 / 540,804 / 0.229042 | 5,101 / 20,893,696 / 34.686032 | 6 / 13.596042 |
| WAL | Candidate | 5,191 / 21,262,336 / 4.853070 | 5,185 / 21,341,760 / 29.046274 | 27 / 29.705628 |
| Main journal | Candidate | 1 / 8 / 0.001083 | 2 / 524 / 0.057208 | 1 / 0.140375 |

Reference WAL/journal/other counters and candidate other counters are zero.
Candidate sync flags were 2 NORMAL and 32 FULL, with zero unknown flags.
Both used SQLite 3.51.0, delegated underlying `unix` VFS version 3, zero live
files at report time and zero close errors. The diagnostic default name was
`layerfs-cause-delegate`; its aligned 176-byte file header wrapped the original
192-byte OS-file structure. Both trace observers reported zero errors and zero
active statements. Native product VM counters are unqualified because the trace
observer resets them; the table uses trace counters.

## Interpretation and next priorities

Candidate WAL and main-file submissions total approximately twice the reference
write bytes. WAL reads are visible at the VFS layer even when C2 reports no pack
or payload rereads. Candidate write-plus-sync wall is 107.231559 ms, versus
32.745796 ms for the reference. Adding read time gives a lifecycle delta of
79.150964 ms; the Init wall delta is 79.812375 ms. Their proximity is evidence
that storage operations deserve priority, not an exact additive partition:
VFS counters include bootstrap, automatic and final checkpoints and nest inside
SQLite step/statement/Init clocks. Sync calls are delegated VFS operations,
not physical fsync counts; submitted/requested bytes are not device bytes.
No unobserved CPU time is attributed to durability by subtraction.

1. Reduce actual publication/checkpoint write work within existing bounds.
   Inspect avoidable repeated dirty pages and small acknowledged publications.
   Preserve WAL/FULL, fullfsync, checkpoint_fullfsync, the 1,000-page automatic
   checkpoint threshold, independent canonical/physical transaction charges,
   first-wins conflicts and atomicity. Confidence in the measured opportunity
   is high; confidence in reaching Phase 4.5 parity remains unproven.
2. Reduce separately acknowledged allocator/policy exchanges where a bounded
   operation can safely share an acknowledgement. Unused pack reservations and
   initial ordinal lookahead are already implemented; remaining changes need
   atomicity and unknown-outcome analysis before implementation.
3. Improve pack utilization and remove redundant encoding/validation work only
   where byte/count attribution demonstrates work remains. FULL construction
   totals were 5.217662 / 5.565894 ms, grouping 1.506580 / 1.764744 ms across three
   saves, much smaller than the observed write/acknowledgement term.
4. Statement preparation, mapping and COMMIT reset are lower priority. Candidate
   statement/VM counts are already lower; COMMIT reset is only 0.258706 ms.

The reference's memory journal/synchronous-off profile and candidate's required
WAL/FULL profile stay explicitly different. This is a causal measurement, not an
excuse to weaken durability or change the competitive threshold. Latest ordinary
Init1000 admission remains FAIL (142.588792 / 223.193959 ms, ratio 1.565298057).
All seven required competitive selections remain incomplete at the latest source;
this diagnostic does not promote historical or unrun rows.

The [prospective VFS contract](SQLITE-VFS-CAUSE-CONTRACT.md) defines delegation,
qualification and scope. [Prior small-step report](SQLITE-PAIRED-SMALL-STEP-COMPARISON.md)
and [step/reset results](SQLITE-STATEMENT-LIFETIME-RESULTS.md) retain earlier
observations without replacing their receipts. Source/tooling validation is reused
from the observer commit; this update changes documentation/evidence only.

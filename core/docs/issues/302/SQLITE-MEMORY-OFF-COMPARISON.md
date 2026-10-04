# Phase 4.5 and Phase 7 with MEMORY/OFF mutating work

> **Status:** Research; informative and not a product contract.

The qualified 1,000-file diagnostic shows comparable speed under MEMORY/OFF:
Phase 7 Init is **133.230042 ms versus 140.552500 ms**, and the complete diagnostic
lifecycle is **151.535000 ms versus 145.632458 ms**. The ratios are **0.947902328**
for Init and **1.040530402** for the lifecycle. Both are numerically within the
1.10 diagnostic margin. **This is not a durable production admission PASS.**

This supports the narrower conclusion that the current implementation does not
show a material Init regression at this size when its actual mutating work uses
MEMORY/OFF. It does not establish repeatability, larger-tier or retained-history
performance, or equivalent speed under the required WAL/FULL production profile.

## Qualification and settings

One child per arm at observer source `96327681f7a7aacd1bb318aa24bb7717e309489e`;
reference product remains unmodified at
`7edddbdb8e8512627aed0ed42533ef099d802384`. Release/locked, prepared fixture reuse
outside timing, four Init constructors, env construction workers1, bounded
channels/waves/packs/transactions and source-content cold contract remain.
Both complete inputs had zero resident content pages; filesystem metadata
residency remains unobserved. No historical receipt was promoted or overwritten.

Both roots match the ordinary production arms:
`a71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c`.
Both inventories have2,003 IDs/20,187,652 canonical bytes and digest
`b997719ad382df32b053decb160b8531093fcf8685c48ad4c058ef86ed9f9b99`.
Independent sampled byte/tree proof, cleanup, trace/API/VFS coverage and budgets
PASS. All observed mutating connections read back `memory`/0 before their first
main mutation and at close: five reference connections and one candidate.
Observer errors/live files/close errors/unknown sync flags are zero.

| Effective database configuration | Phase 4.5 | Phase 7 diagnostic |
| --- | --- | --- |
| Journal / synchronous during mutating work | MEMORY /0 | MEMORY /0 |
| Main C2 /C5 page size | 2 KiB /1 KiB | Combined4 KiB |
| Native cache setting | 2,000 pages | -2,048 KiB |
| Foreign keys /mmap | ON /0 | ON /0 |
| fullfsync /checkpoint_fullfsync flags | 0 /1 | 1 /1 |
| Initial connection profile | MEMORY/OFF | Stock WAL/FULL validation, then diagnostic transition |

These native geometry/cache differences are existing implementation choices;
they are recorded rather than described as identical. The candidate's original
profile validation is retained. Transition to MEMORY/OFF occurs before the first
mutation and is paid inside bootstrap. The public profile object captured before
intervention describes the stock profile; effective diagnostic settings come
from real native PRAGMA readbacks. No result is fabricated. Sync0 disables the
synchronous-persistence request despite retained fullfsync flags.

The candidate's proof uses a fresh independent byte copy, exact SHA equality
before conversion, WAL header and retained empty WAL/SHM read sidecars for the
unchanged production verifier. The original measured database SHA is unchanged
before/after proof. Copy/hash/adapter/verifier wall is in the separate9.5s envelope;
no application table/row mutation and no timed-work reuse.

## Lifecycle and small steps

All times below are observed nanoseconds. Named stages are inclusive or
concurrent where stated; do not add them as a disjoint wall-time partition.

| Clock | Phase 4.5 ns | Phase 7 ns |
| --- | ---: | ---: |
| Complete diagnostic operation | 145,632,458 | 151,535,000 |
| Bootstrap | 4,760,041 | 9,421,333 |
| Init | 140,552,500 | 133,230,042 |
| Final explicit checkpoint | 0 | 12,000 |
| Close | 119,042 | 7,871,083 |
| Profile intervention/effective readback, nested in bootstrap | 25,334 | 5,420,917 |
| Complete cold+child command | 2,563,511,209 | 2,701,817,167 |
| Separate complete proof envelope | 51,255,875 | 625,225,375 |
| Scan | 4,574,542 | 4,600,000 |
| File-owner work | 95,646,625 | 102,887,042 |
| File-read worker sum (2,693 calls /20 MB each) | 87,494,259 | 88,084,097 |
| Constructor worker sum | 352,746,535 | 384,223,580 |
| Producer-send worker sum, backpressure included | 233,687,044 | 265,763,668 |
| Owner receive | 16,308,870 | 16,965,837 |
| Filesystem tree | 1,186,500 | 1,235,166 |
| Reserve inodes | 120,916 | 162,750 |
| Genesis | 162,833 | 206,834 |
| Save begin total | 3,296,625 | 1,516,125 |
| Save finish total | 32,512,042 | 21,736,542 |
| FULL selection/construction total | 5,408,501 | 5,120,321 |
| Group construction total | 1,526,908 | 1,552,173 |

Complete command bounds remain15s and proof bounds9.5s. Candidate proof adaptation
was541,765,875ns, included in the625,225,375ns envelope. Its time is not part of
Init or the performance ratio. Different worker-arrival packing is recorded:
reference99 packs (including19 metadata appends); candidate96 immutable packs.
Canonical work/root/inventory remain identical. Worker sums are concurrent and
send includes waiting; they are not pure hashing/CDC CPU time.

## SQL, acknowledgement and VFS work

Product SQL trace excludes the diagnostic library's nested readback queries.
Separate observer scalar queries were120/reference and25/candidate, with600/125
VM steps; candidate also had one synchronous-assignment exec whose VM count is
not collected here. Native SqlWork VM is unqualified after trace resets; the
following VM numbers are actual trace counters, not EXPLAIN instruction counts.

| Observed work | Phase 4.5 | Phase 7 |
| --- | ---: | ---: |
| Product traced statements | 3,644 | 297 |
| Product trace VM steps | 327,348 | 203,659 |
| Write BEGIN /read BEGIN | 28 /0 | 16 /15 |
| COMMIT /ROLLBACK | 27 /1 | 31 /0 |
| COMMIT sqlite3_step ns | 35,457,916 | 37,528,708 |
| COMMIT sqlite3_reset ns | 0 | 132,830 |
| All API step calls /ns, includes observer work | 5,991 /49,991,368 | 3,393 /48,498,981 |
| All API reset calls /ns | 3,512 /12,631,078 | 297 /8,725,457 |
| VFS xRead calls /requested bytes /ns | 369 /490,835 /366,373 | 269 /956,029 /369,673 |
| VFS xWrite calls /submitted bytes /ns | 10,502 /21,423,104 /33,662,017 | 5,198 /21,283,340 /35,451,701 |
| VFS xSync calls /ns | 0 /0 | 3 /5,503,333 |

Reference VFS writes are main-file only. Candidate main writes were5,196 calls /
21,282,816B /35,405,451ns; journal2 calls /524B /46,250ns. Candidate reads were
main268 calls /956,021B /368,465ns and journal1 /8B /1,208ns. Both WAL and other
classes are zero. Candidate syncs are main2 /5,417,791ns and journal1 /85,542ns;
flags two NORMAL and one FULL. These remaining operations accompany initial
WAL validation/transition, not a WAL payload path during Init. VFS counters cover
the lifecycle; exact phase assignment is source-backed, not a separate VFS phase
counter. VFS calls/bytes are not physical syscall/device-byte measurements.

COMMIT step and trace COMMIT counts agree. Both use native SQLite3.51.0 and
underlying unix VFS3, header176B over native OS-file192B. Dylib full-file archive
hashes differ because their recorded install names include arm output paths.
Sources and normalized compiler/link flags match, as do executable text,
constant/string and interpose sections; the [comparison receipt](checks/memoryoff1000-comparison-diagnostic2/comparison.json)
records that binary-equivalence check and all full archive identities. Neither
artifact is falsely relabeled as byte-identical.

## What this changes

The active engine can achieve comparable1,000-file implementation speed under
MEMORY/OFF in this qualified diagnostic. Fewer statements/VM steps are real, and
COMMIT execution is close to the reference when mutating work uses that profile.
The earlier durable VFS report showed approximately42.24 MB of write submissions
and34 sync operations, versus approximately21.28 MB and three initial-profile
syncs here. Those are different observer/source windows: use them as evidence
of the write/acknowledgement mechanism, not an exact paired durability residual
or additive timing decomposition.

The next optimization target remains successful durable publication/write work,
with actual per-publication byte/row occupancy needed before changing boundaries.
Changing journal/sync is not an optimization treatment for production. Preserve
WAL/FULL, fullfsync/checkpoint policy, bounds, atomicity and unknown-outcome
refusal. The result improves confidence in the1,000-file engine; durable parity
and all seven required selections remain unproven.

Latest ordinary Init1000 remains FAIL:126,033,958 /229,371,791ns (1.819920557x).
Other Init100/10000/100000 and retained-history stride10/3/1 remain NOT_RUN at the
latest source. Historical failure receipts remain unchanged. Revision1 observer
refusal and proof-copy failure are retained in the [prospective contract](SQLITE-MEMORY-OFF-DIAGNOSTIC-CONTRACT.md);
they do not supply a qualified matched pair. No production speed arm was rerun.

Tooling commit9150e7612 and correction96327681f both retain production137867 /
delta0. The correction's final source-identity assertion initially compared
snapshot directory names and failed while the commit command continued. A
subsequent exact whole-crates/manifest/counter blob check confirmed committed
source identity and the unchanged total. The [LOC audit](checks/memoryoff1000-comparison-diagnostic2/loc-audit.json)
retains that omission and confirmation; it does not invent a successful earlier
assertion. This report/evidence update is production delta0, with exact final
staged/committed identity confirmation required. No push/PR/merge.

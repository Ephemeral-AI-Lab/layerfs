# Phase-4 G3 incremental materialization baseline v1

- Status: **G3 PASS / G4 READY — v13 STATICALLY CLOSED AND TERMINALLY SEALED**
- Date: 2026-08-22
- Stage disposition: **G3 COMPLETE; Attempt B retained as a G4 candidate**
- Starting HEAD: `d79f0e0e2582d1bc491410224fec2b6cef7482e9`
- Source-set SHA-256:
  `3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`
- Methodology-set SHA-256:
  `c6d04dd87b0cfc3794533e475be72e1564a87d142816c0360a6126179e0b6f5a`
- Release executable SHA-256:
  `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`

This baseline records the successful, statically closed, and terminally sealed
G3-v13 mechanism screen. It retains one
destination-authority-gated, same-open native seed/clone/patch mechanism for G4.
It does not replace the accepted SQLite writer-memory optimization baseline,
promote a production API, or constitute G4 acceptance.

## Candidate boundary

Attempt A—trusting an ordinary user-editable destination from a receipt or
filesystem hints—was a static NO-GO. Exact current-byte authority would require
a full read/authentication without an exclusive mutation service, persisted
authority, or gap-free journal.

Attempt B instead holds a benchmark-private protected seed through a read-only,
unlinked descriptor for one process/open lifetime. Qualification binds store,
validation authority, profile, integrity epoch, generation, receipt transition,
parent/target roots, destination and seed identities, operation serials, nonce,
and mutation continuity. The mutable destination is not a payload source.

On a qualified same-size route, the candidate clones the protected seed to a
private temp, authenticates/proves only the declared changed ranges, consumes a
single-use permit after clone identity verification, patches those ranges,
syncs data and metadata, and atomically publishes with directory sync. Invalid
authority, external mutation, count change, missing qualification inputs, or
clone failure use complete authenticated fallback with zero permit consumption.
A symlink/wrong-kind destination is a typed preflight error. Ambiguous rename
outcome is freshly reconciled to exactly target/new or prior/old.

## Retained campaign result

The once-only v13 campaign ran one offline release build and exactly nine rows
with zero reruns. Primary and independent analyzers both returned `PASS` and the
same normalized ledger.

| Scenario | Size B | Route/result/state | Defining direct work | Operation ns | Max RSS B | Q high/terminal |
|---|---:|---|---|---:|---:|---:|
| qualified no-op | 10485760 | qualified-noop/success/new | clone 1; payload/canonical-auth/reconstruction/patch/fallback 0 | 993791 | 16465920 | 0/0 |
| one-byte patch | 104857600 | qualified-patch/success/new | clone 1; canonical auth 22551 B; patch 1 B | 3414166 | 16515072 | 28283/0 |
| one-MiB patch | 10485760 | qualified-patch/success/new | clone 1; canonical auth 1086013 B; patch 1048576 B | 2926167 | 16678912 | 1059893/0 |
| invalid authority | 1048576 | complete-fallback/success/new | validation 0/1; reconstruct/write 1048576 B | 3684250 | 8454144 | 24081/0 |
| external mutation | 1048576 | complete-fallback/success/new | reconstruct/write 1048576 B | 4360042 | 8486912 | 24081/0 |
| symlink substitution | 1048576 | typed-rejection/typed-error/old | `NativeDestinationSymlink`; authority/seed-authority, permit, SQL/BLOB/canonical-auth/reconstruction, clone/copy/patch/fallback, temp/sync/rename/reconciliation counters 0; verification still 1048576 B | 7666 | 8273920 | 0/0 |
| count change | 1048576 | complete-fallback/success/new | reconstruct/write 1048577 B | 3837542 | 8601600 | 24081/0 |
| before-publication fault | 1048576 | qualified-patch/typed-error/old | canonical auth 21882 B; patch 1 B; temp removed | 602166 | 8503296 | 7762/0 |
| lost acknowledgement | 1048576 | qualified-patch/success/new | patch 1 B; target reconciliation compared 1048576 B; fixed 32768-B compare buffer charged | 3123083 | 8290304 | 56849/0 |

Campaign wall was **17,722,050,000 ns**. Candidate operation timers summed to
**22,948,873 ns**. The maximum isolated pre-delete allocation was
**440,541,184 bytes**, below 512 MiB. Cumulative allocation was
**552,169,472 bytes**, a descriptive sum across separately retired row roots,
not simultaneous peak usage. All nine rows were byte- and mode-exact with
terminal Q zero and zero temp/seed residue.

Cleanup contains exactly nine durable PREPARE/COMPLETE pairs in schedule order,
used descriptor-relative anchored no-follow deletion of the frozen inventory,
and left every row root and WORK absent. Publication rows recorded data and
metadata sync; ordinary publication completed rename and directory sync. The
before-publication fault retained old bytes. Lost acknowledgement performed a
fully charged fresh reconciliation and returned target/new.

## Evidence identity

| Evidence | SHA-256 |
|---|---|
| Campaign | `70be7a26ada3f0c378faed061819338620cc43708c3e5226aff3a360b5eb7e88` |
| Raw JSONL | `3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c` |
| Primary analysis | `b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7` |
| Independent recomputation | `2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace` |
| Normalized ledger | `19a3fd5ab1d5fb4dc00ffe396de1d118bfc38706d85c4009a974033d0a4010a1` |
| Cleanup summary | `ccb6edddfff96929e15e16b455a92df81314b7be3499143a8f92ebb27e87890e` |
| Row-cleanup JSONL | `1b9e4fbdcb87c686dca9e6852fa535e6db68445114ef83c4e3c24017e172e506` |

v10 retained nine exact campaign rows but could not seal: workspace static
closure found Cargo auto-discovering the G3 module as a standalone binary. v11
repaired the manifest and sealed, but independent post-seal review later found
four acceptance defects: reconciliation-Q undercharge, cleanup custody armed
after temp/seed creation, cleanup masking publication failure, and an
incomplete canonical changed-range proof. Its sealed bytes remain valid
historical evidence under an additive `HISTORICAL REVISE` disposition. v12
repaired those product defects but was frozen as a zero-row PREEXEC REVISE for
five evidence-protocol gaps. v13 retained the repaired source, closed those
five gaps, and freshly ran all nine rows without reuse.

Full evidence and the append-only v1–v13 revision history are in the
[G3 report](../experiments/g3-incremental-materialization/G3-REPORT.md). The
controlling retained campaign record is
[CAMPAIGN-v13.json](../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/CAMPAIGN-v13.json).

## Static closure and terminal seal

| Sealed evidence | SHA-256 |
|---|---|
| [Static closure](../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/STATIC-CLOSURE-v13.json) | `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531` |
| [67-entry payload manifest](../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/PAYLOAD-MANIFEST-v13.tsv) | `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49` |
| [Terminal record](../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json) | `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e` |
| [Terminal verification](../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-VERIFICATION-v13.txt) | `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6` |

Static closure passed **157 workspace tests with 1 ignored and 0 failed**,
**15 focused G3 tests**, clippy with warnings denied, rustfmt, diff check, and
custody review. The finalizer sealed 67 manifested payload entries and
independently verified the terminal hashes, modes, manifest closure, lock
absence, and symlink absence.

## Qualification limits and next use

Physical I/O is unavailable because logical clone/write counters do not derive
it. OS cache warmth is unavailable because the selected APIs do not report
residency. Device stable-media completion is unavailable because fsync dispatch
does not prove it. Logical/apparent/allocated storage, wall, RSS, and Q must not
be substituted for those facts.

The mechanism is benchmark-private, non-production, native to the measured
macOS/APFS environment, and protected only by operation-local,
same-open/process-lifetime seed and namespace custody. It persists no replayable
destination receipt, makes no claim against a malicious same-UID namespace
adversary or cross-process authority, and makes no FUSE/VFS, SDK, projection,
migration, or product-integration commitment.

G4 is roadmap-ready for planning around this exact lane in the compact
1/10/100-MiB materialization matrix but remains UNSTARTED. Integration remains
conditional on a later G4 acceptance result. The accepted
optimization baseline remains the SQLite writer-memory `cache_spill=2000`
baseline, while this file is the sealed G3 mechanism baseline. Phase 4 remains
incomplete, with G5 and G6 pending.

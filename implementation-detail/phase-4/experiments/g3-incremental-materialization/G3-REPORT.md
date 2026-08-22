# G3 incremental materialization report

Status: **G3 PASS / G4 READY — v13 STATICALLY CLOSED AND TERMINALLY SEALED**

Date: 2026-08-22

G3 retained the G2-selected destination-authority-gated incremental
materialization mechanism. The once-only v13 campaign completed all nine rows,
both analyzers returned `PASS` with an identical normalized ledger,
and every schedule, authority, route, direct-counter, fallback, publication,
timer, exactness, resource, cleanup, and custody gate passed.

This report was refreshed after the v13 static closure and final read-only seal.
The campaign, 67-entry payload manifest, PASS terminal, and independent terminal
verification now form the sealed G3 package. G4 is READY for planning but is
UNSTARTED.

## Decision

### Attempt A: static NO-GO

Attempt A—qualifying an ordinary user-editable destination from a receipt or
filesystem hints—was rejected before implementation. The repository has no
exclusive destination mutation service, persisted destination-authority state,
or gap-free filesystem journal. A receipt authenticates the store/head tuple,
not the current bytes at an ordinary destination. Inode, path, size, timestamps,
mode, sidecars, kqueue/FSEvents, and prior publication receipts are invalidation
hints rather than byte authority. Exact qualification would still require a
full destination read/authentication, defeating the proposed avoided work.

Evidence: [Attempt A static NO-GO](v1/ATTEMPT-A-STATIC-NO-GO-v1.md).

### Attempt B: retained as a G4 candidate

Attempt B uses a benchmark-private, same-open protected native seed. The seed is
opened read-only, unlinked, retained by descriptor, and bound to the exact
authority that produced the parent root. The mutable destination is never used
as payload source or byte authority.

The retained operation is:

```text
fresh no-follow destination/type preflight
  -> validate bound logical and seed authority
  -> if same size/count and clone is usable:
       clone protected seed to private temp
       authenticate only declared changed canonical ranges
       prove every byte difference lies within those ranges
       consume one single-use permit
       patch exact ranges
     else:
       complete authenticated reconstruction fallback
  -> data fsync + metadata application + metadata fsync
  -> no-follow atomic rename + parent-directory fsync
  -> exact old/new reconciliation on ambiguous publication
  -> exact temp/seed cleanup
```

This is the same Attempt-B mechanism across the versioned protocols. v13
retains the v12 product repairs: reconciliation charges the fixed 32-KiB
comparison buffer; every temp/seed name has cleanup custody before creation;
publication failure remains primary when cleanup also fails; and permit
minting consumes a stable canonical parent/target changed-range proof. v13
then repairs only the five evidence-protocol defects that made frozen v12 a
zero-row PREEXEC REVISE. No optimization mechanisms were stacked.

## Trust and correctness contract

### Authority

A qualified operation binds the exact store instance, validation authority,
profile, integrity epoch, generation, receipt transition, parent and target
roots, destination identity, open/mutation/publication serials, operation,
nonce, and seed identity. The retained directory and seed descriptors are
revalidated. The seed must remain regular, read-only, unlinked, exact-length,
and identity-equal to its binding. A permit is single-use and is consumed only
after a cloned candidate has been reopened no-follow and identity-verified.

The measured qualified no-op performed one successful native clone with zero
payload SQL/BLOB/authentication, reconstruction, patch, or fallback work. The
100-MiB one-byte row authenticated 22,551 canonical bytes and patched exactly
one byte without complete reconstruction. The 10-MiB one-MiB row authenticated
1,086,013 canonical bytes and patched exactly 1,048,576 bytes without complete
reconstruction.

### Fallback and error precedence

Invalid authority, destination invalidation, and count change took complete
authenticated fallback, consumed zero permits, and reconstructed exactly the
output length. Clone failure is also specified as complete fallback with zero
permit consumption, although it is a negative self-check rather than a measured
v13 schedule row.

A symlink or wrong-kind final component has preflight precedence. The measured
symlink substitution returned `NativeDestinationSymlink` with zero authority
and seed-authority reads/validations, permit consumption, mapping/object/payload
SQL, canonical BLOB/authentication/reconstruction, clone/copy/patch/fallback,
temp creation, sync, rename, and reconciliation counters. Independent output
verification still read 1,048,576 bytes; the row was not laundered into
fallback.

### Durability and publication faults

Every ordinary published row recorded one data sync, metadata sync, rename, and
directory sync. The before-publication fault returned
`InjectedBeforePublication`, performed no rename or directory sync, retained the
old destination, and removed its temp. The lost-ack row reconciled to
`target/new`: it freshly compared 1,048,576 destination bytes with 1,048,576
reconstructed source bytes, separately charged 59 SQL queries, 59 SQL rows, 59
BLOB reads, and 1,051,531 authenticated canonical bytes, and then completed the
directory durability boundary. Its reconciliation Q high-water was 56,849
bytes, including the fixed 32-KiB comparison buffer, and terminal Q was zero.

The generic reconciliation contract accepts exactly `target/new` or
`prior/old`. It requires stable pre/post descriptor identity plus a final
no-follow name observation. Stable exact bytes different from target and prior
map to `PublicationConflict`; read, wrong-kind, descriptor/name instability, or
otherwise uncertain observation maps to `AmbiguousDurability`.

### Cleanup and evidence durability

Each child stream and enriched raw row was file-fsynced before cleanup PREPARE.
Cleanup retained the WORK and row descriptors, inventoried no-follow path,
kind, device/inode, mode, link count, size, mtime/ctime, and allocation, and
fsynced a PREPARE before deletion. It revalidated the exact inventory, deleted
only that set through descriptor-relative unlink/rmdir operations, fsynced WORK,
proved the row absent, and fsynced a bound COMPLETE before the next row.

`ROW-CLEANUP-v13.jsonl` contains exactly nine ordered PREPARE/COMPLETE pairs.
All row roots and WORK are absent, `broad_deletion=false`, and the deletion
method is
`descriptor-relative-openat-fstatat-unlinkat-rmdir-no-follow-exact-inventory-v1`.

## Append-only protocol history

No failed or superseded row was reused.

| Version | Execution | Disposition | Exact reason |
|---|---:|---|---|
| v1 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | Incomplete source custody, followed by a pre-execution candidate audit covering retry, authority/fallback, publication/reconciliation, cleanup, permit-consumption, descriptor-race, and global-fault defects. |
| v2 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | Source-copy consumers did not all enforce exact safe copy paths, containment, size/hash/mode, and distinct identity. |
| v3 | 6 once-only rows | `REVISE` | All six rows passed, but retained row roots accumulated 526,688 KiB and crossed the 512-MiB limit even though the largest isolated row was 430,216 KiB. |
| v4 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | Cleanup traversal/delete set was not descriptor-anchored, PREPARE was not durable before removal, and finalizer enforcement was incomplete. |
| v5 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | Stdout/stderr/raw evidence was not file- and parent-fsynced before durable PREPARE; chronology/failure wording overclaimed durability. |
| v6 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | Finalizer mechanically targeted nonexistent version-matched G2 evidence and its self-check omitted the live sealed anchor. |
| v7 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | The hashed counter dictionary contradicted the code by naming a nonexistent G2 dependency instead of sealed G2-v5. |
| v8 | 9 once-only rows | `REVISE` | Build, rows, and cleanup passed; the copied primary analyzer then resolved its repository one level too high and failed before JSON. |
| v9 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | Actual analysis relocation was repaired, but copied analyzer `--self-check` still used the one-level-high global repository. |
| v10 | 9 once-only rows | `CAMPAIGN PASS / STATIC REVISE` | All rows, cleanup, and both analyzers passed, but workspace static closure exposed Cargo auto-discovery of the G3 module as a standalone binary. v10 was not sealed. |
| v11 | 9 once-only rows | `HISTORICAL REVISE` | The sealed bytes and descriptive observations remain intact, but post-seal independent review found reconciliation-Q undercharge, two guard-after-create gaps, cleanup masking publication failure, and no stable canonical-root changed-range proof bound into permit minting. |
| v12 | 0 rows | `REVISE_BEFORE_MEASUREMENT` | The four product repairs passed source review, but frozen methodology lacked exact G2 analyzer pinning, exact static argv/test-name checks, an external premeasurement anchor, per-row custody equality, and the actual selected child-environment record. |
| v13 | 9 once-only rows | **TERMINAL PASS** | Retained v12 source; repaired only the five evidence-protocol defects; fresh externally anchored source/method/dry-run freeze, binary, rows, analyses, cleanup, static closure, 67-entry manifest, terminal seal, and independent verification all passed. |

The v3 and v8 result roots remain historical failed-attempt evidence. The v10
root remains valid campaign evidence with a failed static closure. Versions v1,
v2, v4, v5, v6, v7, v9, and v12 are zero-row protocol revisions. The sealed
v11 package retains integrity but has the additive historical-REVISE
disposition recorded in
[G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md](G3-V11-POST-SEAL-REAUDIT-DISPOSITION-v1.md).
No v11 or v12 row, binary, source copy, dry run, analysis, closure, or terminal
artifact was reused by v13.

## Exact v13 row evidence

All numeric counters below are raw integer observations. `auth S/F` is authority
validation successes/failures. `payload Q/R/BLOB` is payload SQL queries, SQL
rows, and canonical BLOB reads. Byte counters are bytes. `clone S/F` is clone
successes/failures.

### Route and primary direct counters

| # | Scenario | Route / result | Auth S/F | Permit | Payload Q/R/BLOB | Canonical auth B | Source reconstructed B | Clone S/F | Changed ranges/B | Patch calls/B | Fallback calls/B |
|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | `qualified-noop` | `qualified-noop` / `success` | 1/0 | 1 | 0/0/0 | 0 | 0 | 1/0 | 0/0 | 0/0 | 0/0 |
| 2 | `qualified-one-byte` | `qualified-patch` / `success` | 1/0 | 1 | 4/4/4 | 22551 | 0 | 1/0 | 1/1 | 1/1 | 0/0 |
| 3 | `qualified-one-mib` | `qualified-patch` / `success` | 1/0 | 1 | 59/59/59 | 1086013 | 0 | 1/0 | 1/1048576 | 1/1048576 | 0/0 |
| 4 | `invalid-authority` | `complete-fallback` / `success` | 0/1 | 0 | 59/59/59 | 1051531 | 1048576 | 0/0 | 0/0 | 0/0 | 1/1048576 |
| 5 | `external-mutation` | `complete-fallback` / `success` | 1/0 | 0 | 59/59/59 | 1051531 | 1048576 | 0/0 | 0/0 | 0/0 | 1/1048576 |
| 6 | `symlink-substitution` | `typed-rejection` / `typed-error` | 0/0 | 0 | 0/0/0 | 0 | 0 | 0/0 | 0/0 | 0/0 | 0/0 |
| 7 | `count-change` | `complete-fallback` / `success` | 1/0 | 0 | 59/59/59 | 1051532 | 1048577 | 0/0 | 0/0 | 0/0 | 1/1048577 |
| 8 | `before-publication-fault` | `qualified-patch` / `typed-error` | 1/0 | 1 | 3/3/3 | 21882 | 0 | 1/0 | 1/1 | 1/1 | 0/0 |
| 9 | `lost-ack` | `qualified-patch` / `success` | 1/0 | 1 | 3/3/3 | 21882 | 0 | 1/0 | 1/1 | 1/1 | 0/0 |

The exact row-6 error is `NativeDestinationSymlink`; row 8 is
`InjectedBeforePublication`. All other rows have a null error.

### Reconciliation and durability counters

`SQL Q/R/BLOB` in this table is reconciliation-only. `sync D/M/R/Dir` is data
sync, metadata sync, rename, and directory sync. A published temp has
create/remove `1/0` because rename consumed its name; row 8 explicitly removed
the unpublished temp.

| # | Reconciliation outcome | Calls | SQL Q/R/BLOB | Reconciled auth B | Source compared B | Destination read B | Sync D/M/R/Dir | Temp create/remove | Seed create/remove | State |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 1 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 1/1/1/1 | 1/0 | 1/1 | `new` |
| 2 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 1/1/1/1 | 1/0 | 1/1 | `new` |
| 3 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 1/1/1/1 | 1/0 | 1/1 | `new` |
| 4 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 1/1/1/1 | 1/0 | 1/1 | `new` |
| 5 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 1/1/1/1 | 1/0 | 1/1 | `new` |
| 6 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 0/0/0/0 | 0/0 | 0/0 | `old` |
| 7 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 1/1/1/1 | 1/0 | 1/1 | `new` |
| 8 | `not-needed` | 0 | 0/0/0 | 0 | 0 | 0 | 1/1/0/0 | 1/1 | 1/1 | `old` |
| 9 | `target` | 1 | 59/59/59 | 1051531 | 1048576 | 1048576 | 1/1/1/1 | 1/0 | 1/1 | `new` |

### Exact timer equations

All entries are nanoseconds. Columns are preflight (`PF`), qualification
(`Qual`), payload preparation (`Payload`), data sync (`DS`), metadata (`Meta`),
metadata sync (`MS`), rename (`Ren`), directory sync (`Dir`), reconciliation
(`Rec`), cleanup (`Clean`), attributed (`Attr`), unattributed (`Unattr`), and
operation total (`Total`). For every row, `Attr` is the sum of the ten component
timers and `Total = Attr + Unattr`.

| # | PF | Qual | Payload | DS | Meta | MS | Ren | Dir | Rec | Clean | Attr | Unattr | Total |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 11625 | 24625 | 351083 | 3500 | 50708 | 1417 | 547500 | 1166 | 0 | 1459 | 993083 | 708 | 993791 |
| 2 | 9041 | 31833 | 1269833 | 124833 | 40708 | 2125 | 1924583 | 1542 | 0 | 7875 | 3412373 | 1793 | 3414166 |
| 3 | 9375 | 22834 | 2266917 | 244458 | 20000 | 1791 | 353459 | 1292 | 0 | 4625 | 2924751 | 1416 | 2926167 |
| 4 | 3583 | 23916 | 3157208 | 223833 | 32958 | 1000 | 232625 | 1250 | 0 | 6083 | 3682456 | 1794 | 3684250 |
| 5 | 10500 | 72458 | 3782375 | 221250 | 27375 | 750 | 237917 | 1250 | 0 | 4459 | 4358334 | 1708 | 4360042 |
| 6 | 7000 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 7000 | 666 | 7666 |
| 7 | 4750 | 21500 | 3336875 | 221416 | 23625 | 625 | 222583 | 1167 | 0 | 3458 | 3835999 | 1543 | 3837542 |
| 8 | 2959 | 21416 | 414375 | 69667 | 16042 | 1667 | 0 | 0 | 0 | 74125 | 600251 | 1915 | 602166 |
| 9 | 5541 | 24541 | 437083 | 67000 | 25916 | 1917 | 201667 | 2250 | 2346333 | 8875 | 3121123 | 1960 | 3123083 |

### Exact RSS, Q, storage, and output gates

External real seconds include child preparation and verification outside the
candidate's operation timer; they are not substituted for `operation_total_ns`.
Allocated storage is each isolated row's pre-delete snapshot.

| # | Operation ns | External real s | Max RSS B | Q high/terminal | Isolated allocated B | Exact bytes/mode | Residue temp/seed |
|---:|---:|---:|---:|---:|---:|---|---:|
| 1 | 993791 | 2.11 | 16465920 | 0/0 | 42401792 | true/true | 0/0 |
| 2 | 3414166 | 4.24 | 16515072 | 28283/0 | 440541184 | true/true | 0/0 |
| 3 | 2926167 | 0.71 | 16678912 | 1059893/0 | 43544576 | true/true | 0/0 |
| 4 | 3684250 | 0.08 | 8454144 | 24081/0 | 4280320 | true/true | 0/0 |
| 5 | 4360042 | 0.08 | 8486912 | 24081/0 | 4280320 | true/true | 0/0 |
| 6 | 7666 | 0.06 | 8273920 | 0/0 | 4280320 | true/true | 0/0 |
| 7 | 3837542 | 0.07 | 8601600 | 24081/0 | 4280320 | true/true | 0/0 |
| 8 | 602166 | 0.08 | 8503296 | 7762/0 | 4280320 | true/true | 0/0 |
| 9 | 3123083 | 0.08 | 8290304 | 56849/0 | 4280320 | true/true | 0/0 |

Campaign wall was **17,722,050,000 ns** under 59 seconds. The sum of candidate
operation timers was **22,948,873 ns** under 20 seconds; every individual
operation was under five seconds. The maximum isolated allocation was
**440,541,184 bytes** under 512 MiB. The **552,169,472-byte** cumulative
allocation is descriptive—the nine row roots were retired separately and never
coexisted at that cumulative size. All nine outputs were byte- and mode-exact,
terminal Q was zero, and temp/seed residue was zero.

## Measurement limitations

Every row reports these limitations verbatim:

- `physical_io_status`: `Unavailable: physical I/O is not derivable from logical clone and write counters`;
- `cache_warmth_status`: `Unavailable: selected APIs do not identify OS cache residency`;
- `stable_media_status`: `Unavailable: fsync dispatch does not prove device stable-media completion`.

Accordingly, clone, patch, fallback, logical/apparent/allocated storage, wall,
RSS, and Q counters do not establish physical device I/O, a cold-cache state,
or device stable-media completion.

## Custody and exact evidence hashes

| Evidence | SHA-256 |
|---|---|
| Source set | `3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d` |
| Methodology set | `c6d04dd87b0cfc3794533e475be72e1564a87d142816c0360a6126179e0b6f5a` |
| Frozen release executable | `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e` |
| Campaign | `70be7a26ada3f0c378faed061819338620cc43708c3e5226aff3a360b5eb7e88` |
| Raw JSONL | `3d2b40da82f612441cf1af88ee89f2d8c79b139c75818d6c7e2a5488cbad956c` |
| Primary analysis | `b28003f59dcf3fbfa6a585762d70cdc0beae0b4c81ec51904327d388452820d7` |
| Independent recomputation | `2f137bb1116d1637656d1c89777dcb9e1291e04899f6710a000e5a6933419ace` |
| Normalized ledger | `19a3fd5ab1d5fb4dc00ffe396de1d118bfc38706d85c4009a974033d0a4010a1` |
| Cleanup summary | `ccb6edddfff96929e15e16b455a92df81314b7be3499143a8f92ebb27e87890e` |
| Row-cleanup JSONL | `1b9e4fbdcb87c686dca9e6852fa535e6db68445114ef83c4e3c24017e172e506` |

The source set covers exactly `Cargo.lock`, the engine `Cargo.toml`, the main
benchmark source, and the G3 module. The executable was built exactly once with
the frozen offline release command, copied `0500`, and used for every row. The
retained manifest topology sets `autobins=false` and explicitly declares the one
intended `phase4_create_edit_benchmark` binary; read-only Cargo metadata proves
that the G3 module is not a standalone target. Both analyzers agree
byte-for-byte on the normalized ledger, whose failure list is empty and whose
13 gates are true.

Retained evidence:

- [campaign record](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/CAMPAIGN-v13.json);
- [raw rows](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/rows-v13/G3-V13-RAW.jsonl);
- [primary analysis](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-PRIMARY-ANALYSIS-v13.json);
- [independent recomputation](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/G3-INDEPENDENT-RECOMPUTATION-v13.json);
- [cleanup summary](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/CLEANUP-v13.json);
- [row cleanup events](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/ROW-CLEANUP-v13.jsonl);
- [v13 frozen contract](v13/PROSPECTIVE-G3-INCREMENTAL-MATERIALIZATION-v13.md);
- [static closure](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/STATIC-CLOSURE-v13.json);
- [67-entry payload manifest](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/PAYLOAD-MANIFEST-v13.tsv);
- [terminal record](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json);
- [independent terminal verification](../../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-VERIFICATION-v13.txt).

## Static closure and terminal seal

| Sealed evidence | SHA-256 |
|---|---|
| Static closure | `cbefce3c9ad384105acbf2c81e0a0d4304c8c7eb118d16d874ad6913de9e3531` |
| 67-entry payload manifest | `1581f8f4b890237c6c04f17b79baf445067461767146c916b2d4df80c3030a49` |
| Terminal record | `1230187c702455eb3cf15aaa7d02197ebc5f60b196d08c072e524a87107a828e` |
| Terminal verification | `a9d06860828f14304b7f6fc1ef35146577e7ba770bacc4d4c428250d60169dd6` |

The static closure records **157 workspace tests passed, 1 ignored, 0
failed**, plus **15 focused G3 tests**, clippy with warnings denied, rustfmt, diff
check, and custody review all `PASS`. The finalizer verified campaign,
analyses, normalized ledger, source/method/binary custody, cleanup, sealed
G2-v5 anchors, and static closure; it then sealed the result and independently
verified hashes, modes, manifest closure, lock absence, and symlink absence.

## G4 eligibility and retained limitations

G3's mechanism screen exit condition is met: the exact qualified routes avoided
complete reconstruction, all distrust/fault routes preserved explicit fallback
or typed-error behavior, and the operation, Q/RSS, isolated-storage, exactness,
residue, and once-only gates passed, and the v13 package is now statically
closed and terminally sealed. G4 is therefore **READY** for planning at the
roadmap level but remains **UNSTARTED**. The mechanism is not accepted for
integration unless the later G4 acceptance matrix passes.

The retained mechanism has intentionally narrow scope:

- it is benchmark-private and is not a production materialization API;
- it depends on native macOS/APFS clone and rename behavior, with complete
  fallback when clone qualification fails;
- its seed and destination authority is operation-local under same-open and
  process-lifetime custody; there is no persistent replayable destination
  receipt or cross-process authority;
- the cleanup namespace is private under runner process custody; the residual
  POSIX stat/unlink micro-race is not claimed safe against a malicious same-UID
  process with direct namespace access;
- it does not establish cold physical I/O, cache residency, or device stable
  media;
- it does not authorize FUSE/VFS, SDK, projection, migration, or broad product
  integration.

G4 must test the retained candidate as one explicit materialization lane, keep
authenticated full fallback as the correctness control, preserve the above
limitations, and make no production or platform-general claim from this G3
campaign. Only a mechanism accepted by the still-unstarted G4 stage may be
considered for later integration. Phase 4 remains incomplete; G5 and G6 remain
pending.

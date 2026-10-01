# Six Phase 6 metadata workload adaptations: v2 results

> **Status: Research; informative and not a product contract.**
> 2026-10-01; #294 under #293; [prospective specification](COHORT-V2.md),
> frozen at `b115fe2b34e27bd08ae1a216c8c0f3c64a23b090` before implementation.
> Measured prototype: `32dfbee814147dfb60716b0af5ead5bcd71c4812`.

## Disposition and exact scope

All six once-only component adaptations completed and passed their independent
metadata/source/byte and owner-drain proofs. Cursor field and <=512-row
transaction targets passed for these six schedules. The initial v1 fragmented
bulk overwrite/truncate failures remain FAIL; this collection does not fix or
repeat them. Physical memory and product qualification remain INCOMPLETE.

The owner selected the Phase 6 SQLite prototype for the six shapes from
`Implement R1 infrastructure for #287`. [Historical source capture](source-captures/r1-six-case-cohort.json)
retains literal commands and workload hashes. Original Phase 5 SDK/Exec/Commit/
complete/proof receipts remain unchanged. These new metadata phases cannot be
compared as full Commit/Exec speed ratios to those receipts.

The large file is an immutable source reference to 10MiB of A, with small actual
replacement source values. The timed path does not read/chunk/hash/compress/
pack/upload 10MiB, create a mount, execute commands, certify a canonical namespace,
publish a Branch or install canonical G2. Metadata completion clears changed IDs,
releases the submission and retires eligible old versions; it is not C5 install
or full cleanup. Source GC and the one deliberately retained namespace orphan
remain NOT_RUN. Unchanged package/base entries are omitted as declared.

## Recorded observations

Single raw observations, milliseconds. **Every numerical row is cache
INELIGIBLE; performance_claim=false, admission_eligible=false.** Prototype engine
opening/configuration is reported separately; the complete command includes
launch/open/work/close/observation/output. Prelude and retained-view delivery
remain inside operation/complete command. Proof work is separate.

| Case | Final direct metadata mutation ms | Final metadata prepare ms | Final metadata completion ms | Complete prototype command ms | Separate proof work ms | Proof |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Clean retained writes | no mutation | 0.159 | 0.086 | 227.133 | 38.251 | PASS |
| One edit, retained writes | 0.127 | 4.148 | 0.140 | 237.491 | 40.909 | PASS |
| 4KiB overwrite | 0.266 | 0.312 | 0.265 | 6.842 | 7.863 | PASS |
| 67-descendant namespace | 0.443 | 0.433 | 0.236 | 7.171 | 1.570 | PASS |
| 270 components | 11.345 | 7.261 | 0.558 | 25.459 | 9.675 | PASS |
| 128 files | 6.029 | 2.100 | 0.377 | 15.182 | 2.027 | PASS |

The first two commands include the complete 4,097-write prelude, its first
metadata submission and the retained-view pin, rather than moving that work into
setup. Full logical bytes are reconstructed only in the separate verifier.

| Retained case | Prelude mutation ms | Prelude metadata prepare ms | Prelude completion ms | Retained-view delivery ms | Pin release/retirement ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Clean retained writes | 208.637 | 4.657 | 0.300 | 4.024 | 0.075 |
| One edit, retained writes | 216.895 | 4.588 | 0.242 | 4.156 | 0.153 |

All six complete commands returned within the declared15s watchdog; every
separate verifier returned within9.5s. No timeout/worker/cache limit was changed,
no unchanged case was rerun and no original R1 or v1 row was sampled again.

## Work and remaining scaling issue

| Case | Final changed inodes | Final catalog records | Final extents | Maximum changed rows / transaction | Timed fullscan / sort steps |
| --- | ---: | ---: | ---: | ---: | ---: |
| Clean retained writes | 0 | 0 | 8195 | 6 | 0 / 0 |
| One edit, retained writes | 1 | 8197 | 8196 | 6 | 0 / 0 |
| 4KiB overwrite | 1 | 4 | 3 | 6 | 0 / 0 |
| 67-descendant namespace | 5 | 73 | 0 | 7 | 0 / 0 |
| 270 components | 272 | 543 | 0 | 128 | 0 / 0 |
| 128 files | 130 | 259 | 0 | 128 | 0 / 0 |

- Clean retained writes: after the prelude's changed state is completed, the
  final selected changed catalog has zero inodes/records. No 4,097-write replay.
- One edit: final preparation selects one changed inode but emits **8,197 catalog
  records**, including **8,196 current extents**. The window is bounded, but work
  still depends on the file's whole current fragmentation. These timings do not
  establish efficient single-edit scaling at much larger extent populations.
  Preserve this fact when choosing opaque predecessor/shared subtree construction.
- Both retained proofs check the exact prelude and held G1 catalog after the
  successor edit, plus complete10MiB logical bytes. The pinned selection prevents
  deletion of old inode/extent versions until the pin releases. At final return,
  changed IDs, submission slots, pins and obsolete eligible versions are zero.
- Namespace67 changes five inode facts, emits73 catalog records and preserves72
  final bindings. Five moves use parent/component keys with no descendant path
  rewrite. Replaced inode106 and its source remain as one explicit retained orphan;
  capture cleanup PASS does not mean that orphan/source GC is implemented.
- Components270 creates270 directories and the terminal file, with272 changed
  inode facts and543 selected records. Many128 creates128 files plus its directory,
  with130 changed inode facts and259 selected records. No canonical parent-index
  certification, ordinary FUSE lookup/reference accounting or permission proof.
- All instrumented workloads had zero SQLite fullscan/sort steps. That diagnoses
  these SQL query routes, not the entire product's graph/validation complexity.
  Engine cache -512KiB is an allowance, not whole-process containment.

## Source, checks and custody

[Identity](evidence/cohort-v2/identity.json), [summary](evidence/cohort-v2/summary.json),
[full artifact hashes](evidence/cohort-v2/manifest.json) and individual case folders
retain exact phases/counts/statuses/commands and separate proof output. Small
published files are exact copies. Large sample databases and result catalogs
remain in the owned ignored raw folder, covered by the manifest.
The measured binary was archived before collection by its exact SHA256.
The collector required clean committed source and product-parity dependency
checks. A worktree-local nonblocking lock covered collection; no local build
crossed a timer. Prepared masters were acquired once and independently byte-copied;
no cold-cache claim is made. Release builds used root ARMv8 flags.

Actual reproduction command at measured source (do not overwrite/recollect the
existing output to replace a row):

```sh
python3 core/benchmark/phase6-metadata/run.py --cohort-v2 --output benchmark-results/phase6-cohort/run-v2 --masters benchmark-results/phase6-cohort/masters-v2
```

Owning checks: locked release build, release Clippy `-D warnings`, fmt, Python
compile, all five independent oracle corruption/leak/extra-result tests,
dependency parity, status/local links, staged diff and exact raw manifest passed.
The initial Clippy hex-string allocation finding was fixed before collection.
These harness checks do not substitute for full Core product checks, which were
NOT_RUN because product source is unchanged. No CI or retired preflight.

Native measured engine remains SQLite3.51.0 MEMORY/OFF, allocation tracking
unavailable (`DEFAULT_MEMSTATUS=0`); null/status fields are retained. Independent
readonly proof engine reports SQLite3.51.2 in these receipts. OS/file-cache/
Linux/cgroup physical memory, source/CAS/MinIO/delta, full Exec/Commit/READY/Branch/
current-G2 installation, process failure/Unknown, generalized POSIX and source/
orphan GC remain NOT_RUN. No #288 campaign or release admission occurred.

## Next

Measure the counted preparation work for one changed range while retaining an
opaque immutable predecessor or shared extent subtree, and independently prove
exact selected bytes/zeros/pins/retirement. Preserve the v1 bulk-transaction
failures and obtain a separately declared physical resource observation profile.
The six passing metadata adaptations do not remove current product file/body/
count limits or qualify unlimited-size streaming.

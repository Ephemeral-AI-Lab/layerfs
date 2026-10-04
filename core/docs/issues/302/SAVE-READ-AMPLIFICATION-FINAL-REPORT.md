# Save read amplification final evidence,2026-10-04

The demand-local Save change and owner-approved explicit SQLite group rows reduce
Save VFS requested bytes below the reference in the qualified Disposable histories.
The product is frozen ated6807c6f; the proof30 Python harness is009fe8770.
Implementation and schema contract: [SQLITE-PHYSICAL-GROUP-ROWS.md](SQLITE-PHYSICAL-GROUP-ROWS.md).
Complete receipt identities, arithmetic inputs, SHA/length manifest audits and all
historical failures: [checks/group-rows-final2/comparison.json](checks/group-rows-final2/comparison.json).
The prior final1 index is preserved.

## Measured results

Each eligible case/arm has one cold performance sample. Stride1 reference shares
its original v1 performance explicitly: no new performance sample. Both arms use
unchanged release/locked native binaries, fixture/observer/dependency identities,
one construction worker and source/per-state database cold contracts. Values are
disjoint phase counters; VFS requested bytes are not physical device bytes.
Times below are product comparison clocks; proof is separate. Allocation is final
observed database/WAL/SHM allocation, not peak memory or an unobserved storage peak.

| States | Reference Save VFS B | Candidate Save VFS B | Reduction | Product seconds ref / candidate | Proof seconds ref / candidate | Allocation B ref / candidate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 17 | 812,164,935 | 689,015,411 | 15.1631% | 33.627739834 / 31.637190250 | 3.084929667 / 4.432817584 | 52,473,856 / 50,626,560 |
| 53 | 3,140,227,191 | 2,236,230,650 | 28.7876% | 67.989210750 / 66.849558917 | 6.562325333 / 6.630626458 | 65,142,784 / 64,118,784 |
| 157 | 11,939,753,738 | 8,302,371,102 | 30.4645% | 182.203124208 / 186.952407542 | 16.664784125 / 17.779967792 | 86,179,840 / 84,926,464 |

All three candidate proofs pass, source and per-state database cold checks pass,
and cleanup passes. Stride1 candidate proof is17.779967792s, with the same157
states/904143paths,921174logical bytes and5347088acquired bytes. Stride1 Save
requests fall30.4645%; product time rises2.6066%, within the unchanged10% margin.
The candidate complete command is198.353209333s/300s and final allocation
84,926,464B is below its original92,342,273B ceiling.

The numeric speed guard is10*candidate<=11*reference. Complete-command ceilings
stay60/170/300s; original storage ceilings remain unchanged. The sealed JSON index
records complete command walls, budget outcomes, proofs, cold/cleanup status,
filesystem/construction/acquisition deltas, logical acquisitions and native hashes.
Reference157 performance remains182.203124208s, complete command195.892744500s;
it is never relabeled as a new sample at the proof30 harness identity.

## Mechanism and preserved contracts

Save no longer performs an eager wave-wide physical prewalk. Existing batched
locator membership and per-object resolution authenticate each required physical
base/dependency/canonical chain. Read-side prefetch remains unchanged. The real
caller trace attributes10439acquisitions and778636499VFS requested bytes inside
BLOB reads to eager discovery. The unchanged37-pack reuse fixture drops113to74
acquisitions under existing sparse-first/whole-on-sibling promotion.

Prewalk removal alone gives1,039,486,710Save VFS B/16462acquisitions in17states,
42.20% below the directory treatment but27.99% above reference. A labelled cold
SQLite diagnostic holds32768returned bytes constant: row262144offset0 costs40624
VFS B,offset196608costs237424B; an independent32768-byte row atoffset0costs40624B.
This isolates overflow navigation without a device-throughput or LayerFS speed
claim. Official SQLite btree implementation also uses cursor-local overflow
navigation: https://raw.githubusercontent.com/sqlite/sqlite/master/src/btree.c.

Schema2 is explicit creation-only `SqlitePackLayout::GroupRows`, selected through
`PersistenceConfig.with_sqlite_pack_layout`; monolithic schema1 remains default.
Stored versions deterministically select open behavior. No automatic migration.
Original control bytes and immutable complete encoded groups occupy separate rows;
whole reads reassemble exact original pack bytes and full SHA. Original PackInfo,
CIDs, serialized format and publication visibility remain unchanged. Metadata is
validated before BLOB I/O. Physical fanout and binding bytes are charged
prospectively under existing publication caps; late failure rolls back atomically.
No extra workers, transaction-held cursors, hidden cache, private staging waiver,
new dependency or third-party patch.2MiB/4096encoded,512KiB decoded/value and
32MiB/4096output bounds and canonical/dependency verification remain unchanged.

The count diagnostic yields689,015,411Save VFS B/16462acquisitions and28,993,070
filesystem VFS B/679acquisitions. Pack descriptors, canonical locators/value
catalogue and roots match. Allocation50,626,560B is1,032,192B above schema1,
2.08%, and below the54,278,964B ceiling. Count-clock53.505810083s includes stack
instrumentation and is not used as a speed comparison.

## Proof extension and omissions

Owner requested more time after12.003190125s reference proof timeout. Prospective
15s v2 also timed out at15.007248791s. Both raw receipts and the v1 candidate
NOT_RUN remain unchanged. Prospectively versioned30s v3 reference proof passes
at16.664784125s, checking157states/904143paths,921174logical content bytes and
17878185acquired bytes under unchanged8MiB/32MiB caps. Original successful
performance is shared only after immutable receipt, native binary/compilation,
fixture/observer/product/dependency identity checks; new_performance_samples=0.
Reproof command: `runner.py reprove-reference --run
benchmark-results/fs-bench-pro/issue302-group-rows-history157-reference1 --case
phase7-sqlite-disposable-history-stride1-group-rows-v3 --out <fresh-output>`.
Candidate uses those root pins in the normal sole runner with the same v3 case.
No unchanged performance arm is rerun. Initial dirty-source count launch and
retained-input proof launcher refused before child execution; logs remain.
Initial test fixture/format/assertion failures were diagnosed and repaired;
covering checks ran after changes.

Durable group-row histories, explicit group-row Init, all-seven release admission,
exhaustive retained-store whole-pack audit and schema1stride1 after the schema2
decision remain NOT_RUN. Bounded content proof is not an exhaustive payload audit.
PostgreSQL/MinIO M4pause and their later milestones remain unchanged. Darwin wait4
RSS is per-child lifetime, never phase-only heap/cgroup evidence. No CI or retired
aggregate preflight was run; no push, merge, release or original-worktree edit.

## Verification and production LOC

At the frozen product: `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked
--workspace --all-targets` PASS509tests/97targets/0ignored; corresponding workspace
all-target Clippy with-Dwarnings PASS; fmt--all--check PASS;
`core/tools/check_product_boundary.py` PASS453files;23tool selftestsPASS.
Six actual SQLite group-row tests cover versioned sparse read, exact all-five-lane
whole reassembly/SHA, accessed/unread corruption scope, extent refusal before
BLOB I/O, locator rollback and physical row overflow beforeBEGIN. Observer6tests,
generated-reference/proof/runner/census guards PASS. Product checks were not
repeated after benchmark-only proof supervision changes. Proof-extension3tests
and prospective registry guard PASS before proof30 freeze.

Every commit compares exact first-parent/staged production snapshots using
`tools/production_loc.py` (SHA256c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb),
nonblank/noncomment Rust src/runtime SQL, excluding tests/docs/tooling/generated
artifacts. Reference subtotal remains65417. Product-path equality establishes
benchmark/docs-only deltas with the inherited exact count.

| Commit | Production before | After | Delta | Core after |
| --- | ---: | ---: | ---: | ---: |
|2b39dfe50|139635|139635|0|74218|
|26a453961|139635|139635|0|74218|
|b1151f732|139635|139620|-15|74203|
|509d9f164|139620|139620|0|74203|
|ed6807c6f|139620|140027|+407|74610|
|dab048e99|140027|140027|0|74610|
|721277d61|140027|140027|0|74610|
|009fe8770|140027|140027|0|74610|

Net isolated product change from4032cfe75 is+392production LOC, with schema2
physical layout/validation accounting for the growth. Final evidence-only commit
keeps140027anddelta0; its exact hash is recorded in the handoff.

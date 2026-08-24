# Stage 1.2 Specification — npm / Developer Workspace Benchmark

Status: **prospective controlling specification for Stage 1.2; implementation
not started; measurement not authorized**
Authority: controls only the Stage 1.2 workload, fixture, readiness and
disposition; [10 — handoff freeze](10-handoff-freeze.md),
[17 — Stage 1.0 closure](17-stage1-closure.md), and the accepted Stage 1.1
terminal artifact remain authoritative for product correctness and custody
Purpose: exercise an ordinary APFS code workspace through real shell, Node,
npm, search, edit, build, capture, reopen, history, and rematerialization.

Sequence: **accepted Stage 1.1 -> Stage 1.2 -> mounted Stage Two**.
The historical filename number does not define execution order: `poc/16` is
Stage 1.1 and this `poc/15` document is Stage 1.2.

Entry requires a settled Stage 1.1 source and independently accepted terminal
disposition with no open correctness, durability, authentication, resource,
population or custody failure. Stage 1.2 does not reopen A02 or inherit a
Stage 1.1 performance claim.

Explicit boundary:

```text
ordinary APFS directory + explicit full-scan capture only
no mounted filesystem
no FSKit, macFUSE, File Provider or write interception
no DeltaGit watcher/journal implementation
no Stage Two implementation or measurement
```

## 1. Budget

```text
workspace hard maximum       314,572,800 logical bytes
largest regular file         <= 104,857,600 bytes
expected prepared source     ~155 MiB
expected installed+built     ~175–190 MiB
complete workflows           3
preferred campaign wall      <60 seconds
hard diagnostic stop         <=120 seconds
network                      forbidden
```

Workspace logical size is checked after materialization, npm install, edit,
build, capture, and final rematerialization. LayerFS Store bytes are reported
separately and do not excuse a native workspace above 300 MiB.

## 2. Deterministic workspace

| Class | Count | Bytes each | Approximate total |
|---|---:|---:|---:|
| TypeScript source | 1,024 | 8 KiB | 8 MiB |
| Tests | 256 | 8 KiB | 2 MiB |
| JSON/data | 128 | 64 KiB | 8 MiB |
| Binary assets | 48 | 2 MiB | 96 MiB |
| Tool/wasm-style binaries | 8 | 4 MiB | 32 MiB |
| Markdown/docs | 16 | 128 KiB | 2 MiB |
| Local package source | `16 × 32` | 8 KiB | 4 MiB |
| Tarballs/config/scripts | fixed | bounded | ~4 MiB |
| `node_modules` | 16 local packages | bounded | 4–6 MiB |
| `dist` | deterministic build | bounded | <=16 MiB |

Topology:

```text
~176 directories
16 empty directories
2 executable scripts
2 relative symlinks
1 dangling symlink
1 regular-file hard-link group with 2 paths
3 ordinary supported xattrs
1 FinderInfo/resource-fork fixture
1 ACL fixture
1 BSD hidden-flag fixture
0 unsupported special files in measured population
```

Unsupported files remain focused correctness tests, not benchmark inputs.

## 3. Persisted reusable masters

```text
target/layerfs-stage1-fixtures/workspace-v1/
├── source-store/               sealed Rsource
├── source-native/              sealed source oracle
├── npm-cache/                  sealed offline cache
├── packages/                   sealed local package tarballs
├── package-lock.json
└── master.json
```

Preparation occurs once:

```text
generate deterministic project
-> create 16 local npm packages and fixed DAG
-> npm pack once
-> seed offline cache and lockfile
-> normalize metadata to fixed epoch
-> import Rsource
-> close, verify, hash, seal 0444/0555
```

Each run resets by APFS clone into a fresh owned attempt directory. Never
regenerate the project, reinstall from network, or clean a shared workspace in
place.

Targets:

```text
repeated master admission <=2 s preferred
attempt reset             <=5 s hard
one-time preparation      reported separately, not repeated
```

## 4. Offline npm contract

Packages:

```text
@layerfs-fixture/pkg-00 ... @layerfs-fixture/pkg-15
version 1.0.0
fixed dependency DAG
fixed package.json key order
fixed source bytes and mtimes
```

Measured command:

```bash
<sealed-absolute-npm> ci --offline --ignore-scripts --no-audit --no-fund \
  --cache <attempt-local-cloned-cache>
```

Environment:

```text
TZ=UTC
LC_ALL=C
npm_config_offline=true
npm_config_audit=false
npm_config_fund=false
npm_config_update_notifier=false
npm_config_cache=<attempt-local-cloned-cache>
```

A missing cache object is a failure. It is never permission to access the
network. The sealed absolute Node executable used by npm scripts is first in a
minimal frozen `PATH`; no other mutable tool lookup is allowed.

Master admission resolves and freezes absolute paths, versions, byte sizes,
and SHA-256 hashes for Node, npm, `rg`, Bash, and `/usr/bin/time`. Every measured
invocation uses those exact absolute paths; mutable `PATH` lookup is forbidden.

## 5. Real operation sequence

```text
clone sealed Store/cache
-> reopen Rsource
-> cold materialize ordinary APFS workspace
-> npm ci --offline
-> normalize npm-created mtimes to the fixed epoch
-> capture Rdeps
-> run deterministic multi-file edit
-> build
-> normalize edit/build-created mtimes to the fixed epoch
-> rg search seven times
-> capture Redit
-> reopen
-> fork retained Rsource/Rdeps/Redit
-> switch main Redit -> Rsource -> Rdeps -> Redit
-> direct canonical spot reads after each switch
-> fresh materialize Redit
-> npm test --offline
-> exact tree/metadata oracle
-> terminal cleanup/resource proof
```

Deterministic edit:

```text
replace 4 KiB in one middle source module
create one 8 KiB test
delete one obsolete module
rename one module
update 12 fixed imports
change one JSON value
change executable mode on one tool
preserve hard-link group and symlink oracle
```

Build scripts use Node standard library only and produce a deterministic
`dist/` no larger than 16 MiB.

Search:

```bash
/absolute/sealed/rg --sort path -n --no-heading --color never \
  'STAGE1_NEEDLE_[0-9]{4}' src test packages
```

Expected match count and ordered-output digest are frozen. Timing includes
`rg --sort path`; the oracle hashes that exact ordered stdout.

## 6. Campaign rows

| ID | Operation | Samples | Required proof |
|---|---|---:|---|
| B00 | Master admission | 1 | complete manifest/root/tool validation |
| B01 | APFS reset | 3 | clone success, distinct inode, master unchanged |
| B02 | Reopen source | 3 | exact `Rsource` |
| B03 | Cold materialize | 3 | exact ~155 MiB tree; bytes/s + files/s |
| B04 | Offline npm install | 3 | exit 0; no network; package/link/bin oracle |
| B05 | Normalize + capture dependencies | 3 | fixed mtimes; full-scan class; one transaction/COMMIT; `Rdeps` |
| B06 | Real edit script | 3 | exit 0; exact native oracle |
| B07 | Build | 3 | exact `dist` digest; <=16 MiB |
| B08 | Search | `3 × 7 = 21` | exact sorted count/digest; wall/user/system/RSS |
| B09 | Normalize + capture edited/build | 3 | fixed mtimes; full-scan class; exact `Redit` |
| B10 | Reopen edited | 3 | exact `Redit`; no native authority assumed |
| B11 | Fork retained roots | 9 refs | zero payload-byte copies |
| B12 | Root switching | 9 moves | one ref transaction/COMMIT; no payload copy |
| B13 | Canonical spot reads | 9 | exact bytes from selected root |
| B14 | Final materialize | 3 | exact independent native oracle |
| B15 | Offline test | 3 | exit 0 |
| B16 | Terminal resources | 3 | Q/FD/child/temp/cache-attempt zero/baseline |

The full external capture in B05/B09 remains correct and linear. Stage One
does not pretend npm or arbitrary editor writes provide trusted changed-range
receipts.

## 7. Measurement boundaries

Every workflow:

```text
complete_workflow_wall
  = reset
  + reopen
  + materialize
  + npm
  + normalize_after_npm
  + capture_deps
  + edit
  + build
  + normalize_after_build
  + search
  + capture_edit
  + reopen_and_root_ops
  + final_materialize
  + test
  + oracle
  + cleanup
  + timer_residual
```

Every external command records:

```text
exact argv/cwd/environment class
exit status
wall/user/system time
maximum RSS via /usr/bin/time -l
stdout/stderr digest and retained failure streams
```

Each normalization reports exact files visited/changed and wall time. It is
inside complete workflow wall but outside npm/edit/build operation timers.

LayerFS operation counters are deltas around SDK calls. Whole-process RSS is
not relabeled as operation-owned Q.

## 8. External capture accounting

Report separately:

```text
paths enumerated
unique regular inodes
current bytes digested
changed current bytes CDC-scanned
uncached prior bytes streamed
metadata calls/bytes
hard-link scratch rows/bytes
native read calls/bytes
SQLite statements/transactions/COMMITs
```

Do not collapse these passes into one invented `bytes_scanned` number.

Complexity:

```text
Theta(unique current regular bytes for digest
    + changed current bytes reread for CDC/CAS
    + uncached prior logical bytes for prior digest
    + represented metadata bytes)
+ sum_directories O(D_j log D_j)
+ O(paths log inode_count)
+ disk-backed indexed grouping/enumeration
bounded memory; disk-backed path/hard-link scratch
```

## 9. Correctness gates

| Gate | PASS |
|---|---|
| Size | workspace <=300 MiB at every checkpoint; largest file <=100 MiB |
| Network | offline local packages only; no fallback |
| Native tree | exact paths/kinds/bytes/modes/symlinks/hard links/selected metadata |
| npm | expected package inventory and exit 0 |
| Build | expected bounded `dist` digest and exit 0 |
| Search | exact ordered-output digest and count |
| Capture | honest full scan, exact root, one transaction/COMMIT |
| Unchanged files | prior FileStateRoot retained; digest pass but zero changed-file CDC bytes |
| Hard links | unchanged surviving group retains allowed prior InodeId/topology |
| Metadata-only | content root retained |
| Changed CDC | `native_changed_cdc_pass_bytes` equals changed current regular bytes only |
| Reopen | exact final head; no carried native authority |
| History | `Rsource`, `Rdeps`, `Redit` immutable and directly readable |
| Root switch | ref-only, zero payload copy |
| Determinism | roots repeat across all three cloned attempts |
| Terminal | children reaped; Q/FD/connections/temp/residue zero/baseline |

## 10. Statistics and feasibility

For one-based ordered values `x1 <= ... <= xn`:

```text
heavy n=3:       p50=x2,  p95=x3
search n=21:     p50=x11, p95=x20
root/read n=9:   p50=x5,  p95=x9
```

Every case retains raw ordered observations, minimum, maximum, range, p50,
p95, operation wall, and complete wall. Three-sample p95 is a diagnostic
maximum, not an SLO.

Before rows, zero-row readiness binds all fixed counts and observed reset/tool
startup receipts:

```text
forecast_complete_wall
  = 3*reset
  + 3*(materialize + npm + normalize1 + capture1
       + edit + build + normalize2 + 7*search + capture2
       + reopen/root_ops + final_materialize + test + oracle + cleanup)
  + artifact_write
```

The forecast must leave adequate reserve below 120 seconds. A forecast above
60 seconds is a recorded preferred-target risk; it is not permission to alter
the population after measurement.

## 11. Expected performance

These are planning ranges, not acceptance evidence:

| Component | Expected three-workflow total |
|---|---:|
| APFS resets | 1–3 s |
| Cold materializations | 7–14 s |
| Offline npm installs | 3–8 s |
| Six full captures | 8–16 s |
| Edit/build/test | 4–9 s |
| Search/reopen/root operations | 2–5 s |
| Final materializations/oracles | 7–14 s |
| Cleanup/artifacts | 3–6 s |
| Preferred campaign | **35–65 s** |
| Hard stop | **120 s** |

The preferred goal remains below 60 seconds. The upper planning edge is not a
license to exceed the 120-second hard stop or reduce the population after a
failure.

Per-operation reports:

```text
large-file bytes/s
small-file files/s
operation and complete wall
native/canonical/SQLite/scratch attribution
minimum/p50/p95/maximum/range
raw sorted samples
```

## 12. Artifacts

Use the same compact schema and files as the single-file campaign:

```text
target/layerfs-stage1-workspace-<timestamp>/
├── environment.json
├── master.json
├── readiness.json          exact admitted external receipt copy
├── schedule.json
├── rows.jsonl
├── summary.json
├── summary.md
├── campaign-time.txt
└── stderr.txt              only when nonempty/failure
```

No source copy, npm registry mirror, giant fixture committed to Git, Python
runner, Criterion suite, benchmark lock, or per-operation manifest tree.

## 13. Fixed population and disposition

The B00–B16 table contains exactly 85 measured rows:

```text
B00 admission                                      1
B01–B07, three rows each                          21
B08, seven searches in each of three workflows   21
B09–B10, three rows each                           6
B11–B13, nine ref/move/read rows each             27
B14–B16, three rows each                           9
total                                             85
```

`PASS` requires:

```text
exact 85-row population and three complete workflows
all command, native-tree, canonical-root and retained-history oracles exact
offline npm inventory, build digest, search digest/count and test exit exact
all capture/authentication/transaction equations exact
all resource and cleanup gates pass
roots repeat across the three cloned attempts
complete campaign wall <=120 seconds
fixture, cache and master remain unchanged
```

`REVISE` means every hard correctness, determinism, resource, custody and
120-second gate passes, but report-only timing exposes a worthwhile product
bottleneck or the preferred `<60 s` goal is missed.

`FAIL` includes any byte/path/kind/metadata/root/history/durability/
authentication mismatch, unexpected network access, incomplete population,
resource or hard-time violation, timer/custody defect, escaped child, or
terminal residue. No population, threshold or oracle may be weakened after the
first measured observation.

## 14. Prospective implementation map

```text
tools/layerfs-eval/src/
├── main.rs                   tiny Stage 1.2 command dispatch only
├── stage1_workspace.rs       fixture, schedule, runner, oracles and receipts
└── stage1_fixture.rs         visibility-only reuse of generic seal/reset helpers
```

Expected commands:

```text
layerfs-eval stage1 prepare workspace
layerfs-eval stage1 readiness workspace
layerfs-eval stage1 run workspace <new-run-directory>
```

No product crate, dependency, canonical/SQLite schema, benchmark framework,
Python runner, npm registry mirror, watcher or mount frontend is added for
Stage 1.2. Product-source changes require a concrete correctness defect in an
existing public route and a separate focused proof.

## 15. Execution stages and handoff

| Stage | Work | Exit condition |
|---|---|---|
| W0 | Freeze this specification and exact B00–B16 population | No open workload, tool or oracle choice |
| W1 | Implement/test deterministic project, offline packages and command oracles | Generated project and package DAG match literal model |
| W2 | Prepare and seal the reusable <=300 MiB fixture/cache | Fresh Verified reopen and exact native oracle; preparation recorded once |
| W3 | Implement three-workflow runner, full-scan accounting and receipts | Focused evaluator checks pass; zero network |
| W4 | One workspace fmt/check/test/clippy closure | Zero failures and warnings |
| W5 | One release build and zero-row readiness | Exact source/executable/tool/fixture hashes; 85-row schedule; zero rows |
| W6 | One campaign with hard <=120 s stop | Immutable PASS/REVISE/FAIL artifact |
| W7 | Independent terminal audit | Honest disposition; no mounted Stage Two work started |

An accepted W7 disposition closes Stage One verification and makes mounted
Stage Two eligible. It does not implement or qualify a mount frontend.

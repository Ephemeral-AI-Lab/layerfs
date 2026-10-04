# Owner-directed SQLite closure results, 2026-10-05

**Requested round complete: 22 fresh release/locked arms, 11 matched pairs.** Durable retained history **3/3** and Disposable Init **4/4** joint gates PASS. Durable Init **4/4** joint gates FAIL only the retained **1.10×** reference speed allowance; every functional proof, cold source, allocation, cleanup and doubled absolute deadline passes. Owner explicitly confirmed keeping **1.10×** after reviewing the four Durable Init misses. No all-seven Durable qualification or release admission is claimed. Issue #302 closes at owner request with these limits; original PostgreSQL/MinIO M5/retirement scope remains paused/unqualified.

## Frozen method and proof simplification

Prospective cases/limits were committed before sampling in 7544c0ea6; active harness/examples freeze 702f7a38f. Product source remains ba6499a61; later checkpoint commits change documentation only. Pinned reference product 7edddbdb8 is unmodified Phase4.5 MEMORY/OFF. Candidate Durable readback is WAL/synchronous FULL/fullfsync1/checkpoint-fullfsync1, Disposable MEMORY/OFF; both mmap0/cache-2048/page4096/foreign-keys1/temp-store2. Default public Monolithic layout for Init; explicit creation-only GroupRowsIndexed schema3 for retained history. Source-tree commits differ between checkpoint groups, while each pair matches source tree, product/harness/dependency seals, fixture and canonical roots. Exact identities and commands in[compact statistics](checks/owner-closure1/) and original immutable receipts. These are direct C1/C2/C5 component measurements; public SDK call, daemon, Docker image and FUSE timing are N/A. The reference Init calls its pinned Service route; candidate calls public layerfs_project::init. Both complete product clocks include fresh creation/open, real import, required finalization/checkpoint/close; Init-only clocks reported separately. No lifecycle subtraction.

One sample/case/arm, no repeats or best-of. Identity-checked existing corpus and all four prepared masters reused; masters copied once into owned worktree as independent ordinary byte copies, not APFS clones. Each performance arm invalidates/checks all relevant source pages; retained history additionally checks every database state boundary. All attestations record `resident_after=0`. History construction uses 1 worker, Init retains its permitted 4-worker exception; environment construction limit is 1. Build incremental/worktree-local or seal-checked immutable binary reuse, all release/locked. Host macOS ARM64; actual competing processes are retained in row evidence, not silently excluded. CPU and RSS below are external driver-child lifetime values, not isolated phase memory or a bounded-memory claim.

Proof uses one coordinated path: immutable closed-owner census and pinned reference roots, existing all-state structure and bounded representative content. Permit only regular exclusive single-link main plus exact empty WAL/32,768-byte SHM; reject journals, pending WAL frames, incomplete pairs, aliases and mutations. Preserve original identity/hash before/after. Native Durable history proof uses an independent main/WAL/SHM copy with a 64 KiB buffer, counted aggregate Store ceiling and no oversized growth; F_NOCACHE hints plus whole-copy mincore check records `resident_after=0`. Copy/census/native work all charged inside separate proof timer. Native authentication≤ 8 MiB and acquired content≤ 32 MiB unchanged; copy bytes recorded separately. No product dependency, unsafe, durability or canonical contract change. Proof walls are functional completion bounds, separate from product speed; they are not cold I/O throughput or full-payload claims. Historical sidecar/prototype failures and every old verdict remain unchanged.

## Durable retained history

New cases `phase7-sqlite-history-stride{10,3,1}-group-rows-indexed-v2`. Doubled complete-command caps **120 / 340 / 600 s**; separate proof **24 / 24 / 60 s**. Relative speed stays `10*candidate <= 11*reference`. Original target misses remain distinct from approved allocation-ceiling PASS.

|Stride / states|Reference product s|Durable product s|Difference %|Reference / candidate complete command s|Reference / candidate proof s|Candidate allocated B / ceiling|Joint gate|
|---|---:|---:|---:|---|---|---|---|
|10 / 17|32.630223375|32.356111208|-0.840056054|46.596506375 / 44.885041583|3.018135167 / 3.332632167|50724864 / 54278964|PASS|
|3 / 53|69.063050500|69.114259292|+0.074147886|82.421191000 / 81.654476917|6.765155625 / 6.761100583|64278528 / 70427034|PASS|
|1 / 157|182.112717542|180.930537000|-0.649147714|196.315354542 / 194.080112084|16.976626000 / 17.494827625|85204992 / 92342273|PASS|

|Stride|Original target B|Actual minus target B / %|Independent proof paths / selected content paths|Authenticated / acquired content B|Proof copy B (main + sidecars)|
|---:|---:|---|---|---|---|
|10|49344512|1380352 / +2.797376940%|101477 / 67|970326 / 3301773|50724864|
|3|64024576|253952 / +0.396647687%|306861 / 66|921174 / 3689369|64278528|
|1|83947520|1257472 / +1.497926323%|904143 / 66|921174 / 5347088|85204992|

Each combined candidate main contains both C2/C5, with the retained SHM counted in total. Reference split C2/C5 main/WAL/SHM allocations are individually recorded in JSON. Every state has checked custody/independent roots and canonical inventory. Original stricter targets all FAIL; approved 54,278,964  / 70,427,034  / 92,342,273 B ceilings PASS. Stride 3 is 0.074% slower in this single window, within the accepted margin; no strict-faster or repeatability claim. Prior Disposable history **3/3** PASS is retained at its original identities; it was not rerun or pooled with this Durable campaign.

## Namespace Init: both profiles, all four tiers

Durable cases `phase7-sqlite-init-{100,1000,10000,100000}-v3`; Disposable cases `phase7-sqlite-disposable-init-{tier}-v2`. Both arm command caps are 30 s, independent proof is 19 s, exactly twice previous 15 / 9.5; internal Init deadline is 30 s. Allocation is candidate final main/WAL/SHM<=matched reference final total. Relative limit remains **1.10×** at owner confirmation.

|Profile / files|Reference product s|Candidate product s|Candidate Init-only s|Difference %|Complete command s /30|Proof s /19|Reference / candidate allocated B|Joint gate|
|---|---:|---:|---:|---:|---:|---:|---|---|
|Durable / 100|0.046350375|0.084598417|0.066843959|+82.519379832|0.625966042|0.514674667|7372800 / 5255168|FAIL|
|Durable / 1000|0.132358959|0.256686959|0.225466625|+93.932440191|0.360132334|0.031156291|23101440 / 20561920|FAIL|
|Durable / 10000|1.659485708|2.630212625|2.610942625|+58.495647918|3.528378000|0.347918500|307265536 / 305053696|FAIL|
|Durable / 100000|6.337916541|7.829636333|7.805888125|+23.536437919|14.702016041|1.102071417|518029312 / 514895872|FAIL|
|Disposable / 100|0.038868625|0.036498125|0.031610583|-6.098749313|0.060265125|0.016218042|7372800 / 5222400|PASS|
|Disposable / 1000|0.144970334|0.128361333|0.116436792|-11.456827436|0.232127750|0.029807834|23101440 / 20520960|PASS|
|Disposable / 10000|1.659867917|1.678383625|1.508029292|+1.115492854|2.548770875|0.339552458|317751296 / 304979968|PASS|
|Disposable / 100000|6.051330333|5.770752667|5.645768291|-4.636627825|13.418440083|1.083196041|518029312 / 514879488|PASS|

Every Init path/kind/directory metadata is checked; full file metadata/content is checked only for the declared deterministic sample. All source-cold, root equality, independent proof, allocation and cleanup gates PASS. All command/proof budgets PASS. No full-payload oracle is claimed. Relative Durable misses are retained, not waived by longer deadlines. The original reference uses MEMORY/OFF; comparing candidate Durable to it is the owner-declared qualification, not an isolated estimate of durability cost.

|Files|Source files bytes|Proof paths|Selected files / selected bytes|
|---:|---:|---:|---|
|100|5000000|102|53 / 3354003|
|1000|20000000|1011|70 / 6430827|
|10000|300000000|10101|72 / 101928859|
|100000|500000000|101001|73 / 200286236|

## External driver resource statistics

CPU covers only the recorded external driver child; complete-command wall also includes cold attestation and recorded lifecycle. RSS is Darwin wait4 lifetime peak, never a phase-only peak. Native internal caches and file-cache bounds cannot be inferred from it.

|Case|Reference CPU s / peak RSS B|Candidate CPU s / peak RSS B|
|---|---|---|
|phase7-sqlite-history-stride10-group-rows-indexed-v2|25.718169000 / 263405568|23.633178000 / 248758272|
|phase7-sqlite-history-stride3-group-rows-indexed-v2|52.837804000 / 286523392|45.978069000 / 265240576|
|phase7-sqlite-history-stride1-group-rows-indexed-v2|131.924143000 / 350322688|120.478607000 / 331825152|
|phase7-sqlite-init-100-v3|0.060686000 / 39043072|0.068501000 / 39780352|
|phase7-sqlite-init-1000-v3|0.181172000 / 48562176|0.242304000 / 54968320|
|phase7-sqlite-init-10000-v3|2.214660000 / 59146240|2.796889000 / 73105408|
|phase7-sqlite-init-100000-v3|10.610603000 / 136265728|11.203864000 / 142049280|
|phase7-sqlite-disposable-init-100-v2|0.046957000 / 37617664|0.048115000 / 39124992|
|phase7-sqlite-disposable-init-1000-v2|0.201621000 / 48054272|0.186398000 / 55001088|
|phase7-sqlite-disposable-init-10000-v2|2.196979000 / 59490304|2.225717000 / 66551808|
|phase7-sqlite-disposable-init-100000-v2|10.182559000 / 136822784|9.980902000 / 142180352|

## Validation, custody and production LOC

37 covering Phase7 Python tests PASS after source-grounded corrections to a stale timer assertion, a fixture missing new identity fields and a missing scratch root. Initial failed logs retained. Changed Init example locked Clippy `-D warnings` PASS after its range-pattern fix; workspace fmt PASS; product boundary checks 456 files PASS. Original failed fmt invocation omitted --all on the virtual workspace and is retained as a command failure, not a source defect. The 523 unique Core tests  / 100 all-target reports and owning workspace/all-target Clippy remain carried from exact unchanged product source; the 23 guard self-tests unchanged and retained. No duplicate product tests, CI or retired aggregate preflight were run. All needed examples build in release/locked normal runner; no third-party modifications.

[Custody audit](checks/owner-closure1/custody.json) rechecks all 22 original run manifests, every retained file hash/length, sealed binary identities via runner reuse/build checks, clean source and pair source/harness/fixture/root identities. Exact commands, raw operands, CPU/RSS, profiles, builds, canonical counts and proof data live in `*-stats.json`. No historical receipt was edited or promoted. No run remained NOT_RUN in the requested 11-case selection; other product families and original PostgreSQL/MinIO timed scope remain NOT_RUN/paused, not omitted failures.

This round commits 7544c0ea6 (plan),702f7a38f (harness/example integration),4833a03b8 (history10/3),f10ce26b4 (history1),dcf1ea6b2 (Durable Init), and the final results/closure checkpoint. Each Production LOC: 140,304 → 140,304 (delta +0), reference 65,417 → 65,417 / core 74,887 → 74,887. Same tools/production_loc.py SHA c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb, exact first-parent/staged archives, identical classification/inline-test exclusions, shipped SQL included, examples/tests/harness/docs excluded. Local branch codex/save-vfs-amplification; no push/PR/merge/release performed.

Issue closure records completed owner-requested investigation/measurement, the explicitly retained Durable Init speed FAILs, and paused original service milestones. It is not all-seven Durable qualification, PostgreSQL/MinIO acceptance or release admission.

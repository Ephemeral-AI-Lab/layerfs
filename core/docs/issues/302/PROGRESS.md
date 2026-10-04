# #302 Phase 7 cluster 1 progress

> **Status:** Current implementation record; no release candidate exists.
> Worktree: `phase7-cluster1-storage/layerfs`; branch
> `codex/phase7-cluster1-storage`; base `7edddbdb8`.

## 2026-10-03 — Plan committed; step 1 blocked

- Plan commit: `2ecb607bf`. The uncommitted packet and amended core rules were
  read from the primary checkout; neither was changed or copied.
- M0 is **BLOCKED**, not complete. The guard and service tool are NOT_RUN.
  No new dependency, crate or image pin has been selected. The owner's prompt
  explicitly authorizes steps 2, 3, 4a and 4b while these questions remain open.
- Exact questions posted to [#302](https://github.com/Ephemeral-AI-Lab/layerfs/issues/302#issuecomment-5968520720):

3. Approve a 14th crate for Init, named `layerfs-project`?
10. May `cargo test` for the engine crates fail when the containers are absent (recommended), rather than skip?
12. Which PostgreSQL major version and MinIO release are pinned?

- Checks run: source/plan review, branch and base identity checks, exact
  first-parent/staged-tree production LOC comparison, committed-tree identity
  confirmation. Product fmt, clippy, package tests, boundary guard and tools
  tests NOT_RUN for the docs-only plan commit; they are required per subsequent
  implementation step. No milestone is marked complete.
- Diagnostics: no operation counters recorded; no product or timed sample run.
- LOC method: `python3 tools/production_loc.py --json --root <snapshot>` using
  identical root counter, Rust comment/inline-test exclusions and shipped SQL,
  with `git archive` snapshots of the first parent and staged tree. Nonproduction
  files may be omitted when materializing snapshots; all tracked Rust/SQL inputs
  in both source scopes are retained. The resulting commit tree and parent are
  checked against the comparison.
- Reference: 65,417 -> 65,417; core: 70,279 -> 70,279;
  core old-path: 6,141 -> 6,141; new-path: 0 -> 0;
  rest-core: 64,138 -> 64,138. No relocation or retirement yet.
- All 13 plan questions remain open. Step 5 additionally needs Q2 and a pinned
  MinIO environment; step 6 needs Q1 and a pinned PostgreSQL environment;
  step 8 needs Q3; step 10 needs Q5–Q9. Step 12 waits for cluster 2 retirement.

Production LOC: 135696 -> 135696 (delta +0)

## 2026-10-03 — Step 2 read seam

Implementation commit is the commit containing this entry (parent `15155598b`).
Row types and the mutex helper relocate with adaptation; signature SQL relocates
from encoding into `sqlite/source.rs`. Both storage paths coexist; none is retired.
The source correction is recorded in implementation-plan §2 and architecture 05.

Checks run once at the implemented tree:

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` — PASS.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — PASS; the server and SDK build.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage -p layerfs-server -p layerfs-sdk` — command PASS: 295 tests passed, 0 failed, 1 existing ignored test.
- `python3 core/tools/check_product_boundary.py` — PASS, 360 Rust/SQL files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS, 10 tests.
- `git diff --check` — PASS.

Gap: `layerfs-server/tests/direct.rs::authenticated_generic_save_accepts_4097_separated_final_runs`
remains ignored by the existing owner instruction, "already passed once; owner
directed no further 4097 runs". It was not re-enabled or run. No milestone is
claimed complete, and this omission is retained for M1 review. No red result,
repair or rerun. No timed sample or new operation counter recorded in this slice.

Counter method is the same first-parent/final-staged-tree root counter as above;
reference 65,417 -> 65,417; core 70,279 -> 70,375;
old-path 6,141 -> 6,184; new-path 0 -> 53; rest-core 64,138 -> 64,138.
The increase reflects the coexistence seam and adapted relocation, not a saving.
All owner questions remain open; M0 is still blocked.

Production LOC: 135696 -> 135792 (delta +96)

## 2026-10-03 — Step 3 ports and authenticated reader

Implementation commit is the commit containing this entry (parent `6db678f3a`).
The two ports, Storage, Reader, bounded locator/catalogue caches, chain prefetch,
whole-pack SHA-256 checks and existing canonical reconstruction are implemented.
External memory engines install old-path physical fixtures through register and
conditional put. No engine crate, client package, service tool or timed sample.
The SHA-256 direct dependency uses the already-locked sha2 0.10.9; no package or
version was added. Provider error mapping relocates to the shared error module.

Final checks:

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` — PASS.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — final PASS; server and SDK build without source changes.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage -- --nocapture` — final PASS: 241 tests, 0 failed, 0 ignored (7 new port tests).
- `python3 core/tools/check_product_boundary.py` — PASS: 370 Rust/SQL files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS: 10 tests.
- `git diff --check` — PASS.

Every red attempt and gap retained:

1. First Clippy FAIL: two unused legacy-provider imports after mapping relocation.
   Diagnosed from compiler output and source; removed those imports.
2. Clippy rerun FAIL: the old server exhaustively matches StorageError and cannot
   accept added variants. Source wins: port originals use the old typed error
   carrier and uncertainty wrapper; the plan records this compatibility correction.
3. First package-test invocation FAIL before execution: external SQLite oracle
   used usize with FromSql. It now reads i64 and converts fixture fields explicitly.
4. An assertion in the repair script matched ObjectMissing as well as the new
   Object variant, stopped before writing, and the shell still started Clippy.
   That extra unchanged-tree check failed again on the exhaustive match and
   FromSql errors. This was an execution mistake and exceeds the requested
   one-fix/one-rerun cadence; it is not omitted or presented as new evidence.
   The script was corrected; subsequent checks ran on the repaired source.
5. Guard/tools checks were repeated after the product repair to cover the final
   tree; the source counts remained 370/10. No unchanged successful product check
   or benchmark arm was rerun. The step-2 owner-directed ignored test remains the
   previously recorded coexistence gap; server/SDK runtime tests were not rerun
   for step 3 (only their all-target Clippy builds).

6. Final staged diff check initially FAIL on a trailing blank line in the copied
   successful test output. The documentation copy's trailing blank lines were
   normalized; the original target log remains intact. The covering diff check
   then passed. No product or test rerun was needed for this presentation repair.

Failure and final outputs are retained under `checks/step3-*.txt` beside this
record. Their wall/profile output is build/test information, not a performance
sample. No check invokes a Phase 7 benchmark selection.

Count diagnostics (printed by the port tests; never timing claims):

| Fixture | policy | locate | metadata pack calls | catalogue calls | payload GETs | payload bytes | metadata body bytes | locator hits/misses | pack hits/misses | forced seals |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| PREFIX, two requests for one dependent across two packs | 1 | 2 | 0 | 0 | 2 | 1563 | 0 | 5 / 3 | 24 / 2 | 0 |
| Pooled leaf, two requests for one 100-row leaf | 1 | 1 | 3 | 1 | 0 | 0 | 9247 | 2 / 2 | 9 / 3 | 0 |

Signatures, reserve, register, PUT and HEAD counts for these reads are zero.
Fixture installation precedes the Storage handle and is not in its diagnostic
counts. PREFIX locate calls are asserted <= 6 (2 * 1 chain level + 4); pooled
metadata pack calls are asserted <= 4. These cases do not prove a bound for every
future workload. Wrong digest, missing payload/base, forged identity and oversized
read demand are refused; an absent locator is retained only for the current demand
so it is not silently queried again and is refreshed on the next demand.

LOC by the same first-parent/final-staged root counter: reference 65,417 -> 65,417;
core 70,375 -> 71,198; old-path 6,184 -> 6,159; new-path 53 -> 840;
rest-core 64,138 -> 64,199. The provider mapping is relocation; both physical
paths remain present. M1 is still in progress (steps 4a/4b outstanding), M0 blocked,
and all owner questions remain open. Real engines and their tests are NOT_RUN.

Production LOC: 135792 -> 136615 (delta +823)

## 2026-10-03 — Explicit owner answers Q2 and Q3

The owner answered in this Codex chat after step-3 commit `d6d159885`:

- Q2: "Approve the planned own S3 client" — the layerfs-s3 client adapter,
  using already-locked sha2 and its own HMAC, is approved.
- Q3: "Approve layerfs-project" — the 14th crate for namespace Init is approved.
- Clarification: PostgreSQL and MinIO are separate servers/services; LayerFS
  connects to them. The client dependency options do not embed either server.

Q1 (PostgreSQL client) is not yet answered. Q10 (missing-service test behavior)
and Q12 (server image pins) still block completion of step 1. Q4–Q9 and Q11/Q13
remain open. No client choice or image pin is inferred from this clarification.
No production change, check, counter or measurement in this answer record.
Reference 65,417 -> 65,417; core 71,198 -> 71,198; old-path 6,159 -> 6,159;
new-path 840 -> 840; rest-core 64,199 -> 64,199; no relocation.
Production LOC: 136615 -> 136615 (delta +0)

## 2026-10-03 — PostgreSQL local/cloud requirement

Owner direction in this chat: "the postgresql solution should support both"
local PostgreSQL and cloud PostgreSQL. The plan's §1.2 and Q1 are amended to
require configurable remote endpoints and verified TLS in the metadata adapter.
The two-service architecture and C2 algorithms/formats remain unchanged. Local
Docker services remain the measurement profile. Cloud live proof is NOT_RUN;
no cloud endpoint or credentials have been supplied, and no cloud result is claimed.

The owner asked about blocking versus async clients but has not selected either.
The recommendation for the current synchronous C2 ports is the complete postgres
client with verified TLS. This is a recommendation, not approval. No client or TLS
dependency was added. Q1 remains open; Q2/Q3 retain their explicit approvals.
Q10/Q12 and the other unanswered questions remain open.

Checks: primary client configuration and TLS documentation read; plan/progress
review. No product check or counter rerun for this docs-only scope amendment.
Reference 65,417 -> 65,417; core 71,198 -> 71,198; old-path 6,159 -> 6,159;
new-path 840 -> 840; rest-core 64,199 -> 64,199; no relocation.
Production LOC: 136615 -> 136615 (delta +0)

## 2026-10-03 — Concrete client and folder proposal

In response to the owner's request for the client choice and file structure,
the proposed Q1 choice is postgres 0.19.14 with postgres-native-tls 0.5.3 and
native-tls 0.2.18. The metadata tree now calls the thin database wrapper
`src/client.rs` (formerly planned as connection.rs), separates TLS/error mapping,
and gives PgMetadata's required trait delegation its own provider.rs. The six
C5 external contract filenames are explicit. The approved own S3 client remains
`layerfs-s3/src/client.rs`. No new crate or client file was created; both client
contracts are already implemented under layerfs-storage/src/port/.

The scratch resolution probe is tooling under target/, not a product dependency
addition. Cargo 1.85.1 generate-lockfile/tree/metadata succeeded for the proposed
set, filtered to aarch64-unknown-linux-musl: 77 selected package versions excluding
probe; 46 new names against core's lock; 15 additional versions of existing names
in this fresh standalone resolution. Actual core unification is not yet resolved.
No selected dependency declared an MSRV above 1.85, but nothing was built.
Linux requires system OpenSSL with this TLS connector; no vendoring or system
package installation. Inventory, probe manifest and lock retained beside this
record as proposal evidence. All native-TLS build/live checks are NOT_RUN.

Q1 and TLS dependencies remain proposed pending explicit approval. Q2/Q3 are
approved. Q10/Q12 remain unanswered. No operation counter or performance sample.
Reference 65,417 -> 65,417; core 71,198 -> 71,198; old-path 6,159 -> 6,159;
new-path 840 -> 840; rest-core 64,199 -> 64,199; no relocation.
Production LOC: 136615 -> 136615 (delta +0)

## 2026-10-03 — M0 complete: step 1 contracts and services

The owner explicitly approved Q1's postgres/native-TLS dependency set, then
instructed the agent to make remaining choices without further questions and
record deviations. The plan's owner-delegated decisions table resolves Q4–Q13;
Q2/Q3 retain prior explicit approval. No historical receipt was relabelled.

Implementation commit is the commit containing this entry (parent `fb46d92f6`).
Changes: dependency graph/source-reference/unsafe guard rules for cluster 1,
seeded forbidden-edge tests, and the owned phase7_services up/reset/down/status
tool. Credentials live only in private ignored target files. Runtime-table SQL
is intentionally introduced at steps 6/7; M0 creates the schema namespace and
bucket. The source-ordering correction is in the plan.

Exact checks run:

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` — PASS.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — PASS.
- `python3 core/tools/check_product_boundary.py` — PASS, 370 product Rust/SQL files and the allowed manifest edges.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — final PASS, 21 tests; seeded forbidden edges and source references fail as required; foreign-resource deletion is refused.
- `python3 core/tools/phase7_services.py reset` — final PASS: cleanup/recreate/readiness/namespace and bucket bootstrap.
- `python3 core/tools/phase7_services.py up` on complete setup — PASS: explicit `owned-services-and-volumes` reuse, identical epoch/images/settings hash.
- All three pre-existing unrelated containers checked — PASS: original images retained and running.
- `git diff --check` — PASS.

Rust package runtime tests NOT_RUN for this tools/docs-only slice (no production
source or Cargo input changed); the covering step-1 tests are the tools suite.
Previous Rust check results are not described as a new provider proof. No engine
crate tests, runtime-schema proof, new C2 operation counter or benchmark sample.
M0 has no skipped covering check; M1 remains in progress at completed steps 2/3.

Every red attempt retained in `checks/step1-*.txt`:

- Initial tools FAIL: engine import scanner omitted digits, so an S3 import was
  missed. Added digits; the seeded test passes.
- Initial live up FAIL before creation: Docker's absent-container error is
  lowercase. Normalize case without swallowing daemon/permission failures.
- Next live up FAIL before creation: absent-network message says "network NAME
  not found". Recognize that specific form and add its fixture; no generic error
  fallback.
- Next live up FAIL during startup readiness: an HTTP connection can close before
  MinIO's health endpoint is ready. PostgreSQL's temporary initialization server
  also must not count as final TCP readiness. Require pg_isready over TCP, handle
  only startup connection failures within the setup polling budget, and add the
  readiness fixture. Existing partial setup was explicitly reset after diagnosis;
  the tool refuses silent partial resumption. Final reset and reuse both PASS.
- These separately revealed tool faults required more check invocations than the
  requested single-repair cadence. The extra attempts are disclosed; none was a
  performance sample or an attempt to select a favorable timing.

Service identity/profile (actual tool output):

- PostgreSQL 17.11, image digest `639ab7ceb90e13123085b741fb31ef493fba25463002f6da665352e7b534b652`.
- MinIO RELEASE.2026-09-22T19-25-18Z, commit `df34868a88cc8c396807e04a7e220810b321bdaa`, image digest `4692462f35d97d7e82c30371d82f057703c5d9489bcae726010594c812f2d285`.
- PostgreSQL settings hash `e5e159f48d94d08910307b94056a010c7f41dbe90c0a588760d6a64ccd644f9d`; fsync/full_page_writes/synchronous_commit on; READ COMMITTED; UTF8; UTC; default statement_timeout=0.
- Each server: 2 CPUs, 536,870,912-byte memory cap, no swap, 256 PIDs. MinIO UID 0,
  compression/encryption/browser off, one named-volume drive; loopback ports only.
- Final successful epoch `9571f8de27bece7c53a399ff`; state/secret files under
  `target/phase7-services/`. Health polling counts are not instrumented; no count
  is invented. The product-provider request diagnostics enter with the engines.

LOC method remains the exact first-parent/final-staged root counter. Reference
65,417 -> 65,417; core 71,198 -> 71,198; old-path 6,159 -> 6,159; new-path 840 ->
840; rest-core 64,199 -> 64,199. Tool/docs/test changes contribute no production
LOC. No relocation in this commit. Step 12 still waits on cluster 2 M9; the
original prior-owner 4097 ignored test remains recorded with step 2.

Production LOC: 136615 -> 136615 (delta +0)


## 2026-10-03 — step 4a complete; M1 remains in progress

Implementation commit is the commit containing this entry (parent `366046097`).
`Storage::begin_save`, `Save`, `SaveSink` and ordinary/payload lane orchestration
use the two ports. The existing pending batch, selector, codec and pack assembler
are shared. Reference-closed units register only after every payload PUT is
acknowledged; no retry or guessed cleanup. Same-save reads, exact reuse and lost
first-wins byte comparison use authenticated reconstruction. Registration units
respect row/byte bounds and prerequisite order. Pooled writes remain step 4b.

Source corrections are in the plan and architecture document: allocation order
cannot prove acyclicity with first-wins rows, and a winning base can have a deeper
chain than its private losing representation. The port resolver explicitly checks
cycles; physical candidates are acknowledged before their chain cost is used for
selection. Same-input framing remains unchanged. Extra prerequisite registrations
are counted, not described as one-register-per-wave. No new dependency in this
slice; the legacy pending batch is shared until its step-12 relocation, not moved.

Exact final checks run:

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — PASS.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — PASS, including unchanged server/SDK coexistence builds.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage -- --nocapture` — final PASS, 254 passed, 0 failed, 0 ignored; includes 13 port-write cases. Complete final output: `checks/step4a-tests-sealed.txt`.
- `python3 core/tools/check_product_boundary.py` — PASS, 379 production files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS, 21 tests.
- `git diff --cached --check` — PASS before commit.

Counts below are diagnostics for the tests' complete stated call sequences,
including their deliberate reads/reuse assertions; they are not phase timing or
performance acceptance. The file parity sequence saves, reads the logical file,
resaves and compares reads through a reopened handle. Its 400,000-byte input:
policy 1, locate 5, read_packs 4, signatures 1, reserve 2, register 2, PUT 3,
GET 6, HEAD 0, payload PUT bytes 401,082, GET bytes 802,164, metadata read bytes
5,008, locator hits/misses 212/48, pack hits/misses 343/10, forced seals 0;
24 inserted, 0 reused in the first save, 4 packs, 401,492 canonical bytes.
The 2,000,000-byte sequence: policy 1, locate 5, read_packs 4, signatures 1,
reserve 2, register 2, PUT 9, GET 21, HEAD 0, PUT bytes 2,003,691, GET bytes
4,764,740, metadata bytes 16,692, locator hits/misses 973/218, pack hits/misses
1,533/25; 109 inserted in the first save, 10 packs, 2,006,677 canonical bytes.
Value-group calls and forced seals are 0 for these file sequences.
The closure fixture records forced seals 1, reserve 3, register 2, PUT 1,
GET 1, PUT/GET bytes 16,689 each. The depth-race fixture records policy 1,
locate 12, signatures 1, reserve 2, register 2, PUT 2, GET 19, HEAD 0, PUT
bytes 40,194, GET bytes 62,795, locator hits/misses 34/13, pack hits/misses
83/19; metadata/value-group requests and forced seals 0. Full counters and
other file rows remain in the append-only check outputs.

Every FAIL/gap:

- First formatting invocation omitted `--all` for the virtual workspace and failed
  to find targets. Final correct formatting/check commands pass.
- Initial Clippy compile FAIL: old `cas::batch` is private. Added a crate-internal
  re-export to share the existing pending buffer. The rerun revealed a test using
  tuple syntax for the existing struct-shaped `UnknownOutcome`; fixed the fixture
  pattern. Later all-target Clippy passes.
- First package run stopped at 205 PASS/1 FAIL: the race fixture installed the
  winner before the final dependent pack reservation, so its asserted locator
  order did not model the intended forward edge. Add the next dependent to seal
  the first one within the existing low-id block. That race case passes.
- The next package run stopped at 207 PASS/1 FAIL: a newly added singleton fixture
  requested a 4 MiB cutoff beyond the existing 1 MiB maximum. Read the existing
  capacity vector and use the supported 1 MiB cutoff; no product bound changed.
  The full package then passed 253 tests.
- Subsequent concurrency review identified the deeper-winning-base hazard and
  required the source correction above. The new focused run had 12 PASS/1 FAIL:
  its extra initial full object supplied an eligible alternate cache candidate,
  so the unchanged selector legitimately chose a PREFIX. Remove that unrelated
  candidate to isolate the depth-limit case. Focused rerun: 13 PASS/0 FAIL; full
  final package: 254 PASS/0 FAIL.
- These compiler/fixture repairs and the new concurrency correction required more
  check invocations than the requested single-repair cadence. All attempts are
  retained; no benchmark arm was sampled or repeated, and no result was erased.
- NOT_RUN: real engine tests (steps 5/6), pooled port writes (4b), Init/harness and
  timed samples (steps 8–11). No timed sample before step 10. Server/SDK runtime
  suites were not rerun for this C2-only slice; their build coexistence is checked.
  The pre-existing owner-directed ignored 4097 server test remains recorded at
  step 2. No skipped final covering check for step 4a.

M1 is not claimed complete: step 4b remains. No owner question is pending for
this slice; choices are recorded under the owner's delegated judgment. Step 12
still requires external cluster 2 M9.

LOC method: `python3 tools/production_loc.py --json --root <snapshot>` on the
exact first-parent and final staged trees, with the same root counter and source
classification. Reference 65,417 -> 65,417; core 71,198 -> 72,349; old-path 6,159
-> 6,160 (the shared-buffer re-export); new-path 840 -> 1,976; rest-core 64,199
-> 64,213. New orchestration coexists with the old implementation. No relocation,
legacy retirement or algorithmic simplification is claimed in this commit.

Production LOC: 136615 -> 137766 (delta +1151)


## 2026-10-03 — M1 complete: steps 2–4b

Implementation commits: `6db678f3a` (read seam), `d6d159885` (ports/reader),
`10954a167` (ordinary writes), `e0bf6a752` (pooled writes). M0 remains complete.
The two C2 paths coexist; server/SDK source and dependencies remain available.
No engine crate exists yet. Source corrections are committed in the plan and
architecture 05; no canonical identity, CDC, deduplication, record grammar or
pack framing changes. Reference closure replaces save-level publication on the
port path. Physical candidates are acknowledged before selection charges the
winning chain's depth/work. The legacy chronology rule remains; port reads check
cycles explicitly. Pooled packing/index/selection use the existing builders.

Exact checks and results:

- Step 2: `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`; `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`; `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage -p layerfs-server -p layerfs-sdk`; `python3 core/tools/check_product_boundary.py`; `python3 -m unittest discover -s core/tools -p 'test_*.py'`; `git diff --check` — PASS, 295 runtime tests/0 FAIL, boundary 360, tools 10. The then-existing ignored test is recorded historically below; its current gap is closed by the targeted check.
- Step 3: the same fmt/all-target locked Clippy/guard/tools commands, and `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage -- --nocapture` — final PASS, 241 runtime tests/0 FAIL/0 ignored, boundary 370, tools 10.
- Step 4a: `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check`; the same locked Clippy and storage test commands — final PASS, 254 runtime tests/0 FAIL/0 ignored, boundary 379, tools 21. Complete file, grouped/wave, PREFIX chain, singleton and C1 edit vectors pass; paired sealed bytes equal the old path's. Failed/uncertain upload cannot register a parent, first-wins forward locators are readable, and a deeper winner is charged before selection.
- Step 4b: the same fmt/locked Clippy/storage package commands — final PASS, 263 runtime tests/0 FAIL/0 ignored, boundary 380, tools 21. Pooled reuse, reopened chains, maximum depth 50, bounded ordinal blocks/index window, lost acknowledgement and malformed/missing groups pass. Counts are in `checks/step4b-tests-final.txt`.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test port_pooled real_filesystem_roots_and_every_emitted_object_match_c1_and_the_old_path -- --nocapture` — PASS, 1 test/0 FAIL/0 ignored, after adding the complete sealed-pack population comparison for both 100/1,000-entry fixtures. The full package pass preceded that test-only assertion addition; the final all-target Clippy/fmt checks cover the added assertion. Roots and every emitted canonical object also match C1 and the old path.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-server --test direct authenticated_generic_save_accepts_4097_separated_final_runs -- --exact --ignored` — PASS, 1 test/0 FAIL/0 ignored. The current owner's no-skipped-milestone requirement plus delegated judgment was applied over the older no-further-runs annotation. It ran once, with no annotation or cluster 2 source change. Historical step-2 receipt remains 295 PASS/1 ignored; it is not relabelled.
- Final `git diff --cached --check` and committed-tree/first-parent identity checks — PASS. No skipped or failing M1 covering check remains.

Every FAIL and repair remains append-only in this PROGRESS record and
`checks/step3-*`, `checks/step4a-*`, `checks/step4b-*`:

- Step 3: unused imports; exhaustive server error matching; SQLite oracle FromSql
  type; repair-script assertion stopping before writing and an extra unchanged
  failing Clippy invocation; trailing blank EOF in a copied check log. Final
  checks pass. Typed errors use the existing carrier; no server enum/API edit.
- Step 4a: virtual-workspace fmt omitted `--all`; private pending-batch access;
  wrong UnknownOutcome fixture pattern; race fixture did not create its asserted
  forward locator; unsupported 4 MiB cutoff fixture; deeper-race fixture offered
  an unrelated eligible cache candidate. Source/fixture corrections are recorded
  with their failed attempts; final checks pass.
- Step 4b initial focused run: 7 PASS/2 FAIL. The missing ordinal was requested
  twice (product defect): cache absence for this demand, without an error-driven
  retry. The other fixture requested unsupported depth 63: use the existing
  supported depth 50. Rerun: 8 PASS/1 FAIL, exposing a product prefetch off-by-one
  at exactly 50 edges. Accept the final empty frontier without increasing the
  bound; full final package: 263 PASS/0 FAIL/0 ignored.
- These independently revealed compiler/fixture/maximum-bound faults exceeded
  the requested single-repair invocation cadence. The extra invocations and their
  outcomes are disclosed; no timed arm was sampled or repeated.

Recorded diagnostics, never timing/acceptance claims:

| Stated complete test sequence | policy | locate | read_packs | catalogue | signatures | reserve (ordinal) | register | PUT/GET/HEAD | metadata returned/written B | pooled packs/groups | reserved pooled directory B | forced seals |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | ---: | ---: |
| Save/read one 100-row pooled leaf | 1 | 2 | 3 | 3 | 1 | 3 (1) | 1 | 0/0/0 | 12405 / 11734 | 1 / 1 | 4096 | 0 |
| Save 1100 one-row leaves across waves, then read first/last | 1 | 4 | 5 | 3 | 1 | 77 (73) | 3 | 0/0/0 | 57172 / 141268 | 5 / 1100 | 20480 | 1 |
| Save 1330 hundred-row leaves across the bounded window, then read last | 1 | 4 | 3 | 3 | 1 | 91 (87) | 7 | 0/0/0 | 21748 / 10475925 | 167 / 1330 | 684032 | 1 |

The filesystem diagnostic is specifically a reopened reader: 100-entry fixture
policy 1/locate 18/read_packs 18/catalogue 2/GET 4, payload bytes 1,171 and metadata
bytes 39,145; 1,000-entry fixture policy 1/locate 39/read_packs 39/catalogue 20/GET
4, payload bytes 1,171 and metadata bytes 379,988. It reports zero writes because
that reader performed none. Other counters, the step-3 read cases, and step-4a
file/closure/race sequences remain in their complete outputs and earlier entries.
No lifetime memory or count is promoted to a phase timing.

NOT_RUN: real S3/PostgreSQL provider tests and cloud TLS proof (steps 5–7), project
Init (8), real storage parity (9), benchmark/harness selections and all timed
samples (10–11). These are future milestone checks, not omissions from M1.
Original Q1–Q3 approvals and delegated Q4–Q13 decisions are in the plan. No owner
question blocks M1. Step 12 still requires external cluster 2 M9; no retirement
or release acceptance is claimed.

Production LOC per implementation commit, from exact first parent/final staged
trees with `python3 tools/production_loc.py --json --root <snapshot>`:

- `6db678f3a`: 135696 -> 135792 (delta +96), adapted relocation/read seam.
- `d6d159885`: 135792 -> 136615 (delta +823), mapping relocation plus new port path.
- `10954a167`: 136615 -> 137766 (delta +1151), new orchestration; no relocation.
- `e0bf6a752`: 137766 -> 138273 (delta +507), pooled orchestration; no relocation.

Across M1: reference 65,417 -> 65,417; core 70,279 -> 72,856; old-path 6,141 ->
6,160; new-path 0 -> 2,483; rest-core 64,138 -> 64,213. Combined 135,696 ->
138,273 (delta +2,577). Coexistence is growth, not legacy retirement or an
algorithmic simplification. This milestone-entry commit changes documentation
only: reference 65,417, core 72,856, old-path 6,160, new-path 2,483 and rest-core
64,213 remain unchanged. No code relocates in this entry.

Production LOC: 138273 -> 138273 (delta +0)


## 2026-10-03 — M2 complete: step 5 MinIO engine

Implementation commit: `6be5b585c` (parent `8730ff801`). `layerfs-s3` now contains
`src/{config,sign,http,client,counters}.rs`; `S3Objects` implements the three C2
object calls with the approved own SigV4/HMAC client and already-locked sha2.
No third-party package/version was added. Product code remains under src/ with
external service/fault fixtures under tests/. No domain or cluster 2 source edit.

Exact checks:

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — PASS.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — final PASS, including server/SDK coexistence builds.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-s3 -- --nocapture` with the owned environment loaded from `target/phase7-services/settings.json` — final PASS, 6 tests/0 FAIL/0 ignored. Reproduce by loading the private generated `target/phase7-services/services.env` before the Cargo command. The run used a private target-only Python launcher; no credential value was printed. Missing required service inputs fail; tests have no skip branch.
- `python3 core/tools/check_product_boundary.py` — PASS, 386 production files and allowed dependency edges.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS, 21 tests.
- `python3 core/tools/phase7_services.py status` — PASS, same owned epoch/image/settings profile; setup was reused, not regenerated.
- `git diff --cached --check` and exact committed-tree/first-parent confirmation — PASS.

The pinned MinIO proves If-None-Match conditional creation: first PUT Created,
second PUT AlreadyPresent. Whole/range reads and HEAD match bytes. Missing GET
returns Missing; bad credentials return Refused 403; a controlled proxy damages
an actual GET reply and proves Malformed with no alternate request; a controlled
proxy discards an acknowledged PUT reply and proves Uncertain, while a separate
client confirms the body exists. Unknown/malformed clients stay terminal: a
subsequent call issues no request or connection. Product source has no fault hook.

Source-driven correction and every FAIL:

- Initial Clippy FAIL on int-plus-one range comparison and two format-collect
  string constructions. Simplify the equivalent comparison and append strings
  directly; warning-denying rerun PASS. No lint suppression.
- Initial service suite: 5 PASS/1 FAIL. The repeated conditional PUT returned
  AlreadyPresent correctly, but MinIO's acknowledged 412 response explicitly
  closed its HTTP connection. The following independent HEAD exposed that close.
  Keep at most one active persistent connection. A new operation may open its
  first connection once after an acknowledged successful normal close; no
  completed request is repeated, and no failed/malformed request enters this
  path. This interpretation preserves one attempt per operation and is explicitly
  recorded in the plan/architecture. Final suite: 6 PASS/0 FAIL/0 ignored.
- PUT uses HTTP 100-continue framing: definitive refusal can precede body transfer.
  Interim replies are counted separately from HTTP requests. A chosen IPv4
  address is never replaced after failure. The local profile uses plain HTTP,
  two-second default TCP/wire bounds and the existing singleton-body ceiling;
  HTTPS/other profiles fail explicitly. No error-driven transport downgrade.
- Both red attempts and final outputs are retained in `checks/step5-*.txt`.
  The initial/final body-size rows are count diagnostics with no timers; they are
  not timed samples, replacement performance receipts or proof of cold caches.

Diagnostics for each complete stated sequence (actual client counters):

| Sequence | Connections | HTTP requests | Interim 100 | PUT/GET/HEAD | Wire sent/received B | Entity sent/received B |
| --- | ---: | ---: | ---: | --- | --- | --- |
| Conditional create + duplicate + HEAD + full/range GET | 2 | 5 | 1 | 2/2/1 | 34895 / 35955 | 32000 / 33078 |
| 256 KiB create + HEAD + GET | 1 | 3 | 1 | 1/1/1 | 263863 / 263802 | 262144 / 262144 |
| 16 MiB create + HEAD + GET | 1 | 3 | 1 | 1/1/1 | 16778937 / 16778878 | 16777216 / 16777216 |

Entity counts include refusal bodies. The conditional PUT did not send the body
again; the second connection belongs to the subsequent operation after the normal
close. Lost/malformed cases assert one request and unchanged counters on the next
call. No latency, throughput or memory number is inferred.

Service identity: owned epoch `9571f8de27bece7c53a399ff`; MinIO
RELEASE.2026-09-22T19-25-18Z, commit `df34868a88cc8c396807e04a7e220810b321bdaa`,
image digest `4692462f35d97d7e82c30371d82f057703c5d9489bcae726010594c812f2d285`.
Resource profile remains 2 CPUs/536,870,912-byte memory/no swap, single drive,
compression/encryption/browser off. Status preserves the M0 PostgreSQL settings
hash. Test objects use fresh per-run prefixes in the existing owned bucket;
no other owner's container was changed.

NOT_RUN: PostgreSQL C2/C5/TLS tests (6–7), project Init (8), real C2 parity (9),
harness/frozen acceptance cases and every timed sample (10–11). No skipped M2
covering check. No owner question blocks the next step; Q1's explicit PostgreSQL
client/TLS approval is recorded. The owner-notified operational query layout is
adopted in the plan: schema SQL plus sql/queries/{storage,history}/, embedded by
Rust with binding/decoding/error classification in the engine. No metadata file
scaffold has been added before implementation. Step 12 still waits on cluster 2 M9.

Implementation LOC comparison uses the exact first parent/final staged trees with
`python3 tools/production_loc.py --json --root <snapshot>`: reference 65,417 ->
65,417; core 72,856 -> 73,605; old-path 6,160 -> 6,160; new-path 2,483 -> 3,232;
rest-core 64,213 -> 64,213. Combined 138273 -> 139022 (delta +749). New engine
implementation; no relocation/retirement or algorithmic simplification claim.
This milestone-entry commit changes only documentation with the same subtotals.

Production LOC: 139022 -> 139022 (delta +0)


## 2026-10-03 — step 6 local C2 PostgreSQL complete; M3 in progress

Implementation commit is the commit containing this entry (parent `e828bb35f`).
`layerfs-metadata` implements the seven C2 units, five tables and sequence with
opaque typed arrays. Schema SQL and operational query files are separate under
sql/. The complete published PostgreSQL driver runs behind a synchronous bounded
one-I/O-worker facade; authentication/protocol/TLS remain in the selected client
libraries. No public port becomes async. C2/C5/cluster 2 source is unchanged;
only external C2 fixtures were factored to share the exact metadata contract.

Final exact checks:

- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — PASS.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — final PASS, including server/SDK coexistence builds.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-metadata -p layerfs-storage -- --nocapture`, with the private owned-service environment loaded — final PASS: 270 passed/0 FAIL/0 ignored (264 storage, 6 metadata). The same external metadata contract file runs against both providers. Actual stream observation asserts one Sync/ReadyForQuery exchange for each unit. Two connections prove first-wins. Statement cancellation and dropped committed reply are Uncertain and never repeated; poisoned handles submit no further operation.
- `python3 core/tools/check_product_boundary.py` — PASS, 409 production Rust/SQL files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS, 21 tests.
- `cargo +1.85.1 metadata --manifest-path core/Cargo.toml --locked --format-version 1 --filter-platform aarch64-unknown-linux-musl` — resolver PASS: metadata's selected closure contains 109 packages, none declaring MSRV above 1.85. This is not a Linux build proof.
- `git diff --cached --check` — PASS before commit; exact first-parent/staged/committed identities confirmed after commit.

Every FAIL/repair and gap is retained in checks/step6-*:

- Selective resolver update initially failed because the new workspace package was
  not yet in the lock. Cargo metadata resolved the new graph without a build.
  It initially updated five unrelated WASM lock entries; pin the newly needed
  web-sys to its compatible 0.3.104 version, restoring all five old entries.
  Final lock delta has no removed entries and 71 added package/version entries,
  including the new first-party engine. The selected postgres/native-TLS pins are
  exact; direct use of their Tokio driver/runtime adds no other client stack.
  No patched, vendored or modified third-party source. Inventory/lock delta are
  copied beside the check evidence.
- Initial Clippy compile FAIL: the driver's can_connect method takes a private
  marker and supplies a true default. Remove the unnecessary override and use the
  public TlsConnect implementation; no dependency patch. Rerun exposed one needless
  options borrow, removed without suppression; final Clippy PASS.
- First local engine suite: 0 PASS/3 FAIL during schema creation, SQLSTATE 42601.
  PostgreSQL log position identifies unparenthesized CASE expressions in a PL/pgSQL
  IF. Parenthesize them; complete storage/metadata run then passed 267 tests.
- First connection suite: 2 PASS/1 FAIL. macOS native identity import rejected the
  generated OpenSSL 3 PKCS#12 container (-25293). Use a compatible encrypted test
  container with a fixture password; no product TLS downgrade.
- Connection rerun: 2 PASS/1 FAIL. Trusted test certificate was rejected. A labelled
  diagnostic retained the native TLS reason: extended key usage not valid. Add
  serverAuth/key usage to the external generated test certificate; do not disable
  chain/name verification. The new targeted test passed, followed by the final
  complete 270-PASS suite. These are new receipts; earlier failures remain FAIL.
- The compile/schema/certificate faults required extra invocations beyond the
  requested one-repair cadence. They are disclosed, not omitted. No timed sample.

Owner-notified scope: local PostgreSQL acceptance is current. The instruction to
move on from TLS arrived after the repaired local TLS test had passed. Retain TLS
configuration/client capability and do no further TLS investigation. Earlier
certificate failures are not relabelled. The synthetic local TLS proxy proved
certificate-chain/hostname checks, rejection of wrong/untrusted identity, and no
TLS downgrade at its stated fixture. Remote/cloud provider certificates, endpoint
and deployment qualification remain DEFERRED/NOT_RUN and an open follow-up before
remote/cloud use. This does not remove the cloud-support direction. Linux system
OpenSSL compilation/proof remains NOT_RUN here; the resolver inventory is not that
proof. C5 PostgreSQL contract tests remain step 7. M3 is not claimed complete yet.

Diagnostics (whole stated test sequences, never timing claims):

| Sequence | Connections | Operations | Sync/simple Query | ReadyForQuery | Wire sent/received B | Protocol sent/received B |
| --- | ---: | ---: | --- | ---: | --- | --- |
| Bootstrap + shared metadata contract | 1 | 12 | 11/1 | 12 | 15503/4072 | 15195/3451 |
| First-wins connection A | 1 | 4 | 3/1 | 4 | 11392/1616 | 11084/995 |
| First-wins connection B | 1 | 3 | 3/0 | 3 | 1837/1326 | 1529/705 |
| Open + dropped committed register reply | 1 | 2 | 2/0 | 1 | 1633/953 | 1325/332 |
| Open + cancelled register | 1 | 2 | 2/0 | 2 | 1637/1831 | 1331/1210 |
| Local verified TLS open + policy | 1 | 2 | 2/0 | 2 | 1276/2786 | 520/664 |

Socket counts include startup/TLS; protocol counts exclude startup and observe
clear protocol frames before encryption/after decryption. No count is inferred
from lifetime memory or from a client method name. The cancelled register leaves
no row; the dropped acknowledged register exists, and neither is replayed.
Resource/profile remains the owned PostgreSQL 17.11/default durability service;
per-client statement bound is an explicit startup option, not a changed server
profile. No process construction worker or global writer budget is added.

NOT_RUN: PostgreSQL C5 (7), project Init (8), real combined parity (9), harness and
all timed samples (10–11), remote/cloud deployment qualification. No skipped local
step-6 covering check remains. Source corrections (descriptor-only payload rows,
atomic sequence block, private complete driver/deadlines, query-file layout) are
in the plan/architecture. Step 12 still waits on cluster 2 M9.

LOC comparison uses the root counter on exact first-parent/final staged trees:
reference 65,417 -> 65,417; core 73,605 -> 75,115; old-path 6,160 -> 6,160;
new-path 3,232 -> 4,742; rest-core 64,213 -> 64,213. Shipped SQL is included.
Test-fixture extraction is test code, not production relocation. New engine code
coexists with the old implementation; no retirement or simplification claim.

Production LOC: 139022 -> 140532 (delta +1510)


## 2026-10-04 — M3 complete: PostgreSQL C2 and C5

Commits: step 6 `f141ea927` (local C2); step 7 `d9fe835fc` (complete C5);
this milestone entry is documentation only. Both engines connect to the owned
local PostgreSQL service. The existing C5 trait, identity derivation, stages,
conditional transitions and cursor v2 are retained. The pure 135-production-LOC
cursor codec is relocated for both providers. All operational SQL is embedded
from sql/queries/history/. No C2/C5 FK, cluster 2 change or legacy retirement.

Exact step-7 checks (step-6 commands/results remain in its preceding entry):

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-metadata -p layerfs-history -- --nocapture`, private owned-service environment loaded — PASS: 66 passed, 0 FAIL, 0 ignored. All six copied C5 files ran against PostgreSQL (30 tests) alongside the original native contracts (30) and existing C2/connection tests (6). The long lineage case ran without shrinking its 4,354-Commit history. Its 77.62-s test wall is a functional check, not an acceptance sample.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-metadata --test history_connections -- --nocapture`, same environment — PASS: 1 passed, 0 FAIL, 0 ignored. Two independent writable connections preserve the stale loser's exact stage and return HeadMoved; externally held NOWAIT authority gives Busy, while another connection can read. No retry.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — PASS, including the final added test.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — final PASS, including server/SDK coexistence and all final test targets.
- `python3 core/tools/check_product_boundary.py` — PASS, 462 product Rust/SQL files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS, 21 tests.
- `git diff --cached --check` — PASS; first-parent/staged/committed identities confirmed for the implementation commit.

Every FAIL and decision:

- Offline Cargo metadata failed downloading the already-locked web-sys 0.3.104 for an unbuilt platform. It updated only the engine's reference to the already-locked blake3 dev dependency. No package/version entry changed. Subsequent builds stayed locked and passed; no dependency patch or registry edit.
- First Clippy failed on a redundant inherited row-decoder closure. Replace it with the function. Rerun revealed three unused copied-test imports and nested fixture formatting. Remove unused imports and flatten the formatting; final Clippy PASS. This needed a third invocation beyond the requested single-repair cadence; all outputs are retained.
- PostgreSQL equivalents replace SQLite page-size/profile assertions and raw corruption controls with deferred-key/schema checks. A cancelled mutation replaces the undefined-function unknown-outcome fixture because PostgreSQL reports that function error definitely. Server connection abort releases its transaction; the test asserts no replay/no further protocol calls and retained committed stage rather than SQLite's retained file lock. The exact semantic assertions and all 30 contracts remain covered. Plan and architecture describe these source decisions.

Diagnostics below are actual operations/Sync/ReadyForQuery exchanges in the
recorded fixture, excluding creation from individual units. They are counts,
not speed measurements; first-page/current small-history variants are stated.

| History unit | Operations / Sync / Ready |
| --- | --- |
| catalog_id, incarnation | 0 / 0 / 0 each |
| initialize_layerstack | 8 / 8 / 8 |
| fork from genesis Layer | 11 / 11 / 11 |
| stage_changes | 12 / 12 / 12 |
| commit_staged successful Commit | 11 / 11 / 11 |
| layer_stack, layer_stacks first page, branch, commit, layer, stage | 3 / 3 / 3 each |
| branch_snapshot with Commit head | 6 / 6 / 6 |
| branches first page, stages first page | 4 / 4 / 4 each |
| commit_history one Commit, layer_history genesis | 5 / 5 / 5 each |
| add_layer successful publication | 11 / 11 / 11 |
| discard_stage exact token | 5 / 5 / 5 |
| reserve_inodes first allocation | 6 / 6 / 6 |
| reserve_inodes while catalog locked | 3 / 3 / 3, Busy |

Whole fixture connection A: 1 connection, 119 operations, 118 Sync + 1 simple
Query, 119 ReadyForQuery, wire sent/received 56602/67179 B, protocol sent/received
56294/66558 B. Connection B: 1 connection, 34 operations/Sync/Ready, wire
8352/40835 B, protocol 8044/40214 B. Per-unit protocol byte counts are in
checks/step7-connections.txt. No warm/timed sample, no lifetime-memory claim.

NOT_RUN/open gaps: project Init (8), real combined parity (9), frozen harness
and all acceptance rows (10–11), remote/cloud deployment and Linux system
OpenSSL qualification. Current M3 acceptance is local PostgreSQL, as directed;
remote/cloud capability remains configured but unqualified. No local M3
covering check is skipped or failing. Step 12 remains gated on cluster 2 M9.

LOC method: root production counter on exact first-parent/final staged trees,
including shipped SQL, excluding tests/docs/tools. Step 6: combined
139022 -> 140532 (+1510); reference 65417 -> 65417; core 73605 -> 75115;
old-path 6160 -> 6160; new-path 3232 -> 4742; rest-core 64213 -> 64213.
Step 7: combined 140532 -> 142416 (+1884); reference 65417 -> 65417;
core 75115 -> 76999; old-path 6160 -> 6025; new-path 4742 -> 6626;
rest-core 64213 -> 64348. The 135-line old-path change is cursor relocation;
coexisting PostgreSQL history is replacement code, not simplification.
This milestone-entry commit preserves all current subtotals.

Production LOC: 142416 -> 142416 (delta +0)


## 2026-10-04 — step 8 namespace Init complete; M4 in progress

Implementation commit is the commit containing this entry (parent `cba8a2804`).
Project Init adapts the existing server scan/batch/namespace/attribute builders,
retaining four construction workers and three saves. The server source remains;
this is duplication during relocation, with no legacy retirement. Host authority,
name, scope seed, deadline and ordering-scratch ownership are explicit inputs.
C2 has an explicit four-upload Init path; ordinary saves keep one. S3's explicit
Init connection window opens four independent streams. Every closed batch waits
for its uploads before registration. No encoding/framing/identity change.

Exact checks, with the private owned-service environment loaded for Cargo tests:

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-project -p layerfs-storage -p layerfs-s3 --all-targets -- --nocapture` — PASS: 274 passed/0 FAIL/0 ignored. This includes 265 storage (new parallel window proof), 6 S3, 2 Init suites and the independent verifier's sample-selection test. Both 100- and 1,000-file functional fixtures pass all paths, kinds, mode/mtime and file-byte checks over memory and real ports. These are not acceptance workloads or timed samples.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test parallel_upload -- --nocapture` — final PASS: 1 test. A four-way barrier proves simultaneous uploads; refusal permits no registration and no retry. Existing exact sealed-byte vectors ran in the full suite.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-project --tests -- --nocapture` — PASS: 2 tests after adding before-verifier engine counts; its output remains separate from the final complete suite.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test parallel_upload -p layerfs-project --examples -- --nocapture` — verifier example's 1 test PASS; the initial parallel fixture FAIL below is retained. Example main functions were not run.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — final PASS, including examples and server/SDK coexistence. After the functional suite, native source was cfg-gated for Unix with an explicit Unsupported path; final all-target Clippy built that final source. The active Unix algorithm did not change in that refinement.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — final PASS.
- `python3 core/tools/check_product_boundary.py` — final PASS, 470 product Rust/SQL files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS, 21 tests.
- `git diff --cached --check` — PASS before commit.

Every FAIL/repair is preserved in checks/step8-*:

- First package compile failed on an ambiguous obsolete bridge-style .into() conversion; return ProjectError directly and remove the unused consumer import. The second package suite passed.
- Initial Clippy found inspect_err, an unnecessary progress lifetime, and two obsolete explicit drops of the new non-Drop sink. Use inspect_err, simplify standalone deadline progress (no bridge progress bytes), and rely on ordinary borrow scopes. Its next invocation exposed an incorrectly named ChunkData fixture role; use the actual public role. Final Clippy passed.
- The parallel test first lacked a canonical envelope (UnsupportedFraming), then supplied a foreign value for the Chunk role (0 PUTs; source diagnosis identifies the missing chunk grammar). Replace it with the already-established valid whole-file singleton vector and a declared zero-depth profile. The final four-way barrier test passes. Product validation was preserved.
- The Unix cfg refinement initially duplicated ProjectError/ProjectResult imports. Remove the duplicate; final locked all-target Clippy passed. These separately exposed fixture/import defects required extra invocations beyond the requested single-repair cadence; no output is omitted.
- Offline lock generation succeeded but freshly resolved unrelated entries. Retain only the newly resolved project package block and restore all existing published package/version entries. The final lock adds 13 lines for project alone; every build is locked and no third-party source is changed.

Diagnostics: engine totals before the independent verifier include explicit
bootstrap/open, since these are cumulative handle counters. They are not phase
latencies, and the complete-test totals include the verifier's reads.

| Functional fixture | PG connections / operations / Sync / simple Query / Ready | S3 connections / requests / PUT / GET | S3 entity sent / received B |
| --- | --- | --- | --- |
| 100 files, before verifier | 1 / 25 / 24 / 1 / 25 | 4 / 5 / 4 / 1 | 9739 / 377 |
| 1,000 files, before verifier | 1 / 83 / 82 / 1 / 83 | 4 / 30 / 12 / 18 | 88831 / 70600 |

The final fixture's exact C2 cache/round-trip/forced-seal counters, all wire and
protocol byte counts, and after-verifier counts are in step8-tests-final.txt.
The functional fixtures have 111 and 1,011 namespace entries, respectively.
Per-file mtime and constructor order belong to each stated fixture; no count is
normalised or pooled across runs. The inherited independent manifest verifier
is port-bound in examples; its main awaits the frozen release harness.

NOT_RUN/open gaps: real-engine combined storage parity (step 9), frozen harness
and all acceptance/timed rows (10–11), example main functions, off-platform Init
compilation, remote/cloud and Linux OpenSSL qualification. M4 is not yet claimed
complete. Step 12 remains gated on cluster 2 M9. No service setting, timeout,
worker count or workload is changed to convert an acceptance miss into a pass.

LOC uses the root counter on exact first-parent/final staged snapshots with
runtime SQL included: reference 65417 -> 65417; core 76999 -> 77821;
old-path 6025 -> 6025; new-path 6626 -> 7448; rest-core 64348 -> 64348.
Init is adaptation/duplication while its old source remains, not simplification.

Production LOC: 142416 -> 143238 (delta +822)


## 2026-10-04 — M4 complete; owner-directed pause after step 9

Commits: step 8 `7c45bfbe6` (namespace Init and acknowledged upload window);
step 9 `73a3faef9` (real storage parity/count proof); this M4 entry is
documentation only. Steps 1–9/M0–M4 are complete at their stated local scope.
Owner direction received through the side conversation requires pausing here:
do not begin steps 10–12 or any timed sample. This is a pause, not complete
Phase 7/benchmark/release qualification. Issue #302 remains open.

Completed scope: engine-independent project Init adapts the retained server
algorithms, four file constructors and three saves. Its explicit four-upload
window and four independent MinIO connections acknowledge before registration;
ordinary saves keep one producer/upload. Typed C5 publication follows completed
C2 saves. Real-engine parity supplies identical finalized sequences to old and
new paths and compares every declared sealed pack byte-for-byte after each save.
Canonical identities, CDC, deduplication, FULL/PREFIX/STORED and pack framing are
unchanged. Cluster 2 product source is untouched and server/SDK keep building.

Exact final covering checks (all Cargo tests load the private owned-service env):

- Step 8: `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-project -p layerfs-storage -p layerfs-s3 --all-targets -- --nocapture` — PASS, 274 passed/0 FAIL/0 ignored. Includes four-way acknowledgement/refusal proof, existing exact byte vectors, all six S3 tests, full 100/1,000-file namespace oracles through memory/real ports and the independent verifier sample-selection test.
- Step 9: `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-project --all-targets -- --nocapture` — final PASS, 7 passed/0 FAIL/0 ignored: two Init suites, four real parity tests, one verifier example test. This exercises the final cfg-gated Unix source. Example main functions were not run.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` — final PASS at both steps, including server/SDK and examples.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — final PASS at both steps.
- `python3 core/tools/check_product_boundary.py` — final PASS, 470 production Rust/SQL files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` — PASS, 21 tests, at both steps.
- `python3 core/tools/phase7_services.py status` — PASS; owned pinned-service identity/profile retained in checks/m4-services-status.json. Both services remain running for setup reuse; no teardown or foreign resource mutation.
- `git diff --cached --check` — PASS before each commit; exact first-parent/staged/committed tree comparison confirmed.

Every FAIL/repair remains evidence rather than being relabelled:

- Step 8 compile: ambiguous inherited bridge-style conversion and unused consumer import; use the typed ProjectError directly.
- Step 8 Clippy: inspect_err, unnecessary progress lifetime and obsolete explicit drops; corrected without suppression. The new parallel fixture initially named a nonexistent ChunkData role, then omitted a canonical envelope, then supplied a foreign Chunk value. A declared valid zero-depth whole-file singleton vector now proves four concurrent uploads and refusal before registration. The platform refinement duplicated two imports; remove them. All earlier outputs remain in checks/step8-* and the preceding step entry, including extra invocations beyond the one-repair cadence.
- Step 9 first parity result: 1 PASS/2 FAIL. Whole-file FULL-count assumption was wrong because the inherited selector can choose a shallow signature candidate; metadata body-call assumption was wrong because the pooled reader acquired bodies twice before a missing ordinal. Preserve product behavior, verify the one catalogue lookup, record all body calls and add the explicit chunk predecessor proof for depth-bound FULL. Do not alter cache/codec/worker limits.
- Step 9 second parity result: 3 PASS/1 FAIL. New chunk fixture was 90,000 bytes, exceeding C1's frozen 32,768-byte maximum. Correct the invalid functional fixture to the actual public CDC bound; do not change that bound or any registered acceptance workload. Add distinct corruption of the pooled value-base pack with its ordinary leaf pack intact. Final package result: 7 PASS. These separately revealed fixture faults required an extra invocation; failed logs remain FAIL.
- The initial old-oracle module import produced an unused re-export warning; factor its exact snapshot function into external test support. No product source is included/recompiled by tests. Development controls use already-locked postgres/rusqlite versions; no published version changes, third-party patch, vendor or registry edit.

Diagnostics only; no timed sample or speed claim:

| Real parity case | Saved versions | FULL | PREFIX | Additional proof |
| --- | ---: | ---: | ---: | --- |
| Whole-file, 90,000 B raw | 8 | 1 | 7 | 8 exact reuses add zero PUTs/locators; bases cross packs; every pack equals old bytes |
| Chunk, 32,768 B raw, depth 2 | 8 | 3 | 5 | FULL at versions 0/3/6; each version/pack equals old bytes |
| Pooled 100-row leaf, depth 2 | 8 | 3 | 5 | All PoolCounters equal old path; metadata only, zero S3 requests |

Selected cumulative final diagnostic points include engine bootstrap, reopening
reads and earlier pack-oracle reads; external corruption/SQL-oracle controls are
outside LayerFS client counters. The whole-file point is after its eighth exact
reuse, the other two are after the eighth save/authenticated read and before that
version's final pack oracle:

| Sequence point | C2 locate / reserve / register | PG operations / Sync / simple Query / Ready | S3 requests / PUT / GET | C2 emitted payload or metadata B |
| --- | --- | --- | --- | --- |
| Whole-file after last reuse | 16 / 16 / 8 | 69 / 68 / 1 / 69 | 102 / 8 / 94 | payload PUT 91217 |
| Chunk after last save/read | 8 / 16 / 8 | 58 / 57 / 1 / 58 | 64 / 8 / 56 | payload PUT 100947 |
| Pooled after last save/read | 8 / 24 / 8 | 169 / 168 / 1 / 169 | 0 / 0 / 0 | metadata write 44444 |

Pooled point: 8 value groups/packs, 32,768 reserved-directory B, 33 metadata-body
calls, 17 catalogue calls, 40 pack-cache hits and 33 misses, 67 locator hits and
15 misses, 0 forced seals. Whole-file and chunk points also have 0 forced seals.
All per-version bytes, cache counts, engine wire/protocol bytes and selection
outcomes are in checks/step9-tests-repaired.txt. Step 8's own exact Init and
before/after-verifier counts remain in its entry/checks; rows are not pooled.

Negative reads (fresh port handle; counts cover the failed read, excluding open):

| Damage | Typed refusal | Metadata operation delta | C2 locate / body / catalogue / GET |
| --- | --- | ---: | --- |
| Missing PREFIX base locator | ObjectMissing(base) | 2 | 2 / 0 / 0 / 1 |
| Missing pooled value catalogue | Integrity(metadata ordinal missing) | 4 | 1 / 2 / 1 / 0 |
| Corrupt root metadata pack | Integrity(sealed pack length/digest) | 2 | 1 / 1 / 0 / 0 |
| Corrupt pooled value-base pack | Integrity(sealed pack length/digest) | 5 | 1 / 3 / 1 / 0 |

The multiple pooled body acquisitions remain a recorded count gap for later
harness assessment; no speed/bound claim is made for them. No local M4 covering
check is skipped or failing. NOT_RUN/open gaps: release example main functions,
off-platform Init compilation, Linux system OpenSSL and remote/cloud PostgreSQL
qualification; frozen harness/cases/cache contract (10), all final acceptance
rows (11), retirement (12). Step 12 additionally requires cluster 2 M9. The old
path remains and there is no retirement/simplification claim. No new owner answer
is needed to stop at the expressly requested pause boundary.

LOC method: python3 tools/production_loc.py --json --root <snapshot> on exact
first-parent/final staged trees, shipped SQL included; tests/docs/tools excluded.
Step 8: combined 142416 -> 143238 (+822); reference 65417 -> 65417;
core 76999 -> 77821; old-path 6025 -> 6025; new-path 6626 -> 7448;
rest-core 64348 -> 64348. This is adaptation/duplication during Init relocation.
Step 9 and this entry: combined 143238 -> 143238 (+0); reference 65417,
core 77821, old-path 6025, new-path 7448 and rest-core 64348 unchanged.

Production LOC: 143238 -> 143238 (delta +0)


## 2026-10-04 — step 10 first slice: strict gate, timer and input/accounting contracts

Owner resumes the same storage worktree in a separate chat. Q5 now requires
strict candidate < matched baseline per case; equality FAIL, with storage
passing simultaneously. The direct-engine candidate route is a scoped normative
amendment, not relabelling the historical SDK selection. Server/SDK remain.
The project example now includes engine creation, validation and connections in
its timer, reports bootstrap as overlapping diagnostic work and uses the 15 s
deadline. Source inspection corrects the plan's claim about baseline creation:
Server::create is before its raw timer, inside complete driver wall. Candidate
conservatively pays creation too; no subtraction. The amended current core
WAL/sync rule is reconciled into this worktree with default PG durability intact.

New seven-case registry, strict joint gate, no-content-read invalidation plus
whole-input mincore pass, owned fresh-service validation, PostgreSQL relation
and MinIO regular-file block accounting are implemented. The Init adapter has
release-only arm builds, immutable binary archives, worktree locks and persistent
one-sample claims. History port driver is not yet bound/frozen. M5 and full step
10 remain incomplete; every acceptance case is NOT_RUN at this slice. A source
analysis document distinguishes client operations, physical Sync/Ready exchanges,
server statements and transactions; earlier counts remain diagnostic only.

Checks: new contract tests 5 PASS; existing Init tests 2 PASS, history tests
4 PASS, substrate tests 7 PASS; residency tests 10 PASS. Project all targets
7 PASS/0 failed/0 ignored; locked all-target core Clippy and fmt PASS, boundary
470 files PASS, tools 21 PASS. All output is in checks/step10-*. No core
production source changed; the covering project example checks ran. No CI or
preflight. No service sync/resource/worker/codec changes or new dependency.

Candidate locked release example build: 18,675,087,834 ns / 30 s PASS. Baseline
locked release build in the owned nested clean 7edddbdb8 worktree:
19,680,266,458 ns / 30 s PASS after correcting package selection. The first
baseline attempt FAIL (162,583,042 ns, no compilation): verify_namespace belongs
to layerfs-server, not layerfs-sdk. Its log is retained; corrected selector includes
both as the existing runner does. These are untimed builds, not gate samples.

LOC method: python3 tools/production_loc.py --json --root <snapshot> on exact
first-parent/final staged trees, shipped SQL included, tests/docs/harness/examples
excluded. Reference 65417, core 77821, old path 6025, new path 7448, rest-core
64348 unchanged. Full acceptance/remote/cloud/Linux OpenSSL remain unqualified.

Production LOC: 143238 -> 143238 (delta +0)


## 2026-10-04 — creation-inclusive Init v1 diagnostic retained; v2 boundary repair

Source/harness 92d78ca1c6ed5990b5e285f0efcddcf2319c180d; baseline unmodified
7edddbdb8e8512627aed0ed42533ef099d802384 in its own nested worktree/target.
One 100-file arm each, fixture namespace-100-compact-v3 (5,000,000 logical B),
seed 1, release binaries. Both fresh-service epochs and settings/image identities
are retained; whole-input mincore after invalidation reported zero resident
pages in both arms. No unchanged arm is repeated. Numeric observations are
diagnostic, not M5 admission: v1 conservatively included candidate empty schema
creation and did not capture the Docker VM CPU/memory identity; full history
harness also remains unbound. These limitations are not relabelled after repair.

| Files / arm / case | Raw operation ns | Complete child ns / 15 s | Verifier ns / 9.5 s | Allocated B | Semantic / cleanup | Disposition |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| 100 baseline / phase7-init-100-direct-engines-v1 | 38637291 | 994793250 | 554691292 | 7372800 | PASS / PASS | creation scope asymmetric; diagnostic |
| 100 candidate / same | 280761708 | 866455875 | 764745167 | 5804032 | PASS / PASS | strict time comparison FAIL; admission INCOMPLETE |
| Init 1,000 / 10,000 / 100,000 | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | v1 diagnostic scope repair before more work |
| History stride 10 / 3 / 1 | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | direct-port history driver not yet bound |

Time delta = 280,761,708 - 38,637,291 = +242,124,417 ns (+626.659920%).
Storage delta = 5,804,032 - 7,372,800 = -1,568,768 B (-21.277778%).
Candidate bootstrap alone is 63,098,500 ns; do not subtract it to manufacture
a passing gate. Candidate allocation = MinIO 5,148,672 + PG C2 352,256 + PG C5
303,104 = 5,804,032 B. Database 8,681,139 B, WAL 16,777,216 B and MinIO system
allocation remain separate overhead in the raw receipt. Init has no numeric
storage ceiling; the smaller allocation cannot qualify it. Both lite proofs
cover complete paths/kinds and declared sampled file metadata/content only.

Candidate before-verifier counts: C2 policy1/locate5/body0/catalogue2/signatures1/
reserve9/register5; PUT29/GET12, 5,010,214 sent payload B, 2,153,226 received B.
PG C2 1 connection/24 operations/23 Sync/1 simple Query/24 Ready; C5 likewise
1/24/23/1/24 including bootstrap. S3 4 connections, 41 requests and 29 interim
100-continue replies. These are counts, not per-query latency or storage reads.
Every stdout/stderr/receipt/manifest is retained in append-only raw result paths
and losslessly compressed under checks/step10-v1-*.

Boundary repair follows plan §3.2: new Init v2 IDs bootstrap empty schemas in
untimed, workload-free prepare_storage, then time all engine open/validation/
connections and project::init. Existing v1 evidence keeps its original source
and status. New v2 also captures the Docker VM resource identity before sample.
No product algorithm/worker/codec/cache/buffer/durability setting changes.
Locked release example build PASS 0.89 s; focused project all-target Clippy
PASS 4.59 s; v2 contract tests 5 PASS. These checks cover the example-only
boundary repair; unchanged product functional checks are reused from first slice.
Full step 10, step 11, M5 and retirement remain incomplete.

Production LOC: 143238 -> 143238 (delta +0)


## 2026-10-04 — corrected Init v2 measured speed blocker; M5 incomplete

See STEP10-INIT-PAIR.md and checks/step10-v2-* for every raw operand, scope,
seal, query-plan/count diagnostic and retained failure. One prospectively
frozen matched 100-file pair at c9165d4da, against unmodified 7edddbdb8, has
zero resident input pages, matching VM/image/settings/fixture/harness identities
and separate verifier PASS for both arms. Baseline 38354625 ns;
candidate 248079166 ns; delta +209724541 ns (6.468038x).
The owner strict speed condition is FAIL. Baseline allocation 7372800 B;
candidate 5804032 B (MinIO 5148672 + PG C2 352256 + PG C5 303104).
Init has no numeric storage ceiling; no smaller storage result waives speed.
No unchanged arm is rerun. Six other declared selections are NOT_RUN: no
complete qualifying campaign exists, and the history direct-port driver remains
unbound. Step 10/11 and M5 are incomplete; retirement remains gated on M9.

C2 count reconciliation: 24 Sync/Ready = open policy + Storage policy + locate5
+ catalogue2 + signatures1 + reserve9 + register5. C5: 20 Sync/Ready = open6
(read transaction) + reserve6 + initialize8 (two write transactions). S3 has
37 requests/28 interim replies, PUT28/GET9. Timed open/validation is 34174750 ns;
remaining operation 213904416 ns, never subtracted from the gate. Diagnostics
identify acknowledged network work; no speculative product tuning is called a
speed improvement. Post-proof query plans are explicitly warm diagnostics.

Both proofs inventory 102 paths/100 files/2 directories, read full metadata and
3,354,003 B from 53 selected files; this is sampled content, not all 5 MB.
Both command/verifier bounds and child scratch cleanup pass. Complete seven-case
harness/teardown qualification remains open as stated in the report. Local
owned services retain their pinned profile; remote/cloud/Linux OpenSSL and
off-platform qualification remain deferred. No push, PR, merge or retirement.

Production LOC: 143238 -> 143238 (delta +0)
Reference 65417; core 77821; old 6025; new 7448; rest-core 64348 unchanged.
Method remains the exact parent/staged/committed root counter including SQL.

Owned-service post-proof teardown PASS (1209505875 ns, separate cleanup scope).
Containers/volumes/network removed only after retaining proof/allocation/SQL
evidence. Both services are down; next owned work uses phase7_services.up.
No original performance receipt is rewritten or promoted by this cleanup.


## 2026-10-04 — cause instrumentation prepared before optimization

Owner approves paired Phase 4.5/current diagnostics at 100 and 1,000 files.
No optimization or new admission speed row is claimed. The same workload,
explicit scope/authority, four constructors, bounded four uploads, service
limits, default durability and input-residency contract stay. The baseline
product remains unmodified at 7edddbdb8; a harness example uses public Service
import to return its existing timing report. It is not an SDK gate timer.
Candidate real telemetry records PG caller/queue/driver work (128 template
classes), S3 protocol work (three methods), and C2 save work (ten inclusive
stages, four recent acknowledged snapshots). Detailed setup is in the plan and
architecture counter/import notes. Off-platform observer qualification is open.

External SQLite observer calibration PASS: five explicit probe statements,
positive VM work, three SELECT step calls and one exec-script call. Interposed
step and exec timing paths are disjoint in that calibration. Dynamic library
uses system SQLite without third-party changes. Statement profiles cover exec
statements; PROFILE time is coarse and direct blob API timing remains a gap.

Checks: metadata initial covering suite 37 PASS, unchanged engine proof reused;
first project part 5 PASS/1 FAIL, retained. Repaired storage/project/S3 all-target
command 278 PASS/0 failed/0 ignored. All-target core Clippy PASS, fmt PASS,
boundary 473 files PASS, tools 21 PASS; focused diagnostic harness 3 PASS and
substrate 7 PASS. Locked release builds: candidate final 5.04 s, baseline final
0.63 s. Source product/format/SQL unchanged except telemetry/timing scope.

Every failure/repair is retained in checks/step10-cause-build-* (raw originals
also target/phase7-agent): first metadata compile lacked a diagnostics doc after
adding statement_work; restore the doc. First SQLite probe output was absent
because RTLD_NEXT dlsym returned the interposed function recursively; LLDB
confirmed observed_open repeated on the stack. Direct original-symbol calls
inside the interposer fix it; probe then passes. Initial destructor/atexit
output investigations were superseded by that root cause, not rewritten.
Baseline diagnostic first called private handle_until_bound; use public
set_import_root + handle_until without changing product visibility. Functional
parity failed with PG 23505: server log confirms pg_namespace_nspname_index
collision. External fixtures discarded their atomic counter when naming schemas;
preserve PID/counter/timestamp. No provider retry or SQL change fixes it.

Owned PG observer-profile calibration PASS: shipped pg_stat_statements, all
nested statements/planning, I/O/WAL timing; fsync/synchronous_commit/
full_page_writes remain on. This is a diagnostic profile, not admission.
Freeze source/binary/observer/cases before the four diagnostic children. All
four cause children are NOT_RUN at this commit. Existing v1/v2 speed failures
remain; full step 10/11/M5 and direct history harness remain incomplete.

Production LOC: 143238 -> 143542 (delta +304). Exact first-parent/staged
snapshots counted with tools/production_loc.py (nonblank/noncomment product
Rust and shipped SQL, external tests/examples/tools excluded). Reference
65417 -> 65417; core 77821 -> 78125; old path 6025 -> 6025; new path
7448 -> 7752; rest core 64348 -> 64348. Growth is bounded telemetry.

## 2026-10-04 — four cause diagnostics completed, no optimization

Instrumentation/source frozen at fdfbee48f; Phase 4.5 product pin 7edddbdb8.
Declared 100 baseline/current then 1000 baseline/current, one child each.
Release/locked archives reused, same prepared inputs and explicit authority/seed,
four Init constructors/four uploads, no durability or cache relaxation. All
four whole-input residency checks zero pages; all independent lite proofs,
15 s child bounds, 9.5 s verifier bounds and scratch cleanup PASS. Canonical
roots match per size. Public Phase 4.5 Service import versus direct project
init is a component cause diagnostic, not a matched SDK speed admission pair.

Outer operation ns: baseline100 39,345,500; current100 198,558,750
(including 32,245,208 open); baseline1000 132,890,541; current1000 579,322,083
(including 27,650,041 open). Current project timers 166,310,458 / 551,672,042 ns.
Measured bounded upload windows 89,605,958 / 290,809,582 ns dominate, request
response waits overlap across four connections. PG statements 44 / 61 frontend;
observed query/boundary-derived transactions 27 / 44; nested object INSERTs
346 / 2003, execution 7,087,669 / 39,631,220 ns. SQLite operation statements
697 / 3544, VM steps 56,726 / 325,153, transaction starts 20 / 28 including
one rollback each; object INSERTs batched into 30 / 102 statements.

Detailed small-step, CPU, SQL, VM, protocol, warm post-proof plan, allocation,
identity, reproduction/custody and revised ranking in STEP10-CAUSE-DIAGNOSTIC.md.
No MinIO server trace or exact RTT attribution, direct SQLite blob API clock,
per-worker split or phase memory peak; these stay unavailable. PostgreSQL WAL
sync deltas 2.687 / 5.597 ms are shared observer windows, not product excuses.
First SQLite EXPLAIN missed the pack-ceiling bind; failure retained, SQLite
plan call corrected; no successful PG plan/performance child repeated.
Post-proof write plans use copied disposable schema, rolled back, dropped.
Original1000 canonical inventory confirmed 2003 objects / 20,187,652 bytes.

Allocation B: baseline100 7,372,800/current100 5,808,128; baseline1000
23,101,440/current1000 21,725,184. Numeric Init ceiling remains undeclared.
Four original run manifests, plan and campaign custody PASS; raw append-only
paths and compressed checks retained. Owned services removed PASS; baseline
checkout and immutable build archives retained for reuse. No product tests
repeated for this evidence-only change; frozen covering proofs from the previous
entry reused. No admission FAIL relabeled, no step10/11/M5 completion claimed,
no push/PR/merge/retirement.

Production LOC: 143542 -> 143542 (delta +0), evidence/docs-only exact
first-parent/staged snapshots with tools/production_loc.py. Reference 65417,
core 78125, old path 6025, new path 7752, rest core 64348 unchanged.

## 2026-10-04 — four-path optimization implemented before new pair

Owner explicitly directs the four ranked paths. C2 lane queues share the old
aggregate pack/row budget, avoiding lane-switch-only seals; framing and canonical
identities stay. S3 bounded bodies send without Expect, require the same final
conditional acknowledgement, authenticate body/key, and never retry. Registration
uses bounded unnest/INSERT/RETURNING and input-ordered lost IDs, preserving atomic
first-wins. Reservations consume remaining blocks using conservative next-wave
and actual final-seal bounds without increasing allocation requests. Immutable
checked C1 PoolingLeaf reuses canonical decode. Required S3 hash check remains;
exact MinIO server/storage wait and remaining encode/read cost stay open.

New public-API regression cases for fewer mixed-lane packs, single reservation
across 1600 objects, independent exact readback, mixed conflicts/rollback, pooled
ordinal checks, and immediate acknowledged conditional S3 writes PASS. Project
100/1000 full namespace oracles over memory/live engines and existing physical
codec/reuse/race/unknown-outcome checks PASS. Source boundary 473 files PASS,
tools21 and focused harness5 PASS, warning-denying Clippy/fmt PASS.

Retained failures: initial compile missed same-save provider's reserve_packs
caller; fixed its two-argument conservative bound. Pair-claim edit had a missing
quote; fixed, harness5 PASS. Old S3 test expected2 connections after conditional
conflict; immediate-body request keeps1 and subsequent reads succeed, assert
updated, that covering test PASS. No successful suite resampled.

One unrelated history_remediation stress case FAIL: LOCK history_meta EXCLUSIVE
NOWAIT returns55P03 Busy. PG log failure19:08:52.058 UTC; same schema autovacuum
completes history_meta at19:08:52.086, with autoanalyze immediately afterwards.
This is consistent with background vacuum lock contention. C5 implementation
is unchanged. Preserve failure; no retries, timeout change or autovacuum/profile
relaxation. Other passed targets reused; remaining/unrun targets run separately.
This covering-check gap prevents a claim that all owning tests pass.

Freeze implementation locally before release builds/new matched measurements.
Pair claims bind candidate tree as well as measured source/harness/fixture, so
a changed candidate has its required new baseline arm and cannot resample that
pair. Case profiles, fixtures, gate/timers and budgets unchanged. All prior FAIL
receipts remain. New performance/count diagnostics NOT_RUN at this commit;
large Init/history cases and step10/11/M5 remain NOT_RUN/incomplete. Production
LOC is counted from exact parent/staged/committed snapshots with the unchanged
root counter including shipped SQL; see this commit's message and final report.

Final checks: storage267 PASS, project7 PASS, S3 seven contract cases PASS
(the changed connection assertion verified separately), content plus unaffected
metadata307 PASS with the one retained C5 stress FAIL, remaining metadata7 PASS.
Warnings-denying final all-target Clippy/fmt PASS. Diagnostic dispatch/bounds/
redaction3 PASS; unchanged external SQLite observer calibration reused. Shipped
mc server trace calibration PASS,457 records/198786 bytes, zero omitted/invalid;
fixed16MiB output cap, credential/body redaction, explicit ready sentinel outside
timers and own PID shutdown. Trace adds observer work only in count diagnostics;
default-profile matched speed arms do not enable it. Server events are matched
by measured object path and reconciled against request counts; aggregate request
clocks are not interpreted as disjoint elapsed spans.

## 2026-10-04 — first optimized pair retained; hash backend next

First treatment a7ebff1b4: default-profile raw SDK baseline/current Init ns
100:39326291/209471834,1000:132328709/517663959. Speed FAIL (ratios5.326509
and3.911955); storage7372800/5799936 and23101440/21737472 B. All four input
residency, sampled proof and scratch checks PASS. Cases10000/100000/history
NOT_RUN, Init ceiling open, step10/11/M5 incomplete. Count diagnostics separately
reconcile MinIO server/client28PUT12GET and94PUT22GET, zero trace omissions.
Server API sums211966750/598206024 ns overlap; matched write/sync spans nest.
C2/C5 calls40/54; bulk object-registration statements5/9 replacing346/2003
individual INSERTs. More distinct GET keys remain12/22, not repeated same-key
reads. Registered PUT counts change marginally; no sweeping packing-win claim.
Exact arithmetic, table, reproduced commands, identities, failures and custody
in STEP10-OPTIMIZATION.md and raw issue302-opt-* directories. Services down PASS.

The measured validation cost has a distinct source cause: storage selects
sha2 0.10.9 without asm, hence software SHA256 on aarch64. Already-locked
published0.11.0 selects ARM SHA2 by default. Change only C2's existing dependency
reference; package/version/checksum set unchanged, no new crate, patch or feature
shortcut. Key/body validation remains, canonical/framing/digest bytes unchanged.
Crypto storage/project275 tests PASS, independent empty/abc/million-a SHA256
answers PASS, final all-target Clippy/fmt and boundary473 PASS. Previous metadata/
S3/C1 product proofs reused by unchanged scope; C5 stress FAIL stays unresolved.

Retain preparation/check failures: no-deps metadata did not update dependency
lock reference, first locked test therefore refused before any test ran. Full
offline metadata resolves that reference but reports uncached unrelated wasm
web-sys download; resolved package/checksum inventory unchanged, host locked
build/tests work. Test-only hex format_collect warning fixed with fmt::Write;
covering vector1/final Clippy PASS. No product retry, durability/profile change,
third-party edit or unchanged speed resample.

Freeze separate hash treatment, rebuilt matched default-profile100/1000 pairs
and one changed-mechanism1000 count diagnostic. At this commit those children
are NOT_RUN. Source LOC remains143581 before/after(delta+0): manifest/lock/tests/
evidence only, exact parent/staged snapshot root counter including shipped SQL;
reference65417/core78164/old6025/new7774/rest64365 unchanged.

## 2026-10-04 — hash treatment measured; parity not achieved

Frozen85c1260c9, release build5.991s. Fresh matched pairs one/arm:100 baseline
35506542/current195970292 ns,1000 baseline124550958/current431842125 ns. Strict
speed FAIL, ratios5.519273/3.467192. Current allocation5808128/21741568 B versus
7372800/23101440 B; numeric Init ceiling open. All four input-zero residency,
sampled functional proof and scratch cleanup PASS. Complete runner envelopes
5.935/7.897/5.756/6.479s <=15s, proof separately bounded and outside comparison.

Distinct1000 count diagnostic: SHA validation11911622ns/20038352B (0.59444ns/B)
versus preceding software60988418ns/20038436B (3.04357ns/B); observed5.12x
per-byte decrease, not end-to-end guarantee. C2/C5 frontend35/20, transactions
35implicit+3explicit=38, object bulk10 statements/13347500ns versus old2003
per-object INSERTs. Canonical2003 objects/20187652B and fixed-root match PASS.
MinIO93 PUT/18 distinct GET; reconciled2965 events/1201307B, no omissions.
Server PUT summed630920877ns overlaps across4 requests; upload elapsed
267990626ns remains dominant. No server durability excuse or relaxation.
Current CPU178566000ns; old software count row270540000ns. Full details and
non-additive timing limits in STEP10-OPTIMIZATION.md.

Original manifests/compact custody PASS; owned services removed PASS. Remaining
large Init/history cases NOT_RUN, C5 NOWAIT stress FAIL retained/unresolved,
step10/11/M5 incomplete, phase memory/off-platform qualification unavailable.
No unchanged-arm rerun or best-of selection, no push/PR/merge/retirement.
Evidence-only Production LOC143581->143581(delta+0), exact parent/staged
root counter incl shipped SQL; reference65417/core78164/old6025/new7774/
rest64365 unchanged.

## 2026-10-04 — rolling upload window after side-conversation finding

The side conversation identifies fixed chunks(4) as an admission barrier and
requests considering work-conserving four uploads. Implement under the owner's
existing S3 optimization scope: at most4 scoped persistent workers consume the
sealed pack list, each admits its next pack after its own acknowledgement,
without waiting for unrelated slow requests. Shared counters record attempts/
bytes on the producer after all admitted jobs join; errors stop admission,
retain terminal outcome semantics, and prevent metadata registration. Bodies
remain borrowed from the same bounded ready state, no extra payload queue/copy.

S3 connection admission prefers an idle socket under a short selection mutex;
I/O is outside that mutex. Init's4 callers do not queue behind an unrelated
busy round-robin socket while another is free. Excess ordinary caller contention
waits within the same fixed connection window; no new socket/retry.

Public-API controlled-channel proofs PASS: pack5 starts while pack1 gated,
active calls never exceed4; real MinIO proxy proves request5 reaches free
connection while reply1 is held, exactly4 connections/5 acknowledged HEADs.
Storage/S3/project owning suite PASS; final Clippy/fmt/boundary PASS. Initial
new test compile failed on end-of-block mutex-guard lifetime; retain error and
fix test-only local count binding. No passing suite resampled. Prior C5 stress
Busy FAIL remains.

Freeze another changed-mechanism identity: one new matched default-profile
100/1000 pair each, and one1000 PG/mc count diagnostic. Same fixtures/cold
contract, service defaults, physical/working limits, four constructors/uploads,
15s children/9.5s proof; complete runner envelopes15s (cause25s outer exception).
All earlier FAIL receipts retained, no best-of. These children NOT_RUN here.
Exact production LOC parent/staged/committed comparison in commit message;
root counter/SQL scope unchanged. No architecture or port/format change.

## 2026-10-04 — owner supersedes campaign; SQLite design first

Retain fff66f0e7 rolling implementation and all earlier changes without rollback.
Already-started1000 diagnostic completed before new work, preserved with its
sealed receipts. Four rolling performance rows and trace manifests custody PASS,
owned services down PASS. Speed100 baseline35505041/current199277084ns,1000
baseline126823166/current414721792ns: FAIL, never relabeled. No new MinIO run.

SQLITE-DESIGN-AND-PLAN.md is current: replace metadata crate with persistence,
one shared storage/history flow, bounded atomic database bodies+metadata,
SQLite only; PostgreSQL unavailable and MinIO removed from active execution.
Pin WAL/FULL/foreign keys and macOS fullfsync/checkpoint settings/readback;
process-crash proof is distinct from power-loss claims. A0 design complete;
implementation/verification/measurement A1-A7 and separate namespace-count
qualification B0-B2 NOT_RUN. Count-growing collections in scan/namespace and
whole-slice C1 builder are explicit limitations; no whole-importer bound claim.

Design/evidence-only Production LOC143628->143628(delta+0), exact snapshot
root counter incl SQL; reference65417/core78211/old6025/new7821/rest64365
unchanged. Active retirement/moves happen in later source commits and must be
classified honestly. No push/PR/merge/third-party edit/aggregate gate.

Refinement incorporated before implementation:12 logical tables in one SQLite
Store, Objects/Metadata/History groups shared once; no whole-namespace atomicity
claim. Cross-group publication at adapter/publication.rs. Published packs stay
immutable; private append mechanics cannot mutate previous sealed digests.

## 2026-10-04 — durable database-only implementation; active competitive goal

The SQLite-only implementation replaces metadata crate and both physical ports,
shares Objects/Metadata/History in one database, and removes service/S3 packages
from active workspace. PostgreSQL explicitly unavailable. Common operations,
SQLite mechanics, immutable final inserts, borrowed/shared body publication,
first-wins bulk locator writes, WAL/FULL and all required readback settings are
implemented. Exact source/retirement/LOC classification is in the local commit.
See SQLITE-IMPLEMENTATION-ROUND.md for coverage and limitations.

Initial owning58 PASS; final changed-mechanism Persistence/Project40 PASS;
Clippy/fmt/boundary/tooling23 PASS at their recorded identities. Canonical
namespace oracle100/1000 and live-process SIGKILL recovery PASS. Historical
native/service tests archived and never counted as new coverage. The first
Clippy findings and compilation/lock errors are retained; no third-party identity
is added by the final narrow lock update. No source warmup/speed sample occurred.

Owner's new active goal uses10*candidate_ns<=11*matched_phase45_ns, all four Init
tiers and three history selections, no implicit budget. All competitive rows
remain NOT_RUN; history deadline exception/Init allocation criterion pending.
Count instrumentation is implemented; large-tier memory qualification and
conditional paging remain incomplete. No whole-import boundedness, terminal
success, release admission, push, PR or merge is claimed.

New direct Init/verifier examples compile; legacy PG/MinIO speed/cause entrypoints
refuse before service start. Final changed-counter publication/recovery7 PASS.
Actual statements include connection/readback/schema work; write commits are
separate from read commits and never misrepresented as observed sync syscalls.
New matched reference harness source is product-unmodified and recording-disabled.
Both fresh database creation and final close remain in the proposed complete
child comparison, with candidate checkpoint included. New runner/receipts pending.

## 2026-10-04 — prospective SQLite runner/Init contract

Retired metadata directory deletions omitted from bd9 staging are separately
committed in b604389a4. Exact LOC140936->137448(-3488), reference65417 unchanged,
core72031, active27987 unchanged, inactive core reference44044. No active compiled
mechanism changes; preserve first checkpoint and its original count.

SQLITE-STEP10-CONTRACT.md freezes complete child creation/import/checkpoint/close,
10*candidate<=11*matched baseline, conservative Init final allocation<=baseline,
regular-file cold attestation and fresh Store, release/seals/one-sample claims,
15s performance/9.5s proof. Source metadata/cache observation limits explicit.
New runner lists all seven and uses blocking wait4 with a deadline watchdog,
avoiding timer-poll quantization and lifetime aggregate RSS. Required history
budget/actual driver/proof remain pending. Harness4 tests PASS; no speed sample.

Final runtime-limit audit adds actual SQLite column/row-BLOB capability readback
and fail-closed minimums. Dynamic location/descriptor/ordinal queries now split
against both actual bind-variable and SQL-length limits; no zero-bound fallback.
This is a changed required-capability mechanism before sampling, so covering
publication/profile and namespace proof are run at the final identity.

Pre-freeze review fixes writable-open profile handling: only fresh creation can
switch journal mode. Existing incompatible/non-WAL databases are refused before
persistent journal mutation; a new public-API foreign-database test proves the
original DELETE mode and marker remain unchanged. All changes precede sampling.

First frozen SQLite100 reference attempt e97d6ee0f is NOT_RUN/sample_count0:
build FAIL because verify_namespace is owned by layerfs-server, not SDK. Raw
receipt/build stdout/stderr retained. Fix the reference build to select both
packages before any performance sample; a new harness seal/matched pair follows.
No old row is promoted and no speed sample is repeated.

## 2026-10-04 — first durable SQLite pairs; no competitive closure

First100/1000 matched arm each atc80a. Root/proof/cache/cleanup PASS; final
allocations5246976/20557824 versus7372800/23101440B. External100626732000
versus905740083ns passes frozen arithmetic but startup masks internal75.93
versus52.59ms (44.39% gap).1000217724792 versus140899000ns FAIL; internal
208.57 versus134.59ms. Do not claim Phase4.5-like speed. Current write commits
15/28, commit wall34774082/104273083ns; statements285/1308, VM37861/210182.
Read/write/SQL/save stages overlap; fsync syscall count UNAVAILABLE.

Retained systemSQLite3.51.0 EXPLAIN/plans and dbstat inventory captured under
declared populated diagnostic state, main hashes unchanged. Primary-key plans;
canonical346/5028076B and2003/20187652B exactmatches. Rawsql-explain.json and
comparison.json underissue302-sqlite-analysis-c80a26567. No resample or warming
of later timed Stores. Final-row status and all unrun work in
SQLITE-STEP10-FIRST-PAIRS.md. Finer reference count diagnostic and bounded
transaction-charge/reservation treatment pending. All-seven goal remainsACTIVE.

## 2026-10-04 — count diagnostics and bounded transaction treatment

Distinct count-driven1000 diagnostics atba7 use existing sealed reference/current
binaries and external first-party SQLite observer; no PG/MinIO, no speed resample.
Both root/proof/cold/cleanup PASS, no observer omissions. Reference operational
classes3542 statements/323375 VM; current whole-lifecycle classes1307/210246.
Scopes differ explicitly (reference creation outside operation counter); reference
records detailed timing tree. Raw receipts remain underissue302-sqlite-count1000-
{baseline,candidate}-ba7a27d02. Plans/inventories and first speed failures retained.

Implement independent canonical/physical4MiB-minus1 charges without added body
copy or changed wave/row/worker/cache bound. Carry acknowledged unused pack-ID
range between exclusive saves; consumed IDs never recycle on failure. Demand
counts actual unfinished groups, preserving worst-case construction coverage.
New bounds/reuse/abandonment tests and full100/1000 namespace oracle cover it.
Initial test fixture offered oversized ordinary FileState records, correctly
rejected by unchanged64KiB bound; corrected to legal46KiB records. The next
assertion incorrectly omitted the final-open-group publication; preserve that
failure and source-grounded correction. No gate or product bound was changed
for a test. Final proof result is retained separately.

Before this treatment's samples, Initv2 compares the complete product clock,
still including freshcreate/import/checkpoint/close. External child/envelope
remain reported/bounded. This tightens qualification after v1 startup hid a44%
product regression; old v1PASS/FAIL rows unchanged. History storage ceilings
retain their original strict< semantics. History deadline/driver remain pending.

## 2026-10-04 — transaction treatment: all four Init tiers FAIL retained

One matchedv2 pair per100/1000/10000/100000 at75a. Product timesreference/current
41570583/95885000,130144458/227274084,1590900959/2614821875,
5518847875/9201830208ns. All roots/proof/cache/cleanup PASS; all speed FAIL.
Final allocations7372800/5251072,23101440/20557824,314605568/314642432,
518029312/520343552B: large-tier allocation FAIL.100000 envelopes19301947333/
21727073917ns exceed15s, nonchild terms13774041458/12512674792ns. Never waive.

Write commits11/20/135/402; commit ns50726958/127525834/1207574352/2783380893.
Fewer commits15->11/28->20, but no latency win in this window.100000 payload
acquisition4597 reads/1079610281B plus270179431 batched B needs attribution.
RSS146800640 vs137887744B; entry/job/path capacities grow with count, no total
bound claim. Captured EXPLAIN/plans/inventory for all eight retained dbs, hashes
unchanged, diagnostics populated/warm. All detailed arithmetic and nonpass lines
inSQLITE-STEP10-TX-TREATMENT.md; rawanalysis/comparison JSON retained.

Next: bounded allocation/publication composition, repeated payload acquisition,
cold-preconditioning envelope cause and namespace-count qualification; actual
history driver/proof/deadline still pending. All-seven goal ACTIVE; no unchanged
arm replay or deadline/worker/buffer/durability relaxation.


### Operation-owned Reader treatment1735fb8b0

The public-API count test exposed repeated acquisition across demands within one
Reader (read_packs1->2). Retain existing bounded authenticated map across demands;
new Reader still pays. Four owning tests, scoped Clippy, fmt/boundary+23 pass.
Matched1000 v2 product143347625/224717708ns: timeFAIL (1.56764x).
Exact roots/cache/proof/cleanup/allocationPASS; storage23101440/20561920B,
envelopes254645208/1105380958ns, proof50093167/539248542ns. No established
material Init speed benefit. Preserve all earlierFAIL/NOT_RUN. Details and
commands inSQLITE-READER-CACHE-TREATMENT.md. Active goal unchanged.


### Logical reference acquisition treatmentb32c2a8eb

Valid extent leaf/128 persisted32KiB chunks triggered2batched+36individual
reads/12275774B before change; membership-preserving physical-root filter removes
all child acquisition. Missing dependency/exactreuse/readback coveragePASS;
14 scoped tests,3-package Clippy,fmt,boundary+23PASS.
Matched10000-v2 product1646280917/2450084791ns: speedFAIL1.488254383x.
Roots/cache/proof/cleanupPASS; allocation312508416/307597312B PASS in this pair;
envelopes2648136875/5121216333ns, proof615745708/978408792ns. C2reads now4911B
vs prior126603056B; no controlled cross-window latency gain claim. SQL13677,
VM2580946,writecommits136,commit1226820153ns. More boundary work remains.
All priorFAIL/historyNOT_RUN retained, goal active; report
SQLITE-LOGICAL-REFERENCE-TREATMENT.md and raw comparison JSON.


### Native cold-contract work f5abcee18 /2e6c0edd0 /2aaf6c7d1

Preserve complete invalidation/immediate/whole-input attestation inside15s,
seal native helper/compiler/source/flags/binary, resource scopes per child.
Cold count diagnostic100000:6675567875ns, zero product samples; all residency0.
First largest pair candidate cold12993975333ns leaves2005504333ns product,
times out; envelope15011762583nsFAIL, proofNOT_RUN, product time unavailable.
Warm cause82296invalidations and164592extra maps. Reuse shared read-only mapping,
retain writable capability check; descriptor metadata and streaming identity
fingerprint replace redundant traversal stat.4native qualification testsPASS.
New largest pair reference5376246166ns product/11622127959ns envelope; candidate
cold first5666985000+final3231373000ns,198000maps despite95358invalidations.
Candidate again times out, envelope15015172917nsFAIL, no product time/proof;
partial476270592B not allocation admission. No unchanged source resampling.
Detailed failures/commands/unrun rows inSQLITE-NATIVE-COLD-TREATMENT.md.
Next product work: publication/validation cost; actual history binding still
required. All-seven goal remains active. ProductLOC137505 unchanged(+0 each).


### Signature batchingdd19f21ea

512signature count514->3 statements;513rows4, one acknowledgement; stale/tie,
second-page FK whole-unit rollback and namespace proofPASS.10 scoped tests,
Clippy/fmt/boundary+23PASS. New matched1000 product140170375/250016458nsFAIL,
1000005568755291/6851490875nsFAIL. Largest envelope14125868875nsPASS; allocation
518029312/521052160BFAIL,3022848B overage. Both roots/cache/proof/cleanupPASS.
Actual statements320/9800,VM203940/14757575,writecommits20/401.
Current main logical515006464 vsbaseline515342336B; allocated521019392 vs
517996544B. Excess filesystem allocation rather than free-page evidence.
Disposable APFS transfer-extra-extents removes16MiB unused allocation, preserves
4096logicalB; same-size truncate does not. No production trim yet or gate relabel.
Detailed raw/diagnostic identities and all unrun work in signature treatmentreport;
all-seven goalactive. ProductionLOC137505->137544(+39); evidencefollowup+0.


### Explicit allocation releaseb9a31c755

Safe already-locked nix API releases only physical extra EOF extents after
unobstructed explicit checkpoint; no logical copy/truncate/new worker or profile
relaxation.4allocation/9publication/2process-kill/namespace tests16PASS; clippy,
fmt,boundary438+23 and4harnessPASS. Original package identities unchanged.
Largest matched candidate times out aftercold9577450416ns, productbudget
5421834500ns; envelope15012046292nsFAIL, proofNOT_RUN, partial341667840B not
admission. Reference5509839083ns product/12519750500ns envelopePASS.
10000pair completes:1561609792/2475484209ns timeFAIL; allocation308314112/
305098752B PASS. Release main308535296->305065984B (3469312B), checkpoint/release
6048625ns in product timer. Roots/cache/proof/cleanup/budgetsPASS. Raw counts3974
statements/2522963VM,writecommits141. All unrun cases and priorFAIL retained,
all7goalactive. Details SQLITE-ALLOCATION-RELEASE-TREATMENT.md.
CodeLOC137544->137678(+134),evidencefollowup+0, no dependency version/unsafe edit.


### Bounded pack queue carry22e5f1d3c

Remove unconditional wave queue drain, keep existing256KiB/512queued-row budget,
ready-wave publication and dependency/read/finish closure.1536small-object count
5->4packs; fullreadback and same-save carried-data closurePASS.19unique scoped
checks,3-package Clippy,fmt/boundary438+23PASS.
Matched1000 product137991542/228675584ns timeFAIL; roots/cache/proof/cleanup,
allocation23101440/20553728B and envelopes201397375/1143337458nsPASS. Current317
statements/203876VM,writecommits20,95bodies/20125893B; pack optimization does not
reduce acknowledgements in thisrow. Other6requiredcasesNOT_RUN at thisidentity.
All priorFAIL retained, goalactive. SQLITE-PACK-CARRY-TREATMENT.md has arithmetic,
raw IDs/repro; CodeLOC137678->137677(-1), evidencefollowup+0.

### Retained-history vehicle port (first-state diagnostics)

Add real single-producer C1/C2/C5 shared-SQLite history example and independent
readonly corpus read-back. One first stride10 state only: producer1,008,797,500ns
complete diagnostic; corrected verifier544,587,084ns,359paths/29content samples/
306,296authenticatedB. Initial receipt-schema failure retained. No cold claim,
independent root pins unchecked, no speed sample/admission. All historiesNOT_RUN,
matched baseline/cold/timing/budget still required. See
[vehicle report](SQLITE-HISTORY-VEHICLE-PORT.md); all-seven goal ACTIVE.

### Phase4.5 history reference transition diagnostics

Generated strict public-API baseline adapter, original7edddbdb8 product/manifest
unchanged and clean before/after release/locked build. Two states agree on both
roots and1,447distinct canonical identities /8,207,253B /same inventory digest.
Separate candidate readonly proof checks1,502paths and118samples/846,229B.
Candidate counts46publications/65write commits/1,978statements/161,159VM steps.
Full17-state baseline count diagnostic hits25s cap (25,014,771,375ns), SIGKILL
after15published roots; full roots/inventory INCOMPLETE, no cleanup admission.
No controlled-cache speed sample or full-history PASS; all historiesNOT_RUN.
No unchanged-arm retry or deadline extension. See
[reference/transition report](SQLITE-HISTORY-REFERENCE-TRANSITION.md).

### Exact initial pooled ordinal lookahead

Coalesce up to4exact fresh-leaf demands when synchronized pooled index is empty;
nonempty-index reuse/window behavior and later16leaf-block policy unchanged.
Fixed lookahead<=660values/one decoded leaf, released before admission; no
wave/cache/transaction/worker/durability expansion. Deterministic dense assignment,
deduplication, existing acknowledged-gap tests PASS;21covering tests,all-target
Clippy/fmt,boundary439/guard23PASS. Matched Init1000-v2 pair prospectively next;
no speed result yet. See [treatment](SQLITE-ORDINAL-LOOKAHEAD-TREATMENT.md).

Matched e0149ce56 Init1000:140,071,000/204,476,000ns,1.459802529x,timeFAIL;
allocation23,101,440/20,561,920B,roots/cold/proof/cleanup/envelopesPASS. Actual
reserve5/ordinal3/publication10/write commits18,307SQL/203,498VM,93bodies/
20,125,527B. Prior counts8reserve/9pub/20commits; mechanism works but grouping
addsone publication and target stillmissed. No cross-window causal time claim.
Raw ordlook1000 pair/compact checks and treatment report retain exactdata; other
3Init/3history NOT_RUN atidentity,all-seven goalACTIVE. CodeLOC+66,evidence+0.

### Pack allocation per concrete operation

Replace whole-wave speculative tail test with existing open/queued group bound
plus current object/value-group demand, before allocation. Original block/limits/
base acknowledgement unchanged, no newowner/retry. Deterministic91large+1024small
records21packs:old2reserve callsFAIL,new1PASS/readback. Corrected valid native
pressure test proves replenishment beyond initialrange,distinctIDs/readback.
23covering tests/full100/1000oracle,Clippy/fmt/boundary439/guard23PASS; fixture
limit/grammar failures retained. Matched Init1000 prospectively next; no speed
result yet. See [pack demand](SQLITE-PACK-DEMAND-TREATMENT.md).

Matchedcf360c1d1 Init1000:142,588,792/223,193,959ns,1.565298057x,timeFAIL;
allocation23,101,440/20,553,728B androots/cold/proof/cleanup/budgetsPASS. Actual
reserve4/ordinal3/write commits17/pub10,303SQL/203,509VM,93bodies/20,125,509B;
packreservationmechanismremovesonecall. No cross-window causaltimeclaim. NEXT
PREREQUISITE paired small-step cause diagnostic with baseline publicSaveOutcome
profiles and equivalentcandidatework observations, SQLplans/counts and missing
observations explicit, beforeanotherproductoptimization. Scope/route equivalence
andobserverfreeze required; current speed receipts unchanged,all-sevenACTIVE.

### Paired cause diagnostic vehicle and observer (prospective)

Shared snapshot of real bounded namespace caller through publicC1/C2/C5, narrow
unmodifiedPhase4.5 facade capturesSaveOutcome profiles; candidate retains final
drain selection/group telemetry. Fixed source-stage and publicSQLite trace observers
(rawprofilequantization separate; nativeVM reset/unqualified), representative
native readonlySQLplan tool. Capability3.51.0 configPASS/oneSELECT6VM, notproduct
sample. Build adapters compile after lifetime/error-conversion fixes;18C2checks/
full100/1000oracle,Clippy/fmt/boundary439/guard23PASS. Cause child execution still
pending; [prospectivecontract](SQLITE-PAIRED-CAUSE-CONTRACT.md), all-sevenACTIVE.

### Paired small-step cause report at e5163f151

Onecold diagnostic child perarm qualified:root/inventory2003IDs/20,187,652B/
samedigestmatchbothproductionroots;independentproof/cold/budgets/cleanupPASS.
Init140,896,750/229,324,417ns,diagnostic extra88,427,667ns. Fileowner
100,174,542/178,857,917ns; worker-sendΣ247,848,957/525,319,422ns backpressure,
not isolated hashCPU. FULL5,370,878/5,927,064ns,group1,530,614/1,871,887ns.
Trace3640/305statements,326740/203615VM;writeBEGIN28/18;COMMITtrace
43,585,792/70,006,792ns,globalcandidatewrapper129,505,084ns broader/unpartitioned.
NativeVMafterobserverresetUNQUALIFIED,traceused; no syscall/fsync count.
Native3.51plansPKlookups/oldtemp-scope subqueries/newguardedFKchildscan,main
hashesunchanged. [Fullpairedreport](SQLITE-PAIRED-SMALL-STEP-COMPARISON.md) includes
comparablework,distinctinclusive/unequalscopes,SQLcounts/plans/missingobservations.
No furtheroptimization selected beforereport; nextmeasurementacknowledgement
prepare/step/reset/VFSpath, noWALweakening. All-sevenACTIVE/notadmission.

### Statement lifetime diagnostic revision2 (prospective)

Add seven fixedordinarystatement/COMMIT phase counters around samecalls/order:
prepare,bind,next(step+DONEreset),map,cursordrop,status,statementdrop. No query/
retry/policy/limitschange. Publishedrusqlite inspection showsnextincludesreset;
no dependency edits.18coveringC2checks,Clippy/fmt,boundary440/guard23PASS;
namespaceoracle coversunchangedoutput. Onefreshpairedcold diagnostic perarm next,
revision1 retained. See [lifetimecontract](SQLITE-STATEMENT-LIFETIME-CONTRACT.md).

Revision2qualifiedroot/inventory/cold/proofs/budgets:COMMIT102,156,536ns,
next(step+DONEreset)102,118,960ns; remainingnamedprepare/bind/drop/status~29us,
map0. Trace57,301,376ns narrower; no inventedpure-reset/syncresidual. Revision3
prospectiveAPIdelegationstep/reset observer,4fixedrows,capability2steps/1reset,
no dependency/VFS/policy changes. Newpairedone-child/arm identitynext.

Revision3qualifiedAPIcalls/root/inventory/proof/cold/budgets:referenceCOMMITstep
27calls52,099,082ns/reset0;candidate31step113,223,999ns/reset193,791ns.
ActualSQLite_stepdominates,notmapping/reset/cache-drop. Tracecallsagree;same
args/return/one-delegate. [Lifetime/APIresults](SQLITE-STATEMENT-LIFETIME-RESULTS.md)
retainsrevision2/3scope/data/limits. NextqualifiedVFSwrite/syncobserverneeded,not
weakerdurabilityorselectedoptimizer. No admission/allsevenPASS,goalACTIVE.

### Delegated VFS diagnostic revision4 (prospective)

PublicAPIstep dominatesrevision3, resetminor. Addfirst-partywholeVFS/IO delegation
toactualpriorunix default, samearguments/return/options; observer176B/fileheader,
fixedclass/flags/call/byte/ns counters. TinyWAL/FULLtransaction/checkpoint/read42
probePASS,live0/errors0. No product/dependency/profilebufferchange; nameddefault
isinstrumentedwrapperandunderlyingidentityreported. Onefreshpaired--vfschild/arm
next,samecold/budgets/proofs. [VFScontract](SQLITE-VFS-CAUSE-CONTRACT.md).

### Delegated VFS revision4 results

One child per arm qualified: root/inventory, independent proof, cold, cleanup,
observer and budgets PASS. Whole-lifecycle VFS writes 21,416,960 / 42,235,980 B
and 32,745,796 / 63,789,514 ns; sync 0 / 34 calls and 0 / 43,442,045 ns.
Init 131,673,875 / 211,486,250 ns. Counters include bootstrap/checkpoints and
nest in step clocks; no physical syscall/device-byte or exact partition claim.
Publication/write work is next priority while preserving durability/limits.
[Full report](SQLITE-VFS-CAUSE-RESULTS.md) and append-only receipts retained.
Latest ordinary Init1000 speed FAIL; all seven selections remain active.

### Bounded pack INSERT pages (prospective)

Replace95per-body INSERTs observed by revision4 with borrowed multi-row pages
bounded by actual SQL limits/512rows, within the same publication transaction.
513pack count/readback and second-page conflict rollback proof PASS; existing
publication/ordinal/transaction checks PASS, fixture errors retained in report.
No profile/buffer/cache/schema/dependency change. One matched ordinary Init1000
pair next; allseven remain active. [Treatment](SQLITE-PACK-INSERT-PAGES.md).

### Small-pack INSERT matched Init1000 result

At c55bfa2f3 reference126,033,958ns/candidate229,371,791ns,1.819920557x,timeFAIL.
Root/proof/cold0/cleanup/budgets/storagePASS (23,101,440/20,557,824B). Candidate
300statements/203,513VM/32transactions/17write commits/10pub/4reserve/95bodies.
Small-pack count benefit does not remove the measured write/acknowledgement term.
[Full treatment and receipts](SQLITE-PACK-INSERT-PAGES.md) retain exact arithmetic
and NOT_RUN selections; allsevenACTIVE. Next causal count: actual per-publication
byte/row occupancy before any boundary change. No unchanged-arm speed retry.

### MEMORY/OFF diagnostic (prospective)

Priority is matched effective MEMORY/OFF mutation/Init comparison before another
publication optimizer. ProductionWAL/FULL checks remain; tooling switches after
validation and before first mutation, with actual before/effective/final readback.
Candidate initialWAL validation/transition remains measuredbootstrap and is
explicitly a lifecycle difference. Proof uses independent byte-copy WAL header,
measured original unchanged, within9.5s. Capability/read42/refusal/copy checks,
C-Werror/Python/reference seams/exampleClippy/fmtPASS; product unchanged.
[Prospective contract](SQLITE-MEMORY-OFF-DIAGNOSTIC-CONTRACT.md). One new labeled
paired diagnostic perarm next, no production speed rerun or admission claim.

### MEMORY/OFF revision1 observer/proof failures; corrected identity next

ReferenceFAIL beforeInit: observer wrongly required4KiB for existing1KiB C5;
first temporary-scope observation preceded2KiB C2 geometry. Candidate actual
MEMORY/0 child completed root/inventory but proofcopy lacked WAL read sidecars,
readonly native SQLITE_CANTOPEN14; pairINCOMPLETE, allreceipts retained.
Revision2 observes main mutations, records realreferencegeometry, preserves
candidate4KiB and budgets; proofcopy retains empty WAL/SHM via publicfilecontrol.
2KiB/1KiB/4KiB capability and unchanged sampled native verifier PASS on fresh
proofcopy, original measured hash unchanged. One new corrected observer pair
next; no production speed replay/profile weakening. See [contract](SQLITE-MEMORY-OFF-DIAGNOSTIC-CONTRACT.md).

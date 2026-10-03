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

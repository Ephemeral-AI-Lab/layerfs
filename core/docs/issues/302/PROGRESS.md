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

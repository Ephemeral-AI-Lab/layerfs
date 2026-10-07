# Historical evidence and prospective proof review — retained reviewer report

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Retained verbatim from one explicitly authorized read-only review subagent of
> the S8 specification task, 2026-10-08, local `main` at `32d969776`
> (product pin `f0797c646`). The reviewer ran no build, test, mount or
> measurement and edited nothing. Findings are input; their accepted, rejected
> or deferred disposition is in the [finding ledger](05-finding-ledger.md), and
> the [S8 specification](../../S8-SPECIFICATION-20261008.md) owns every decision.
> Paths and line numbers are the reviewer's citations at that pin.

---

# Historical evidence review and prospective S8 proof-plan skeleton

Read-only review; nothing was built, run, measured or edited. HEAD `32d969776151588aec5aee1e9296658b8d00908d`; the three untracked owner notes are untouched.

## A. Scope, pins, artifacts read, unavailable

**Pins.** Main `32d969776`; the dispatch prompt pins reviewed source `f0797c646`. Experiment commit `1451b68a720bbe2175a103dd9b35693ad05e2be1`, read only through `git show` / `git ls-tree`.

**Read in full**
- Policy: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/AGENTS.md`, `core/AGENTS.md`, `docs/general/{agent-measurement-policy,benchmark_rules,optimization-guide}.md`, `benchmark_agent_report.md`, `core/benchmark/fs-bench-pro/AGENTS.md`.
- Issue snapshots: `core/docs/issues/307/checks/s8-spec-prompt-20261008/sources/issue-{305,306,308,309,313,314}.json` (bodies and all comments) and `02-source-evidence-index.json`.
- Experiment commit, `core/docs/issues/305/`: `EXPERIMENT-PLAN`, `PREPARATION-REPORT`, `STAGE-A/B/C-REPORT`, `STAGE-B-PROTOTYPE-SCOPE`, `STAGE-B-SUPPLEMENTARY.json`, `SUMMARY`, `E08-LONG-LIMIT-AMENDMENT`, `LONG-CASE-LIMITS`, every `A2-*-CONTRACT/REPORT`, `CF-FSBENCH-CONTRACT/REPORT/V2-CONTRACT`, `CF-COMPUTERD-REAL-TREE-CONTRACT/REPORT`.
- Experiment commit, `core/docs/issues/306/`: `BENCHMARK-PLAN`, `EXPERIMENT-LEDGER`, `RUNTIME-IDENTITIES.json`, `COMPARISON-REPORT`.
- Experiment commit, `core/experiment/`: `AGENTS.md`, and in `real-tree/`: `workload.py`, `runner.py`, `manifest.py`, `prepare.py`, `oracles.py`, `launch.py`, `exclusions.json`, `largest-path`, `Dockerfile`.
- Raw receipts parsed record by record: `STAGE-A` (51), `STAGE-B` (2), `A2-READDIRPLUS` (12), `A2-WALK` (14), `A2-NOPERM` (11), `A2-STATELESS` (24), `A2-PIN` (16), `A2-NEGLOOKUP` (4), `A2-IDENTITY` (7), `A2-IDENTITY-CHURN` (4), `CF-FSBENCH-V2` (60), `CF-COMPUTERD-REAL-TREE` (20), `CF-COMPUTERD-STOCK-X64` (9), `306/JUICEFS` (32), `306/DRIVE9` (32 + 31 retained).
- Main, `core/docs/issues/307/`: `PRE-S8-COMPLETION`, `PRE-S8-ACCEPTANCE-DECISION`, `PRE-S8-F13/F14/F15`, `PRE-S8-RESOURCE-GROWTH-RESULTS`, `DISPOSABLE-WAL-MATRIX`, `SEAL-ALLOCATION-STRIDE1`, `PRE-S8-INIT-WAL-RESULT`, `PERFORMANCE-ACCEPTANCE-S7-S12`, `E04-ORIGINAL-RECEIPTS`, `E04-NATIVE-BACKING`, `E04-DISPOSAL-CONSISTENCY`, `HANDOFF-E04-CLOSED-S7-S9`, `HANDOFF-S8-SPECIFICATION-20261008`; and `core/docs/issues/303/07-implementation-validation.md`.

**Read only in part (claims from these are marked where used)**
- `INCUMBENT-RESTORATION-RESULTS-20261007.md`: lines 1–67 only.
- `PRE-S8-WAL-BASELINE-DECISION-20261007.md`: partly.
- `CLUSTER-ONE-END-REGRESSION-RESULTS-20261006.md`: grep only.
- `CF-COMPUTERD-STOCK-X64-CONTRACT/REPORT`: key lines only.
- `CF-FSBENCH-V1-RECEIPTS.json`: status and identity tally only.

**Not read**
- `PRE-S8-F0` to `F12` reports.
- Frozen family contracts: `families/phase7_sqlite.py`, `phase7_history.py`, `shared/sqlite_contract.py`, `SDK-VERIFIER-LITE-20260924.md`.
- Experiment `src/*.rs`. Mount-option and profile statements below therefore come from plans, contracts and reports, not from the Rust source.
- `306/DRIVE9-REPORT.md`, `306/JUICEFS-REPORT.md`, `CF-COMPUTERD-BUILD-REPORT.md`, `PLAN-303-PROPOSALS.md`, the smoke and `computerd_*` / `drive9_*` / `juicefs_benchmark.py` scripts.

**Unavailable**
- Raw run directories (`benchmark-results/experiment/real-tree/...`: stdout/stderr bodies, `tree.json`, progress files). They live only in the experiment worktree, which I was barred from.
- For B/E10/F specifically, the report says "No complete tree.json was emitted before the verifier was killed."
- `A2-OPTIMIZATION` (`e04-a2-narrow-enum-v1`) has no receipts JSON at the commit; its numbers are report-only.

**Discrepancy found in a source (preserve, do not fix).** `A2-STATELESS-REPORT.md` line 5 says "nineteen OK and five SLOW". Its own table and the raw receipts give 20 OK / 4 SLOW (E03/N is OK at 8.751261212 s; SLOW on A2, A2W, A2WN, A2S).

### Distinction checks

| Claim | Result | Evidence |
|---|---|---|
| A2 is ext4 passthrough, no Store or Commit | CONFIRMED | `EXPERIMENT-PLAN.md` §3; CF contract "pass requests through to an ext4 directory and keep no store"; `306/COMPARISON-REPORT.md` line 25 "permission-enforcing ext4 passthrough, native ARM64, with no store work"; `PERFORMANCE-ACCEPTANCE-S7-S12` "A2 has no real canonical host acquisition or Commit" |
| Promoted profile is a prospective candidate | CONFIRMED | issue-305 body, "Owner decision — promote A2, 2026-10-04": "It does not change the product implementation or qualify a release"; issue-314: "These remain candidates until their mounted correctness/resource proofs pass" |
| A2O/A2P not promoted | CONFIRMED | issue-305 body: "**A2O and A2P are not promoted.**"; `A2-READDIRPLUS-REPORT`: "Always-plus is experiment-only." |
| Permission-removing arms not promoted | CONFIRMED | `A2-NOPERM-REPORT`: "**A2WN enforces no permissions inside the mount**… not a product candidate"; A2S, A2S1, A2SL, A2SI likewise. `A2-PIN`: "A fixed pin is a harness setting, not a product policy." |
| B/E10/F exited 0, verifier hit 10 s, FAILED | CONFIRMED from raw receipt | `"status": "FAILED"`, `"cause": "separate verification exceeded 10 s"`, `"verification_within_10s": false`, `exit_code: 0`, `survival_root_exists: true`; supplementary `"tree_verified": "UNAVAILABLE"` |
| Stage C NOT_RUN | CONFIRMED | `STAGE-C-REPORT.md`: "**Stage C was not started.**"; C01–C08 `NOT_RUN`, cause "B E10/E12 prerequisite unmet" |
| #306 numbers INELIGIBLE, not a ranking | CONFIRMED | every completed row carries `status`/`display` `INELIGIBLE` and `admission_eligible: false`; report: "Architecture, timer, backend and cache differences preclude a matched product ranking." |
| Fixture counts | CONFIRMED | receipt `master`: file 103108, directory 16867, symlink 10070, bytes 3475776149, sha256 `98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658` (130,045 entries). S = 31,215 / 3,781 / 28 = 35,024 entries, 1,349,267,039 B. Replay = 95,021 entries / 2,126,509,110 B (Stage A `progress`). Z = empty directory (`clone_method 'none; empty directory'`). Master HEAD `639ed015397290b3745d163aafe02ffee4aa3f84` |
| Pre-S8 Q1/Init fixtures are not F | CONFIRMED | Init: synthetic 100/1000/10000/100000 files (5,000,000 / 20,000,000 / 300,000,000 / 500,000,000 B). History: `deepseek-history-data` manifest `03f21acf…` (180,444 source files). F11: 28 mixed paths, 100000 names, 500000000-byte dense, 1000000019-byte sparse. E04: 16,777,216-byte `base.bin`. None is the #305 tree |

Qualifications the primary should carry:
- `survival_root_exists: true` is root-directory existence only; it is not tree survival.
- Stage B ran on the **A1** profile (zero TTL, direct I/O), not A2. No overlay slice on the promoted profile exists.
- No artifact shows F ever acquired by current core Project Init into a sealed Store. `303/07` §5.2 (2026-10-05) says "Native Init currently refuses symlinks (P12)". Whether P12 has since landed is **not verified**.

## B. Comparison and eligibility matrix

### B.0 Shared envelope for every #305 record

- **Platform:** Linux `6.12.76-linuxkit`, ARM64 Docker VM shared with other owners.
- **Launch:** `docker run --rm --pull never --network none --privileged --security-opt no-new-privileges -e LAYERFS_CONSTRUCTION_WORKERS=1`; command run as `/bin/bash -o pipefail -c <body>`, uid/gid 1000.
- **Setup:** `GNU cp --reflink=never -a` byte copy of the base.
- **Cache:** `cache_contract: "sync + VM drop_caches=3, no residency proof"`; `admission_eligible: false`; one record per cell.
- **Status rule (`runner.py`):** `OK` if verified and in-container mount+exec+unmount ≤ 15 s; `SLOW` if verified and above 15 s; `FAILED` on a verification miss or verifier over 10 s; `EXCEEDED` at the hard stop; `INVALID` otherwise.
- **Hard stop:** 600 s for E02, E03, E08, E11, E12, E13, E14 (owner amendments); 60 s otherwise.
- **Timer erratum:** every contract before `cf-fsbench-v2` rounds Exec up by up to about 51 ms and Unmount by about 16 ms. Mount (1 ms poll) is unaffected. Receipts were not rewritten; Stage A, CF v1 and the A2 follow-ups carry no `timer` key.

**Commands (`runner.py`, verified)**

| Case | Body |
|---|---|
| E01 | `true` |
| E02 | `find . \| wc -l` |
| E03 | `tar -cf - . \| wc -c` |
| E04 | `git -c core.fsmonitor=false status --porcelain=v1` |
| E05 | `git log -20 --stat && git diff --stat HEAD~1` |
| E06 | `git grep -n tool -- packages \| wc -l` |
| E07 | `node -e "require('typescript')"` |
| E08 | `node node_modules/typescript/bin/tsc -b tsconfig.client.json` |
| E09 | `node --test .github/review-ownership/*.test.mjs` |
| E10 | `sed -i '1i// experiment 305' packages/acp/acp/src/codec.ts && git add -A && git commit -m 'experiment 305'` |
| E11 | `git checkout -q HEAD~100 && git checkout -q -` |
| E12 / E13 / E14 / E16 / E17 | `python3 /code/workload.py copy` / `link` / `remove` / `append` / `churn` |
| E15 | `cp "$(cat /code/largest-path)" experiment-large && cmp …` |
| E18 | E04 after an untimed first run in an earlier mount of the same arm |
| E19 | E18 plus every 50th tracked regular file changed on the backing tree between mounts (282 of 14,090) |
| C01–C12 | Cloudflare fs-bench bodies on Z, wrapped `{ body; } >/dev/null`: MK1000; +stat; +rm; MKTREE; +find; DD 64 MiB; +cp; +cat; cat (untimed prep DD); cp (prep); overwrite `conv=notrunc` (prep); `git init` + commit 100 files |

### B.1 Contract-level rows

Prerequisite and matched-arm notation: **L** = S8 product mount; **N** = native ext4 at a new identity; **P** = a newly authorized promoted-profile passthrough at a prospective identity (owner decision G-4).

| Id | Source / image | Backend and arms | Cache | Oracle | Budget | Actual verdicts | Reusable context | Disqualifying differences | Proposed matched arm; prerequisite |
|---|---|---|---|---|---|---|---|---|---|
| Stage A (CP2/CP3) | `d126c2c25`/`afe8692f09a7` (E01, E04); `dd38e2af7`/`1958eb1a5d97`; `cdc3b9968`/`ebf2f5c224ed` (bulk-long) | N ext4 volume; A1 passthrough (zero TTL, direct I/O); A2 passthrough (60 s TTL, KEEP_CACHE) | B.0 | exit+stdout hash only for E01–E07; + scoped tree for E08, E10, E11, E14–E17; + `replay_faithful` for E12, E13; tree read from ext4 backing **after unmount** | 15 s SLOW line; 60/600 s stop; verifier 10 s | 51 records, 36 OK / 15 SLOW; E09 on N, A1, A2 `NOT_RUN: Oracle stdout contains nondeterministic TAP durations` | Workload bodies, request totals (E12: A1 5,834,083; A2 698,292), fresh-mount cost shape, native floor | No Store or Commit; passthrough persistence; polling timer; no residency proof; every F host command above 15 s | N + L (+ P); prerequisites per workload in C |
| Stage B (CP5) | `1aefa496534c561d8fee4b87e94a1d3c547a9c0e` / `sha256:dede5f480490…` | B: four-table SQLite overlay prototype, A1 profile, local-dir base, stand-in publish | B.0 | as Stage A, plus survival tree read through a fresh mount inside the 10 s verifier | 60 s stop | E01/F `"status": "OK"` (mount 0.140428125, exec 0.025487209, unmount 0.014400334 s). E10/F `"status": "FAILED"`. E01/S, E02–E08, E09, E11–E17 NOT_RUN | Only that a through-FUSE whole-layout verifier on F can miss 10 s; E10 exec 30.758211097 s with 4,691,869 SQL statements / 1,259,910 transactions (failed cell, not a control) | Throwaway prototype; not A2 profile; no canonical Store; verdict FAILED | None. Cannot be an arm or a speed control |
| Stage C | none | none | none | none | none | "**Stage C was not started.**" C01–C08 `NOT_RUN` | none | none | Whole lifecycle with Commit: S10 |
| `e04-a2-narrow-enum-v1` | `eb1017d25` / `sha256:ad972159c5d2…` (report only) | N, A2O | B.0 | exit+stdout+tree | 60 s | E04 N 1.866620876 OK; A2O 4.143380001 OK | Request-shape hypothesis only | "not proof that the code caused a speed-up"; A2O not promoted | none |
| `a2-readdirplus-v1` | `614511ead` / `6891f341504a` | N, A2, A2O, A2P | B.0 | E04, E10 with tree; E02 stdout only | 60/600 s | 12 OK | READDIRPLUS mixed sign: −6.323% / −4.490% / +7.882% versus A2 | A2P experiment-only; host 18.79–36.21 s | Adaptive readdirplus is a hypothesis for L only |
| `a2-walk-v1` | `a9228b9b2` / `952cfb7c4c47` | N, A2, A2O, A2P, A2W | B.0, with another owner's containers starting in the window | as above | 60/600 s | 13 OK, 1 SLOW (E14/A2 16.768777) | Round-trip diagnostic (warm smoke, not a record): FUSE fstat 43.84 µs unpinned vs 5.20 µs on one CPU | Declared interference; A2W unpromoted | none |
| `a2-noperm-v1` | `9fdbfe529` / `d03c1c64123e` | N, A2, A2W, A2WN | B.0 | tree; E12 `replay_faithful` | 60/600 s | 8 OK, 3 SLOW (E12 on A2, A2W, A2WN) | Cost of retained permissions: one GETATTR per directory change (E12 94,149; E14 96,102; E17 20,001) | A2WN enforces no permissions; trust probes not rerun | Count hypothesis for L with permissions retained |
| `a2-stateless-v1` | `1ada59d67` / `46dc50afc0e4` | N, A2, A2W, A2WN, A2S | B.0 | E04, E10, E17, E14 tree; E05, E03 stdout only | 60/600 s | Raw receipts: 20 OK / 4 SLOW (report headline says 19/5) | OPEN/FLUSH/RELEASE elision effect; kernel 6.12.76 refused-OPEN behaviour | A2S has no permissions; two unexplained control cells (E05/A2WN, E03/A2W) | FLUSH elision hypothesis only |
| `a2-pin-v1` | `b2e08ebb4` / `97ba5edd08aa` | N, N1, A2S, A2S1 (CPU 7 pin) | B.0 | as above | 60/600 s | 12 OK, 4 SLOW (E08, all arms) | Scheduler sensitivity; native E08 71.535252575 s | Pin is a harness setting; A2S has no permissions | none |
| `a2-neglookup-v1` | same binary and image | N, A2WN, A2S, A2SL | B.0 | tree + `replay_faithful` | 600 s | 1 OK, 3 SLOW | Negative-lookup request reduction 676,038 → 604,146 → 460,360 | No-permission arms | Negative-entry hypothesis for L (needs coherence proof) |
| `a2-identity-v1` | `b6660f2d9` / `17c3cc8bd9f3` | N, A2, A2S, A2SI | B.0 | exit+stdout+tree (`.git/index` excluded) | 60 s | 7 OK | Git stat identity: 254/254 tracked files keep inode across mounts on A2SI vs 3/254 on A2/A2S (smoke) | A2SI has no permissions and borrows the backing inode "which an overlay does not have" | L must supply its own stable ino/ctime; see hazard G-h2 |
| `a2-identity-churn-v1` | `9cde71bff` / `f9ae1a614f65` | same arms | B.0 | same | 60 s | 4 OK | 282 of 14,090 files changed; A2SI 21,922 requests | same | S10, or two prepared roots |
| `cf-fsbench-v1` | `0103e5d78` / `88bab972520d` | N, N1, A2, A2S, A2S1 on Z | B.0 | full-tree SHA256 of Z | 60 s | 60 OK | none: resolution-limited, "not pooled" | Polling timer | superseded by v2 |
| `cf-fsbench-v2` | `80cf06ba9` / `970549974e24` | same arms on Z | B.0; `timer: "monotonic; phase ends at pidfd readiness"` | full-tree SHA256, complete for Z (C12 excludes `.git/index`); verifier 0.0013–0.109 s | 60 s; host 0.330–1.354 s | 60 OK; network rows NOT_RUN | The only correctly timed fresh-mount floor: A2/A2S mount 17–20 ms, unmount 5–18 ms | Empty base; no Store; A2S arms have no permissions | N + L (+ P) on an empty or declared small root: S8 only |

### B.2 Per-cell actuals (seconds)

**Stage A: Exec, N / A1 / A2**

| Cell | N | A1 | A2 |
|---|---|---|---|
| E01/F | 0.029041 | 0.024430 | 0.025510 |
| E01/S | 0.026982 | 0.028193 | 0.025920 |
| E02/F | 0.229466 | 10.359265 | 4.162990 |
| E03/F | 9.217839 | 70.143894 SLOW | 21.399468 SLOW |
| E04/F | 2.222719 | 17.418609 SLOW | 4.324254 |
| E05/F | 2.851299 | 42.261186 SLOW | 5.137499 |
| E06/F | 0.234203 | 1.912244 | 0.895150 |
| E07/F | 0.335575 | 0.280099 | 0.280095 |
| E08/F | 78.100455 SLOW | 96.242262 SLOW | 75.748050 SLOW |
| E10/F | 1.862166 | 27.429389 SLOW | 4.716923 |
| E11/F | 2.429736 | 28.946412 SLOW | 6.423870 |
| E12/S | 10.158530 | 242.831911 SLOW | 42.201692 SLOW |
| E13/S | 12.224564 | 204.707863 SLOW | 58.601360 SLOW |
| E14/F | 0.742724 | 26.216855 SLOW | 16.288058 SLOW |
| E15/F | 0.290660 | 5.230302 | 0.640569 |
| E16/F | 0.125400 | 0.231546 | 0.226881 |
| E17/F | 0.281396 | 5.294007 | 3.344438 |

E01/F on A2: mount 0.028993, unmount 0.015009, total 0.069512.

**Follow-ups: in-container totals from raw receipts**

- **readdirplus (Exec):** E04 N 2.014887 / A2 4.158740 / A2O 4.355688 / A2P 3.895786. E02 0.232566 / 4.208433 / 4.164438 / 4.019483. E10 2.121000 / 4.350425 / 4.286776 / 4.693317.
- **walk:** E02 N 0.233280, A2 4.156092, A2O 4.425709, A2P 4.875624, A2W 2.902192. E04 2.020759 / 4.526070 / A2W 4.553259. E10 2.228290 / 4.741350 / A2W 4.594828. E14 N 0.794695, A2 16.768777 SLOW, A2W 12.489502.
- **noperm:** E14 N 0.956515, A2W 11.629507, A2WN 8.486792. E17 N 0.336331, A2 3.601509, A2W 4.171714, A2WN 3.287533. E12/S N 12.040808, A2 44.048837 SLOW, A2W 43.179605 SLOW, A2WN 41.380672 SLOW.
- **stateless:** E04 2.066525 / A2 4.397857 / A2WN 4.434058 / A2S 3.385219. E10 1.974273 / 4.627903 / 4.696130 / 4.081913. E05 2.949881 / 5.312435 / 6.498802 / 4.376585. E03 N 8.751261 OK; A2 22.065377, A2W 28.283973, A2WN 21.004691, A2S 18.848986 (all SLOW). E17 0.279831 / 3.522023 / 3.007898 / 2.589361. E14 N 0.796086, A2WN 8.536068, A2S 8.399816.
- **pin:** E02 N 0.236366, N1 0.238733, A2S 2.889022, A2S1 1.335683. E14 1.028363 / 0.747994 / 8.151974 / 2.540504. E04 1.886449 / 1.917895 / 3.344449 / 2.767227. E08 71.535253 / 93.304773 / 73.339195 / 99.090950 (all SLOW).
- **neglookup (E12/S):** N 11.611615 OK; A2WN 33.788260, A2S 32.569615, A2SL 27.339878 (SLOW).
- **identity:** E18 N 0.078895, A2 4.403938 (Exec 4.346287), A2S 3.555239, A2SI 0.577977. E04 N 1.966744, A2S 3.647849, A2SI 3.773572.
- **churn (E19):** N 0.133511, A2 4.620389 (Exec 4.529085), A2S 3.585959, A2SI 0.767405.
- **cf-fsbench-v2 (Exec ms, N / N1 / A2 / A2S / A2S1):**

| Case | N | N1 | A2 | A2S | A2S1 |
|---|---|---|---|---|---|
| C01 | 31.8 | 25.7 | 183.4 | 225.1 | 129.6 |
| C02 | 827.2 | 693.6 | 965.0 | 905.7 | 785.4 |
| C03 | 32.5 | 30.9 | 410.8 | 293.4 | 125.7 |
| C04 | 720.8 | 600.5 | 952.6 | 872.0 | 773.0 |
| C05 | 743.1 | 652.1 | 949.6 | 881.4 | 807.8 |
| C06 | 34.4 | 33.5 | 78.6 | 74.6 | 53.7 |
| C07 | 64.9 | 58.7 | 171.6 | 181.3 | 100.3 |
| C08 | 46.7 | 63.5 | 106.1 | 102.3 | 65.9 |
| C09 | 36.4 | 36.3 | 106.2 | 116.1 | 86.5 |
| C10 | 46.2 | 37.2 | 178.2 | 158.4 | 115.9 |
| C11 | 35.4 | 34.5 | 85.8 | 75.2 | 46.8 |
| C12 | 33.1 | 34.1 | 189.3 | 194.7 | 98.5 |

I re-checked C01, C03 and C12 against raw keys; the other rows are from the report.

### B.3 #306 rows

Shared rules: one cell each; "Budget 15 s, no exceptions"; inner lifecycle stop 15 s; full post-command verification SKIPPED; `admission_eligible: false`.

| System | Backend / architecture | Actual dispositions (raw receipts) | Disqualifying differences |
|---|---|---|---|
| **A2 column** | Reused historical ARM64 Stage A / identity / v2 receipts | No new samples. E18 4.346287 and E19 4.529085 are Exec values from the identity contracts | Polling erratum on E rows; passthrough |
| **Computerd 0.4.0** | Official x86-64 binary `da2a9f64…` emulated on the ARM64 VM. Real-tree uses file-backed SQLite WAL/normal; fs-bench uses default in-memory SQLite. Root daemon, `allow_other`, no `default_permissions` | Real-tree: 7 `INELIGIBLE` (E01/F exec 0.051985792, mount 0.833595251, unmount 0.067127125; E01/S; E06 5.123792669; E07 0.842591625; E09 1.682833917; E15 2.416458666; E16 1.464359584). 11 `EXCEEDED` (E02, E03, E04, E05, E08, E10, E11, E12, E13, E14, E17: `TimeoutError('complete inner lifecycle reached 15 s')`). E18, E19 `NOT_RUN`. Partial progress E12 6,900 entries / 1,303,973 B; E13 7,000 / 1,417,399 B; E17 9,800. fs-bench (REPS=1, WARMUP=0): 6 groups `INELIGIBLE`, C02/C04/C05 `EXCEEDED` | `"INELIGIBLE: emulation, timestamp mismatch, no residency proof"`; `"timestamp_match": "FAIL: unsupported persistent input mtimes; no matched performance claim"`; E09 TAP equality SKIPPED |
| **Drive9** | Native ARM64, TiDB unistore + MinIO, `--profile=none`, client 512 MiB / 1 CPU | 4 `INELIGIBLE` (E01/F exec 0.009270334, host budget FAIL; E01/S 0.008917041; C04 2.426050126; C12 10.826101339). 4 `EXCEEDED` (E12/S, E13/S, C02, C03). 3 `FAILED` (C06, C07, C08). 2 `INCOMPLETE: exit0 with filesystem errors` (C01, C05). 16 `NOT_RUN` ("E01F observed fixed complete-command cost 15.548593374s exceeds15s; no budget enlargement"). 3 `NOT_RUN final: preconditioning/drain FAILED` (C09–C11). Preparation: F v1 FAILED 2672.554550595 s (OOM); F v2 PREPARED 3973.693120561 s | Remote backend; "provider readiness may warm provider heap"; unmount drain about 5.5 s on C01/C04/C05 |
| **JuiceFS 1.4.1** | ARM64, SQLite meta + local file objects, `--cache-size=0` | 16 `INELIGIBLE` (E01/F 0.013480375; E01/S 0.013796250; E02/F 8.471189421; E04/F 11.511041922; C01–C12 all completed, C10 11.008 ms "may use CopyFileRange; not bandwidth"). 4 `EXCEEDED` (E03, E05, E12, E13). 12 `NOT_RUN` ("observed independent closed F backing copy 11.83–12.65 s; fixed lifecycle/cleanup…") | No residency proof; different store model |

**Reusable from #306:** dispositions as cautionary budget context only: an F backing copy of 10.36–12.65 s alone nearly consumes 15 s. **Proposed matched arm: none.** Third-party systems are not S8 arms unless the owner prospectively selects one.

### B.4 Main-branch component rows

None of these is a mounted arm.

| Row | Verdict to carry | Reusable for S8 | Not comparable because |
|---|---|---|---|
| Pre-S8 F13/F15 binds | Fresh object-cache bind 10 batches (1 file) / 11 (1024) / 12 (100000); warm repeat zero object Store demands, with the history snapshot still paid. "A successful control reply is not FUSE readiness"; "no timing admission" | Count baselines for hypotheses H1/H2 | Logical bind, no FUSE |
| Pre-S8 F14 | Engine/Store complete commands 1.19–3.86 s against 15 s; `phase_peak_bytes` null; "E3 exact peaks/unsupported attribution INCOMPLETE; numerical acceptance OWNER_DEFERRED" | Resource-domain inventory shape | Sampled RSS only |
| Resource growth | 14 selections PASS functional/count/cleanup "diagnostic memory only"; "natural cache, no speed comparison" | Ownership and release oracles | Natural cache |
| Init WAL matrix, history strides, seal | See D | Host component context | Host Init/history, synthetic or history corpus |
| E04 (CLOSED) | "E04 functional checkpoint CLOSED on both Store profiles"; complete commands 52,921,882,250 ns (Disposable) and 52,693,429,083 ns (Durable); "Cache and interference are uncontrolled… no speed comparison follows" | Oracle pattern (all 16,777,216 bytes + EOF + mode/nlink/time + membership), custody rules | Retired host-mediated topology, debug builds; "prospectively WITHDRAWN… never rerun" |

## C. Workload to oracle mapping

Structural facts that apply to every row:

- **Manifest scope (`manifest.py`).** Rows record mode (S_IMODE), kind, size, symlink target and optional SHA-256 (one hash per `(st_dev, st_ino)`). They do **not** record uid/gid, mtime/ctime, nlink, inode identity or xattrs. The historical oracle therefore never verified the hard-link alias relation (even for E13), timestamps or ownership.
- **Content hashing is scoped.** `hash_scope` is "whole layout; SHA256 of all files in the command-mutable subtree(s), identical hardlinks hashed once". Unchanged content is not hashed, so call this a **scoped (affected-subtree) content oracle**, not full-byte.
- **Where the tree was read.** A arms: from the ext4 backing directory after unmount. N: natively. Only B read through a fresh mount.
- **Semantic gap for L.** A terminal unmount discards uncommitted Workspace state. Any "state after unmount" oracle is either an in-mount read before unmount (S8) or Commit + fresh mount (S10). The historical A-arm oracle has no L equivalent.

| Case | Historical oracle | Complete or scoped | Observed oracle cost | L prerequisite | Prospective oracle note |
|---|---|---|---|---|---|
| E01 | exit + stdout SHA-256 (empty) | Proves nothing about the tree | about 0.001 s | S8 | Add readiness/detach/join ownership oracle; this is the fresh-mount floor |
| E02 | exit + stdout hash (`wc -l` count) | Count only: names not compared | 0.001 s | S8 + F root in Store | Needs a full name/kind manifest to be a namespace oracle |
| E03 | exit + stdout hash (tar byte count) | Byte count only, not content | 0.001–0.004 s | S8 + F root | A full-byte read of 3.48 GB; a content digest of the stream is needed for a real oracle |
| E04 | stdout hash; tree only in follow-up contracts (`.git/index` excluded) | Scoped | 2.1–2.8 s with tree | S8 + F root | May rewrite `.git/index`; not a pure read |
| E05, E06 | stdout hash | Complete for stdout | 0.001 s | S8 + F root | none |
| E07 | stdout hash | stdout only | 0.001 s | S8 + F root (symlinked `node_modules`) | none |
| E08 | stdout + tree (`.tsbuildinfo` + TS output dirs hashed) | Scoped | 7.56–8.25 s | S8; S10 for survival | Native Exec 71.5–78.1 s; see F |
| E09 | none | none | none | none | `NOT_RUN: Oracle stdout contains nondeterministic TAP durations`; needs an owner-approved normalized oracle or stays NOT_RUN |
| E10 | stdout + tree (`.git/` + `codec.ts` hashed) | Scoped | 2.05–3.08 s natively/ext4; **over 10 s through FUSE on B** | S8 in-mount; **S10** for survival | Git object files are new; commit hash is deterministic through fixed dates and identity |
| E11 | tree (`.git/` + tracked files) | Scoped | 2.39–2.56 s | S8; S10 for survival | none |
| E12 | tree + `replay_faithful` (node_modules rows equal the master's) | Scoped to `node_modules`; all 95,021 entries / 2,126,509,110 B hashed | 7.31–9.33 s | S8 on base S; S10 for survival | Complete for the affected subtree but near the 10 s limit even on ext4 |
| E13 | same, plus `.experiment-store/` | Scoped; **alias relation not verified** | 8.59–9.64 s | S8 with hard links; S10 | Needs a (dev, ino, nlink) equivalence-class oracle |
| E14 | tree (node_modules absent) | Complete for removal | 0.58–0.72 s | S8; S10 | none |
| E15 | tree (`experiment-large` hashed) plus in-command `cmp` | Complete for the one file | 2.05–2.21 s | S8 | Large inherited read plus write |
| E16 | tree (`experiment.log`); 10,000 × 100-byte unbuffered appends | Complete for the file | 1.88–2.13 s | S8 | Tiny-write case |
| E17 | tree (no `experiment-temp-*`) | Complete | 1.81–2.74 s | S8 | Create/unlink churn |
| E18 | stdout + tree after an untimed earlier mount | Scoped | 2.13–2.31 s | **S10** for the index to persist; S8-only measures the unrefreshed path | See hazard G-h2 |
| E19 | same, with 282 backing files changed between mounts | Scoped | 2.54–3.42 s | **S10**, or two Init-prepared related roots (declare which) | Expected status output must be re-derived for L's change mechanism |
| C01–C12 | full-tree SHA-256 on Z | **Complete** | 0.0013–0.109 s | S8 only (C12 too) | Cheapest full oracle; fits all default budgets |

"Sampled" must also stay attached to these existing main-branch oracles:
- Init namespace oracle "covers every path/kind and sampled file metadata and content. It is not a full-byte oracle" (100000 files: 73 selected files, 200,286,236 B of 500,000,000).
- Stride1 proof is "all-state structure plus bounded sampled content, not every payload byte" (66 paths, 921,174 B).
- WAL Init: 70 selected files / 6,430,827 B, "It is not a20MB full-byte oracle."
- E04 is the one all-byte oracle, at 16 MiB.

## D. Historical dispositions to preserve verbatim

**Init speed and allocation**
- `PRE-S8-COMPLETION`: "All eight prior speed failures and eight prior strict-allocation failures remain unchanged. New strict-allocation selections are NOT_RUN — mechanism removed."
- Eight speed FAILs at `2fced797d` vs control `197d2fb7d` (`INCUMBENT-RESTORATION-RESULTS` lines 1–67): durable +16.812% / +15.432% / +15.779% / +38.424%; disposable +17.913% / +20.140% / +18.135% / +33.549% (100 / 1000 / 10000 / 100000). Strict allocation FAIL on all eight.
- WAL Init candidate (`PRE-S8-INIT-WAL-RESULT`): Disposable1000 `serverless-wal-v1` 198720291 ns vs 155291459 ns; "`1987202910 > 1708206049`"; "+43428832ns (+27.9660145379%) and−16384B"; Speed gate **FAIL**; accepted as baseline, "original receipts and the speed FAIL remain unchanged".
- WAL matrix (`DISPOSABLE-WAL-MATRIX`), Init product ns 63,918,250 / 186,204,834 / 2,327,953,500 / 7,495,571,834:
  - vs MEMORY incumbent 1.10×: FAIL (+39.900084%), FAIL (+19.906681%), FAIL (+19.772207%), PASS (+0.972229%).
  - vs original cluster-one-end: all FAIL (+64.959901% / +44.056301% / +41.493165% / +34.847126%).
  - Limits 30 s / 19 s unchanged.

**Cold ineligibility**
- "Original100k v1: INELIGIBLE, zero product attempts,1626 resident pages after source invalidation"; the v2 `.noindex` copy passes.
- Durable stride10/3 first selection "**INELIGIBLE, zero samples**" (1,466 / 1,345 resident pages).
- F11 wide run08 "budget-INELIGIBLE at53.88s".

**History and seal**
- Stride 10/3/1 allocated bytes vs ceiling: 51,384,320 / 54,278,964 PASS; 64,872,448 / 70,427,034 PASS; 101,498,880 / 92,342,273 **FAIL** (+9,156,607 B, 9.915943%).
- Successor at `4156e90707b4b9fa8ca1bdc1b28cf3b5d8999521`: product 177,632,200,958 ns; complete 200,933,227,625 ns / 300 s; proof 18,952,711,334 ns / 30 s; allocated 85,348,352 / 92,342,273 B → "PASS / PASS / PASS / PASS"; "admission_eligible=false remains explicit".
- The original FAIL stays. The fix is macOS-only; a live Linux Store is never sealed. "S8 daemon FUSE/Exec and S10 live namespace normalization: NOT_RUN here".

**Profile and scale**
- Durable: "**NOT_RUN — disabled by owner until explicit reauthorization**" (current AGENTS wording). The older "NOT_RUN — deferred by owner for Disposable-only development" is preserved in its documents.
- "Actual native files above4GiB are **NOT_RUN — waived by owner**"; "The1000000-name legacy proposal is NOT_RUN/unselected".
- F14: "E3 exact peaks/unsupported attribution INCOMPLETE; numerical acceptance OWNER_DEFERRED".

**E04**
- "E04 functional checkpoint CLOSED on both Store profiles"; "All 27 E1 selections remain NOT_RUN/count zero, qualification NOT_EVALUATED and admission false"; E05 NOT_RUN.
- Failed receipts 50, 60, 68 and native-backing 38/39, disposal 13 ("FAILED as a hang"), 22, 35, 42, 45 stay as recorded.

**#305 and #306**
- Stage A E09 NOT_RUN.
- B/E10/F `FAILED` with cause "separate verification exceeded 10 s": "This is neither verified correctness nor release admission".
- Stage C "was not started".
- "CP5/CP6 remain incomplete."
- All SLOW rows stay SLOW.
- #306 `INELIGIBLE` / `EXCEEDED` / `FAILED` / `INCOMPLETE: exit0 with filesystem errors` / `NOT_RUN…` stay exactly as in B.3.
- Network rows NOT_RUN.

## E. Prospective complete-lifecycle proof-plan skeleton

Nothing here is a registered selection. Numeric fields must be frozen in the registration before any sample ("invent no acceptance ceiling", #314).

**(i) Functional oracles**
1. **Native lifecycle.** Mount returns Ready only after a kernel-visible root. `/bin/bash -c true` runs as the O-24 unprivileged user. Terminal unmount fences, detaches and joins: mount absent; request/open/lookup/process/stream owners zero; no surviving worker. A control reply is not readiness (F13).
2. **Full-root read oracle.** Name/kind/mode/size/symlink-target manifest equal to manifest SHA-256 `98fd2644…` through the mount. Content digest separately, complete or labelled scoped.
3. **Affected-state oracle for each mutation.** Full byte + name + metadata + alias for the affected subtree. Extend the manifest with nlink and (dev, ino) classes and mtime/ctime, which the historical oracle lacked.
4. **Stat identity.** Identical ino/size/mtime/ctime/mode across two fresh mounts of the same root (`303/07` §5.1); across install, S10.
5. **Coherence under the candidate profile.** Alias writes, replacement rename, truncate/regrow tails, mmap-origin writes, stale GETATTR/LOOKUP ordering, permission denial retained.
6. **Confinement.** Bash cannot open either database or daemon credentials (paths, inherited descriptors).
7. **Survival.** Commit + fresh mount equals captured state: **S10-dependent**.

**(ii) Falsifiable count hypotheses (frozen before timing)**
- **H1.** Fresh bind performs a bounded number of Store batches independent of namespace size beyond the observed 10/11/12 shape, and no whole-tree walk.
- **H2.** An eligible warm bind performs zero object Store demands; history snapshot and local bind work are still counted.
- **H3.** No additional overlay DB open or schema setup per mount.
- **H4.** FUSE requests by opcode per workload, against a predicted range: the same class of prediction the identity-churn contract used (A2SI 21,922 requests, "422 over predicted range").
- **H5.** With permissions retained, GETATTR per directory change is counted, not assumed away.
- **H6.** Duplicate concurrent acquisitions of one immutable ID: count declared.
- **H7.** Bytes copied under the cache mutex per hit.
- **H8.** Every admitted runnable Workspace makes progress within a declared bound while another scans.
- **H9.** Per-mount worker/buffer residency returns to baseline after unmount.
- **H10.** Reclaim debt reaches zero with no further API call.
- **H11.** A second fresh-mount `git status` issues no content READ for unchanged tracked files: **S10-dependent** (G-h2).

**(iii) Resource domains (never summed into one number)**
- Daemon heap/RSS.
- CanonicalCache retained charge, plus borrowed-after-eviction.
- SQLite pager/WAL: Store readers and writer, and the overlay MEMORY DB.
- Per-mount FUSE receive buffers and threads.
- Kernel page/dentry/inode cache attributed to the mount.
- Store file logical vs allocated bytes; overlay DB logical vs allocated bytes.
- cgroup file cache. (Precedent: Save256m cgroup 343,416,832 B, of which 287,899,648 B was file cache.)

Sample maxima are not continuous peaks; lifetime high-water is not a phase peak.

**(iv) Three cache classes: never pooled, each with enforced state**
1. **Fresh mount, cold daemon immutable cache.**
   - New daemon or proven-empty CanonicalCache; new Store connections; new overlay; new kernel connection.
   - Store file and sidecar OS residency **measured** before the attempt; a nonzero result yields INELIGIBLE with zero attempts (the 1626-page precedent). `drop_caches` alone is not proof.
2. **Fresh mount, warm daemon cache, fresh kernel connection.**
   - Declared untimed identical prior call in an earlier mount of the same daemon, then terminal unmount.
   - Enforced by the measured phase's own receipt showing zero object Store demands (H2).
   - Store-file OS residency for the still-paid history snapshot declared.
3. **Same mount, warm native caches.**
   - Untimed identical prior call on the same mount.
   - Monotonic interval between warm-up end and measured start recorded and below the 60 s entry/attribute TTL; otherwise the sample changes class and is INELIGIBLE.
   - Enforced by opcode counts.

Cross-arm note: class 2 has no native-ext4 analogue. Cross-arm comparison is defensible only in classes 1 and 3; class 2 is a within-L comparison.

**(v) Scenario coverage**

| Scenario | Dependence |
|---|---|
| Sequential per-call churn (mount → Exec → unmount, repeated, fresh identity each) | S8 |
| Several Bash commands on one Workspace, sequential and concurrent, short and long-lived | S8 |
| Simultaneous Workspaces on one daemon; scanning Workspace (E02/E03 shape) against a hot one | S8 |
| Shared cold miss with subscriber cancellation or unmount | S8 |
| Tiny/dispersed writes and fragmentation (E16, E17 shapes; 10,240 and 100,000 writes) | S8 |
| Hard links and rename (E13 shape with alias oracle) | S8 |
| Truncate/regrow/mmap | S8 |
| Log rotation and open orphans | S8 |
| Terminal detach/drain with live descendants, descriptors, dirty mappings | S8 |
| Related roots and branches | S8 if roots come from host Init; **S10** if from Commit |
| Repeated incremental Commit on one mount; Commit + remount survival; E18/E19 persistence | **S10** |
| Faithful F acquisition (10,070 symlinks) | Prerequisite; current support **not verified** |
| Network rows; Durable; >4 GiB | Unsupported / NOT_RUN under existing dispositions |

**(vi) Priced boundaries**

Recorded separately, phases balancing to the complete command (benchmark_rules §5), with no overlapping-span sums:
- setup (clone/reuse, declared);
- mount/readiness;
- Exec including output drain;
- explicit Commit when selected (S10);
- terminal drain/unmount;
- eventual cleanup to zero debt;
- independent verifier, separate.

A retained mount is not charged a fresh mount per call, and a fresh mount is never amortized. Speed and storage are reported together: Store and overlay logical/allocated deltas.

## F. Budget analysis

All conflicts are PENDING OWNER DECISION; no threshold is proposed. Observed facts:

1. **F setup alone.**
   - Of 128 F-based #305 records, the complete host command (byte-copy setup + Docker + in-container lifecycle + verifier) ranged 14.577–121.956 s. Exactly one (E02/F N1) was at or under 15 s; 32 exceeded 25 s.
   - E01/F (`true`) cost 16.25–18.34 s on the Stage A arms with in-container totals under 0.07 s.
   - #306 measured the F backing copy alone at 10.36 s (Drive9) and 11.83–12.65 s (JuiceFS), and recorded NOT_RUN rather than enlarging the budget.
   - S-based E01 fits (5.44–6.99 s); Z-based C cells fit (0.33–1.35 s).
   - An F-rooted sealed Store's size and copy time are unknown: none exists.
2. **Workloads whose native Exec is at or over the lines.**
   - E08: native 71.5–78.1 s, over 25 s on every arm.
   - E12: native 10.2–12.0 s; A2 family 27–44 s.
   - E13: native 12.2 s; A2 58.6 s.
   - E03: native 8.75–9.22 s; A2 family 18.8–28.3 s.
   - E14: A2 16.3–16.8 s (native 0.74–1.03 s).
3. **Verifier.**
   - Scoped oracles on ext4/native already cost 7.3–9.3 s (E12), 8.6–9.64 s (E13) and 7.6–8.25 s (E08) against under 10 s.
   - Preparation whole-tree hashing of the install cases "took over 10 s".
   - The only through-mount F verifier (B/E10) exceeded 10 s.
   - A complete full-byte F oracle read through an S8 mount has no evidence of fitting.
4. **Governing text.**
   - "The old #305 60/600 s amendments do not transfer automatically. If a full workload cannot fit the admissible budget, retain NOT_RUN and its exact budget conflict or obtain a prospective owning exception before selection" (`PERFORMANCE-ACCEPTANCE-S7-S12`).
   - The host history 300 s / 30 s exception is scoped to that family.

Decisions this forces, each a choice between retaining NOT_RUN with the conflict stated and a prospective scoped exception:
- (a) Complete-command scope for F-rooted samples.
- (b) E08, E12, E13, E03 and E14 Exec bounds.
- (c) Proof bound, or an explicitly labelled scoped oracle for F.
- (d) Whether a read-only S8 sample may bind one closed sealed Store without a per-sample byte copy. This requires a before/after identity proof that the sample did not mutate it (WAL sidecars included) and a separate residency proof per cache class.

## G. Hazards and owner decisions

**Hazards for comparability**
- **h1. Timer.** Sub-second #305 values before `cf-fsbench-v2` are inflated by up to about 51 ms (Exec) and 16 ms (Unmount). E01/E18/E19 native ratios are unreliable (the erratum quotes "anywhere from about 7× to 14×" for E18). Only v2 and mount phases are resolution-clean.
- **h2. Git stat identity.**
  - On A2, E18 re-read everything (80,057 requests; Exec 4.346 s vs native 0.079 s); A2SI needed 21,158 requests / 37 READ.
  - For L, the committed `.git/index` in F holds ext4 stat data from the master, so a first `git status` on any L mount will mismatch all 14,090 tracked files.
  - The refreshed index written inside a Workspace is lost at terminal unmount unless committed.
  - The fast path therefore needs stable ino/ctime across mounts **and** a committed refreshed index (S10). An S8-only E18 measures the slow path and must be labelled so.
- **h3. Oracle location.** The A-arm tree oracle read ext4 backing after unmount; L has no such directory.
- **h4. Shared VM.** `declared_interference: "none deliberately introduced; shared Docker VM, other worktrees not controlled"`. The walk window saw another owner's containers start. `drop_caches=3` is VM-wide and touches the protected containers `9cf2fe345496`, `ce75ac504df9`, `d2433851ea59`, `d2550144998b`.
- **h5. Profile mismatch.** The only overlay evidence (Stage B) used A1, not the promoted profile.
- **h6. Permission cost.** The promoted profile retains `default_permissions`; the faster follow-up arms removed it. Do not carry A2S/A2SI numbers as targets.
- **h7. `.git/index` exclusion and E04 writes.** A "read" case may mutate the Workspace.
- **h8. Stale artifacts.** E04 notes "Recorded execution hashes remain historical observations, not currently reusable binary artifacts"; the experiment prototype is "throwaway" and capped.
- **h9. A2-STATELESS headline miscount** (A, above).
- **h10. F contains no file over 4 GiB** (total regular bytes 3,475,776,149), so the waiver is not exercised by F. Large-offset correctness still needs its own case.

**Genuine owner decisions**
1. F-rooted complete-command scope (F-a, F-d).
2. Long-workload bounds or NOT_RUN (F-b).
3. Full-byte versus labelled-scoped F oracle and its bound (F-c).
4. Whether to authorize new matched controls at prospective identities: native ext4 (N), and a promoted-profile passthrough (P), given the experiment tree is throwaway and old numbers cannot be arms.
5. What a "materially better" claim compares against. No completed LayerFS-route control exists: B has one OK `true` cell and one FAILED cell.
6. Whether VM-wide `drop_caches` is permitted on the shared VM, and which residency-proof mechanism is accepted inside it.
7. E09: an approved normalized TAP oracle, or it stays NOT_RUN.
8. E18/E19 before S10: run as explicitly "unrefreshed-index" S8 cases, or defer to S10.

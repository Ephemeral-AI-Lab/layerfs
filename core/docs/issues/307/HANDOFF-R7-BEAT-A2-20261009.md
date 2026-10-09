# Handoff: R7, optimize until every cell beats LayerFS A2

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` on local `main`, one
checkout. You take over R7, the benchmark-driven optimization of the mounted
product. **Loop in small steps — measure, find the cause, change the product,
measure again — and do not stop until the product is faster than the LayerFS A2
baseline in every benchmark cell.** Changes may be aggressive. Do not push,
open a pull request, start another worktree or publish anything.

You start with no other context. This file and the documents it names are
enough.

## Owner direction, 2026-10-09

The owner's words, in the order given:

> create a handoff prompt to work on r7, which i want the next agent work
> iteratively, targeting aggressive changes, and do not stop until everything
> perform better than original layerfs baseline in the benchmark

Asked which measurement "original layerfs baseline" means, the owner answered:

> layerfs a2 in https://github.com/Ephemeral-AI-Lab/layerfs/issues/306

Asked what must be better, the owner chose: wall time must be faster, and
storage must not be worse.

Earlier the same day, to the previous lead, about how to work:

> do it in small step, do not batch full thing
>
> we want no multiple sampling, one sample is enough because we want fast
> iteration
>
> 420s is too ridiculous

The last line was about a 420 s wall limit on a sample that takes 9 s.

How the previous lead reads this. Each reading is mine, not the owner's.

- **The target is the "A2" column of issue #306**, copied in
  [The target](#the-target). A2 is the permission-enforcing ext4 passthrough
  FUSE prototype of experiment #305. It does no Store work.
- **"Everything"** is every cell that has an A2 number: E01 to E19 except E09,
  and C01 to C12. Commit and concurrency cells have no A2 number; they are
  measured and reported but do not decide the exit.
- **"Storage not worse"** cannot be compared with A2, which stores nothing. It
  is the existing gate: no kept change may enlarge the Store or the overlay,
  logical or allocated, for the same cell.
- **"Do not stop"** means a missed target, a failed test, a refuted idea, a
  long build or a context compaction is ordinary work. Stop only at the
  [exit condition](#exit-condition).
- **"Aggressive"** widens what you may redesign inside the active core crates.
  It does not lift the standing rules in [Still forbidden](#still-forbidden).
- The earlier instruction to finish the whole baseline matrix before the first
  optimization is superseded by "small step". Add cells one at a time.

### How this sits beside the handbook and the guide

The [optimization guide](../../../../docs/general/optimization-guide.md) is
binding and the
[optimization handbook](../../../../docs/general/optimization-handbook.md) is
the working procedure. This handoff departs from them in three places only,
each by owner direction:

| Handbook or guide | Here | Why |
| --- | --- | --- |
| Baseline every cell once, then triage (handbook §7, steps 2–3) | One cell at a time | "do it in small step, do not batch full thing" |
| No numerical target is invented; experiment numbers are not comparison arms (handbook intro, §7 step 10) | The A2 column is the target | The owner named it |
| A change to an interface between crates or to the negotiated request set needs the owner (handbook §2, §9 "Needs the owner") | Allowed; see [Authority](#authority) | Owner direction in the [original assignment](HANDOFF-R7-OPTIMIZATION-20261009.md): "allowed to optimize aggressively" |

Everything else in both documents applies unchanged. The parts most likely to
decide a step: counts before time and the cause sentence (handbook §2, §10);
the triage ratios and groups (handbook §4, §5); the scaling check at 1×, 2×
and 4× with a count test at two sizes (handbook §6, guide §3); SQLite as the
backed mutable state (guide §4.1); a query plan plus a runtime profile for any
statement claim (guide §4.2, §4.3); and what never counts as a fix (handbook
§11).

## The target

From issue #306, read 2026-10-09. E values are Exec seconds, excluding mount,
unmount, setup and verification. C values are milliseconds from the
event-timed `cf-fsbench-v2` run. All were taken once, fresh mount, on this
machine's Docker VM, ARM64, `sync` plus VM `drop_caches=3`, as uid 1000.

| Cell | Workload | A2 | | Cell | Workload | A2 |
| --- | --- | ---: | --- | --- | --- | ---: |
| E01/F | `true` | 0.025510 s | | C01 | create 1000 files | 183.416 ms |
| E01/S | `true` | 0.025920 s | | C02 | stat 1000 files | 965.006 ms |
| E02 | `find . \| wc -l` | 4.162990 s | | C03 | rm 1000 files | 410.750 ms |
| E03 | tar whole tree | 21.399468 s | | C04 | mkdir tree 10×10×10 | 952.589 ms |
| E04 | git status | 4.324254 s | | C05 | find tree | 949.630 ms |
| E05 | git log and diff | 5.137499 s | | C06 | write 64 MiB | 78.594 ms |
| E06 | git grep | 0.895150 s | | C07 | copy 64 MiB | 171.586 ms |
| E07 | load TypeScript | 0.280095 s | | C08 | read 64 MiB | 106.146 ms |
| E08 | TypeScript build | 75.748050 s | | C09 | pure read 64 MiB | 106.178 ms |
| E09 | Node tests | no A2 number | | C10 | pure copy 64 MiB | 178.210 ms |
| E10 | edit, git add, commit | 4.716923 s | | C11 | overwrite 64 MiB | 85.839 ms |
| E11 | git checkout | 6.423870 s | | C12 | git init, commit 100 files | 189.313 ms |
| E12/S | install replay, copy | 42.201692 s | | | | |
| E13/S | install replay, hard links | 58.601360 s | | | | |
| E14 | remove `node_modules` | 16.288058 s | | | | |
| E15 | large file copy and compare | 0.640569 s | | | | |
| E16 | 10,000 appends | 0.226881 s | | | | |
| E17 | 10,000 create and delete | 3.344438 s | | | | |
| E18 | status in a second mount | 4.346287 s | | | | |
| E19 | status after 282 changed files | 4.529085 s | | | | |

F is the full fixture (130,045 entries, 3,475,776,149 file bytes); S is the
same tree without `node_modules`.

A cell is **beaten** when one complete sample of the product arm `L` shows:

1. its command phase below the A2 value above; and
2. its mount + command + unmount below A2's mount + Exec + unmount for the
   same cell. A2's mount and unmount are in the original #305 receipts on
   branch `codex/phase7-experiment-305` (read with `git show` only); for the C
   cells issue #306's source reports mount 17–20 ms and unmount 5–18 ms. Where
   a receipt cannot be read, the Exec comparison alone decides and the row
   says so; and
3. the verifier passes, cleanup reaches `Gone`, and the Store and overlay are
   no larger than at the previous kept identity for that cell.

The deciding sample is the coldest registered cache class of the cell that
the harness can run to a complete row. Report the class beside every number.

These are historical numbers under a different envelope (user id, setup,
cache contract). The owner chose them as the target knowing that; say so in
the completion record and do not call the result a matched ranking.

Notes on single cells. E07 and E08 need `node`, which the pinned image does
not contain; find a sealed offline way or record the cell as blocked. An
earlier owner ruling (2026-10-08) set E08 to not run; I read "everything" as
selecting it again, as a priced run with a declared wall stop. E03, E08, E12,
E13 and E14 exceed 15 s on A2 itself; give each a wall stop just above its A2
value.

## Where things stand

- `HEAD` is the commit that adds this handoff, on top of `2a2042ea6`.
  **Product source identity: `87e234a62`.** Production LOC there: combined
  185,992; core 120,575; active core 77,701; excluded predecessors 38,878;
  excluded integration 3,996; root reference 65,417.
- **One cell is measured: C01, class B, arm `L`.** Latest sample (receipt 250,
  `87e234a62`): command **2,085.7 ms against the A2 target of 183.4 ms, about
  11.4 times slower.** Mount 6.6 ms, unmount 6.5 ms, cleanup to `Gone` 59.7 ms.
  Store 213,072 logical / 217,088 allocated bytes; overlay 557,056 logical /
  268,992,512 allocated (a reserved tail, not data).
- Three steps were kept since the first complete sample (receipt 223):

  | Identity | Step | Owner jobs | Statements | Owner wait + service | Command |
  | --- | --- | ---: | ---: | ---: | ---: |
  | `82780f72e` | start | 18,002 | 342,017 | 2,166.8 ms | 2,226.0 ms |
  | `9244dc8c6` | one post-reply release job | 16,002 | 334,017 | 1,806.0 ms | 2,004.7 ms |
  | `952e0b3bb` | no FileRead for LOOKUP and GETATTR | 15,002 | 302,017 | 1,762.0 ms | 2,202.1 ms |
  | `87e234a62` | one Workspace state read per check | 15,002 | 278,013 | 1,651.8 ms | 2,085.7 ms |

- Every sample row is `INCOMPLETE` for one harness reason: the second declared
  count interval spans a remount and the per-mount opcode counters restart.
  Fix that in the harness early; it is small.
- Not measured: `N` and `P` for C01; every other cell; Commit and concurrency.
  Not prepared: closed oracle bundles and configurations for every cell except
  C01, the Commit and concurrency runners. The full fixture has a sealed Store
  master for `L` (volume `layerfs-r7-master-20261009-980c169e6`); its native
  deployment for `N` and `P` fails on symlink modes, which does not block `L`.
- Not run at the current identity: the full suites and Linux Clippy. Each kept
  step ran its affected tests on host and Linux and host Clippy. The Linux
  suite campaign of the first lead stopped at index 175 with index 034 failed
  for a missing precondition.
- **Owed on the three kept steps.** The previous lead worked from the R7
  handoffs, the ledger and the source, and read the handbook and the guide
  only while writing this file. Each step has a count test and a cause with
  numbers, but none has the handbook's scaling check (§7 step 8: the changed
  path at 1×, 2× and 4×) or the guide's review record (§7, seven items,
  including the complexity statement). All three remove a fixed amount of work
  per request, so no size dependence is expected; that is an expectation, not
  a result. Do the sweep once for the request path before building on it.

Read the stage's [ledger](checks/r7-optimization-20261009/LEDGER.md) and
[candidate list](checks/r7-optimization-20261009/CANDIDATES.md) for every
receipt, failure and correction. The earlier handoffs are
[the original R7 assignment](HANDOFF-R7-OPTIMIZATION-20261009.md) and
[the owner-stop record](HANDOFF-R7-OWNER-STOP-20261009.md).

## What the time is made of

Findings from the C01 receipts and the source. They are the previous lead's
analysis; verify before relying on them.

- **The command is one serial chain through the single owner thread.** Owner
  queue wait plus service is 1.65 s of the 2.09 s command.
- **Per created file: 7 kernel requests, 15 owner jobs, 278 SQL statements.**
  Requests: LOOKUP, CREATE, GETATTR, WRITE, FLUSH ×2 (answered inline),
  RELEASE. Jobs per handed-off request: LOOKUP source + observe ×2 + release;
  GETATTR source + observe + release; CREATE source + mutate ×2 + release;
  WRITE source + mutate + release; RELEASE close.
- **Every owner job is its own transaction**: a freelist read, `BEGIN
  IMMEDIATE`, the work, `COMMIT`, four file-identity observations and one
  `fallocate`. Service costs about 4.2 µs per statement.
- **Per-request custody lives in SQL.** A request inserts rows into
  `native_source`, `base_source` and `lease` when it starts and deletes them
  when it ends. The Lease statement family is 147 of the 278 statements.
- **Each job costs two thread wake-ups**, one to the owner and one back.
  Queue wait is 17–61 µs per job on an owner that is idle between jobs, and
  about 430 ms of the command lies outside the owner with no counter at all.
  That remainder was 59 ms in the first sample and 199, 440 and 434 ms after;
  whether it is noise or grew as the owner got less busy is not known. Build a
  counter for it before attributing more wall time.
- Refuted: the per-transaction `fallocate` and stat calls are about 7 µs per
  job, measured. They are a minor constant.
- **Unverified lead from the handbook.** Group C says each owner round trip is
  a submit, a wake of the owner thread and a wake back, and group L quotes the
  experiment's FUSE `fstat` round trip at 44 µs unpinned against 5 µs on one
  CPU. The uncounted 434 ms divided by 15,002 jobs is 28.9 µs per job, the
  size of one wake back to the dispatch worker. That fits groups C and D
  (fewer round trips, less waiting) better than group E, and it means removing
  statements alone will not move the wall much. It does not explain the 59 ms
  of the first sample. Do not pin CPUs to make a sample pass: A2 ran unpinned,
  and a changed placement is a changed envelope.

A2 answers the same 7,000 requests in 183 ms, about 26 µs each. To get under
that, C01 needs roughly one cheap owner visit per mutating request and none
for most others. Removing a job or a statement at a time will not close an
elevenfold gap. Directions that could, none of them decided or reviewed:

- Decide a request in its first owner job without a separate source
  acquisition, and release in the deciding job (candidates C02b and C02c).
  This is the handbook's first direction (§8: fewer round trips) and has no
  conflict with the guide. Start here.
- Drop requests the kernel need not send: the GETATTR after CREATE (B03),
  negative entries, FLUSH elision. Handbook group B; each needs its coherence
  proof first.
- Keep per-request custody in fixed-size owner memory instead of SQL rows.
  **This one touches a binding rule.** Guide §4.1 puts operation scratch and
  reference relations in SQLite and forbids a resident container that mirrors
  that state; handbook §11 rejects a private mutable mirror. A table bounded
  by the admission limit (16 per mount today) that holds nothing which must
  outlive its request may fit the guide's "fixed processing buffers"; custody
  that teardown, reclamation or failure scope reads must stay in SQL. Write
  that argument down and have it reviewed before building. If it does not
  hold, this is a proposal.
- Serve several queued jobs under one transaction. **Also touches a rule.**
  Handbook group E expects one short transaction per job, and guide §4.2 wants
  transactions short. It is only acceptable if the group is bounded, one job's
  failure cannot roll back another job's published mutation, and no reply is
  sent before its own work is committed.
- Answer GETATTR and similar requests from state the mount already holds, so
  they never reach the owner. Guide §4.1 allows only a bounded cache with
  exact identity and version keys whose eviction cannot lose live state; an
  unkeyed copy of inode state is the forbidden mirror.

## How to work

One step is one candidate.

1. **Pick** the candidate with the largest expected reduction on the cell that
   is furthest from its target. Form the handbook's ratios (§4) to name its
   group, then write the cause sentence and the candidate entry in the
   handbook's templates (§10), with the expected count and the named growing
   variables (guide §3.1), before touching code.
2. **Change** the product. Add or update a count test that shows the counter
   dropped and that the count per unit is equal at two sizes (handbook §6).
   Update the affected architecture document in the same change.
3. **Check** only what the change touches: the affected test binaries on host
   and Linux, host Clippy `-D warnings`, `fmt --check`, the boundary guard.
4. **Commit** with the production LOC line.
5. **Sample once.** `core/target/r7-iterate-C01.sh <first-receipt-number>`
   seals the source, builds both release binaries, makes a fresh clone and
   configuration and takes one C01 sample, in about 25 s.
   `core/target/r7-summary.py <receipt.json>` prints phases, counts and
   storage. Write the equivalent for each new cell.
6. **Record** the result in the ledger and candidate list, commit, go to 1.

Rules for the loop.

- **One sample per identity.** No rerun, no best-of, no averaging.
- **Counts decide small steps.** One wall sample varies by 5–10 % here: the
  same command differs by up to 5.6 % inside one run, and unchanged work was
  23 % slower in one sample. A change whose counts fell and whose wall rose is
  kept only with a diagnosis from the receipts; otherwise revert it with a new
  commit. Claim a wall gain only when it is clearly outside that variation.
- **A statement, index or schema change needs both** the plan from the
  family's `explain_*` function and the `StatementWork` counters of the same
  run (guide §4.2, §4.3). Removing executions of an unchanged statement is a
  caller change and needs only the counts.
- **Use the passthrough arm for request volume.** A2 is the target, but `P`
  under the same profile is how the handbook tells "too many requests" (group
  B) from "each request costs too much" (group C). Take one `P` sample of a
  cell when that question decides the next step.
- **Wall limits close to the real runtime.** A 9 s sample gets 30 s. Never
  copy a large limit forward.
- **Every test command at most 120 s**, built with `--no-run` first, never
  looped, backgrounded or output-filtered. A test at its limit failed as a
  hang.
- **Extend the matrix one cell at a time**, the furthest from target first
  within each phase (write, metadata, cold read, large file, fixed cost).
  Each new cell needs a closed oracle and a sealed configuration before its
  first sample; the C01 preparation in the ledger (receipts 194 to 211) is the
  pattern.
- **Run the full suites**, every active package on host and Linux, one run per
  test binary, after every third kept product change and at the end. A
  functional failure outranks all optimization.
- **Review coherence and custody changes.** Any kept change that touches
  kernel coherence, request custody, admission, failure scope or teardown gets
  a fresh-context read-only review before it is kept. Subagents may do
  parallel read-only analysis and review. Only the lead runs Git, counts LOC,
  edits records, takes samples and decides.
- **Keep the state on disk.** Update `LEDGER.md` after every commit. On
  resuming, read this handoff, the ledger, the candidate list and `git log`.

## Authority

Taken under the owner's direction of 2026-10-09; record each use.

- Redesign anything inside the active core crates, including interfaces
  between them, the owner's job model, request custody, the overlay schema and
  the Fuse request path, when it lowers counted work.
- Reduce kernel requests while keeping permissions, stable identity, coherence
  and kernel writeback off: negative entries, adaptive READDIRPLUS, FLUSH
  elision, wider directory replies, attribute and entry reuse. The handbook
  marks these as the owner's; the original assignment allowed them, each with
  its coherence proof before it is kept.
- Move, restructure and delete first-party active code. Report relocation and
  deletion honestly in the LOC line.

Still the owner's, proposal only: the canonical format, a persistence
profile, a public contract of cluster one, entry and attribute lifetimes, and
any limit or threshold.

### Still forbidden

- Durable execution. Global Store is Disposable / WAL / synchronous=OFF only;
  the daemon overlay stays MEMORY / OFF / EXCLUSIVE. No `fsync`-family call on
  Disposable backing.
- Kernel writeback caching. Dropping or weakening permission checks.
- Any change to cluster one's canonical format or public contracts.
- A new dependency; any third-party edit beyond the authorized fuser patches.
- Retry, replay, refresh, busy handler or a guessed outcome.
- Test hooks or test-only API in product source; a path that recognises a
  benchmark command; a second construction producer
  (`LAYERFS_CONSTRUCTION_WORKERS=1`).
- More threads, more receive loops, spinning, larger limits, longer cache
  lifetimes or longer timeouts used as the fix.
- Changing a workload, a cache state, an oracle or a limit to turn a miss into
  a pass.
- Retiring the excluded predecessors or the root reference; editing the pinned
  LOC counter; pushing or publishing.

When a forbidden item is what stands between a cell and its target, do not
build it. Write a proposal with the counted benefit and continue.

### Gates on every kept change

- **Disk.** For the same cell the Store with its sidecars and the overlay are
  no larger, logical or allocated. No new persistent table, index, column or
  file unless it replaces something larger and the measured total is not
  higher.
- **Memory.** No resident state whose size follows an input dimension; no
  enlarged cache allowance, queue, buffer or credit. A table bounded by
  requests in flight is allowed. Record daemon `VmHWM` and the owner's byte
  counters beside every sample. `VmHWM` has read about 30 MiB and about
  47 MiB with no source change; it is recorded, not an exit gate, until
  someone finds why.

## Exit condition

Finished only when, each shown by a receipt at one final identity:

1. Every cell in [The target](#the-target) is beaten, or is closed as blocked
   under the rule below.
2. The full suites pass on host and Linux, apart from cases whose precondition
   the suite does not supply, listed by name.
3. Clippy `-D warnings` on host and Linux, `fmt --check` and the boundary
   guard pass.

A cell may be closed as **blocked** only when all of these are written down:
its counted structural floor derived from the contract; the measurement
showing the product at that floor, or the forbidden item that prevents
reaching it; and a proposal naming what would have to change. A cell is never
blocked while an allowed candidate with a non-zero expected reduction is
open. A2 does no Store work, so some cells may have a floor above their A2
value; that has to be shown with counts, not assumed.

Then write `core/docs/issues/307/R7-OPTIMIZATION-COMPLETION-<date>.md`: per
cell the A2 value, the first and final `L` measurement, class, storage and
verdict; every kept, reverted and proposed candidate with its counts; every
failed attempt and correction; production LOC per commit; and what is not
claimed. Do not start predecessor retirement, frozen qualification or
reference retirement.

## Read first

1. [Root instructions](../../../../AGENTS.md) and
   [core instructions](../../../AGENTS.md).
2. The [optimization handbook](../../../../docs/general/optimization-handbook.md)
   and [optimization guide](../../../../docs/general/optimization-guide.md).
3. The [measurement workflow](../../../../docs/general/agent-measurement-policy.md),
   [benchmark rules](../../../../docs/general/benchmark_rules.md) and the
   [report template](../../../../benchmark_agent_report.md).
4. The [proof plan](S8-PROOF-PLAN-20261008.md), sections 2.2, 2.3, 6, 7 and
   8.1: arms, workloads and oracles, cache classes, priced boundaries.
5. Architecture notes for the request path:
   [native read custody](../../architecture/73-native-read-custody.md),
   [native request service](../../architecture/75-native-request-service.md),
   [native mutation and coherence](../../architecture/77-native-mutation-coherence.md).
6. The stage ledger and candidate list, then `git log`.

## Environment and mechanics

- Host: macOS ARM64, `cargo +1.85.1 ... --locked`. There is no `timeout`
  binary; use the helpers.
- Linux: image
  `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
  Mount tests need `--device /dev/fuse --cap-add SYS_ADMIN --security-opt
  apparmor=unconfined`. Do not assume Docker has network access.
- Helpers in ignored `core/target/`: `r4-locked.pl` (checkout lock, wall
  limit, group kill); `r4-build.sh` (`check`, `clippy`, `test-no-run`, `fmt`,
  `fmt-check`, `guard`); `rx-run.sh` and `rx-linux.sh` (one bounded test run
  with an append-only receipt; set `RX_STAGE=r7-optimization-20261009`);
  `rx-count.py` (`staged` or `committed`, from the repository root);
  `r7-iterate-C01.sh`, `r7-seal-build.py`, `r7-configure-C01.py`,
  `r7-summary.py`. Harness code is tracked under `core/benchmark/`;
  `core/benchmark/r7-tools/run.py` wraps one command with a receipt directory.
- One Cargo, Docker, test or measurement command at a time, under the lock.
- After editing source on the host, a Linux build can stop with
  `RX_STALE_SOURCE`: Docker served an old copy of the edited file. Running the
  build again succeeds; the check itself refreshes the file.
- Prepared inputs are under `/tmp/layerfs-r7-*` and in Docker volumes named
  `layerfs-r7-*`. `/tmp` does not survive a reboot. C01 needs
  `/tmp/layerfs-r7-C01-packaged-code-host-group-20261009-1b028a24a`,
  `/tmp/layerfs-r7-native-reference-inputs-20261009-1b028a24a`, the
  `/tmp/layerfs-r7-empty-*` manifest and Store, and the volume
  `layerfs-r7-empty-master-20261009-2ac7cc762`. If any is gone, rebuild it
  from the receipts in the ledger; never by replaying a sample.
- Every sample leaves one stopped container and one clone volume. The stopped
  containers each hold about 269 MB of reserved overlay space. You may remove
  clone volumes named `layerfs-r7-C01-L-<commit>-20261009` and their stopped
  sample containers once their receipts are committed; record it. Never remove
  a master volume or anything belonging to a failed attempt.

## Preservation

Do not touch these unrelated running containers:
`9cf2fe345496b45d79ff272e6c698552948f1e92ea106926826a770592623b24`,
`ce75ac504df9d0e58cedcdd0df93fe7ef39c3b66ef3c13b0bb6b580def64516c`,
`d2433851ea5990e273b0673fdbd5c180917f1216f3c81bdfb4517da0207e2c2a`,
`d2550144998b4b71de98cb9ecf6448f87c7b99a3c2f69ea778a29413fffa5ffb`.
Leave the exited `layerfs-e04-*` and `cf-build-probe` containers and
`layerfs-experiment-305-dev` alone.

Keep these untracked files unstaged: `HANDOFF-PRE-S8-SERVERLESS-20261007.md`,
`HANDOFF-S7-S9-RESUME-20261006.md` and `S7-S9-SPEED-TEST-PLAN.md` in this
directory, and
`checks/multi-workspace-model-confirmation-20261008/04-commit-confirmation.json`.
Do not rewrite raw evidence. Sample 212 stays FAIL. Do not run commands in or
write to `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness`. Do not touch
`/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff`.
The experiment branch `codex/phase7-experiment-305` may be read with
`git show` only.

## Every commit

Local `main` only. Each message records
`Production LOC: <before> -> <after> (delta <signed>)` with scope, method and
subtotals from the pinned counter (`tools/production_loc.py`, SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`), compared
between the first parent and the staged tree and confirmed on the committed
tree. Harness, test and document commits report the unchanged total and
delta 0. End each message with the attribution line the session gives you.

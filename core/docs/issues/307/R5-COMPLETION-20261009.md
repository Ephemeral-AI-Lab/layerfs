# R5 completion: mounted live Commit and known install

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

R5 connects the R4 captured namespace producer to the real control Commit on a
mounted Workspace. This record states what is wired, each proof row with its
verdict, every failed attempt, the decisions that need owner review, how
subagents were used and the source-size comparison for each commit. Design is
in architecture [79](../../architecture/79-product-commit.md); the plan and
its two amendments are in the
[deepest-file plan](checks/r5-mounted-commit-20261009/01-deepest-file-plan.md).

- Start: `3ab89dc66` (R4 record). Final product source identity: `57244684b`.
  Later commits change tests, receipts and documentation only.
- Local `main`, one checkout, nothing pushed, no pull request.
- Global Store profile: Disposable / WAL / synchronous=OFF only. Durable:
  `NOT_RUN — disabled by owner until explicit reauthorization`.
- Evidence is functional results and counted work. No timing, cold-cache,
  storage or resident-memory claim is made anywhere in R5.
- Three product defects were found and fixed inside the stage, two of them in
  code R5 added and one present since R2. They are listed under
  [Every failed attempt](#every-failed-attempt).
- R6 is planned
  ([plan](checks/r6-concurrency-teardown-20261009/01-deepest-file-plan.md)) and
  not started. The root reference tree is untouched.

## What is wired

Production LOC from the pinned `tools/production_loc.py --files` at
`57244684b`. Only the files R5 added or changed are listed, so every bracket
covers the listed files only. Tests live under each crate's `tests/`.

```
layerfs-daemon/src/                 [1717]  (part)
  control/                           [593]  (part)
    failure.rs                        234   (bounded Commit detail; Unknown when custody is kept)
    operations.rs                     222   (control Commit calls commit_captured; unreleased owner fences)
    registry.rs                       137   (the unavailable-Commit refusal removed)
  overlay/                           [492]  (part)
    admission.rs                      199   (submit_waiting; configuration)
    captured_namespace_port.rs         91   (waits for admission)
    captured_run_port.rs               49   (waits for admission)
    operation_record_port.rs          153   (waits for admission)
  store/                             [600]  (part)
    captured.rs                       186   (new: product constructor, classification, release)
    commit.rs                         168   (namespace field in the success literal)
    commit_types.rs                   187   (CapturedConstruction, ReleasedOwner)
    mod.rs                             24   (declarations)
    settle.rs                          35   (Commit jobs wait for admission)
```

```
layerfs-workspace/src/               [197]  (part)
  construction/                      [197]  (part)
    driver.rs                         197   (construct_untimed)
```

```
layerfs-fuse/src/                    [323]  (part)
  operations/                        [323]  (part)
    directory.rs                      323   (one published window ends a READDIR reply)
```

What a control Commit now does, in order:

1. `Request::Commit` is admitted to `Committing` and runs
   `BoundWorkspace::commit_captured` on the control thread.
2. The unchanged driver captures the shared published frontier once, begins a
   Save and calls the product closure.
3. The closure acquires the captured reader and one operation owner, runs one
   `CapturedNamespace` attempt into the Save's sink, and hands the driver the
   first original cause on failure.
4. The driver finishes the Save, publishes through History with the captured
   parent, and installs the known root locally. The Store stays open.
5. Only when the driver's outcome is known are the reader and then the owner
   released. An uncertain cause, a known publication with a failed install, or
   a failed local resolution keeps both with the failure.
6. Every owner job of that thread waits for a credit before its one attempt.

Not wired by R5: forced unmount, any exit from retained custody, kernel
notification at install (none is needed under the recorded contract and none
is sent), and a changed Commit through the real daemon binary (see R5-1).

## Proof rows

Verdicts are for the scope stated. A partial-scope receipt does not close a
whole row. Receipts are in
[`checks/r5-mounted-commit-20261009/`](checks/r5-mounted-commit-20261009/);
the evidence receipts named here were taken with `--nocapture` at the final
identity.

| Row | Verdict | Scope and basis |
| --- | --- | --- |
| R5-1 | PASS | Real kernel mount, changes by externally launched Bash, control Commit, fresh mount of the Branch. The oracle is a native replay of the same script on the container's own filesystem, compared for names, bytes, metadata, hard links and symlinks, with `.git`, ignored, cache and output files. `mounted_commit` |
| R5-2 | PASS | Mounted: an unchanged Workspace is UpToDate and History has no new Commit. Store scope: a stale unchanged candidate over a moved head is Committed with the captured parent. `mounted_commit`, `product_commit` |
| R5-3 | PASS | Three launches with no Exec registration, one exiting 3 and one still alive with a descriptor and cwd in the mount; all captured. That no registration exists is structural: the daemon has no registration API. `mounted_commit` |
| R5-4 | PASS | A later Commit overwrites the head and keeps its captured parent; the displaced Commit stays stored and readable; sibling mounts stay isolated. `mounted_install` |
| R5-5 | PASS, with stated limits | Same mount and fresh mount. Six kept descriptors, two of them unlinked, read identically through the page cache and `O_DIRECT` across two installs; `fstat`, forced `statx` and both stat tables over 74 paths are unchanged; a removed working directory stays usable. Writes made while a Commit ran stay live and reach the next Commit: a second thread reads the writer's log through the mount while control Status says `Committing`, and the test asserts that this count exceeds the captured prefix (267 against 5, and 463 against 268, in `N-attempt7`). This holds because a Commit spans hundreds of writer iterations; a Commit shorter than one iteration would fail the test, not pass it. Not covered: mappings across install, atime, `O_DIRECT` reads of files with no kept descriptor; every capture observed landed between writer iterations. `mounted_install` |
| R5-6 | PARTIAL | Staged on the mount: Store Busy from an external process holding the writer before Save, at publication and inside the producer; a definite failure settled locally; a lost History acknowledgement kept as unknown; a known publication with an unattempted install. Not staged: a missing dependency on the mounted route (driver scope only, `store_commit` and `captured_commit`); an install that was attempted and failed; an unknown outcome where nothing was published. The unknown and install cases use an external catalog boundary, not a native SQLite failure. `mounted_commit_failures`, `product_commit` |
| R5-7 | PASS, with one unrun case | The product closure hands the driver the original cause: a refused owner acquisition's own completion, a real Store Busy inside the producer, and a cause taken from the producer's custody (F6: with every admission registration reserved during construction, the operation-record port's own refused command arrives as `Owner(Unattempted { AdmissionFull, IndexedOperationRecord(Get) })`, the local resolution is itself unattempted, and reader, owner, record and file custody are all kept). A definite refusal is settled and releases what was acquired; an uncertain one keeps reader, owner and custody, shown by the engine's own retained queries. Unrun: a release that is itself refused after a settled failure has no hook-free staging; the code path that fences it is unproven. `mounted_commit_failures`, `product_commit` |
| R5-8 | PASS | Five incremental Commits on one mount, each exact against the mount's own view, then a fresh mount; six on one open Store, each exact against an independent model. `mounted_commit`, `product_commit` |
| R5-9 | PARTIAL | A store through a shared mapping is in the next Commit with exact bytes once the kernel has written it back; the test forces writeback before the Commit. No Commit runs while a page is still dirty, so this does not close row 6 of R4-3: that row stays PARTIAL. `mounted_commit` |
| R5-10 | PARTIAL, structural | One construction producer: the product route runs the producer on the Commit thread and spawns none; no test counts producers. The Store stays open: each fixture opens one writable Store and the product has no reopen or seal call on this path; one Store reservation set is observed across six Commits. No Durable execution: every fixture asserts the Disposable profile on its configuration, and the journal mode read back is `wal`. The `synchronous` value in the receipts is the observer connection's own and proves nothing about the writer. `LAYERFS_CONSTRUCTION_WORKERS=1` is exported in every run and is read by no active core source. `product_commit` |
| R5-11 | PASS | Counted work of one small and one large-change Commit at Store scope, recorded and not timed; see [Counted work](#counted-work). `product_commit_cost` |

### Counted work

From `product_commit_cost` on Linux (`S2-attempt5-linux-product_commit_cost.txt`).
Counts only; input to the optimization checkpoint.

| Commit | Captured inode + entry rows | Owner jobs (operation-record class) | SQL statements | Store object batches | Full-scan steps |
| --- | --- | --- | --- | --- | --- |
| Small change, 12 steps | 14 + 10 | 564 (485) | 2,366 | 6 | 0 |
| Large change, 576 steps | 480 + 192 | 21,182 (19,076) | 87,578 | 267 | 0 |

About 31 owner jobs and 130 statements for each captured row in the large
case, nine tenths of them operation-record jobs: the per-row record
point-read cost that R4 item 10 names. The Store cache was as left by bind
and the mutations, not cold. The number of Save publish batches for the large
change varied between 3 and 6 across runs.

### Final suites at `57244684b`

One run per test binary, each under its own 100 s wall limit, all thirteen
active packages.

| Side | Binaries | Passed | Failed | Tests passed |
| --- | --- | --- | --- | --- |
| Host | 216 | 214 | 2 | 1,043 |
| Linux, pinned image, FUSE device | 216 | 214 | 2 | 1,043 |

Receipts: [host](checks/r5-mounted-commit-20261009/final-host-suite.txt),
[Linux](checks/r5-mounted-commit-20261009/final-linux-suite.txt), with a full
log beside each. The four failing invocations are the same three cases as in
R4, each needing a precondition that was not supplied; they are failures of
those invocations and unrun as proofs:

- `complete_installed_roots::huge_native_namespace_is_complete_after_install`
  (host and Linux): needs its separately prepared sealed namespace.
- `host_handoff` (host): needs its Docker placement.
- `shared_processes` (Linux): needs explicit named-volume placement.

Test files changed after that suite, with no product change, were each run
once more at the final product identity with `--nocapture`, and these are the
evidence receipts: `mounted_install` 5 of 5 (`N-attempt7-linux`),
`mounted_commit_failures` 6 of 6 (`F-attempt3-linux`), `product_commit` 11 of
11 on host and Linux (`E-attempt2`), and unchanged `mounted_commit` 7 of 7,
`native_control` 7 of 7 and `product_commit_cost` (`E-attempt1-linux`). The
receipt headers name the commit that was checked out, with the test files
still uncommitted; the product source under them is `57244684b`.

Also at the final identity: Clippy with `-D warnings` for all targets on host
and Linux, `fmt --check`, the boundary guard (846 production files), the
tooling self-tests (49 passed) and the fuser provenance check.

## Every failed attempt

All receipts are kept. None was relabelled.

| Receipt | What failed | Cause | Resolution |
| --- | --- | --- | --- |
| `K-attempt1-product_commit` (host, 5 of 6) | The test helper's retained-owner query was refused `AdmissionFull` | The test asked the Overlay while still holding the Commit's outcome, whose completions held both Lifecycle credits | Test corrected to drop the outcome first |
| `K-attempt2-product_commit` (host), `K-attempt1-linux-product_commit` | After a settled refusal `ReleaseOperation` was refused `AdmissionFull` and the owner stayed held | **Product defect in R5 code.** The local resolution's completion and the kept reader-release completion held both Lifecycle credits | Fix S2, `6e8cb6ede`: a done release's completion is dropped at once; Commit jobs wait for admission |
| `S2-attempt2-product_commit`, `S2-attempt3-product_commit` | The new admission-wait test panicked while filling slots | The test's own staging: it did not wait for the engine to drop a credit, then used a Lifecycle-class job to fill ordinary slots | Test corrected; `S2-attempt4` passes |
| `N-attempt1` to `N-attempt5` (`mounted_install`, Linux) | Listing a directory after a Commit returned `EIO`; the mount then answered `ENOTCONN` | **Product defect present since R2.** A READDIR whose 64-name window fit the reply and had names remaining asked the same directory read for a second cookie plan; the Overlay allows one and answered `Stale`; the request was retained and fenced the mount. A directory of 65 six-byte names could not be listed. Attempt 3 also contains a mistake of the test's own panic message, which re-read a fenced mount | Fix in `8e6f61c40`: one published window ends the reply. `N-attempt6` passes 5 of 5 |
| `R1-attempt1-product_commit` (host, exit 3) | The Commit hung; the test ended the process after 30 s | **Product defect in R5 code**, found by the fresh-context review and reproduced on purpose with the fix line removed. A refused owner acquisition's completion plus the local resolution's held both Lifecycle credits while the reader's release waited for one | Fix R1, `57244684b`. `R1-attempt2` passes |
| `E-attempt1-linux-product_commit` (10 of 11) | The new slot-guard test panicked in the fixture's live-view read, `AdmissionFull` | The test's own staging: under an owner with one ordinary slot the fixture's readers cannot run reliably. The host run of the same source had passed, by timing | Test corrected to make no mutation or view read and to assert that the refused call admitted no owner job. `E-attempt2` passes on host and Linux |
| Final suites, four invocations | See above | Preconditions not supplied | Unrun as proofs, as in R4 |

No test reached its wall limit. No attempt was repeated unchanged.

### Corrections of the lead's own statements

- The plan kept "two original release completions" in the receipt and left
  Commit jobs on a refusing submission. Both were wrong; Amendment 2 records
  the corrections.
- The fix that made Commit wait for admission (S2) introduced the
  self-deadlock that fix R1 removes. The first review of S2 was the lead's
  own and did not see it; a fresh-context reviewer did.
- The track M commit message says every successful Commit "released its
  reader and operation owner" on the evidence of a receipt count of two. That
  count was of kept completions, which S2 then removed; the statement is true
  of the later receipts, which name the released owners.

### Not verified

- The release-refusal branch after a settled failure (control records
  `Uncertain`, reply `Unknown`) has no test.
- The page-cache pass of R5-5a is inferred from its request count (12 against
  33 for the direct pass), not from per-opcode accounting.
- A changed Commit through the real daemon binary. `native_application`
  commits an unchanged Workspace through the binary; every changed or mounted
  Commit runs the same product `Service` composed inside the test process.
- Whether a Fuse task ever holds a Lifecycle completion while it waits for
  another.
- Kernel behaviour assumed by R5-9: that an `O_DIRECT` read flushes the range.

## Decisions that need owner review

Each was taken without the owner, on the conservative and reversible side.

1. **Owners are released after the driver's outcome is known, not inside the
   closure.** Alternative: release before finish and publication, as the R4
   test closure did. To change: move two calls.
2. **The request number of the reader and owner is the capture generation.**
   Alternative: a daemon counter.
3. **Custody that is not released fences the Workspace**, after a known
   Commit (`LocalFailure`) and after a settled failure (`Uncertain`, reply
   `Unknown`). Alternative: report the outcome and continue. No product exit
   from either state exists until forced unmount (R6), and none at all for an
   Unattached Workspace.
4. **No notification and no view comparison at install.** The contract allows
   that only while names, bytes, links, attributes and serials are identical
   before and after install, which R5-5 observes and does not enforce.
5. **Commit forces no kernel writeback.** A store through a shared mapping is
   committed only once written back.
6. **`Service::execute` and `serve_one` stay public** with no product caller,
   for the driver's tests. R7 decides.
7. **Unknown History and install failures are staged by an external catalog
   boundary**, and keep that scope label.
8. **An opaque pre-admission refusal stays uncertain.**
9. **Commit waits for owner admission instead of refusing.** The handoff's
   one-attempt rule allows a readiness wait before the attempt. The wait has
   no bound and no queue order, so sustained traffic can delay a Commit
   without limit. Alternative: the earlier before-effect `Capacity` refusal.
   R6 measures it by counts.
10. **`commit_captured` refuses below two ordinary or two Lifecycle job slots
    per Workspace.** A daemon set up with one Lifecycle slot still starts and
    mounts, and then refuses every Commit. Rejecting that setup at startup
    would change a setup contract and is left as a proposal. The guard does
    not cover a credited-byte budget too small for the thread's own held
    completions; that is not reachable at the default 8 MiB by estimate.
11. **The done local resolution's completion is not kept on the product
    route.** `CommitFailure::local` is then `None` and `locally_settled`
    carries the fact. The driver's own route still keeps it.
12. **A `NotApplied` record reply is classified with the unknown outcomes**
    and keeps both owners, although nothing was applied: the producer's own
    records were not what it had written, and the custody is kept for
    inspection. Alternative: a definite `Content` failure that is settled.
13. **One READDIR reply carries at most one 64-name window.** Alternative:
    let a directory read own several cookie plans, which is an Overlay
    contract change.
14. **Carried to R6, not fixed here:** install can deadlock on owner slots
    held by fenced source acquisitions under sixteen concurrent requests; an
    install parked behind a retained request has no bound; a Commit failure
    recorded `Uncertain` whose error maps to another code still answers that
    code.
15. **Host scratch left in place.** Earlier failed host runs of this stage
    left `layerfs-installed-8450-*`, `-8981-*`, `-11956-*` and `-12060-*`
    fixture directories under the macOS temporary directory. Nothing was
    removed; the protected `layerfs-installed-56560-full-native-handoff` was
    not touched.

## How subagents were used

| Role | Count | Work |
| --- | --- | --- |
| Read-only audit | 5 + 2 | Five for R5's source facts before the plan; two for R6 while R5's tracks ran |
| Implementing, product | 1 | Track S. The lead reviewed and corrected the diff before commit |
| Implementing, tests | 4 | Tracks M, N, F and K, each with one file list; N and F were resumed once for the review's findings |
| Fresh-context review | 2 | One of product source, one of whether the tests prove their rows |
| Planning | 1 | The R6 plan draft |

The lead wrote fixes S2 and R1, the READDIR fix, three proofs in
`product_commit`, the plan, its amendments and this record, and made every
commit.

Findings the reviewers made that were real and fixed: the Lifecycle
self-deadlock; a settled failure with an unreleased owner recorded `Idle`;
R5-5b asserting nothing about writes during a Commit; no product-route test
of a cause from the producer's custody; F5 ignoring the original cause; the
hang watchdog reporting a panic as a hang; a comment claiming a byte was
read; missing limit statements. Findings accepted as stated limits: R5-10's
structural basis; R5-9's scope; receipts taken before a committed identity
(the evidence receipts were retaken); the real binary committing only an
unchanged Workspace.

Findings rejected, with the reason:

- *Classify a `NotApplied` reply as a definite failure.* Kept uncertain;
  decision 12.
- *Refuse an owner setup below two slots at startup.* A setup contract
  change; decision 10.
- *Bound the hang-prone `join` in the admission-wait test.* The binary's
  100 s wall limit already fails it as a hang; a second mechanism adds
  nothing.
- *`Service::execute` is a test-only public API.* It is the driver's public
  entry and predates R5; R7 decides.

## Production LOC per commit

Pinned `tools/production_loc.py` (SHA-256 `c0fe7f36…24adb`), first parent
against the committed tree; first-party Rust and shipped SQL; comments,
blanks, tests, docs and tools excluded. Root reference 65,417, excluded
predecessors 38,878 and excluded integration 3,996 are unchanged throughout.

| Commit | Subject | Combined before → after | Delta | Active core |
| --- | --- | --- | --- | --- |
| `91e616291` | Handoff recorded | 182,952 → 182,952 | 0 | 74,661 |
| `bb5b3c220` | Plan | 182,952 → 182,952 | 0 | 74,661 |
| `a7ef29672` | Track S, product constructor | 182,952 → 183,178 | +226 | 74,887 |
| `c4020ee4c` | Track M, tests | 183,178 → 183,178 | 0 | 74,887 |
| `6e8cb6ede` | Track K and fix S2 | 183,178 → 183,212 | +34 | 74,921 |
| `2ac037602` | Track F, tests | 183,212 → 183,212 | 0 | 74,921 |
| `8e6f61c40` | Track N and READDIR fix | 183,212 → 183,212 | 0 | 74,921 |
| `57244684b` | Fix R1 | 183,212 → 183,237 | +25 | 74,946 |
| `d5e9312b0` | R6 plan (documentation) | 183,237 → 183,237 | 0 | 74,946 |
| this commit | Review follow-ups in tests, and this record | 183,237 → 183,237 | 0 | 74,946 |

R5 total: +285 production lines, all in active core.

## Not claimed

- No timing, cold-cache, storage or resident-memory result.
- No Durable execution.
- No qualification of the mounted route under concurrency: every proof here
  runs one Commit at a time against at most a few processes. That is R6.
- No forced unmount and no exit from retained custody.
- Nothing about the Sandbox topology or the real daemon binary beyond an
  unchanged Commit.

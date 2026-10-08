# R4 completion: captured namespace construction and incremental topology

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

R4 builds the producer that turns one exact Capture into one complete
canonical filesystem root, with validation and topology work that follows the
change. This record states what is wired, which proof rows passed, every
failed attempt, the decisions that need owner review, how subagents were used
and the source-size comparison for each commit. Design is in architecture
[78](../../architecture/78-captured-namespace-construction.md); the plan and
its two amendments are in the
[deepest-file plan](checks/r4-captured-namespace-20261008/01-deepest-file-plan.md).

- Start: `37dcf5405` (R3). Final source identity: `5747f0119`. The commit that
  adds this record changes documentation only.
- Local `main`, one checkout, nothing pushed, no pull request.
- Global Store profile: Disposable / WAL / synchronous=OFF only. Durable:
  `NOT_RUN — disabled by owner until explicit reauthorization`.
- Evidence is functional results and counted work. No timing, cold-cache,
  storage or resident-memory claim is made anywhere in R4.
- R5 (mounted Commit) is not started. The Commit driver is unchanged. The root
  reference tree is untouched.

## What is wired

Production LOC from the pinned `tools/production_loc.py --files` at
`5747f0119`. Only the files R4 added or changed are listed, so every bracket
covers the listed files only. Tests live under each crate's `tests/`.

```
layerfs-content/src/filesystem/     [1444]  (part)
  validate.rs                         414   (one validator, windowed classification)
  validate/                         [1030]
    backed.rs                         330   (topology evidence in indexed records)
    cycles.rs                         225   (territory walk and rooted proof)
    entries.rs                        100
    in_place.rs                       113   (new: renames inside the base parent)
    incremental.rs                    262   (new: 64-row classification windows)
```

```
layerfs-overlay/src/                 [225]  (part)
  database/                          [108]  (part)
    statements.rs                     108   (one new point statement)
  namespace/                         [117]  (part)
    captured_namespace.rs             117   (new: three reader jobs)
```

```
layerfs-workspace/src/              [1358]  (part)
  construction/                     [1288]  (part)
    driver.rs                         174   (new: CapturedNamespace)
    outcome.rs                         51   (new: attempt, custody, work)
    namespace/                      [1063]
      cursor.rs                       356   (new: sealed rows as Content's input)
      inodes.rs                       199   (new: values, symlinks, metadata)
      mod.rs                            8
      normalize.rs                    258   (new: name and inode passes)
      records.rs                      242   (new: sealed record kinds)
  ports/                              [70]  (part)
    captured_namespace.rs              70   (three reads added to the port)
```

```
layerfs-daemon/src/overlay/          [896]  (part)
  captured_namespace_port.rs           91   (port over owner jobs)
  commands.rs                         805   (three Read-class commands)
```

1. **Reader jobs (Overlay, Daemon).** Parent-local captured names, exact name
   points and local symlink targets of one retained reader, each an indexed
   seek on the sealed generation. None reads an active row.
2. **Validation (Content).** One validator on every route. Aliases are refused
   from derived counts with no base walk. Directory bindings are classified in
   64-row windows into restated and placed. Renames inside the base parent are
   recognized from that parent's own change rows. A territory walk lists a
   moved stored directory's subtree only when the gate is open, and a
   step-bounded upward walk proves every placement reaches the root or an
   untouched base position. On the backed route the evidence is indexed
   records in windows of 64 and no total derived from `ordering_bytes` is
   refused. The label `cycle check work limit` no longer exists.
3. **Producer (Workspace).** `CapturedNamespace::new` (no I/O) and
   `construct`, one attempt. It rebinds the base to the reader's root, seals
   one header per changed parent, one typed value per live inode and one rank
   per fresh inode into operation records, runs the existing changed-file
   constructor one file at a time, and calls Content's
   `update_filesystem_streamed_backed`. It releases nothing and returns the
   reader, the operation owner, the record custody, a failing file's whole
   custody and the first original failure.
4. **Not wired.** No product code calls the producer. A daemon test's closure
   runs it inside the unchanged `BoundWorkspace::commit`. Mounted Commit and
   the product closure are R5.

There is one encoder and one validator. The producer builds canonical bytes
only through Content's constructors and the existing `CapturedFileEdits`.

## Proof rows

Host is macOS ARM64 with `cargo +1.85.1 --locked`; Linux is the pinned image
`sha256:378b799e…2cd6`. Each row passed on both sides at `5747f0119` unless
stated. "Harness" means the Workspace crate's direct Overlay harness, which
has no Persistence Store; "driver" means the daemon test over a real installed
Disposable Store.

| Row | Verdict | Evidence and scope |
| --- | --- | --- |
| R4-1 Full namespace, bytes, metadata and link identity against an independent oracle | PASS | Harness: `captured_namespace` (10), `captured_namespace_states` (18), `captured_namespace_rebound` (8), `captured_namespace_metadata` (2). Driver: `captured_commit::every_kind_of_change…` reads the root back from a fresh bind. Two separately written walkers; each decodes the inode table itself and requires it to equal the reachable set |
| R4-2 Wide, deep, sparse, alias and cycle; a cycle is refused with its exact failure and nothing is published | PASS at producer scope; not exercised through the driver | Content: `filesystem_topology` (26), `filesystem_topology_backed` (45, both routes), `filesystem_topology_model` (400 generated batches). Producer: `captured_namespace_perturbed` — an alias is `multiple parents` after two directory leaves were offered and no filesystem root; a cycle is `effective tree cycle` with nothing offered. Real Workspace operations cannot state an alias or a cycle, so these two need a row-perturbing provider and were not driven through `BoundWorkspace::commit` |
| R4-3 Every row of the R3 state table | PASS for rows 1–5 and 7; row 6 PARTIAL | Rows 1, 2, 7: `captured_namespace_holds` (7) with real open descriptors and lookups. Row 3: link-count cases in `captured_namespace_states` and the driver test. Rows 4, 5: `captured_namespace_rebound`. Row 6: `captured_namespace_fragments` covers shrink, regrow, truncate-to-zero and stores clipped at the size through `StoreOpen`; **real dirty mapped pages from a kernel mount are not reachable from this harness** and are not proven |
| R4-4 Fragmentation gives the same root | PASS | `captured_namespace_fragments` (9): root identity between shapes whose unwritten ranges are unwritten in both |
| R4-5 Cursors and points agree; long shared prefixes and 255-byte names | PASS | Overlay `captured_namespace` (5), Daemon `captured_namespace` (3), Workspace `captured_namespace_cursor::every_cursor_pass_and_point_answers_the_same_sealed_rows` |
| R4-6 Incremental cost, counted not timed | PARTIAL | Flat over base size: producer rows, pages and points, placements, ancestry steps, territory, in-place scan, owner jobs (538 / 566 / 538) and SQL statements with no full scan. **Not flat:** Content's sorted merge read 8 / 36 / 206 inode pages and the Store 55 KiB / 174 KiB / 1.1 MiB of packs at 310 / 1,738 / 13,162 entries; validation read waves 15 / 15 / 20. See owner item 9 |
| R4-7 Bounded state by counted windows | PASS on the backed route, by counts | Largest captured page 64, classification window at most 64 rows, record windows of 64, one page and its records applied before the next page is requested (`captured_namespace_order`), pages bounded as the change grows (`captured_namespace_cursor`). No resident-memory measurement |
| R4-8 Later active mutations do not appear; install and Close leave the reader's original state | PASS | Overlay `later_active_rows_install_and_close_leave_reader_answers_until_exact_release`; Daemon `direct_provider_pages_and_points_keep_the_exact_capture_after_install_and_close`; Workspace `captured_namespace_order`; driver `mutations_after_the_capture_stay_out_of_its_root_and_survive_install` |
| R4-9 Failure custody for file, namespace, backing and captured reader | PASS at producer scope; one cause through the driver | `captured_namespace_custody` (14): 33 injected failures over all 13 provider call kinds, each found in exactly its expected slot with no provider call after it. Driver: `a_failed_attempt_keeps_its_custody_publishes_nothing_and_a_later_commit_is_exact` with the engine's Stale refusal |
| R4-10 The producer through `BoundWorkspace::commit` over a real Disposable Store; fresh-bind read-back equals the oracle | PASS | `captured_commit` (5 tests), host and Linux. The closure is test code; no product caller exists |

No row rests on a resident `FilesystemInput` demonstration or on an
existing-file-only case.

### Final suites at `5747f0119`

One run per test binary, 100 s wall limit each, `--test-threads=1`, all 13
workspace members, binaries built once beforehand.

| Side | Binaries | Passed | Failed | Tests passed |
| --- | --- | --- | --- | --- |
| Host | 211 | 209 | 2 | 1,032 |
| Linux, pinned image, FUSE device | 211 | 209 | 2 | 1,015 |

Receipts: [host](checks/r4-captured-namespace-20261008/final-host-suite.txt),
[Linux](checks/r4-captured-namespace-20261008/final-linux-suite.txt), with a
full log beside each. The four failing invocations are the three cases the
handoff lists as needing explicit preconditions that were not supplied. They
are failures of those invocations and unrun as proofs:

- `complete_installed_roots::huge_native_namespace_is_complete_after_install`
  (host and Linux): needs its separately prepared sealed namespace.
- `host_handoff` (host): needs its Docker placement.
- `shared_processes` (Linux): needs explicit named-volume placement.

Platform-gated binaries report zero tests on the other side (macOS acquisition
and seal cases on Linux; native mount cases on the host).

Also at the final identity: warning-denying Clippy over all targets and
`fmt --check` for the four changed crates on the host, the product boundary
guard (845 production files), and the 49 tooling self-tests. All passed.

## Every failed attempt

All receipts are kept under
[checks/r4-captured-namespace-20261008](checks/r4-captured-namespace-20261008/).
Each failure was diagnosed from source before a new source identity was run;
none was rerun unchanged and none is relabelled.

| Receipt | Result | Cause | Outcome |
| --- | --- | --- | --- |
| `settle-attempt1-root_qualification` | FAILED 0/1 | Run before any change: the host owner-credit race R3 recorded (the test reads `outstanding` right after a completion while the engine thread still holds its credit) | Test-only bounded wait added to three binaries (`8885bb765`); `settle-attempt2-*` passed. The three R3 host failures (`captured_file_edits`, `root_qualification`, `edit_backing`) pass at the final identity |
| Plan section 4.2, before any product edit | Counter-example | The territory gate accepted a cycle through a fresh directory (`e/p`, `p/d`, unbind `d`) | Plan Amendment 1; a test of the counter-example |
| `C-attempt2-filesystem_topology` | FAILED 23/1 | An existing fixture named a directory its base had dropped | Fixture corrected |
| `C-attempt1-filesystem_topology_counts` | FAILED 11/2 | The lead's expected demand counts were wrong | Expectations corrected from source |
| `C-attempt1-filesystem_state_backed` | FAILED 9/1 | Whole-counter comparison between routes whose window differs | Comparison narrowed to route-independent fields |
| `W-attempt1-captured_namespace_custody` | FAILED 13/1 | A role-based rejection in the test hit the root's metadata value first | Test corrected |
| `W-attempt2-captured_namespace_states` | FAILED 18/1 | A guessed counter value in the test | Corrected from source |
| `W-attempt3-captured_namespace_cursor` | FAILED 2/1 | A ratio taken from inside one window | Test corrected |
| `W-attempt1-namespace_daemon` | FAILED 7/1 | The same host owner-credit race in a fourth binary | Bounded wait added in the test |
| `W-attempt1-captured_namespace_holds` | FAILED 4/3 | Expected a tombstone row after install; the engine moves a held orphan to its own domain | Expectation corrected |
| `W-attempt1-captured_namespace_metadata` | FAILED 1/1 | Counted the untouched root as patched | Corrected |
| `W-attempt1-captured_namespace_order` | FAILED 1/1 | Helper counted Content's own records in scope 0 | Corrected |
| `W-attempt3-captured_namespace_order` | FAILED 1/1 | **Deliberate** single run against the pre-fix record flush, to show the test detects it | Source restored; attempt 2 and the final suites are the result at the committed source |
| `I-attempt1-captured_commit` | FAILED 2/2 | The closure returned Content's label, which the driver classifies as unknown; a settle call ran while the Commit's completions were held | Closure hands the driver the original cause; see owner item 5 |
| `I-attempt2-captured_commit` | FAILED 3/1 | Asserted producer `record_jobs` equal across base sizes; it varies with slot collisions of the 64-slot answer window | Assertion replaced by the exact arithmetic |
| Linux build, two invocations | Compile error "unclosed delimiter" | The container saw a stale, shorter length for files just edited on the host (first `layerfs-content`, then `layerfs-daemon`) | Not a source defect: all 1,418 tracked files then matched the host by SHA-256 inside the container and the third build succeeded. No test ran against a stale file |
| Benchmark harness `cargo check --locked` | Failed | `core/benchmark/fs-bench-pro-storage-content` has a stale lockfile at HEAD | Not updated. The harness is unverified against R4; see below |

Test-side corrections found by review rather than by a failing run are in the
three review-round commit messages.

### Corrections of the lead's own statements

- The track I commit message (`758f4594d`) says validation and topology
  counters are identical across base sizes. Only placements, ancestry steps,
  territory, peak window and the in-place counters are, and only those are
  asserted; read waves and inode pages are not.
- The track P and track W commit messages call their Overlay-only harnesses
  "Disposable". Those harnesses use `ProfileConfig::default()` on the daemon
  overlay and have no Persistence Store. Only the daemon `captured_commit`
  tests run over a Disposable global Store.
- `C-attempt3-filesystem_topology_counts.txt` contains a wall-time line the
  lead asked for. It is diagnostic, was removed from the test, and supports no
  claim.
- The committed plan's section 3 printed five wrong directory subtotals and
  did not contain the in-place design. Amendment 2 corrects both.
- Architecture documentation was not updated in the same commits as the
  product changes; it is updated in the commit that adds this record.

### Not verified

- `core/benchmark/fs-bench-pro-storage-content` still batches under its own
  `WALK_CEILING` of 4,096 and its comments and `namespace_batch_probe`
  describe the retired refusal. Its lockfile is stale, so it was neither built
  nor changed. `tests/pinned_expectations.rs` names a field `additions`.
- Real dirty mapped pages (row 6), and a name row under a fresh tombstone
  directory from a real sequence (only the perturbing provider produces one).

## Decisions that need owner review

1. **Ancestry evidence.** The canonical format and the Capture record no
   parent pointer, so a directory moved across parents under a stored
   directory lists its surviving subtree once. That is change-plus-subtree
   work, not change-only. Accept it, or add ancestry evidence to the format or
   the Capture.
2. **Inductive base validity.** Alias validation reads no base page; it relies
   on every published root having passed this validator.
3. **Refusal timing.** `multiple parents` for a derived count arrives after
   rebuilt directory pages, file content objects, symlink objects and metadata
   trees were offered to the consumer. The filesystem root never is. The
   "missing base inode" header check also precedes the symlink verdict.
4. **Stricter update route.** A directory bound only inside a directory the
   operation allocates and never binds is now refused with
   `effective tree cycle` on build and update alike. The old update route
   accepted it and emitted an unreachable inode.
5. **Failure classification is the caller's.** On failure the producer's
   Content result is a label, and the unchanged driver treats that label as an
   unknown outcome. The R5 closure must hand the driver the original cause
   from `CapturedNamespaceCustody`, and must decide the order of releasing the
   reader and the operation owner when the cause is uncertain. The custody has
   no slot for a failed base read through the canonical client.
6. **Admission during construction.** Every port call is one synchronous
   attempt; nothing is deferred or retried inside the producer.
7. **Page and point predicates differ** (`gen = captured` against
   `(installed, captured]`). They agree because a failed capture's rows are
   folded before the next capture; a test proves it, including across a fold.
8. **`FilesystemResources::default()`.** The producer uses Content's defaults;
   no Store-derived resource source exists.
9. **Sibling reads in the sorted merge.** A one-row change reads the sibling
   pages of each inode branch it touches, at most 127 per level. This existing
   cluster-one behaviour is why row R4-6 is partial.
10. **Per-row owner jobs.** Above one 64-row window each captured row costs
    about 19 producer record jobs and about 35 record point reads in total
    with Content's backed state (640 files: 12,358 record jobs). Linear and
    window-bounded, but it should be addressed before a large Commit is timed.
    A fix needs a batched record read or grouped points in Content.
11. **The territory gate is one decision per operation.** An unrelated `mkdir`
    under a stored directory makes a directory moved to the root list its
    subtree. A narrower sound gate exists and is not implemented.
12. **The in-place pass scans the root** as a parent when the gate is open.
13. **Rows written by `Overlay::publish`** can carry shapes Workspace never
    produces; the producer was proven against Workspace-shaped rows and four
    perturbed shapes only.
14. **`CheckedInput.additions` and `validate::check`'s unused argument were
    removed** from Content's crate-internal surface.
15. **Smaller cost points left as they are:** the operation input is checked
    twice, so name change points are twice the entry rows; a key sequence ends
    only on an empty window; a file bind re-reads its inode.

Items 9, 10 and 11 are optimizations the lead chose not to implement inside
R4.

## How subagents were used

| Phase | Agents | Role |
| --- | --- | --- |
| 1 | Six read-only audits | Source facts for the plan; they ran no Cargo, Docker or test command. Findings are in plan section 1 |
| 3 | Track C (Content), track P (reader jobs), then track W (producer) | Disjoint write sets. The lead reviewed each diff and receipt, staged and committed |
| 3 | Track I | Done by the lead: the daemon model, the driver closure and the cost test |
| 4 | Four fresh-context reviewers in parallel: proof coverage, custody, counted cost, rules | Each finding was checked against source before any change |

Every brief carried the binding rules. No subagent staged or committed.

**What review changed.** The removed-only in-place rule missed
`mv d d.old; mkdir d`: widened. The validator memo was cleared when a demand
did not fit: it now keeps what the current demand names. Records could cross a
captured-page boundary: flushed per page. Attribute key names were spelled in
Workspace: taken from Content's export. The symlink reply charge omitted its
mask and header: corrected. Ten proof gaps (real holds,
rebound names, repeated moves, perturbed rows, metadata patch, unreachable
inodes, custody slots, later mutations, ordering, fragments): tests added.

**Findings rejected or not acted on, with the reason:**

- Custody nit: rows already in a retained window are served without a
  readiness check. Rejected: no provider job runs and nothing can fail there.
- Rules: `explain_reader_directory_entries` as a test hook. Rejected: it
  follows 26 existing `explain_*` diagnostics on the same type.
- Cost: narrow the global gate. Sound and not implemented; owner item 11.
- Cost: constant-factor points. Recorded as owner item 15.
- Custody: release order for an uncertain cause, and the closure depositing
  strings instead of custody. These are the R5 closure's contract; owner
  item 5.
- Rules: backed record guards against silent resident inserts. Recorded; the
  resident maps are bounded by the existing ordering budget.

**Deviations by subagents, kept on record.** Track P ran `rustfmt` once over
`layerfs-workspace` (formatting only; track W confirmed nothing was lost) and
invoked Cargo once outside the helper, under the lock and read-only. Track C
found the plan's gate unsound before editing anything.

## Production LOC per commit

Method for every row: pinned `tools/production_loc.py` (SHA-256
`c0fe7f36…24adb`), the commit's first parent against its committed tree,
first-party Rust and shipped SQL, with comments, blanks, tests, docs and tools
excluded. Unchanged in every commit: excluded core predecessors 38,878,
excluded integration 3,996, root reference 65,417.

| Commit | Subject | Combined before -> after | Delta | Core active after |
| --- | --- | --- | --- | --- |
| `33a34bbb3` | Handoff and layout listing rule | 180,871 -> 180,871 | 0 | 72,580 |
| `8885bb765` | Bounded credit wait in three host test binaries | 180,871 -> 180,871 | 0 | 72,580 |
| `71548aa1e` | Deepest-file plan | 180,871 -> 180,871 | 0 | 72,580 |
| `8dc5aa2a2` | Plan Amendment 1 | 180,871 -> 180,871 | 0 | 72,580 |
| `61c0f4aa3` | Track P | 180,871 -> 181,122 | +251 | 72,831 |
| `fc7fc7037` | Track C | 181,122 -> 181,643 | +521 | 73,352 |
| `e88dee6e4` | Track W | 181,643 -> 182,929 | +1,286 | 74,638 |
| `758f4594d` | Track I (tests) | 182,929 -> 182,929 | 0 | 74,638 |
| `dd6891ebe` | Track C review round | 182,929 -> 182,937 | +8 | 74,646 |
| `d2d9d7b17` | Track W review round | 182,937 -> 182,947 | +10 | 74,656 |
| `5747f0119` | Track I review round | 182,947 -> 182,952 | +5 | 74,661 |
| this commit | Documentation and this record | 182,952 -> 182,952 | 0 | 74,661 |

R4 in total: 180,871 -> 182,952 (delta +2,081), all of it in active core
(72,580 -> 74,661). Nothing was retired and no legacy code moved; the growth
is new product code for the reader jobs, the validator and the producer.

## Not claimed

- No timing, cold-cache, storage, resident-memory or throughput result.
- No mounted Commit, no product caller of the producer, no change to the
  Commit driver.
- No Durable execution.
- No qualification of the benchmark harness against the new validator.
- Owner acceptance of the fifteen items above.

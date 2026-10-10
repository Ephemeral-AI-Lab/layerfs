# R8b deepest-file repair and requalification plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Written 2026-10-10 at `4a920a3ee` (product tree `core/crates`
`9a76077239b4db1955b83bb29e9134745efb963c`, unchanged since `7999a6356`).
Committed before any product edit. It plans work; it proves nothing. The closed
[R8](../../R8-COMPLETION-20261010.md) and [R9](../../R9-COMPLETION-20261010.md)
records and every earlier receipt keep their outcomes.

## 1. Authority and what cannot change

The owner's hand-over of 2026-10-10 assigns repair of the remaining
qualification gaps, a new frozen R8 examination, and R9 executed or truthfully
closed. Decisions follow "Deciding without the owner" in the
[R5–R9 handoff](../../HANDOFF-R5-R9-20261009.md#deciding-without-the-owner).

- Global Store: Disposable/WAL/synchronous=OFF, selected explicitly. Durable is
  `NOT_RUN — disabled by owner until explicit reauthorization`. Overlay stays
  MEMORY/OFF/EXCLUSIVE. `LAYERFS_CONSTRUCTION_WORKERS=1` is exported.
- Product edits are fixes of demonstrated contract defects only. No
  optimization, new dependency, canonical-format or negotiated-profile change,
  product test hook, retry or replay, new thread, spinning, CPU pinning, larger
  limit or longer lifetime.
- No workload, oracle, cache state, limit or timeout changes to turn a miss into
  a pass. No test, verifier, guard or checker is weakened.
- Every test command has a wall stop of at most 100 s. One Cargo, Docker, test
  or measurement command at a time under the checkout lock.
- No push, pull request, publication or additional worktree. Only the lead
  stages, commits, counts LOC, edits the ledger or a completion record, or
  removes anything.

## 2. What the audits established

Six read-only audits at `496bb5643` read the contracts, the source and the
tests for every open row. Their reports are working notes, not evidence; each
claim below is re-read against source before it is acted on.

**No audit found a product contract defect on a proved path.** Two contract
behaviours are absent from source (section 3). Most open halves are closable by
new external tests (section 4). A third group cannot be closed in this run
without an owner ruling, or cannot be constructed without a product hook
(section 6). The consequence is stated now so that it is not a surprise at the
end: unless the owner rules on section 6, "every R8 functional proof passed"
cannot become true, timing stays gated by the proof plan's fixed order, and R9
closes again as `NOT_EXECUTED — conditions unmet`. The work below is still
owed: it converts PARTIAL rows into proved scopes and leaves each remaining
gap with one named decision.

## 3. Product defect fixes

Production LOC is the `prod` column of the pinned `tools/production_loc.py
--files` (SHA-256 `c0fe7f36…4adb`) at `4a920a3ee`. Bracketed totals cover only
the files shown. Tests live in each package's `tests/` and are not counted.

### 3.1 P-A: low-water early serial reservation (gate G01, row P-1)

Contract: owner ruling P-1
([specification 15.3](../../S8-SPECIFICATION-20261008.md#153-owner-rulings-2026-10-08)):
"Once the local unconsumed serial range is below an explicit low-water
configuration value, a create makes at most one reservation attempt, outside
every Workspace lock as today. A `Busy` result is that attempt's exact
before-effect refusal and is never replayed." Source reserves only at
exhaustion (`workspace/serials.rs`); no low-water value exists.

```
layerfs-workspace/src/                [42]  (part shown)
  workspace/                          [42]
    serials.rs                          42   (early attempt below a given low-water)
```
```
layerfs-bridge/src/                  [164]  (part shown)
  daemon_setup.rs                      164   (explicit serial_low_water in DaemonLimits)
```
```
layerfs-daemon/src/                  [891]  (part shown)
  application/                       [231]
    config.rs                          120   (refuses a value at or above the refill window)
    filesystem.rs                      111   (passes the value to the Store)
  service/                           [370]
    filesystem_port.rs                 370   (reserve_serial: early outcome disposition)
  store/                             [410]
    open.rs                            114   (Store keeps the configured value)
    operation.rs                        73   (exposes it to the request port)
    ports.rs                           223   (separate custody scope for the early attempt)
```

Ownership. `serials.rs` owns the decision: under its existing mutex take one
serial and compute the unconsumed remainder; after the lock is dropped, if a
serial was taken and the remainder is below the low-water, make the one
`reserve(SERIAL_REFILL)` call and push the range on success. If no range
existed, the existing exhausted path is the one attempt. Never both, so
attempts per create stay at most one. The early attempt uses a custody scope of
its own, because `StorePorts::attempt` keeps an operation's first failure and
an early `Busy` must not fail the same create's later base demand.

Frozen interface between tracks: `DaemonLimits { …, serial_low_water: u64 }`,
encoded as one additional `u64` at the end of the limits in the private
configuration record, with the record magic advanced to `LFSD\x02`.

Proofs owned: a mounted test with a real external peer holding the Store
writer (the existing `HeldWriter` technique): with an explicit low-water, a
create below it makes exactly one early attempt, an early `Busy` leaves the
create successful and makes no second attempt, a later create makes its own
single attempt, and with low-water 0 no early attempt is made. A component test
of the range arithmetic with a counting allocator. Attempt counts come from the
existing `serials` counter.

### 3.2 P-B: Mount refused while maintenance is stopped (gate G02, rows FP-26 and R6-7)

Contract: specification 4.2: "New `Mount` while maintenance is stopped or
declared debt headroom is exhausted | `Capacity`, phase `mount:debt` | None
(D-13)". Source keeps the first maintenance failure
(`overlay/queue.rs`, `OwnerClient::maintenance_failure`) and nothing reads it;
`Service::mount` checks only duplicate incarnation and Workspace capacity.

```
layerfs-daemon/src/                  [231]  (part shown)
  control/                           [231]
    operations.rs                      231   (mount(): refuse before Entry::Binding)
```

One check before the registry entry is created, reusing the existing accessor,
`ControlCode::Capacity` and the string phase. No thread, limit, configuration,
wire byte or owner job is added, so the warm-mount count of H-2 is unchanged.

Proof owned: a daemon test in which a real resource stops a maintenance turn
and the next `Mount` is refused `Capacity` / `mount:debt` with owner admission
unchanged and no registry entry, while an already mounted Workspace keeps
serving. R6 judged this unstageable without a hook; the admission audit found
one candidate (a real change of the overlay backing file's path identity while
reclamation is pending). If no deterministic hook-free stage exists, the fix is
**not** committed unproven: the row stays PARTIAL with the attempted stage
recorded.

Not built here, each an owner decision (section 6): the declared debt headroom
value, and maintenance failure in Status.

## 4. Rows closable by external tests only

No product change. New test files are preferred to edits of existing ones; an
existing assertion is never removed or loosened.

| Track | Rows | Tests to add (package `layerfs-daemon/tests` unless stated) |
| --- | --- | --- |
| T-A mapped pages | FP-15 (Commit inclusion), R3-STEP-6, R5-9, R4-3 row six | Store through a shared mapping, no `msync` and no direct read, Commit at once: request frames unchanged, published bytes are the old bytes; then `msync`, second Commit exact. The contract says a still-dirty page is outside the frontier ([architecture 79](../../../../architecture/79-product-commit.md)); an early kernel flush is detected and reported as not established, never looped on |
| T-B install survival | R5-5 | `stx_atime` in the forced table; `O_DIRECT` read of every model file after install with no kept descriptor; a mapping held across two installs; a pipe-stepped writer parked at each step with a Commit at each |
| T-C failure matrix | R5-6, R5-7 | Mounted missing dependency (one object-location row removed by an external SQLite process, then a neighbouring change); attempted install failure and unknown-without-publication by a real file-size limit on the test process at the existing history gate; release refused after a settled failure by releasing the Commit's reader through the public owner client first |
| T-D producers and profile | R5-10 | Per Commit `storage.policy == 1` and one initial reservation; the writer session's own profile read-back. Recorded as a fact, not fixed: no product source reads `LAYERFS_CONSTRUCTION_WORKERS`; one producer is structural |
| T-E topology through the driver | R4-2, R4-9 | Alias and cycle rows written into the real engine through the public publish command, then `commit_captured()`; each remaining provider cause through `BoundWorkspace::commit` with a failing owner adapter in the test closure |
| T-F actual executable | R5-1 note, R2-STEP-3 | Changed Commit and fresh-mount oracle through the real daemon executable |
| T-G negotiation | FP-2 (flag half), R2-STEP-2 | The forbidden negotiation set compared explicitly with the Ready receipt and mountinfo |
| T-H detach stages | FP-21, R2-STEP-5 | Close retained by holding exactly one Lifecycle completion; OPEN and READDIR inside a provider read at a real mount; FORGET units behind held Lifecycle credits at detach |
| T-I reader health | FP-27 | Marker at the READ step (open healthy, quarantine, then a direct read); `Capacity` retained by taking the mounted Workspace's own read tickets through the public Store |
| T-J lookup counts | FP-31 | Underflow and failure custody on a real kernel FORGET, with the count reduced first through the public owner port and FORGET forced by cgroup reclaim |

## 5. Harness, counts, resources and the frozen examination

- **H-A full fixture (gate G07).** Done in `4a920a3ee`: counted diagnosis and
  the concurrent comparator. One invocation at the final registered identity,
  under the unchanged 100 s command stop and 85 s comparator stop.
- **H-B actual runtime (gate G05).** `lifecycle_proof.py` once at the final
  identity (FP-1, R2-STEP-3, R2-STEP-7 small root). New external harness
  commands, product unchanged: protected path, `/proc` alias and descriptor
  matrix with two Workspaces (FP-5-Runtime); 64 MiB per stream with verifying,
  stalling and failing sinks (FP-6-Runtime); descendant-held pipes, fast exit
  with a delayed consumer, window-edge sizes (FP-7-Runtime, FP-30-Runtime);
  Busy for cwd, descriptor and mapping, A never holding B, Force refused at
  `force:capability` (FP-22-FS). Each records the actual outcome; where a
  contract sentence is disputed (section 6) the row is reported against both
  readings and not judged.
- **H-C counts (gate G08).** Assertions over sections the receipts already
  emit for H-1, H-2, H-4, H-6 to H-9, H-15, H-16, H-18 and H-19 where a source
  of observation exists; each remaining quantity is listed with the reason no
  observation exists. No expected number is invented after the fact: a count
  recorded here for the first time is a baseline, labelled so.
- **H-D resources (gate G09).** Phase-boundary observations from the runtime's
  existing snapshot (daemon RSS and threads, cgroup memory, backing file
  logical and allocated bytes) around Mount and Unmount, with the sampling
  limit stated; per-file residency through the existing `r7-cache` tool.
- **H-E content/storage harness.** Port
  `core/benchmark/fs-bench-pro-storage-content` to current public APIs (build
  receipt 021 failed on removed ones). The historical 4,096 batch stays a
  workload constant.
- **H-F registration and run.** A new registration is committed before its
  first invocation and pins source, product, flags, dependencies, binaries,
  image, harness, workload, oracle, cache state, limits, arms, classes and
  verdict vocabulary. Original selection identifiers stay visible; successors
  are appended. Order: full-byte proof once, affected functional proofs once,
  then every eligible measured case and arm once.

Timing is run only for selections whose prerequisites are complete. If the
mandatory gates are still open at the freeze, every timing selection is
recorded `NOT_RUN` with zero attempts and its specific reason, as before.

## 6. Open without an owner ruling

Carried as `PENDING OWNER`; independent work continues around each.

| Item | Rows | What is needed |
| --- | --- | --- |
| Deployed low-water value | P-1 | The ruling sets no number. This plan builds the mechanism with an explicit configuration value and deploys 0 (no early attempt, today's behaviour) |
| Declared debt headroom | FP-26, R6-7 | No value or comparison exists in any contract; reading debt at Mount would add an owner job against H-2 |
| Maintenance failure in Status | FP-26 | Needs an additive wire record or a changed meaning of an existing field |
| System-call fixture for the test's own threads | FP-2 (READ/WRITE maxima), FP-15 (WRITE header identity), FP-16 (held GETATTR), FP-21 (Join), FP-33 and FP-23-FS (abort outcomes, independent trace) | Whether a seccomp fixture outside product source is acceptable; the alternative is product telemetry that looks like a test hook |
| Stages with no producer or no kernel input | FP-21 (Owner, Lane, Registry), FP-27 (release after a failed demand, concurrent demand, foreign reader), FP-31 (foreign incarnation), FP-33 (refusal after admission, unknown abort), R5-7 (release refused before admission) | Scope each row to what a real mount can present, or accept component scope |
| Runtime contract points | FP-5-Runtime, FP-6-Runtime, FP-7-Runtime, FP-22-FS, FP-25-Routes | Which stream-EOF sentence governs; whether commands may create namespaces in the Sandbox; the sibling-visibility declaration; Force in the Sandbox topology |
| Base-size growth | R4-6 | Recorded limit; flattening needs canonical summaries, which are forbidden proposals |
| Telemetry that does not exist | H-11, H-13, pager and journal statistics, continuous request maxima, receive-buffer accounting, per-statement correlation in a mounted run | Product telemetry is not a defect fix |
| Passthrough arm | all `P` selections | The fuser integrity checker refuses the passthrough manifest; checker unchanged, `P` unrun |
| Inherited | A2 cells, the materially-better threshold, the five kept-or-dropped items, O-10, R2/R3/R4 acceptance, group commit, payload outside SQLite, inode-update tradeoff | As listed in the R8 completion record |

## 7. R9b

- Migrate the content fixture-seal guard to an independent current route:
  frozen hashes, ignored-only generation and the explicit
  `LAYERFS_SEAL_FIXTURES` gate are kept, and the migrated guard is shown to
  fail when it should.
- Generate the historical receipt recovery catalogue as an appended file; no
  raw evidence or verdict is rewritten.
- Re-audit manifests, includes, tests, scripts, harnesses and needed
  documentation at the final identity, with an explicit disposition for the
  root Cargo members, the reference benchmark and schema reader, the evaluator
  SDK dependency and the legacy container build route.
- Remove root `crates/` and `core/reference-tests` only if all four written
  conditions hold, by `git rm` in an isolated retirement commit. Otherwise
  close `NOT_EXECUTED — conditions unmet` with each unmet condition, its
  receipt, the attempted resolution and the prepared removal plan.

## 8. Tracks, ownership and order

Write sets are disjoint. A needed change outside a track's list is reported,
not made.

| Track | Owner | May create or edit |
| --- | --- | --- |
| P-A, then P-B | one implementing worker | The product files of section 3; `layerfs-bridge/tests/daemon_records.rs`; every `DaemonLimits` constructor (`layerfs-api/sdk/examples/`, `layerfs-daemon/tests/observed_application.rs`, `native_application.rs`, `core/benchmark/r7-runtime/src/runtime.rs`); new `layerfs-daemon/tests/mounted_low_water.rs` and `mount_debt.rs`; `layerfs-workspace/tests/namespace.rs`; the affected architecture documents |
| T-A to T-F | one implementing worker | New files under `layerfs-daemon/tests/` named for their rows; `layerfs-daemon/tests/mounted_commit.rs` and `captured_commit.rs` (additions only) |
| T-G to T-J | one implementing worker | New files under `layerfs-daemon/tests/` named for their rows |
| H-A to H-F, R9b, every commit and record | lead | Everything under `core/benchmark/`, `core/docs/`, `core/tools/`, `layerfs-content/tests/` |

Order: this plan; P-A and the test tracks in parallel; P-B; review of every
diff by the lead, then a fresh-context review; final host and Linux suites once
at the final identity with fmt, warning-denying Clippy, the boundary guard and
the tooling self-tests; registration; the full-byte proof; the remaining
proofs; outcomes for every selection; R9b; completion records.

## 9. Decisions taken without the owner

| ID | Decision | Alternative | If the owner rules otherwise |
| --- | --- | --- | --- |
| L-1 | The full-byte comparator observes with 8 bounded walker threads and compares in one thread; assertions, stops and inputs unchanged | Split the proof into several commands over disjoint path ranges, each under the unchanged stops; or keep the serial comparator and record FAIL until the owner enlarges the exception | Restore the serial traversal (one constant) or register the split form; receipt 053 is untouched either way |
| L-2 | P-1's mechanism is built with an explicit `serial_low_water`; deployments and harnesses set 0; tests set a nonzero value | Leave the mechanism unbuilt until the owner names a number, as R6 did | Change one configuration value; no stored state depends on it |
| L-3 | The configured value must be below the 1,024-serial refill window; a larger value is refused at startup | Accept any value | Remove one validation |
| L-4 | An early attempt that fails with anything other than writer contention fails that create with the original error; the serial it took is consumed, never recycled | Drop every early failure | One match arm |
| L-5 | Mount is refused on stopped maintenance only; no headroom number is invented and Status is unchanged | Add a configured headroom and an additive Status record | Additive later; nothing built here blocks it |
| L-6 | A still-dirty mapped page is proved to be outside the Commit, following the #303 contract and architecture 79 over the R5-9 handoff sentence read alone | Treat "in the next Commit" as requiring a flush | A product change with its own contract revision; the new test then documents the old behaviour |
| L-7 | Rows that need a ruling in section 6 are reported against the written text and left PARTIAL; none is closed by reinterpretation | Close them at component scope | Rescope and reuse the receipts recorded here |

## 10. Corrections after independent review (2026-10-10)

Appended after the fixes were committed (`c127b6d55`, `ea812d3f8`); the rows
above are left as written. Review record:
[019-product-review.md](019-product-review.md).

- **L-4, consequence understated.** "Fails that create" is true of the reply,
  but the request service retains any mutation failure that is neither fenced
  nor a base demand, and a retained request makes the mount serve nothing
  further; its unmount then stops at retained requests. A non-contention early
  failure therefore ends the Workspace's service, exactly as the same failure
  on the exhausted path always has. The behaviour is kept as the exact-custody
  treatment of an uncertain Store write; the comment, the port contract and
  architecture 77 now say so. Alternative for the owner: fail only that create
  and keep serving, which needs a new failure class in the request service.
  No test reaches this arm, and it is unreachable while the value is 0.
- **L-3, reason misstated.** A finite value at or above the window does not
  make every create attempt a reservation: held ranges are summed, so 1,024
  behaves like 1,023. The actual property of the bound is that one early
  reservation of 1,024 always restores a low-water below 1,024, while a larger
  one would have consecutive creates each attempt one until enough windows are
  held. The bound also caps the reserve P-1 provides at under 1,024 creates.
  Code unchanged; the comment is corrected.
- **P-B, scope of the proof.** The staged failure is a quarantined engine, for
  which the previous product already refused the Mount (as `Unknown`, leaving
  a binding entry and an owner job). Maintenance stopped while the engine is
  otherwise usable is not staged. FP-26 stays PARTIAL.

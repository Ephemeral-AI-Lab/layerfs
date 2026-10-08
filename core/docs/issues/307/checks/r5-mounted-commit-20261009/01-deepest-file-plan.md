# R5 deepest-file plan: mounted live Commit and known install

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Input source `91e616291` (product equal to `5747f0119`). Assignment:
[handoff](../../HANDOFF-R5-R9-20261009.md#r5-mounted-live-commit-and-known-install).
Global Store profile Disposable / WAL / synchronous=OFF only; Durable
`NOT_RUN — disabled by owner until explicit reauthorization`. R5 reports
functional results and counted work; it makes no timing, cold, storage or
memory claim.

## 1. Source facts from the audit

Five read-only audits (control route, producer and test closure, install and
kernel coherence, test infrastructure, contracts) ran no Cargo, Docker or
test command. The lead checked each fact below against source.

1. `Service::execute_control` is the only product caller of control; for
   `Request::Commit` it calls `unavailable_commit` (`control/registry.rs`),
   which refuses `Invalid` after the idle check. `Service::execute` and
   `serve_one`, which take a caller constructor, are reached only by tests.
2. `BoundWorkspace::commit` (`store/commit.rs`) owns Capture, begin Save,
   the constructor call, `Save::finish`, validation, `prepare_base_install`,
   `stage_and_commit`, `InstallPrepared` and the snapshot swap. It stays
   unchanged.
3. The producer `CapturedNamespace` needs the exact reader, one operation
   owner, the Store-derived construction policy, the Save as authenticated
   objects and its sink as consumer. It releases nothing. Its result on
   failure is a label; the first original cause is in exactly one of
   `custody.failure`, `custody.file` and `custody.records.failure`.
4. The only working composition is the test closure in
   `tests/captured_commit.rs`. It releases the reader and the owner inside the
   closure, before the driver finishes, publishes, installs or settles.
5. `captured_reader` and `operation_owner` are `UNIQUE(ns, request)`. No other
   product code acquires either, so the request-number domain belongs to
   Commit alone.
6. Each control connection runs on its own `layerfs-control-{slot}` thread.
   A Commit there holds one control slot, never the SQL owner thread and never
   a Fuse receive or service worker. Each port call is one short owner job.
7. Install is one atomic Overlay job that changes only the `workspace` row and
   queues retirement. It waits for `base_readers == 0`; a queued install holds
   back later Source-class jobs. Open-file, lookup, orphan and captured-reader
   owners do not pin a base root; per-request sources do and fence install.
8. Kernel node id is `serial + 1`; generation is the mount owner; neither
   changes at install. After install an inode whose newest row was captured is
   stated from `BaseStat` instead of the Overlay row. Nothing in source checks
   that the two agree beyond root inode identity.
9. `layerfs-fuse/src/coherence` sends no notification and has no install rule.
   The contract (`303/fuse.md`, S8 specification row "Known base install")
   permits install without notification only when names, bytes, links,
   attributes, serials and link counts are identical before and after.
10. The wire already carries `Reply::Committed(Committed | UpToDate)` and a
    refusal with code, phase string, optional publication and detail. Commit
    refusal detail is not bounded to the encoder's 2,048 bytes.
11. `LAYERFS_CONSTRUCTION_WORKERS` is read by no active product source. One
    construction producer is structural: one `store.producer()`, one Save, one
    `FnOnce`, the `committing` flag, and no thread spawned in Workspace,
    Content or Storage construction.
12. A real kernel mount with the daemon assembled in-process, an externally
    launched shell under uid 65534, a shared-mapping helper and a
    mutation-following model oracle (`Model::native`, `canonical`) all exist
    in `layerfs-daemon/tests/support`. No test combines mount, producer and
    Commit.

## 2. Design

### 2.1 Product constructor

`BoundWorkspace::commit_captured` in a new `store/captured.rs` calls the
unchanged driver with the product closure and owns what the test closure did
by hand:

1. Inside the closure: build one `StoreOperation`; take the request number
   from the capture's generation (refused if not positive); acquire the exact
   captured reader, then one operation owner; run `CapturedNamespace` once
   into the Save's sink with `store.policy().construction()` and disabled
   timing; take the first original cause out of the custody and hand it to the
   driver as the `CommitError` it was.
2. **Nothing is released inside the closure.** The reader, the owner and the
   attempt's custody leave through a cell the wrapper owns.
3. After the driver returns, the outcome is known and the wrapper decides:
   - **Committed or UpToDate with known install:** release the reader, then
     the owner.
   - **Definite failure that the driver settled locally:** the same release.
   - **Anything else** (uncertain cause, known publication with failed
     install, failed local resolution): release nothing. The reader, the
     owner and the record and file custody stay in the failure.
4. A release that fails or is refused stops the sequence; what was not
   released stays in the receipt with the original completion or refusal.

Classification is the test closure's, with one addition: an `Arc<PortError>`
inside `WorkspaceError::Service` goes back as `CommitError::Content` with that
provider, so a definite Store refusal is not forced to uncertain. A base read
that failed through the canonical client is taken from the operation's ports,
as in the test. No custody slot is added for it.

### 2.2 Custody slots

`CommitSuccess` and `CommitFailure` gain `namespace: Option<CapturedConstruction>`:
the producer's counted work, Content's filesystem counters, the two original
release completions, and whatever was retained. `None` means the closure never
ran. `CommitFailure::locally_settled` keeps its meaning.

### 2.3 Control route

`execute_control` routes `Request::Commit` to the existing private `commit`
with `commit_captured`. `unavailable_commit` is removed. `Service::execute`
and `serve_one` keep their caller-constructor form (existing driver tests use
them); whether they stay public is an R7 reconciliation.

After a known Commit whose reader or owner could not be released, the registry
records `LocalFailure` with the publication and the reply is the existing
`Failure::After` form: the publication is reported and later Commit and normal
unmount are refused, as for any retained custody.

Commit refusal detail is bounded to the encoder limit.

### 2.4 Install on a live mount

No notification is added and no Fuse product change is planned. The
precondition "identical before and after" holds by construction: the root is
built from exactly the captured rows, serials are declared, and the driver
checks root inode identity. A runtime comparison of the whole view would be
whole-namespace work and is not built. The mounted proofs observe the
precondition directly; if they show a difference in any kernel-visible field,
that is a product defect to fix in the producer or the attribute projection,
recorded as a plan amendment.

A store through a shared mapping reaches the daemon as a WRITE when the kernel
writes the page back. Commit captures the published frontier, so a page that is
still dirty in the kernel is not in it and arrives later as an active
mutation. Commit forces no writeback.

## 3. Files

Production LOC from the pinned `tools/production_loc.py --files` at
`91e616291`. Only files R5 plans to add or change are listed, so each bracket
covers the listed files only. Tests live under `crates/layerfs-daemon/tests/`.

```
layerfs-daemon/src/                 [1032]  (part)
  control/                           [578]  (part)
    failure.rs                        225   (bounded Commit detail)
    operations.rs                     204   (Commit routed to the product constructor)
    registry.rs                       149   (unavailable_commit removed)
  store/                             [454]  (part)
    captured.rs                         —   new (product constructor, classification, release)
    commit_types.rs                   161   (namespace custody slots)
    mod.rs                             21   (declaration and export)
    operation.rs                       70   (cond.: accessor only if the constructor needs one)
    settle.rs                          35   (cond.)
    commit.rs                         167   (unchanged driver; success literal gains one field)
```

```
layerfs-fuse/src/coherence/           [15]  (part)
  pages.rs                             15   (header comment states the install rule; no code)
```

```
layerfs-api/sdk/src/workspace/        [16]  (part)
  commit.rs                            16   (comment only)
```

`layerfs-daemon/Cargo.toml` moves the first-party `layerfs-telemetry` from
dev-dependencies to dependencies (the producer takes a `TimingScope`). No
third-party dependency and no lockfile change.

Tests (new unless marked):

```
layerfs-daemon/tests/
  control_commit.rs                 product Commit through execute_control, no mount (host and Linux)
  control_commit_cost.rs            counted work, small and large change (host and Linux)
  mounted_commit.rs                 Linux: R5-1, R5-2, R5-3, R5-8, R5-9
  mounted_install.rs                Linux: R5-4, R5-5
  mounted_commit_failures.rs        Linux: R5-6, R5-7
  native_control.rs                 existing: the refusal case becomes a real Commit
  native_application.rs             existing: the real binary's Commit is no longer refused
  support/mounted_commit.rs         rig: built Store, mounted harness, bash, model, kernel stat table
  support/mounted.rs                existing: commit helpers
```

## 4. Tracks

| Track | Owner | Write set | Depends on |
| --- | --- | --- | --- |
| S | lead | `layerfs-daemon/src/{store,control}`, `Cargo.toml`, `control_commit.rs`, the two existing tests, comments in Fuse and SDK | — |
| M | subagent | `support/mounted_commit.rs`, `support/mounted.rs`, `mounted_commit.rs` | S |
| N | subagent | `mounted_install.rs` | S, M's rig |
| F | subagent | `mounted_commit_failures.rs` | S, M's rig |
| K | subagent | `control_commit_cost.rs` | S |

Frozen interface between S and the test tracks: `Request::Commit(token)`
through `Service::execute_control`; `Success.commit: Option<CommitSuccess>`
and `Failure::Commit(Box<CommitFailure>)` with `namespace` as in 2.2;
`Reply::Committed(CommitStagedOutcome)`. Test tracks change no product file;
a needed product change is reported to the lead.

## 5. Proof rows

| Row | Where | How |
| --- | --- | --- |
| R5-1 | `mounted_commit.rs` | Base tree with `.git` and its index, an ignore file and ignored files, a dependency directory, cache and output files, hard links and symlinks. Externally launched bash under the command identity changes every kind of thing. Explicit Commit. Unmount, fresh mount of the Branch, then `Model::native` over the fresh mount compared with the model of the first mount taken through ordinary syscalls before Commit, plus `canonical` of the published root from a fresh bind, plus kernel `st_nlink`, `st_ino` identity of aliases, uid and gid. The expected tree comes from the mounted view and an independent replay of the script on a native directory, never from constructor parameters |
| R5-2 | `mounted_commit.rs`, `control_commit.rs` | Unchanged Workspace: `UpToDate`, same head, history page unchanged. Changed: `Committed`. Stale unchanged candidate over a moved head: `Committed` that overwrites, per the Branch overwrite decision |
| R5-3 | `mounted_commit.rs` | Three separate bash launches, one exiting non-zero after writing, one background process still holding a descriptor at Commit; none registered anywhere; all changes captured |
| R5-4 | `mounted_install.rs` | Two Workspaces on one Branch, both mounted. A commits; B, bound at the earlier head, commits. B's record has the captured parent, the Branch head is B's, A's Commit stays readable, a fresh mount shows B's tree |
| R5-5 | `mounted_install.rs` | Before Commit: open descriptors on an unchanged file, a changed file and an unlinked file, a cwd in a removed directory, cached reads of every file, a full kernel stat table. A writer process appends numbered files throughout the Commit. After install on the same mount: descriptors read their exact original bytes cached and with O_DIRECT; the stat table (ino, mode, nlink, size, mtime, uid, gid) is identical cached and forced; the committed root is an exact prefix of the writer's sequence and the mount shows all of it; the next Commit captures the rest. Then unmount and fresh mount |
| R5-6 | `mounted_commit_failures.rs`, `control_commit.rs` | Real Store Busy from an external process holding the writer, at Begin and at publication; missing dependency; definite construction failure settled locally; unknown History outcome through the existing external catalog boundary, labelled as that scope; known publication with a local install that cannot be attempted. After each: the registry activity, what was published, what the mount still serves, and whether a later Commit is exact or refused |
| R5-7 | same | For each cause: the `CommitError` is the original, a definite one is settled and both owners released (`Retained*` answers none), an uncertain one keeps reader and owner retained (`Retained*` answers both) and the failure carries them |
| R5-8 | `mounted_commit.rs` | Five incremental Commits on one mount, a full oracle after each from a fresh bind; then unmount, fresh mount, oracle |
| R5-9 | `mounted_commit.rs` | A store through a shared mapping, synchronized, is in the next Commit with exact bytes on a fresh mount; a child that maps, closes the descriptor, stores and exits is captured once the kernel has written the page back |
| R5-10 | receipts of all of the above | One producer by structure (fact 11); the same `Arc<Store>` across every Commit with no seal; Disposable selected explicitly by every fixture; no Durable execution |
| R5-11 | `control_commit_cost.rs` | One small and one large-change Commit through the product route: owner jobs by class, SQL statement families, Store read batches, producer work, Content counters. Printed as evidence, exact where arithmetic exists, never timed |

Rows that the mounted route cannot stage without a product hook are reported
with the scope at which they are proven and the reason.

## 6. Decisions taken without the owner

1. **Release after the outcome, not inside the closure.** Alternative: the
   test closure's release before finish and publication. Chosen because the
   handoff says to release nothing while the outcome is unknown, and the
   outcome is known only when the driver returns. Reversible: move two calls.
2. **Request number is the capture generation.** Alternative: a daemon
   counter. Chosen because it is unique among live rows without new state.
3. **Unreleased custody after a known Commit is `LocalFailure`.** Alternative:
   report success and continue. Chosen as the option that lets nothing proceed
   past retained custody.
4. **No install-time comparison and no notification.** Alternative: compare
   the view or invalidate. The first is whole-namespace work, the second
   changes the negotiated profile's stated behaviour.
5. **Commit forces no writeback of mapped pages.** Alternative: invalidate
   inodes before Capture. Not built: the contract says Commit inclusion is the
   published frontier, and notifications are not used.
6. **The caller-constructor entry points stay** until R7 reconciles exports.
7. **Unknown History and install failure are produced by the existing
   external boundary**, not by a product hook, and keep that scope label.
8. **Pre-admission refusals that arrive as opaque `Service` errors stay
   uncertain**, the conservative side, except the typed `PortError`.

## 7. Risks

- Install waits for per-request sources. A request retained with its source
  would park install after known publication. R5 observes this under a live
  writer; a structural fix, if needed, is an amendment.
- A large-change Commit costs about 35 record point reads per captured row.
  If a proof cannot fit 100 s, R4 item 10 is brought forward with counts.
- The stat source flips from the Overlay row to the base at install. Any
  field the producer does not reproduce exactly will show in R5-5.

## Amendment 1, with track S

Section 3 planned to make `layerfs-telemetry` a production dependency of
`layerfs-daemon`. The product boundary guard refuses that edge, and the guard
is not weakened. Instead `layerfs-workspace`, which already owns the
dependency, gained `CapturedNamespace::construct_untimed`
(`construction/driver.rs`, 174 production lines before), and the daemon
manifest is unchanged. `store/operation.rs` and `store/settle.rs` needed no
change. The host-and-Linux product test without a mount is a rewritten case in
the existing `native_control.rs`; the fuller model test moves to track K as
`product_commit.rs`, which calls `BoundWorkspace::commit_captured` directly
because the registry does not expose its bound Workspace to a test.

## Amendment 2 (recorded with the fixes, 2026-10-09)

Three corrections to sections 2.2 and 2.3, each made as its own source
identity after a proof track or a fresh-context review found the plan wrong:

- **Release receipt.** The plan kept "two original release completions". Each
  holds a Lifecycle credit and a Workspace has two, so a settled refusal could
  not release its operation owner (track K, `K-attempt2`). The receipt now
  records which owners were released and keeps only a refused release's
  completion (`6e8cb6ede`).
- **Admission.** The plan left Commit jobs on `try_submit`. They wait for a
  credit before their one attempt (`OwnerClient::submit_waiting`,
  `6e8cb6ede`). The wait made two self-deadlocks reachable, which the review
  found: a refused owner acquisition's completion plus the resolution's held
  both Lifecycle credits (`R1-attempt1`, hang reproduced), and an owner started
  with one slot of a kind. The resolution's completion is dropped before the
  releases, and the product route refuses before effect below two slots.
- **Control after a settled failure.** Section 2.3 covered an unreleased owner
  only after a known Commit. A settled failure with an unreleased owner is now
  recorded `Uncertain` and answered `Unknown`.

Not changed: a `NotApplied` record reply stays classified with the unknown
outcomes (decision for owner review in the completion record).


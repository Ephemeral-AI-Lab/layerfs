# Stage 5 verification addendum (2026-09-17): qualified rows

> **Status:** Dated prospective contract. Frozen before any qualified candidate
> result in §5 was collected. It governs **new** rows only; every result already
> recorded in [stage-5-verification.md](stage-5-verification.md), the
> [completion report](stage-5-report.md) and the evidence directories keeps its
> original status and is treated as a diagnostic.

## 1. Why this addendum exists

`stage-5-verification.md` called itself frozen while §4 said performance gates
were not frozen, and §5 listed the comparative arm as absent. Both were true of
that document and neither was a qualification contract. This addendum supplies
the missing parts: the exact paired entry points, the exclusions applied
symmetrically, the physical sizes, the cache and worker policy, the sample count,
the numerical gates and the failure schedules. It does not relax any correctness
requirement from the handoff or the repository benchmark rules.

## 2. Row families and their paired entry points

| Family | Reference arm | Candidate arm | Symmetric exclusions |
| --- | --- | --- | --- |
| `component.primitives` | `directory_apply_sorted_with_budget`, `compact_inode_table_apply_sorted`, `compact_inode_table_from_sorted`, `build_initial_directory_sorted`, `encode_namespace_root` (public v0.1.6) | `filesystem::directory::update::apply_bindings`, `filesystem::inode::update::apply_inode_values`, `FilesystemRoot::encode` (public candidate) | input normalization, deriving final reference counts, topology validation, ordering preparation, persistence |
| `pipeline.c1` | none exists (see §6) | `filesystem::build_filesystem` / `update_filesystem` over an in-memory provider and consumer | none: the complete operation is the measured unit |
| `pipeline.c2` | none exists | `Store::begin_save` / `accept` / `finish` over supplied canonical objects | construction of those objects (prepared outside the timer) |
| `pipeline.integrated` | none exists | candidate C1 construction plus C2 save acknowledgement in one region | read-back is timed separately and labelled |

Both arms of `component.primitives` are separate executables with separate
dependency graphs: the reference one builds from the root manifest, the candidate
one from `core/Cargo.toml`. No private Workspace implementation is transplanted,
and neither arm calls the other.

## 3. Physical sizes, cache state and worker policy

| Field | Frozen value |
| --- | --- |
| Cases | `small` = 200 files / 20 change pairs; `wide` = 2,000 / 200; `large-few-changes` = 20,000 / 20 |
| Realized input | a base tree built through the arm's own primitives **before** the timed region, plus a prepared, strictly sorted change batch |
| Timed region | sorted directory merge, sorted inline-inode merge, root encoding |
| Cache state | warm in-process fixture; the base tree is built in the arm process before the timer. No cold-OS or storage claim is made |
| Worker policy | one thread; the driver starts one process per arm per case and adds no workers |
| Samples | one per case per arm, no best-of selection |
| Build profile | declared per receipt (`release` for the recorded campaign) |
| Identity precondition | the two arms must agree on base directory, base table, base root, updated directory, updated table and final root, or the driver reports `MISMATCH` and returns nonzero without a ratio |
| Limits | the component boundary has no storage or bandwidth gate; each command must fit the repository's ordinary 15 s complete-command budget |
| Failure schedule | none: a case that cannot run is recorded as `NOT_RUN` with its measured wall time |

## 4. Numerical gates

1. **Identity gate.** Every case must report `identity MATCH` for all six
   identities. A mismatch is a failed case, not a slow one.
2. **Budget gate.** Each arm's complete command stays inside the ordinary
   complete-command budget; the timed region is reported in nanoseconds and the
   command wall time in seconds.
3. **Against the reference.** The handoff's owner rule is existing-or-better
   performance. A candidate row that is slower than its matched reference row is
   reported as measured, with the case named; it is not averaged away and it is
   not a reason to change the workload, the cache state or the worker count.
4. **No aggregate claim.** No ratio is computed across cases and no
   complete-operation, storage or cold-cache claim follows from this family.

## 5. Collected rows (`component.primitives`, release, 2026-09-17)

Source `3b4941f1ededc6407504438cfe0bd5059fc159f9`, clean tree, receipt
`docs/roadmap/0.1/0.1.7/evidence/stage-5-component-comparison-20260917T073017Z/run-1/receipt.json`
(sha256 `23c923acf114e943223eaea31864f8dd9620a13c8880baa3f350e9ffa7732c45`).

| Case | Reference elapsed ns | Candidate elapsed ns | Candidate / reference | Identity |
| --- | ---: | ---: | ---: | --- |
| small (200 files, 20 change pairs) | 436,000 | 141,375 | 0.324 | MATCH |
| wide (2,000 / 200) | 2,800,292 | 972,125 | 0.347 | MATCH |
| large-few-changes (20,000 / 20) | 3,895,708 | 4,286,541 | 1.100 | MATCH |

Read plainly: the candidate is faster on the two change-heavy component cases and
**10% slower** on the large tree with few changes. The driver's own command wall
times were 3.938 s / 0.609 s / 0.633 s (reference) and 4.758 s / 0.589 s / 1.061 s
(candidate), all inside the ordinary budget. One sample per case per arm; the
first case's wall time includes a cold Cargo example build for that arm.

## 6. Rows this addendum does **not** qualify, and why

- **Complete-operation comparison.** The reference has no public entry point that
  consumes final sorted bindings plus typed values and produces a filesystem root:
  its filesystem update lives inside `layerfs-workspace/src/changes.rs` behind
  `CandidateInputs::build`, a `LayerStackStore`, a `SnapshotReader`, a workspace
  id and a spool path. Comparing a native preordered C1 operation against the old
  whole Workspace pipeline would measure different work, and building that runtime
  only to time a primitive is explicitly out of scope. This row stays **NOT_RUN**
  with that reason; the handoff's component-comparison clause is satisfied by §5
  and no complete-operation speed claim is made.
- **Cold-cache, pack-footprint and simultaneous-memory rows.** The component
  family is a warm in-process fixture; the pipeline family measures real storage
  but was collected for correctness and resource structure, not for a cold claim.
  Both stay diagnostic.
- **Ordering-I/O and backing rows.** Their contract is the ordering target's own
  assertions (reserved-before-growth, real held/peak bytes, counted merges), not a
  comparative family.

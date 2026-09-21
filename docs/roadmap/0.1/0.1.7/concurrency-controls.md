# Concurrency controls: Workspace count and Store writers

> **Status:** #216 is **partly implemented**. `max_concurrent_writes_per_store`
> is implemented and verified by the checks in §6; `max_workspaces_per_sandbox`
> is **not implemented** and has no enforcement point in this tree (§4). Nothing
> here is a performance claim, and no historical W2 receipt is re-labelled.

Tracking: [#216](https://github.com/Ephemeral-AI-Lab/layerfs/issues/216). Source
pin of the implemented half: `152b9c3a2e8ec2536a1d63601b681e1f7ef34455` (the
pair-2 merge), plus this change.

## 1. The two controls

| Control | Meaning | Enforcement owner | Status |
| --- | --- | --- | --- |
| `max_concurrent_writes_per_store` | Maximum admitted logical writes targeting one logical Store/database authority, shared by every sandbox and process using it | C2 save-slot ownership (authoritative) plus service per-Store write admission | **Implemented** (schema 8) |
| `max_workspaces_per_sandbox` | Maximum live Workspace instances owned by one sandbox; a frozen generation and its live successor are one Workspace | Pair 1 sandbox/Workspace lifecycle ([#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179)) | **Not implemented** — blocked on #179 |

These are the only two primary concurrency controls. No third operator knob was
introduced, and the service's read bound (§3) is an internal resource bound, not
a setting.

## 2. `max_concurrent_writes_per_store`

**Scope.** One logical Store/database authority. Every sandbox and every process
that opens the same Store file competes for the same budget; another Store keeps
its own. The value is persisted in the Store (`store_policy.max_concurrent_writes`),
so it is not a per-process copy that two sandboxes could disagree about.

**Default.** 2 — the v0.1.6 behaviour, unchanged. Raising it is an explicit
operator decision, not a new default.

**Supported range.** 1..=64 (`MAX_CONCURRENT_WRITES_LIMIT`). The shipped schema
constrains the column to that range; 0, 65 and larger are refused.

**What a writer is.** A logical mutation in flight: `ConstructFile`, `EditFile`,
`UpdatePreparedFilesystem`, or a `HistoryCommand`. It is not one SQLite
transaction and not one local FUSE `write()` callback; short database
transactions still serialize inside the Store, and one admitted operation holds
exactly one permit for its whole life, however many saves or statements it
performs. Read-only operations (`ReadFile`, `Inspect`, `HistoryQuery`) take no
writer permit at all. A metadata-only history mutation takes a writer permit but
never allocates a C2 content save: only `history::stage` (initialization, stage,
commit) calls `begin_save`.

**Enforcement.**
1. *Storage (authoritative).* `begin_save` reads the persisted budget inside its
   own `BEGIN IMMEDIATE` transaction, counts every live private save row, refuses
   the next one with `StorageError::OwnershipUnavailable`, and allocates the
   lowest free slot inside `1..=budget`. Because the count and the allocation are
   one transaction, two processes cannot admit more writers between them than the
   Store allows, and a refusal is bounded and immediate — there is no queue.
2. *Service.* Each configured Store has its own writer counter, and the service
   reads the budget from that Store when it is assembled. A refused writer gets
   `Code::Capacity`. Reads hold a separate process-wide read bound
   (`MAX_READ_OPERATIONS = 2`), so a busy writer set no longer refuses reads.
3. *Transport.* A session carries one operation, so the native adapter admits
   `session_capacity(budget) = budget + MAX_READ_OPERATIONS` sessions (plus the
   acceptor's one refusal slot) instead of the previous fixed four. At the
   default budget of 2 that is exactly the previous four sessions.

**Configuration change.** `Store::set_max_concurrent_writes` is the only
supported way to change the value: one persisted update, refused outside
`1..=64`. Its behavior against retained ownership is defined, not incidental:

- **Lowering releases nothing.** A private save that is already recorded keeps
  its slot, keeps counting against the new budget, and is never deleted,
  rescanned or reassigned. A lowered setting can therefore never make an
  unresolved owner reusable, and it cannot be used to bypass one.
- **Admission uses the budget's own space.** A retained owner recorded in a
  wider slot (for example slot 5 from a budget of 8) stays exactly where it is,
  while new writers are admitted into `1..=budget` as long as the live count is
  below the budget.
- **Readers are unaffected.** Content written under a higher budget remains
  readable after it is lowered: locator ownership is bounded by the supported
  slot space (64), not by the current setting, so persisted data is never
  redefined as corrupt by an admission change.
- **A running service keeps the number it read at assembly.** The storage
  allocator re-reads the authoritative value on every save, so a lowered setting
  is enforced immediately; the process-local counter can only refuse earlier than
  storage would, never later, and the storage refusal is explicit
  (`Code::Ownership`) rather than a silent over-admission.

**Resource consequence of a raised budget.** Each admitted writer owns one fixed
16 MiB encode workspace and one 1 MiB decode workspace, plus its connection,
bounded batch state and a 2 MiB service thread stack, so the aggregate memory
ownership of the transport and the Store scales with the budget (the W=2 instance is recorded in the
[resource profile](../../../../core/docs/architecture/proposal/service-daemon-transport/implementation/10-resource-profile.md)).
No resource qualification exists for a raised budget, and the default stays 2 for
exactly that reason.

Measured on one host at 16 concurrent 4 MiB writes, resident size after the work
phase grew from 125 MiB at a budget of 1 to 253 MiB at a budget of 8, about
+18 MiB per additional writer.

**The budget is not a throughput knob.** At the same total work through the real
service route, a budget of 2 bought 1.13x (16 writes) and 1.25x (64 writes) over
a budget of 1; a budget of 4 bought 1.11x/1.19x and a budget of 8 bought
0.96x/1.00x, against a 1.4% spread for a repeated budget-1 arm. The measured
reason is the work mix: about 18% of one logical write is the construction half
that can overlap, and about 82% is the C2 save half, which serializes on the
Store's short write transactions. See the
[writer-budget diagnostic](evidence/issue216-writer-budget-20260921T004651Z/README.md),
which reports the plateau, the variability control, the interference on a host
that was not idle, and the gaps.

**Schema compatibility.** Schema 8 replaces the fixed two-slot model. A schema-7
Store is **rejected, not migrated**, exactly as versions 2–7 were, and no row is
rewritten: `saves.active_slot` now spans `1..=64` while the budget decides
admission. Compatibility that *is* supported and tested: data created under a
higher budget read under a lower one, and retained owners in slots above the new
budget.

## 3. What is deliberately not a control

- The service's read bound (`MAX_READ_OPERATIONS = 2`) is a resource bound: one
  read owns one bounded decode workspace and one connection. It is not a writer
  budget and is not operator-configurable.
- The pack/ordinal allocators, the arbitration mutex, the transaction cadence and
  the batch limits are unchanged.
- No generic scheduler, work queue or second construction producer was added; one
  operation still has one construction producer, and `init_namespace` keeps its
  existing multi-worker exception.

## 4. `max_workspaces_per_sandbox` — not implemented

There is no Workspace, sandbox or FUSE lifecycle component in this tree: pair 1
([#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179)) owns it and is
not merged at this pin. **No code admits, counts or refuses Workspace creation,
so this control currently enforces nothing.** It is recorded here rather than
implemented elsewhere because a limit enforced by a component that does not exist
would be a fictitious control.

The intended definition, for the pair-1 implementation:

- the unit is a live Workspace **instance**, not a Branch, Commit or stored root;
- a frozen generation and its live successor are one instance;
- capacity is reserved before the instance becomes visible, and an incomplete
  teardown keeps counting until its resources are released;
- separate sandboxes have independent counts, which requires a trusted sandbox
  identity and an aggregation rule if more than one caller can create
  Workspaces.

Default, supported range and configuration-change behavior are **open** and must
be selected with the implementation; the previously discussed 16 was
illustrative and is not an approved default.

## 5. Source changes

| Area | Change |
| --- | --- |
| `core/crates/layerfs-storage/sql/schema.sql` | schema 8: `store_policy.max_concurrent_writes` (1..=64) and `saves.active_slot` spanning the supported slot space |
| `core/crates/layerfs-storage/src/policy.rs` | `MAX_CONCURRENT_WRITES_LIMIT`, `DEFAULT_MAX_CONCURRENT_WRITES`, `SAVE_SLOT_SPACE`, schema version 8 |
| `core/crates/layerfs-storage/src/sqlite/schema.rs` | persists, validates and updates the budget; refuses an out-of-range persisted value at open |
| `core/crates/layerfs-storage/src/sqlite/ownership.rs` | budget-counted admission and lowest-free-slot allocation replace the slot-1/slot-2 special case |
| `core/crates/layerfs-storage/src/cas/store.rs` | `max_concurrent_writes`, `set_max_concurrent_writes` |
| `core/crates/layerfs-storage/src/sqlite/lookup.rs` | the locator ownership bound is the supported slot space, not the current budget |
| `core/crates/layerfs-bridge/src/contract/request.rs` | `MAX_OPERATIONS`/`MAX_SESSIONS` replaced by `MAX_READ_OPERATIONS` and `session_capacity`/`connection_capacity` |
| `core/crates/layerfs-service/src/owner.rs` | per-Store write admission; reads take no writer permit |
| `core/crates/layerfs-service/src/native/startup.rs` | session capacity derives from the Store's budget |
| `core/crates/layerfs-service/examples/measure_admission.rs` | the diagnostic vehicle: one logical write set at a configured budget through `Service::handle`, separated phases, byte-identical read-back |

## 6. Verification

Run from the repository root, on this worktree:

```text
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

New cases: `core/crates/layerfs-storage/tests/write_admission.rs` (default budget,
budget 8 admission and refusal, eight concurrent duplicate owners publishing and
reading back, retained owners under a lowered budget, a retained owner in a slot
above the budget, content written high and read low, unsupported values, schema-7
refusal, schema constraint on the persisted value, and the whole ladder over
settings 1, 2, 3, 5, 8, 16 and 64 - admission, slot layout, simultaneous duplicate
ownership, publication, read-back, reuse and a later lowering at each setting) and
`core/crates/layerfs-service/tests/admission.rs` (four writers admitted through
the real service route then refused, reads admitted while all four writers are
busy, two sandboxes sharing one Store budget, a second Store with its own budget,
a saturated read bound that neither blocks writers nor exceeds itself, and the
same ladder over settings 1, 2, 3 and 8).

Settings were also measured, not only tested: the
[writer-budget diagnostic](evidence/issue216-writer-budget-20260921T004651Z/README.md)
runs the real service route at budgets 1, 2, 4 and 8 with
`core/crates/layerfs-service/examples/measure_admission.rs`, reports one sample
per arm with a variability control, and states that the setting above 2 buys no
throughput on that host while admission, refusal and byte-identical read-back
stay correct at every setting.

**Gaps, stated plainly.** The measured settings cover budgets 1-8 in time and
1-64 functionally; the host was not idle and no per-arm CPU accounting was taken,
so the throughput plateau's cause is measured for the work mix and only declared
for host contention. An unknown `COMMIT` outcome cannot be induced in this
slice (no fault injection in product source), so the retained-ownership cases
above seed the persisted state a lost acknowledgement or a failed cleanup leaves
rather than inducing one; that induction gap is pre-existing and unchanged. The
Docker daemon/host routes
(`core/crates/layerfs-daemon/tests/*.py`, `core/crates/layerfs-service/tests/*.py`)
were **not run** here; they need a Linux Docker host. The one measurement taken
is the diagnostic above: it is not a gate, it is not release admission, and a
larger budget is not a throughput claim - on that host it bought none. The
H04/H06/H08/H14 gaps in [#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210)
remain open. Historical W2 arms and receipts stay tied to W2 and were not
retuned or re-labelled.

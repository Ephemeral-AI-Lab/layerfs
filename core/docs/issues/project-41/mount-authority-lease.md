<!-- Source: copied from design-gc-authority:/core/docs/issues/project-41/mount-authority-lease.md at review start. -->

# Mount admission authority, leasing, and recovery

> **Iterative review branch:** `design-review`, forked from `design-phase1` head
> `12941806`. The phase-1 bridge is implemented on `design-phase1`; this copy
> records the converged phase-1/phase-3 split, the live-6 reality, and the
> VM/container authority split. The source is the `design-gc-authority` copy of
> this document.

> **Status:** phase-3 implementation in progress (`p3-schema` ->
> `p3-intent` -> `p3-admission` -> `p3-writers` + `p3-lease` -> `p3-repair`).
> The additive schema, the typed contract, the place-first admission rule, the
> place writers (machine incarnation, Sandbox tombstone, worker-start reconcile),
> the lease heartbeat and the durable audit are implemented. The operator repair
> reaches a worker-hosted SQLite Store and a PostgreSQL Store through one typed
> holder view, and the Store-wide compact barrier is retained with tests. Mount
> intents do not yet carry a Sandbox generation, no call renews a lease and no
> production mount sets one, and the live-6 settlement remains open (§9).
> Target: the `gc_intent` `Mount` admission lock and the GC recovery path,
> layerfs-project#41. Read
> `LayerFS-GC-spec.md` (§4.4, §5, §10) and `gc-runbook.md` (§2, §4) first.
> Owner: LayerFS GC owner. This document supersedes the "close/by-sandbox release"
> idea (RV5 FAIL-1/2) and is the essential layer beneath the safe partial commits
> 17370050f4 / 7ab1040594 / 41788e2c25.

## 1. The problem in one paragraph

A `Mount` is a **durable, Branch-scoped Store admission lock**: `mount_intent` writes a
`gc_intent` row of kind `mount` (`engine/crates/layerfs-store-service/src/history/admission.rs:164`,
registered by `store/bind.rs:127`) whose owner is an **ephemeral Sandbox daemon**. Its
liveness authority is the holder process's **PID namespace + boot** (`OwnerView::dead`,
`engine/crates/layerfs-persistence/src/history/admission.rs:251-300`): local SQLite adopts
an intent from an earlier boot, but another PID namespace, a shared PostgreSQL Store, or
missing/unparsable owner evidence stays **conservatively LIVE forever**. There is no
lease, expiry, heartbeat, or fencing. `branch.remove` and `store.compact` refuse while a
mount exists (`classify`, same file:360-374; GC spec §5, line 428: **any** mount blocks a
Store-wide compact), so one unprovable Branch lock blocks **all** reclamation — the live
state (6 branches blocked, 331 retired, compact blocked) is the evidence. The only escape
is the operator's `gc_mark_uncertain` + `gc_release` (admission comment; GC runbook §2),
which is raw SQL, out of band, and not a first-class call.

**Essential vs safe partial.** 17370050f4/7ab1040594/41788e2c25 are a *safe partial*:
7ab's forced remove ends **only the mount intents its own known Workspaces held** by
workspace id (`control/crates/layerfs-sandbox/src/mount_intents.rs`); it explicitly
refuses a folder/by-sandbox sweep and reports `left`. 41788's doctor
`mount_holders` (`control/crates/layerfs-control/src/doctor/mount_holders.rs`) *surfaces*
unmatched holders and reclaims only **recorded** Sandbox/Workspace mounts; its first line
is the design constraint: *"Missing records are evidence of a blocker, never authority to
release an admission."* None of them releases a live-owned forgotten Sandbox, a
private/place-null mount, or an ownerless/legacy row. They fix observation and the
graceful/forced path; they do not fix the authority/lease gap below.

## 2. Goals, non-goals, invariants

Goals:
- A provable owner death ends the lock automatically, per place, without an operator.
- An unprovable/ownerless lock has a **bounded**, auditable recovery path (first-class,
  not raw SQL), and never auto-releases a possibly live mount.
- SQLite and shared PostgreSQL both work; the schema change is additive.
- One Store writer; no global sweeps; the forced-remove own-intent model stays.

Non-goals:
- Not a folder/by-sandbox release (RV5 rejected; missing records are not authority).
- Not loosening the Store-wide compact barrier by default (see §4).
- Not changing the own-intent forced-remove model; not auto-releasing live private mounts.

Invariants:
1. **Safety first:** a mount that may be live is never released automatically.
2. **Place is authority:** where a place exists, the machine incarnation and Sandbox
   lifecycle decide liveness; the caller's namespace is only a fallback.
3. **No sweep:** only the owner, the place authority, or an explicit operator repair ends
   an intent; every end is auditable.
4. **Additive, compat:** existing Stores keep working; old rows are migrated to
   `uncertain`/legacy and need an explicit settle.

## 3. Design

### 3.1 Durable place / incarnation authority

Record the authority on the mount intent and make admission consult it:

| Field | Where | Purpose |
| --- | --- | --- |
| `machine_incarnation` | `gc_intent` (new), `MachineRecord`, worker start | Identifies the kernel/worker generation that placed the mount. A mount from an older incarnation of a machine is provably dead. |
| `sandbox_generation` | `gc_intent` (new), `SandboxRecord` (new), durable tombstone | Identifies the Sandbox incarnation; a tombstone/retired generation proves the Sandbox gone even after its home record is deleted. |
| `workspace_id` / `branch` | existing `gc_intent.workspace` / `branch` + `project` | The own-intent end key (7ab's `mount_intents::end`); never a path/folder. |
| `owner_runtime` / `owner_boot` | existing `gc_intent` | The fallback evidence for private/place-null mounts; unchanged. |

Schema (as built, superseding the additive columns proposed here): no new
column; a live row keeps both tokens in `gc_intent.evidence` as
`place:v1;inc=<token>;gen=<token>`, decoded as no token when NULL, prefixless
(a `done` row's audit text) or malformed. Proposed: additive columns on `gc_intent` (see `engine/crates/layerfs-persistence/sql/sqlite/gc.sql`
and `sql/postgresql/gc.sql`; the `GcIntent` struct at
`core/crates/layerfs-history/src/contract/records.rs:750`, builders `placed`/`owned`
at 847/861). A `machine_incarnation` authority row (per machine) and a
`sandbox_tombstone` marker (`sandbox`, `machine`, `generation`, `retired_ms`) follow the
existing `branch_retired`/`layer_stack_retired` marker pattern (GC spec §4.4).

Admission (`OwnerView::dead` / `settleable` / `settle_dead` in
`engine/crates/layerfs-persistence/src/history/admission.rs:251-380`) becomes:

1. If `owner_runtime`/`owner_boot` are present and the owner is provably dead by the
   existing same-boot/same-namespace/local-SQLite rules: dead (unchanged).
2. Else, if the mount has a `machine_incarnation` and the Store's machine authority says
   the machine's current incarnation differs: **dead** (this covers another PID
   namespace and a rebooted machine; no `/proc` probing).
3. Else, if the mount's `sandbox_generation` is absent from the Sandbox authority or a
   tombstone marks it retired at that machine incarnation: **dead** (the Sandbox is gone;
   `missing records` are not proof, the **tombstone** is).
4. Else, if the place is shared PostgreSQL: compare the Store's per-machine incarnation
   row; a machine's older incarnation is dead; another live machine's current incarnation
   stays live. **No global sweep.**
5. Else (private/place-null/legacy): fall back to owner evidence; if still unprovable,
   the lease/repair path (§3.2/§3.3), never auto-release.

Worker-start reconcile: at machine-worker start (the launch/listen path that logs
`layerfs worker: launched`; `control/crates/layerfs-machine/...`), a **bounded, per-machine**
reconcile scans `gc_intent` where `machine = this` and `machine_incarnation` is older than
the current one (or the Sandbox tombstone says gone) and ends them once, with an audit row.
It never touches another machine's rows and never sweeps by folder. For local SQLite this
is the same adoption the current `dead` does for old boots, made explicit and audited; for
shared PG it is per-machine, keyed by the Store's incarnation row.

Code touchpoints:
- `core/crates/layerfs-history/src/contract/records.rs` `GcIntent` fields/builders,
  `MountHolders`/`GcHolder`.
- `engine/crates/layerfs-store-service/src/history/admission.rs` `mount_intent`,
  `release_or_mark`; `store/bind.rs` register.
- `engine/crates/layerfs-persistence/src/history/admission.rs` `OwnerView::dead`,
  `settleable`, `settle_dead`, `classify`; `owner_dead`.
- `engine/crates/layerfs-persistence/sql/{sqlite,postgresql}/gc.sql` and the `add_gc`
  migration; `records`/`status` serialization.
- `control/crates/layerfs-machine/src/record.rs` + worker start (incarnation);
  `control/crates/layerfs-sandbox/src/record` + remove/close (generation/tombstone).
- `control/crates/layerfs-sandbox/src/mount_intents.rs` (7ab own-intent end; keep it as
  the graceful/forced path; the authority is the Store-side complement).
- `engine/crates/layerfs-daemon/src/control/registry.rs:157` `close_bound` (normal end).

### 3.2 Lease / expiry / fencing for ownerless / place-null / private / legacy — deferred (#775)

> **Deferred (#775).** This trimmed PR ships the ordered place authority
> (`machine_incarnation` + `sandbox_generation`) and the durable tombstone
> only. Lease expiry, heartbeat renewal, fencing tokens and the cross-home
> merge are deferred; the rationale below is kept for the follow-up.

Add `lease_expires_ms` (and a fencing token, reuse `op_id`/generation) to `gc_intent` (or
a `gc_lease` side table). The owner (daemon or private Store-call workspace) renews the
lease on a heartbeat while it holds the mount. Admission treats an **expired lease as
uncertain, not dead by default**: a slow owner is not a dead owner. A first-class repair
(§3.3) or a policy-configured grace may release it. The fencing token prevents a
resurrected old owner from renewing/releasing a newer lease.

SQLite: leases are local to the machine; the worker start can expire old leases safely
(same machine). PostgreSQL: the lease row is in the Store; a live owner on another machine
renews it; a machine's old leases expire only with its incarnation authority; no global
sweep. Migration: rows with no lease/place (including the live 6) are marked
`uncertain`/`legacy` and require an explicit settle; they are never auto-released.

### 3.3 First-class recovery (not raw SQL)

> The shipped subset of the recovery call is `list`, `preview`, `settle`,
> `release`, `settle_legacy` and a Branch-scoped `audit`. The jsonl
> `archive`/`--prune` export is deferred (#775).

New Store-side call (proposed): `store repair mount-holders` (CLI/SDK), plus the
`gc_release` primitives behind it:

- `--preview` lists holders (op, branch, workspace, sandbox, machine, owner evidence,
  state/uncertain) — the same `mounts_detail` that already exists
  (`engine/crates/layerfs-store-service/src/history/lookup.rs:70/142`), exposed in the
  public short form and the doctor (41788).
- `--settle` marks a holder `uncertain`/`legacy` with an audit row; `--release` ends it
  only after the preview, with an audit row; both idempotent, one writer, backup guidance.
  Requires the operator role; the doctor reports but never auto-releases.
- The GC runbook's `gc_mark_uncertain`/`gc_release` becomes the implementation of this
  call (no hand-edited `gc_intent`). The "Historical mount holders" procedure becomes the
  call's manual mode.
- For the live 6: type them first with a **read-only** `gc_intent` query (owner_runtime,
  owner_boot, sandbox/machine, state/uncertain; the runbook query) — the rows are
  plausibly live-owned forgotten-sandbox/legacy, not proven stale. Then `--settle` /
  `--release`; then `branch.remove` the 6 and `store.compact` the 331.
- Never auto-release a live/unproven admission; the repair is explicit and audited.

### 3.4 The Store-wide compact barrier

Current: `store.compact` refuses on **any** mount Store-wide (GC spec §5, line 428). This
is a correctness barrier for a single-generation Store rebuild: a live mount may hold a
Store reader/snapshot, and the rebuild/verify must not race it.

Options:
- **A (recommended):** keep the barrier; make dead mounts clearable via §3.1/§3.2/§3.3.
  The barrier is not the root; the leaked lock is. No policy change.
- **B:** scope compaction admission to the branches/layers the compact would reclaim, so
  a mount of an unrelated Branch does not block. This needs a proof that the rebuild does
  not read/race the mounted Branch (read-only mounts, snapshots); high risk, not in this
  design.
- **C:** generation-based compaction (a new Store generation; mounts keep the old
  generation; the old is retained). A larger redesign; out of scope.

The essential point: with a sound place authority/lease and a first-class repair, A is
sufficient; until then, the barrier amplifies any leak. Revisit B only with a documented
read-only mount/generation model.

### 3.5 Observability (secondary, but do it first)

`MountsActive` already builds holders (`mounts_detail`, `lookup.rs:70/142`), but the
public short form omits `detail` (`api/crates/layerfs-api/src/calls/refusal.rs`
`value(refusal, detail=false)`; the Node SDK keeps `refusal.detail` only if the addon
sends it). Fix: include `detail.holders` for `MountsActive`/`InUse` (or make the CLI/SDK
request `--detail`); the daemon `control/failure.rs:162` drop is the daemon-control path,
not the live `branch.remove` path. Doctor 41788 surfaces the same rows. This is an
observability bug, not the lock lifetime; it is cheap and unblocks diagnosis.

## 4. Tests

Add or extend (the project's test layout):
- `engine/crates/layerfs-persistence/tests/admission.rs`: machine-incarnation death;
  Sandbox tombstone; old boot; shared-PG per-machine; missing evidence; lease expiry;
  never auto-release live.
- `engine/crates/layerfs-store-service/tests/admission.rs`: `mount_intent` place fields;
  worker reconcile; `settle_dead`; `release_or_mark`.
- `api/crates/layerfs-api/tests/gc_calls.rs`: the forgotten-sandbox case
  (`a_live_mount_intent_of_a_forgotten_sandbox_keeps_holding_the_store`, line ~387-409 at
  ed9118) after the authority/repair; `branch.remove`; `store.compact`; the 6/331 shape;
  `mount_holders`; `store repair` preview/settle/release; audit; idempotency.
- `control/crates/layerfs-control/tests/doctor_*`: `mount_holders` warning/fix; repair.
- `control/crates/layerfs-sandbox/tests`: forced-remove own intents; `left`; no folder
  sweep; `WasGone`; same-boot lost Sandbox.
- `engine/crates/layerfs-persistence/tests/sqlite_compact_matrix.rs` and
  `postgresql_gc_*`: mount + compact; authority; pre-GC migration.
- Scenarios: live private mount; same-boot lost sandbox; old-boot adoption; legacy/
  ownerless; shared PG; worker restart; compaction after repair; migration; no sweep.
- Build/test in a Workspace; note the current Sandbox network limitation (crates.io
  index/fuser git timeout) in the test report.

## 5. Compatibility and rollout

- Additive schema (the GC runbook: additive migrations, no down migration; `add_gc`);
  SQLite and PostgreSQL; old clients refuse/ignore new columns safely; one writer.
- Deployed ed9118: the live CLI/bundle/worker/SDK are all ed9118; the project tree at
  4e65ebbae is older; host HEAD f06180c5 has 7ab/41788. The design targets the source;
  deploying 7ab/41788 alone does not release the 6.
- The live 6/331: the read-only query + the repair/migration is the operational path;
  the authority prevents recurrence; the compact reclaims the 331 after the 6 are ended.
- Phases (converged review):
  1. **Phase 1 bridge (implemented)**: observability (short-form holders + doctor 41788)
     and the explicit local-SQLite, per-Branch `store.repair_mount_holders`
     preview/settle/release/settle_legacy. No schema; no automatic release. It does not
     by itself reach a worker-hosted Docker SQLite Store or a PostgreSQL Store.
  3. **Phase 3 essential**: place/incarnation authority; caller namespace/boot fallback;
     lease/fencing; additive schema/migration; worker/PG repair + audit; compact barrier
     retained; tests/rollout/reclaim. The old phase-2 and phase-4 details are folded into
     phase 3, not dropped.
  5. Compact barrier decision (retain A; revisit B/C only with a generation/read-only
     mount design).
  6. Tests/rollout.
- The safe partial 7ab/41788 can land first (observation/own-intent); the essential
  authority/lease/repair follows.

## 6. Risks and open questions

- Machine incarnation source: kernel boot id vs worker incarnation; native/macOS vs
  docker kernel; shared PG per-machine authority; clock/expiry false death.
- Sandbox tombstone durability: home record vs Store marker; the removed sandboxes (the
  live 6) have no tombstone; the migration must not treat missing records as death.
- Private/place-null mounts: owner evidence only; lease policy (uncertain vs release);
  fencing; ownerless legacy.
- The operator/audit/backup path; the read-only `gc_intent` query policy; the 6 type
  (live-owned vs legacy) must be proven before the repair.
- The compact barrier: retain vs scope vs generation; the Store rebuild/reader race.
- Schema/migration on PostgreSQL; the `UNIQUE(scope_kind, scope_id, owner_boot)` with
  new columns; old clients; the ed9118/legacy rows.
- The `mount_intents::end` own-intent model and `sandbox.remove` (root permission) are
  separate; do not conflate with the authority.
- The project tree's older `api` layout may lack the current `refusal.rs`/`mount_holders`
  paths; port the partial first or map the equivalent path when implementing.

## 7. First implementation step

If only one thing lands next: **observability + the explicit repair/migration
bridge (phase 1)**. It is cheap, safe, and makes the essential work measurable; it does
not by itself unblock a worker-hosted Docker SQLite Store or a shared PostgreSQL Store.
The live 6 are on the Docker worker; phase 1 runs only where the local SQLite Store file
is reachable. Phase 3 (worker/PG repair plus place authority/lease) is required to
release them; the authority prevents recurrence, and the compact reclaims the 331 after
the 6 are ended.

## 8. References

- `core/docs/issues/project-41/LayerFS-GC-spec.md` §4.4, §5 (line 428), §10.
- `core/docs/issues/project-41/gc-runbook.md` §2 (operator), §4 (limits).
- `core/crates/layerfs-history/src/contract/records.rs:750` `GcIntent`; `placed`/`owned`
  847/861.
- `engine/crates/layerfs-persistence/src/history/admission.rs` `OwnerView`/`dead`
  251-300, `settle_dead` 342, `classify` 360, `conflicts` 452.
- `engine/crates/layerfs-store-service/src/history/admission.rs:164` `mount_intent`;
  `store/bind.rs:127`; `history/lookup.rs:70/142` `mounts_detail`.
- `engine/crates/layerfs-persistence/sql/sqlite/gc.sql`, `sql/postgresql/gc.sql`
  (`gc_intent` columns); `engine/crates/layerfs-daemon/src/control/registry.rs:157`.
- `control/crates/layerfs-sandbox/src/close.rs` (`WasGone`); `.../mount_intents.rs`
  (7ab, not in this tree); `control/crates/layerfs-control/src/doctor/mount_holders.rs`
  (41788); `api/crates/layerfs-api/src/calls/refusal.rs` (short form; upstream path).
- Live evidence: 6 stuck branches, 331 retired, compact blocked; ed9118 test
  `api/crates/layerfs-api/tests/gc_calls.rs` ~387-409.

## 9. Converged phase 1 / phase 3 (iterative review)

**Phase 1 bridge (implemented on `design-phase1`).** The short form and doctor 41788
surface holders without releasing anything. The explicit operator call
`store.repair_mount_holders` is local SQLite only and per Branch: `preview` changes no
intent; `settle` marks one named `op` uncertain; `release` requires that same named `op`
to be uncertain already, so settle and release are two explicit steps (preview, settle,
release); and `settle_legacy` marks only rows with no owner or place evidence. It is not
automatic and does not touch the live Store. Phase 1 audit is the CLI call log plus the
`gc_intent` state change; the durable operator audit row is phase 3 and is not claimed
here. Phase 3 keeps that row as a retained `gc_intent` `done` row (the action, the operator
label and the bounded evidence), and a fenced heartbeat extends a lease without ever
shortening it; the preview is the CLI call log only. The GC spec §5.1 records both. The phase-1 call cannot reach the live Docker worker
Store from a host home, so it is a bridge and proof, not the operational release of the
6.

**Phase 3 essential.** The place authority is the primary liveness proof:
`machine_incarnation`, `sandbox_generation` and a durable `sandbox_tombstone`; admission
consults place first and falls back to the existing caller PID-namespace/boot evidence
only for private/place-null rows. A dead place ends the intent; an absent home record is
never death. Lease expiry is **uncertain**, never auto-dead; the owner renews, and a
fencing token (reusing `op_id`/generation) prevents an old owner from renewing or
releasing a newer lease. The schema is additive on SQLite and PostgreSQL; old rows are
marked uncertain/legacy and require an explicit settle. The operator call is extended to
the worker Store and PostgreSQL through one typed holder view, with a durable audit row;
the doctor still only reports, and its worker-side read forwards the same authority
fields. The Store-wide compact barrier is retained and tested against a live and an
uncertain authority.

**VM/container split.** On native, the kernel boot/incarnation is the place authority. On
Docker/VM, the worker/container generation is the authority, not the host kernel boot: a
restarted worker or VM proves an old placement dead even when the host kernel did not
reboot. A shared PostgreSQL Store carries a per-machine incarnation row; another live
machine current incarnation stays live, and there is no global sweep. The caller PID
namespace is only a fallback for place-null/private intents; another namespace is never
assumed dead.

**Live 6/331.** The 6 are plausibly live-owned forgotten Docker worker mounts, not proven
legacy. Phase 3 first types them with a read-only `gc_intent` query, then settles/releases
them with operator authority on the worker Store; only then can `branch.remove` end the
6 and `store.compact` reclaim the 331. Phase 1 alone cannot do this.

**Remaining essential gaps.** The worker-start reconcile and the machine-incarnation
and Sandbox-tombstone writers are in place (`p3-writers`); operator repair reaches a
worker-hosted SQLite Store and a PostgreSQL Store, the typed holder view covers the
worker path, and the Store-wide compact barrier is retained with tests (`p3-repair`).
Mount intents still do not carry a Sandbox generation (the daemon and forced-remove
plumbing), no call renews a lease and no production mount sets one, and the live-6/331
settlement has not been run. The phase-1 bridge never releases a live-looking holder
automatically.


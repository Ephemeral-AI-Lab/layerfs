# R1B known-clean scratch pool and rebind implementation freeze

This is a prospective infrastructure freeze, after control-target publication
b98bc780d7d2b626142077595cd607a9679f2602. It is not a native heap, strict memory,
performance or numeric acceptance claim. Builds/tests/measurements remain root-owned.

## Exact class and fixed slots

Reuse selects an exact `(private profile version, native S)` class, with existing
schema-compatible versions4/5/6/7/8. Graph defaultS remains16MiB; configured48MiB
remains a different class. Declared D/B and full captured source/capacity subjects
can change only through rebind, before dependent work. No different profile or
S may borrow an idle file. An idle file stays in its original occupied authority
slot and keeps its full S charge. The ordinary fixed two-slot service profile
therefore retains at most two idle files; no growing map/path registry or extra
native slot is introduced. Legacy1/2/3 have no known-clean pool admission.

The filename belongs to native birth, not the latest operation token. Native FD,
file identity, private directory identity and stable absolute path stay unchanged
across successful reuse. Every operation receives a fresh issued token and full
selector/source/namespace/base/capacity native binding. Old scopes and headers
remain invalid. No rename, close/open, unlink/recreate, source adoption, schema
rebuild or error-driven fresh fallback occurs on selected reuse.

## Eligibility and transitions

Active -> Idle requires known successful exact empty/reset state, no failures,
unknown state, pending attempt, release attempt or live data/metadata consumer.
Roots4/5/7/8 use their exact existing retirement/reset seals. Draft6 requires its
actual Finished acknowledgement, empty eight draft data relations, zero records/
bytes, selectedNone, and an exact known fixed-row reset. Failure cannot become Idle.

Idle -> Rebinding atomically selects one matching slot and marks it active before
SQL; authority locks cover selection/counters only and never the operation SQL.
Before dependent effects, verify native descriptor/path and exact old reset rows,
prepare complete fresh context, and retain complete old/proposed rebind metadata.
One BEGIN IMMEDIATE transaction compares the old full header, checks empty data
relations and updates the fixed owner rows/scopes/declarations/full new header.
COMMIT is acknowledged only after checked native observation and exact new-row
verification. Only then replace the old Rust context and admit producer use.

Any known rebind failure retains the selected owner; any Unknown COMMIT/native
observation retains old/proposed context and S, quarantines the slot, and forbids
reset, adoption, replay, refund, cleanup and cold fallback. Returning to Idle is
explicit and distinct from release. Explicit drain visits only known Idle slots,
closes/unlinks each at most once, and refunds S only after actual removal. Active,
failed, Unknown and failed-drain owners remain retained.

## Bounded work and observable counters

Selection scans the existing fixed slot table; rebind changes only fixed owner
rows (four baseline graph rows, alias fifth, facts sixth/seventh, counts eighth/
release ninth; Draft uses two). At most8 bound parameters per statement, existing
SQL limits unchanged. Draft reset has no population deletion: actual Finished
already performs its existing bounded64 retirement. Pool reset merely confirms
empty projections and updates fixed metadata.

Expose checked monotone counters for fresh native births, successful rebinds,
returns to Idle, matching-class refusals, rebind failures, successful drains and
drain failures, plus current fixed idle count. Count after known acknowledgement;
failed attempts remain visible and never count as successful reuse. Counter
exhaustion refuses before the corresponding effect, without wrap or saturation.
Actual new context/Box/vector layouts and credit handoff must be funded before
allocation; scoped graph64KiB is not enlarged. Draft working admission retains
its explicitly unqualified fixed1MiB scope. Encoded header/class lengths do not
stand in for RAM allocation proof.

## Ownership and required hooks

Worker owns authority.rs, new pool_*/rebind_* modules and native stable-path helpers
only after root releases native.rs. Root preserves begin_canonical and owns shared
Resource/session initialization, Plan/Header/profile/reset and profile8 registration.
Required shared hooks are narrow session success/destination/resource-transfer and
Resource exact idle eligibility/rebind-attempt custody; their finalized signatures
are coordinated before source integration. Read-only GraphMemory observers must
end, alongside charged results, before Idle can become reusable.

External owning tests cover same-authority repeats, fixed pool exhaustion,
16/48 classes, source/token/header/scope renewal with stable FD/path, held metadata,
known failure/corrupt rebind, real shared-reader COMMIT Unknown, Draft successful
reset, and explicit known-Idle drain. No historical receipt is changed.

## Final source interfaces and engine participation

`return_to_idle(&mut self)` and `drain_idle(&self) -> Result<usize>` are the explicit
public transfer and cleanup calls. Idle stores Resource inline in the existing
fixed Slot Vec; its actual Vec capacity must equal admitted owner count before
NativeDirectory preparation. Counters live under that same slot mutex, which is
released before native/SQL work. A counter reserves one attempt/pending before
its effect; acknowledged success/failure cannot exceed its checked attempt count.
Fresh success observes a real native birth with captured descriptor identity,
including a file whose later SQL initialization fails. It does not label such an
operation successful. Native descriptor observation grants no adoption/close right.

GraphMemory remains the SAME controller across rebind. Exact internal handle and
credit counts exclude externally held data and status/controller observers before
Idle; Idle status itself holds no observer. Old/proposed phase owners and a fixed
Context/Attempt plus temporary194-byte subject, full386-byte header and compiled
Hasher fit only by the existing64KiB admission, before allocation. Draft's actual
old/new fixed layouts are checked against its unchanged explicitly unqualified
1MiB working shape; no leased physical/global claim is inferred for that class.

`new_guarded(parent,max,&'static EngineGuard)` explicitly captures the established
actual process guard; compatibility `new` captures None. Reuse also matches that
exact captured guard pointer and provider ownership. Guard validation precedes
native creation/open and each owned BEGIN/COMMIT/rebind boundary. Scratch retains
its own actual512KiB connection profile. Foreign guard failure in an open owned
transaction becomes Unknown and preserves pending/native custody; no implicit
global guard lookup, error-driven fallback or hard-limit/profile relaxation occurs.

## Covering-cause correction: full solver scope rebind

The first pool reuse reached exact Graph verification and refused because the
solver_owner row retained its old188-byte GraphScope while graph_owner had the
new scope. Scalars already matched the reset/new Solver::initial image (current0,
root0,next1,SCC0 and zero flags). Rebind must CAS solver_owner.scope from the exact
old full scope to the exact new full scope in the SAME transaction, with every
reset scalar checked. No scalar/state gate is weakened and no encoded grammar,
working class or native S changes. An independent external vector compares both
native scope rows to the actual newly issued scope and rejects the old bytes.

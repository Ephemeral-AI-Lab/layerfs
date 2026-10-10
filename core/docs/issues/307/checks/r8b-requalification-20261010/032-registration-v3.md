# R8b prospective registration at the final repair identity

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

[Registration v3](../../../../../benchmark/fs-bench-pro/registry/r8-integrated-qualification-v3.json)
freezes source `77a9b52dbe1e221954f8a2afbf7e0495a82b615e` before any registered
invocation. It is derived from v2 by
[make_registration_v3.py](tools/make_registration_v3.py), which computes every
hash from the file that will be executed. Registration v2 (commit `64609e47b`),
its FAIL 053 and the closed examination's outcomes 061/062 are unchanged.

## What is frozen

| Item | Identity |
| --- | --- |
| Source commit and tree | `77a9b52db`, tree `3f90565f496b` |
| Product tree `core/crates` | `2e0d94c8835e` (v2: `9a76077239b4`) |
| Harness tree `core/benchmark` | `95ee62603021` |
| Runtime, host release | SHA-256 `2aaaf317…f310b`, build receipt 027 |
| Daemon, Linux release | SHA-256 `632b17ea…b7bf4`, build receipt 028, source hashes checked in the container (024) |
| Image | `sha256:378b799e…2cd6` |
| Locks, fuser provenance, Cargo config | identical to v2; the generator refuses a difference |
| Full fixture inputs | preparation 029, volume `layerfs-r8-full-proof-20261010-2aca347ad746` |
| Lifecycle inputs | preparation 030, volume `layerfs-r8-full-proof-20261010-f1291be40649` |
| Shared-process proof volume | `layerfs-r8b-shared-proof-20261010-77a9b52db`, empty (031) |

Global Store profile Disposable/WAL/OFF, selected explicitly; Durable `NOT_RUN`;
overlay MEMORY/OFF/EXCLUSIVE; `LAYERFS_CONSTRUCTION_WORKERS=1`.

## Selections and order

One invocation each, in this order. All are functional with natural cache
warmth declared; none is a performance sample.

| Selection | Stop | Output |
| --- | --- | --- |
| `R8b-full-fixture-mounted` (successor of `R8-full-fixture-mounted`) | 100 s command, 85 s comparator | 033 |
| `Q1-wide-host`, `Q1-wide-linux`, `Q1-host-handoff`, `Q1-shared-processes-linux` | 9 s, inner 8 s | 034 to 037 |
| `R8b-confinement` | 60 s | 038 |
| `R8b-holder-cwd`, `-descriptor`, `-mapping`, `-private` | 45 s each | 039 to 042 |
| `R8b-streams` | 60 s | 043 |
| `R8b-lifecycle` | 60 s | 044 |

The full-byte comparator differs from v2 in one respect: it observes with 8
bounded walker threads (lead decision L-1, [diagnosis](006-full-oracle-timeout-diagnosis.md)).
Oracle, inventory, limits and both stops are unchanged.

## What this registration does not authorize

Timing attempts authorized: **0**. Gates G01 to G06, G08 and G09 cannot be
closed by any invocation registered here, whatever those invocations return;
each gate's status and reason is in the registration. All 282 timing selections
therefore stay `NOT_RUN` with zero attempts. E08/E09 stay owner-excluded, the
`P` arm stays unexecuted at the unchanged provenance refusal, and A2 supplies
no arm.

Known before invocation, from exploratory receipts 010 to 018: the
`R8b-holder-private` arrangement ended in retained teardown custody, which this
registration judges FAIL. It is registered and run once as written; the
criterion is not changed to fit.

The final host and Linux suites (021, 025), Clippy (022, 026), static checks
(020) and tooling tests (023) ran once at this source identity before the
registration was written. They are cited as covering receipts and not
re-invoked. Their only failures are the four environment-gated precondition
tests, which are registered above with their inputs.

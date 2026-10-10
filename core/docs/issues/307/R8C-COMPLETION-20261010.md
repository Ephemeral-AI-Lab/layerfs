# R8c close under the owner instruction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

**R8c CLOSED — QUALIFIED on function, counts and resources at the
owner-rescoped scope; timing NOT_RUN.** All twelve selections registered in v4
passed, one attempt each. This is not a timing, cold-cache or release claim.
The [R8](R8-COMPLETION-20261010.md) and [R8b](R8B-COMPLETION-20261010.md)
records keep their outcomes, including the registered FAIL 042.

The owner's instruction, after R8b: **"fix all of them with simplicity and i
want to close r8 and r9 fast"**. Twenty decisions were taken under it and are
recorded, each with its alternative, in
[the decision record](checks/r8c-close-20261010/00-owner-instruction-and-decisions.md).
Read this closure with that table: 28 of the 82 passing rows pass because of
one of those decisions, and 18 of them only after a scope was withdrawn.

## What closed each kind of blocker

| Kind | Count of rows | What it means here |
| --- | --- | --- |
| As scoped | 50 | Passed against the row as written: 41 since the closed R8 and 9 closed at R8b |
| Registered precondition proof | 4 | Invoked once at v4 with its prepared input |
| Fix | 5 | A product or deployment change removed the gap: FP-2, FP-5-Runtime, FP-22-FS, R2-STEP-6, P-1 |
| Ruling | 5 | Existing behaviour was declared the contract: FP-7-Runtime, FP-26, R4-6, R5-6, R6-7 |
| Withdrawn scope | 18 | A stage no real mount can produce without a product hook was removed from the row; it is listed, not proved |
| Withdrawn by R0 | 8 | Unchanged |

## Fixes

| Fix | Commit | Proof |
| --- | --- | --- |
| C-1: the Sandbox container denies user-namespace creation | `307618496` | The command's `unshare` is refused in the real Sandbox (confinement 35); the private-namespace holder that failed in R8b now holds only a working directory and gets the reversible `Busy`, and its sibling unmounts (39) |
| C-2: largest READ and WRITE frames in the request accounting | `307618496` | Both are 131072, the negotiated limit, for one megabyte written and read in single calls (9 READ and 8 WRITE frames) |
| C-4: deployed `serial_low_water` 256 | `307618496` | Every runtime proof at v4 ran with it |

C-1 has a cost. A tool that needs an unprivileged user namespace (bubblewrap,
rootless container tools, some browser sandboxes) now fails in a Workspace
command with `EPERM`. The container also no longer runs under the Engine's
default system-call filter; it runs under one that denies only namespace
creation. The alternative, vendoring the Engine's default profile with these
rules added, is recorded and not taken.

## Outcomes

[51-outcomes.md](checks/r8c-close-20261010/51-outcomes.md) and its
[machine form](checks/r8c-close-20261010/50-outcomes.json).

| Selection | Outcome |
| --- | --- |
| Full-byte mounted fixture | PASS: 130,046 names, 103,108 regular files, 3,475,776,149 bytes, 0 differences; comparator 58.5 s under the 85 s stop |
| Four precondition proofs | PASS each |
| Confinement | PASS: 84 protected routes refused; namespace creation refused |
| Holders: cwd, descriptor, mapping, private namespace | PASS each |
| Streams | PASS |
| Lifecycle | PASS |

Functional rows: 82 PASS, 8 withdrawn by R0. Timing rows: 282 `NOT_RUN`, zero
attempts (C-15). Gates: G01, G03 and G07 met; G02, G04, G05, G06, G08 and G09
met as rescoped, each naming what was withdrawn.

Resources reported by the lifecycle proof at six phase boundaries (not
continuous peaks, no bound): daemon resident set 8.8 MB before any mount,
42 MB after the Commit, 65 MB after the fresh mount; 7 threads unmounted and
10 mounted; Store WAL 0.9 MB after the Commit; Overlay file 0.3 MB logical.

## What is not established

- Any timing, cold-cache, storage-size or bounded-resource claim; any `N` or
  `P` arm; Durable.
- Every withdrawn scope in the decision record: the unstaged teardown and
  reader-failure stages, WRITE header identity, a held GETATTR, stream
  backpressure, a lost runtime result, abort-write failures.
- Count hypotheses beyond what existing tests assert: ten of nineteen are
  asserted in part or at component scope, two not at all
  ([map](checks/r8c-close-20261010/22-count-hypotheses-map.md)).
- The request maxima as an independent trace; they are the product's own count.
- The low-water arm that fails for a reason other than contention.
- Reproduction R5-6a still fails against the product; it is ruled outside the
  profile's guarantees, not fixed.
- OWNER-1 to OWNER-7 remain `PENDING OWNER`.

## Checks, accounting and custody

At source `307618496`: fmt, boundary guard (674 files), host and Linux Clippy
with warnings denied, host suite 275 binaries, Linux suite 275 binaries (one
more passing test than at R8b), tooling tests; Linux source hashes verified in
the container. The only suite failures are the two input-gated precondition
tests on each side, which passed as registered proofs.
`check_fuser_integrity.py` still refuses the passthrough manifest (OWNER-3).

| Commit | Production LOC |
| --- | --- |
| `5654d0181` decisions | 144,753 -> 144,753 (delta +0) |
| `307618496` fixes | 144,753 -> 144,783 (delta +30) |
| `a2b8bc022` registration v4 | 144,783 -> 144,783 (delta +0) |

Active core 79,336 -> 79,366; root reference 65,417 unchanged. Pinned counter
`c0fe7f36…624adb`. The commit carrying this record reports its own comparison.

[Custody](checks/r8c-close-20261010/52-resource-custody.txt): the eight exited
containers and three volumes of this run were removed, every selection having
passed. Everything retained or untouched at R8b is unchanged.

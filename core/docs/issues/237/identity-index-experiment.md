# Bounded same-Save identity index experiment

## Preregistration

Control source is the integrated C1+C2 4-MiB-wave tree at `2529a4101` in the
isolated `codex/issue237-integrated-hot-profile` worktree; that commit changes
documentation only from the integrated product source `f26865747`. A temporary,
matched aggregate probe counts `pending_member` and same-wave sealed lookups,
hits, misses and inspected `ObjectId`s. It emits one summary per completed Save,
not per object. No algorithm differs in the control. Candidate changes only the
Save's ephemeral membership indexes: a bounded hash map records open/queued
same-Save members and a wave-local hash set records placed IDs. Existing ordered
vectors remain the serialization and collision-validation inputs. No benchmark
shortcut or persistent format change is allowed.

Take **one** control diagnostic, then **one** candidate diagnostic on the public
`namespace-10000` native Init operation, seed 1, exact same harness, release
profile, fixed stack/scope identity, four Init constructors, one C2 owner, 512
objects / 4 MiB per wave, 256-KiB packs, 128-KiB whole-file cutoff and 4-KiB
SQLite pages. Each arm has a fresh Store and its own independent byte copy of
the sealed source master. The H3 driver hashes and invalidates the whole
300-MB payload outside the timer, then nonfaultingly checks all source pages
immediately before the timer; require zero resident pages in both arms.
Metadata residency remains unqualified, so both results are diagnostics and
cannot establish a fully cold speedup. Full verification is `SKIPPED` during
the performance calls, then a separate full persisted readback against each
arm's retained output is required before accepting equivalence. No repeated
arm, enlarged deadline, changed worker count or warm source is permitted.

Control and candidate record the same probe schema: lookup calls, hits,
misses and **linear IDs inspected** for pending and sealed checks. Candidate
linear-inspection counts should be zero by construction while hash lookup
calls/hits remain comparable. Additional fixed counters are `SaveOutcome`
inserted/reused/COMMIT/pack/statement totals, public and Service spans,
CPU/RSS, Store geometry, root, ordered object-ID digest, full readback and
source/harness/patch hashes. Preserve every failure. The probe is temporary
and archived as a diff; the final product commit excludes it.

Decision: reject the candidate on any changed canonical root, missed exact
collision, same-Save read/dependency error, failed readback, wrong 4-KiB page,
unbounded index, or physical-space regression. A faster raw diagnostic without
equivalent proof remains experimental. The control's inspected-ID total
determines whether the linear scans are actually a material part of the
previously observed ~150-ms unnamed wave work; do not infer a speedup merely
from the source's complexity.

## Evidence and result

Pending one control and one candidate.

### Pre-run verifier clarification

Before the first arm: the full namespace oracle needs the run's ephemeral
history cursor key, which the harness holds only during that run. Therefore
invoke the H3 driver once per arm with `--verify`. Its one public performance
sample ends before the full verifier child starts; the verifier records its own
wall/status and contributes nothing to the operation speed comparison. This
supersedes the earlier `SKIPPED` sentence above solely for the separate
readback: no second public performance call or rerun of an arm is authorized.
Keep any oracle timeout/failure as nonpassing evidence.

### Owner correction before first arm: performance-only public runs

The owner directed that both public H3 runs remain performance-only. This
supersedes the preceding `--verify` clarification: use the driver **without**
`--verify` for each arm and retain `verification.status=SKIPPED` in each public
receipt. After both timed operations, invoke each arm's archived
`verify_namespace` release binary once against that arm's retained Store,
History, fixed root/stack and sealed manifest, with a fresh nonzero read-only
History capability. The external verifier performs full reopened path,
metadata, size and SHA-256 readback and does no History pagination; it does not
claim to replay the ephemeral cursor chosen during the public call. Record its
own command, binary hash, wall, stdout/stderr, status and any failure outside
the performance receipt. No additional public Init sample is permitted.

The one-pair measurements, independent readbacks, physical-space rejection and
exact source identities are in the [result](identity-index-result.md).

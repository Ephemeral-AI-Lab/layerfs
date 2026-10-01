# Remaining R1 and focused speed selection

> **Status: Current planning checklist; no release candidate exists.**
> Assigned implementation parent `53b6bf741a5693f4d00ec98b914ce34645ee9ab3`.

The owner assigned [the published handoff](HANDOFF_R1_FINISH_AND_SPEED_20261001.md)
to this implementation chat on 2026-10-01. Its preparation-only pause ends for
this assigned R1 scope. R2–R7 remain unstarted. #288 is read for dependencies
and remains untouched. Root owns integration/publication/Bridge/measurement;
current-thread reviewers have nonoverlapping navigation, namespace and C2 files.
No other active writer/build/measurement was found in the clean assigned worktree.
Primary/foreign checkouts, targets, inputs and receipts remain owned elsewhere.

## Prospective executable selection

[SPEC.json](evidence/r1-finish-speed/SPEC.json) freezes the six ordered existing
case IDs, literal commands/preludes, independent old/new manifest hashes,
one seed/sample per arm, actual public route and 15s command limits (25s only
components270). Separate proof remains9s. The published pre-R1 product control
is `7edddbdb8e8512627aed0ed42533ef099d802384`; the final candidate is unselected
until all implementable assigned infrastructure is frozen. The common harness
overlay is copied to a newly owned control worktree with product trees unchanged.
No post-R1 product code is transplanted into the control. No measurement is run
against the developing dirty implementation worktree.

Use existing `runner.py run --case CASE --out FRESH` separately for each row,
then `runner.py prove --run RETAINED --out FRESH-PROOF`. Core has no `--setup`
or `--reuse-pass` flag here; families make closed independent writable byte
copies internally. The clean/one-edit4097-write prelude, pin, first Commit and
final operation remain inside the complete command. Exec/Commit/prelude/command
are reported separately; nested spans are not added.

Control observations and their separate proof are collected once after this
specification/setup/custody publication. Exact control-derived Exec, Commit and
complete-command targets are committed before speed optimization or candidate
collection. A miss remains TARGET_MISS; no timing allowance or resampling is
introduced. Existing registered cases outside this cohort remain out-of-selection
and unrun, independently of these six rows.

## Source and fixture custody

The existing runner now uses `core-build-and-harness-inputs-v2`. Its ordered
product/build inventory includes all shipped source and SQL, build scripts,
manifests/lockfile, actual SDK/verifier example sources and repository-root ARMv8
flags. Compilation identity adds exact environment overrides and release/toolchain
selection. The harness inventory includes JSON registries, C writers, fixture
inputs, helpers and the imported residency provider. Every inventory lists
per-file SHA256, and executable reuse requires the new compilation method/seal
and actual immutable binary hashes. Older Rust-only caches fail closed.
Families retain actual driver/verifier/daemon binary hashes, generated Dockerfile
hashes, static writer hash and immutable image inspection; the image is not inferred
from a mutable tag. Existing row profiles/schemas/cache assessments are preserved.

F5's old Docker-created native masters are prohibited for new SDK rows. New private
host namespace setup uses existing public `benchmark_init` and `benchmark_shell
seed` for the exact small/wide tree and marker, then the existing full manifest
verifier before publication. Its distinct host-v2 path/provenance cannot consume
an old native master. Unsupported deep/unrelated setup refuses before access.
Setup success/physical compatibility remains UNRUN until actual acquisition;
external tests establish routing and independent corpus, not provider qualification.
F7 keeps its host Init/seed method. F4 master hashes are mandatory, and its
producer chain must be audited back through #271/#261 before acquisition.
All arms use the same closed qualified master with independent writable copies.

## Independent statuses and closure

All numeric latency remains INELIGIBLE: F4/F5/F7 do not prove the complete
source/dirty-byte/OS/Store/daemon Commit cache domain. Raw target outcomes and
command-budget results remain useful diagnostic evidence. No evaluator constant
changes that status. Resource attribution unavailable to the existing shared
SDK/Server host process stays unavailable; lifetime peaks or heap-only checks
cannot qualify strict physical memory. Strict/native Apple SQLite support remains
CAPABILITY-LIMITED while exact32MiB readback is unavailable. Direct Darwin logical
library composition remains separately scoped.

Original R1 required gates stay visible in the closure matrix. Legacy composite
early content release remains R3-dependent; v2/FileSet/schema11/liveWorkspace/
runtime/concurrency are excluded. Owning implementation correctness/count proofs
do not imply speed, engine headroom, physical containment or protected native PASS.
The assignment ends after its final R1/speed report; no later rollout is authorized.

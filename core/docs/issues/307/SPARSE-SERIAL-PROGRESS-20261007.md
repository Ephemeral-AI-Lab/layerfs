# Sparse editing, backed serial state and observed supervisor progress

> Status: verified source checkpoint in preparation after local first parent
> `dcdf527584675849e7839ca4118d71ac9aa4b193`; covering functional and static
> checks pass. Final staged/committed confirmation follows. S0/S7–S13 remain
> incomplete and unchecked.

This continues the [full goal](IMPLEMENTATION-PLAN-S7-S13-20261007.md) through
three owned Content/filesystem/SDK lanes and the shared captured-input provider.
The root reference and excluded predecessor sources remain. No dependency,
database, canonical tree algorithm or provider/construction lane is added.
No performance treatment is sampled or requalified.

## Implemented boundaries

Run-aware editing adds `EditSource::read_run_at` with the existing Data/Zero/End
grammar and legacy byte-source default. One shared Scanner/ExtentBuilder consumes
continuous complete input or each localized replacement. Zero spans preserve
frozen CDC state and ordinary canonical partition/predecessor semantics. Positive
zero evidence uses at most 64 immutable witnesses and validates complete referenced
domains, exact summaries and root/non-root context. Partial zero does not certify
a whole payload/subtree. Original provider identity/absence/denial/decoder errors
end the operation before backing/output. Eviction pays reacquisition rather than
refusing larger input; fragmented distinct identities still pay their visits.
See [the owning source/work limits](../../architecture/54-run-aware-localized-file-edits.md).

Backed streamed filesystem construction uses the same canonical driver and the
neutral aliases of the existing opaque record protocol. Filesystem kinds are
`0x46530000..=0x46530003`; a Missing-guarded 106-byte context binds exact base/root,
scope/profile and root serial. Indexed records own declared new-parent/held
membership, rebuilt directory roots and initial counts. Sorted sealed header/
fresh passes provide iteration and final rows, without collecting whole key,
count, root or final-row domains. Missing required rows refuse. The memory API
keeps its former maps/arrays and phase ordering. This supplies selected P7/P13
state; grouped validator/addition/graph state, whole-base alias walks, reducer/
touched/zero/release containers and their existing limits remain. See
[serial-state scope](../../architecture/53-backed-filesystem-serial-state.md).

Captured input now has exact inode points and a fixed forward run cursor under
the original reader/root/generation/installed floor. Its query projects cell
offset/epoch/data+mask lengths and returns at most one metadata row per relevant
existing layer. Exclusive seeks and future lookahead persist across windows/gaps;
stale rows pay bounded continuation and the real shrink staircase. Actual cutoff/
lower EOF establishes local Zero; absence alone remains Inherited. A Window uses
the old masks/composition within one 4,096-byte cell, with its old base root after
install/close. Monotone `seek_forward` preserves this state; backwards/out-of-file
positions refuse. `reader_inode` deliberately excludes current orphan-domain
overrides. No canonical I/O runs under the SQL owner. Direct Overlay and the fair
Daemon Read class provide original boxed-command/Completion custody. Success
clones one bounded window; End/drop releases no reader. See
[captured run/copy/admission scope](../../architecture/55-captured-sparse-run-cursor.md).

Supervisor `step_observed` advances the same one phase and at most one service
unit as `step`. It reports actual selected progress/wait, dispatched ticket/
attachment/correlation and original event. A fixed shared latch/Condvar permits
parking only after a complete quiet occupied rotation, outside provider locks,
with no runnable service or pending native joins. Publication/sender drop/credit
release precede notification, and credit's ledger lock is released first. A
nonclone borrowed permit cannot survive another owner mutation. Wait deadlines
bound waiting only; original failure/packet/Save/Workspace/Bash lifetimes remain.
Notification poison is independently visible without replacing queued originals.
See [progress/parking contract and limitations](../../architecture/45-runtime-supervision.md).

## Functional verification and retained repairs

[Append-only receipts](checks/sparse-state-progress-20261007) retain exact argv,
wall bounds, stdout/stderr and every failure. Locked no-run precedes bodies;
normal construction has `LAYERFS_CONSTRUCTION_WORKERS=1`. Each test command has
an explicit outer limit at most 120 s; Linux uses the retained pinned ARM64 image
with child 110 s/kill-after 1 s. No test timed out. These library/public-API checks
are functional, without cold-cache, physical-I/O, aggregate residency or numerical
performance qualification. All earlier receipts retain their source identity.

| Check scope | Distinct selected bodies |
| --- | ---: |
| Host Content: existing affected 246 + new run 11/filesystem 10 | 267 |
| Host SDK, including real application queue/wake 2 | 62 |
| Host Daemon, including real captured provider 3 | 22 |
| Host Workspace | 36 |
| Host Overlay captured runs | 5 |
| Host component total | 392 |
| Linux Content/Daemon/Workspace/Overlay/portable SDK | 352 |
| Separate macOS-host/Linux-consumer proof | 1 body, both persistence profiles |

The initial Daemon captured 2 bodies in 09 are superseded by the strengthened 3
bodies in 15; they are counted once. Linux includes additional Overlay payload/
indexed targets from matching package selectors. The macOS Store/native progress,
slot and application targets have 0 Linux bodies and are not claimed there.
Untouched codec/limit evidence remains source-pinned to the earlier checkpoint;
no unrelated full legacy sweep is run. The lifetime-name cleanup after these
bodies does not change behavior or public API; final compilation checks cover it.

The 11 run bodies cover real >4 GiB replacement/no-op, mixed canonical/byte equality,
partial-zero poisoning, more than 64 witnesses, changed summaries/non-root context,
early comparison End, original IdentityMismatch and hash-matching wrong-role/
malformed canonical bodies, original source/consumer failures, predecessor hints
and occupied-scope refusal before mutable effects. The 10 filesystem bodies cover
canonical/count/membership/root equivalence, multiple headers/fresh rows, guarded
scope reuse, original missing/refused/malformed state, ordered update and accepted
child custody without a final root. External fixtures are not actual Commit.

Independent FS review found a required rebuilt ROOT could disappear after replay,
then let a caller's stale supplied value overwrite the reducer value. The repair
requires the declared non-dropped header's ROOT before applying the supplied
value. The tenth regression demonstrates disappearance, exact refusal, retained
DirectoryLeaf output/no final FSRoot/no later record call, and a valid metadata-
only/no-header control. It ran only after repair; no failed performance gate,
workload or frozen oracle was relaxed.

Retained failures 02/03 concern a new diagnostic assertion and external argument:
the large gap's two reader checks return 64 bytes of Workspace root metadata,
so an all-family “no BLOB” assertion was wrong. The corrected case explicitly
requires Payload BLOB 0 and Workspace BLOB 64 with unchanged workload/product.
The following no-run's `DatabaseWork::since` needed a borrowed argument.
Failure 06 retains 35 SDK missing-doc errors and an unused Content import;
precise API docs/import cleanup changed no behavior. Failure 12 retains the
new inode port's `Clone`, not `Copy`, result mismatch, repaired with a bounded
scalar clone. Clippy 20's private redundant lifetime is elided without suppression.
Clippy 21's external application test explicitly dropped a non-Drop borrowed
permit; its unused borrow now ends through Rust's ordinary lifetime analysis
before control mutation. The following host and Linux Clippy invocations pass.
Imported unused helpers are allowed only on the external application test module.

Captured public bodies check mask/shrink/regrow bytes, exact returned reader/root
after later mutation/install/close, stale and interleaved physical metadata visited
once per forward pass, trimmed-prefix starts, logical size/seek refusal, explicit
reader retirement, original attempted/unattempted custody and independent scope
progress. The application proofs use a real Store/Sessions/native composition
and bounded original Attach/Fence queues. Control is polled on every turn and
after obtaining a permit; a pre-clear queued connection remains visible, and
publication after an Empty predicate wakes the same owner. Exact correlation/
provider/Delivery, actual joined fences, stale IDs and final credits are checked.
There is one Sessions/provider owner and no added producer lane. Watchdogs bound
test coordination rather than runtime operation lifetimes.

Poison with retained originals and some partial-join/partial-round scheduling
cases remain source-reviewed rather than induced through a private test hook.
Those limits are not converted into passing native/resource gates.

Boundary check 28 refused the `DroppedParents` trait and its BTreeMap implementation
in a declaration/delegation `mod.rs`. Both definitions moved verbatim to private
`state/types.rs`, with the same visibility and thin module reexport. [Final source caps](checks/sparse-state-progress-20261007/source-line-caps-final.json)
cover 46 changed production files: maximum 807 physical lines and maximum 60
in a thin entry module. This is
relocation, with no algorithm, behavior or API change. Final affected host/Linux
compilation and warning-denying Clippy cover that source arrangement. The earlier
functional bodies remain evidence for unchanged behavior; their receipts keep
their original identity and are not relabeled or repeated.

## Native consumer and final checks

The [fresh native proof](checks/sparse-state-progress-20261007/26-real-host-linux-proof.json)
passes under Durable and Disposable. Its complete command wall is
4,765,769,666 ns with unchanged Docker 40 s, host 45 s and outer 120 s bounds.
Each profile serves 131 real host deliveries and verifies 13 paths, seven regular
names/six regular inodes, hard-link identity, opaque symlinks, modes, timestamps,
exact bytes and EOF after source removal. It retains original RemoteRefusal,
observes automatic idle maintenance and terminal Gone, and fences 262 fragments
with zero live messages/credits. This is functional evidence from an uncontrolled
cache; it is no SDK-call timer, treatment comparison, cold proof or full Commit.

The [consumer identity](checks/sparse-state-progress-20261007/consumer-binary.json)
pins the 72,460,096-byte debug Linux consumer, SHA-256
`0b9330084b415bc9afc5443bf9bb997b510d00e43cc0b5008ae1958283572bdb`,
the exact ARM64 image/config/dependency hashes and build command. Host and consumer
retain the repository's ARMv8 AEAD flags; normal construction has one producer.
This binary and proof precede the declaration-only DroppedParents relocation.
The native consumer does not call the filesystem update constructor; that
unaffected proof is reused with its original binary/source scope. Final affected
Content compilation/lint covers the relocation. No binary is resealed as a
performance arm and no old proof is silently assigned new source identity.

Affected five-package all-target Clippy passes on both host and Linux; final
Content-only Clippy passes after relocation. Formatting, the source boundary
guard, its 41 external tooling tests and affected document links/anchors are
recorded in the append-only directory. The final guard covers 721 product files;
the documentation checker covers 16 documents, 247 local links and 13 anchors.
Authored whitespace passes with raw stdout/stderr excluded so byte-exact diagnostic
output remains unchanged. The fuser patch remains unused in this
active package graph; these checks establish no FUSE implementation/qualification.

## Correlated SQLite work

Metadata discovery's actual EQP selects `payload_generation` by the complete
scoped prefix and offset range, with no scan/temp sort. The retained VM projects
offset/epoch and lengths with the original production template. A plan alone
claims no physical I/O or speed; runtime counts accompany it.

The large cutoff gap+End pays seven executions, six returned rows, 177 VM steps,
328 bound bytes, 336 delivered value bytes and 64 root BLOB bytes. Payload has one
empty metadata seek, 19 VM steps, zero returned BLOB/value bytes and no composed
window. There is still root/lease/layer acquisition, binding/parser work and SQL
cache/pager behavior; no “free read” or zero-allocation claim follows.

The 96-stale/one-live fixture pays 99 cursor steps, 97 metadata rows/96 stale rows,
then one 4,096-byte composed window with four actual local bytes. Scoped totals
are 491 executions/rows, 12,778 VM steps, 21,248 bound bytes, 20,516 value bytes and
3,172 BLOB bytes (3,168 repeated root metadata + 4 payload). Payload family itself
has 194 executions/rows, 4,957 VM steps and 4 BLOB bytes: 97 metadata seeks, 96 actual
staircase reads and one cell window. All fullscan/sort/autoindex/reprepare counts
are 0. These are uncontrolled-cache fixture counts, no universal hole complexity,
independent service-rate, inclusive/exclusive time or physical resident bound.

## Remaining goal

Actual final-state captured normalization needs authenticated serial/file/length
binding, coalesced interval records, a fallible indexed replacement-length source
and one retained FileView over the same edit driver. Reducer/touched/zero/release
backing needs non-destructive bounded key enumeration and exact coupled work
acknowledgements. Root-qualified topology evidence, one Save producer, conditional
history publication, known install, unknown custody and native application/kernel
integration remain. Initial acquisition's eight speed/allocation failures and six
scoped history-pair passes stay unchanged. E1 sample count stays 0/NOT_RUN, and
E01's earlier diagnostic consistency is neither a new-source receipt nor E/Q
admission. The full goal continues after this component checkpoint.

## Exact source-size comparison

Production LOC: **165716 -> 167676 (delta +1960)**. Core is
**100299 -> 102259 (+1960)**; retained root reference is
**65417 -> 65417 (0)**. Actual manifest-active core is
**57134 -> 59094 (+1960)**; excluded first-party predecessors including the
retired Server remain **40321**, and excluded API-core/FUSE/Sandbox integration
source remains **2844**. No relocation or reference deletion is claimed as an
algorithmic simplification.

Affected active package subtotals are Content **16311 -> 17417 (+1106)**,
SDK **6592 -> 6985 (+393)**, Overlay **7005 -> 7362 (+357)**,
Daemon **2513 -> 2592 (+79)** and Workspace **2771 -> 2796 (+25)**.
The [exact snapshot receipt](checks/sparse-state-progress-20261007/production-loc-source.json)
compares first parent `dcdf527584675849e7839ca4118d71ac9aa4b193` with the prepared
staged tree, product subtree `917a84242457c1e42995e25148f82422f2ea8617`.
Method: `git archive SNAPSHOT crates core/crates`, then
`python3 tools/production_loc.py --root ARCHIVE --json`, using unchanged counter
SHA-256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Both sides count nonblank/non-comment Rust and required runtime SQL, excluding
inline/test-only code, external tests/examples/harnesses/tools/docs/dependencies.
Manifest-based [migration classification](checks/sparse-state-progress-20261007/production-loc-classification.json)
retains all first-party implementation; there is no new adapter outside core.
Final staged and committed snapshots are separately recomputed/confirmed without
including protected untracked notes. Reference retirement, push, release and
milestone completion are not performed by this checkpoint.

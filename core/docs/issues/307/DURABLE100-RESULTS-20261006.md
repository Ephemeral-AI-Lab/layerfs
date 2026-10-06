# Durable100 correction and rejected publication experiment

> **Status:** Dated source/measurement checkpoint. Durable100 remains FAIL;
> S7/S8/S9 remain incomplete. No release, push or deployment.

The retained active correction fixes an avoidable pack-ID reservation. Its
qualified Durable100 observation is83,626,792 ns, down22.0965% from the preceding
107,346,666 ns candidate, but still above the unchanged46,192,208.7 ns threshold.
That is one observation per source, not a repeatability claim or attribution of
the complete23.719874 ms difference to the removed reservation.

The subsequent prerequisite/final-wave experiment removes two more publication
commits, but its qualifying complete product time is104,343,333 ns and allocation
is8,192 B greater. It is rejected and its product/tests/architecture restored
byte-for-byte to the reservation-only source9b74ac035. The production seal matches
that measured source exactly. The failed experiment, code and proof remain in
3d40600be and append-only receipts; there is no unchanged treatment resample or
new numeric qualification of the restoration commit.

## Selected route and identities

Only `phase7-sqlite-init-100-acquisition-v2` ran in each prospective checkpoint:
[queued-tail](../../../../docs/roadmap/0.1/0.1.7/namespace-init-durable100-tail-20261006.md),
[prerequisite/final-wave](../../../../docs/roadmap/0.1/0.1.7/namespace-init-durable100-prerequisites-20261006.md).
Candidate is public Project Init over Monolithic opt-in acquisition schema4,
Durable WAL/FULL/fullfsync/checkpoint_fullfsync, fixed automatic checkpoint1000.
Complete product clock covers fresh create, Init, required checkpoint and close.
Four Namespace Init constructors and environment construction-workers1 apply.
No profile, workload, timeout, cache, row/byte limit or gate was weakened.

Reference is the original eligible7edddbdb8 Phase4.5 MEMORY/OFF split Store/history
public Service caller. Its41,992,917 ns and7,372,800 B are reused after product,
SQL, compilation, dependency, binary, helper, wrapper/harness and fixture identity
verification. It remains a distinct competitive reference, not a matched-profile
old Project comparison. Previous diagnostics and every earlier verdict keep
their own identities. [Reference/setup verification](checks/durable100-tail-results-20261006/setup-reference-reuse.json).

Case fixture is100 files,2 directories,102 paths,5,000,000 logical bytes, seed1,
manifest SHA2569b97114260decac04edb93b288b2c1b6037b2d9d7f4127c21186cc21a43201e3.
Canonical root isca1c20f806277e3fb0447ef526a5ca8d298753d01667f501e8b58b6350cd86f1
in reference, both candidates and the full functional evidence. Other seven v2
cases and every v1 tier are NOT_RUN at these new checkpoints. Earlier
Disposable PASSs and Durable FAILs are neither reopened nor promoted.

Locked release Rust1.85.1 builds inherit the repository ARM64 flags. The same
chat-owned managed measurement worktree uses clean committed source, local target,
prepared master, immutable binary archive, fresh output and normal nonblocking
runner locks. Earlier independently byte-copied setup is reused; no measured
Store is reused. Source content is invalidated and all pages attest zero residency
anew. Metadata residency remains unobserved. Four unrelated containers remain
declared interference and untouched. SDK/daemon/FUSE attribution is N/A for this
host component selection; lifetime RSS and final allocation are not phase/system
or high-water observations.

## Qualification receipts and disposition

| Durable100 source | Complete product ns | Final allocated B | Speed / storage / joint |
| --- | ---: | ---: | --- |
| Reused original reference7edddbdb8 |41992917|7372800|Reference|
| Earlier candidatea6bd6860c |107346666|5304320|FAIL / PASS / FAIL|
| Retained reservation-only9b74ac035 |83626792|5296128|FAIL / PASS / FAIL|
| Rejected prerequisites/final-wave3d40600be |104343333|5304320|FAIL / PASS / FAIL|

Exact gate is `10*candidate_product_ns <= 11*reference_product_ns`.
Tail operands836267920 >461922087; experiment1043433330 >461922087.
The retained candidate still needs37,434,583.3 ns less whole product time to meet
the threshold. It is1.991449× reference. No phase is subtracted from the gate.

| Candidate source | Performance command ns /30s | Separate proof ns /19s | Build ns /30s | Driver CPU ns | Lifetime RSS B |
| --- | ---: | ---: | ---: | ---: | ---: |
|9b74ac035|1605039459|523676708|5975403000|70085000|39763968|
|3d40600be|1508155583|663214250|5433663667|78925000|39895040|

Both complete-command/build/proof budgets, content-cache attestation, root equality,
functional proof and cleanup PASS. Independent verifier inventories every
path/kind/directory metadata and samples53 files/3,354,003 B, policy
stride64-size-log2-endpoints-v1. This is not a complete payload oracle; covering
tests complement it. Absolute budget/proof PASS does not change the relative FAIL.
The sole runner commands and raw receipts are in
[tail ledger](checks/durable100-tail-results-20261006/ledger.json) and
[rejected experiment ledger](checks/durable100-prerequisites-results-20261006/ledger.json).

| Inclusive product phase ns |9b74ac035|3d40600be|
| --- | ---: | ---: |
|Bootstrap|12833917|27879125|
|Init|67458583|73418000|
|Required checkpoint|2891208|2704875|
|Close|402292|309375|
|SQL COMMIT, nested|38971251|58336547|
|SQL statements / VM steps / writes|277 /90190 /18|265 /89285 /16|

The experiment's raw latency is24.7726% higher and allocation0.1547% higher than
the tail checkpoint. Bootstrap accounts for15.045208 ms of the20.716541 ms raw
difference, despite no bootstrap implementation change. Remaining Init COMMIT
also varies. These single observations do not establish repeatable causality or
an environment explanation. Fewer SQL statements/VM steps/commits alone did not
prove lower complete durable cost; there is no VFS/device attribution at either
current100-file source to settle why.

The reference main DB has5,175,296 logical B and7,340,032 allocated B; candidate
tail main DB has5,263,360 logical/allocated B. Reference's2,164,736 B beyond EOF,
plus32,768 B other allocation on each arm, explains the apparent28.1667% final
allocation saving. The candidate main logical length is88,064 B larger. Reclaimed
physical tail is neither a compression gain nor proof of lower WAL/main writes.

## Cause, checks and exact restoration

The retained fix changes `finish_pack_bound` from queued groups to queued packs
under already enforced byte/directory bounds. Nonempty unsealed groups keep a
conservative ID each; acknowledged pack tail, canonical encoding, final packing,
publication boundaries, error ordering and durability are preserved. The public
128-compressible-chunk regression fails before and passes after. In the actual
Durable100 diagnostic, Storage reservations3→2 and complete writes19→18.
Provider-linked SQLite3.51.0 EXPLAIN correlates singleton INTEGER PRIMARY KEY
reservation searches with actual per-port statements/VM/commit counts.
[Retained cause/checks](checks/durable100-cause-20261006/CAUSE-AND-CHECKS.md).

The rejected experiment changes scan/file Save lifetime and final bounded drain
registration, preserving existing publication partitions. Its locked count
diagnostic has publications5→3 and complete writes18→16, with unchanged root and
bounded per-wave output. Qualification nevertheless fails without improving
whole time/allocation. Its source, explicit failure semantics, representative
publication EXPLAIN and tests remain recoverable, not described as active behavior.
[Experiment cause/checks](checks/durable100-prerequisites-20261006/CAUSE-AND-CHECKS.md).

Retained source:260 host bodies and128 Linux portable bodies PASS. Experiment:
261 host and129 Linux portable bodies PASS. Both have explicit bounded builds
before tests, host/Linux Clippy warnings denied, formatting and652-file boundary
guard.40 unchanged tooling tests PASS and are reused. Failed initial observer
compile/import, incorrect regression fixture/expectation and nonexistent
diagnostic source path are preserved; none reached a test hang ceiling.
The early count diagnostics lack explicit run-lock/old post-binary seals and are
INELIGIBLE for speed. Later diagnostic binary hashes match pre/post under a lock;
its enabled clocks still are not admission observations. No vendor/dependency or
benchmark harness was modified.

[Exact restoration files](checks/durable100-prerequisites-results-20261006/restoration.json)
and [restored whole-product seal](checks/durable100-prerequisites-results-20261006/restored-product-seal.json)
show byte identity with9b74ac035 across production Rust/shipped SQL. Its passing
checks and measured result remain pinned there; no unchanged tests/timings are
rerun. Extended read-only EXPLAIN remains outside production. Full closed Stores,
binaries and outputs are independently copied and SHA256-verified in primary
ignored `benchmark-results/fs-bench-pro/durable100-{tail,prerequisites}-retained-20261006/`.
Compact text receipts/manifests here retain their original identity and clocks.

## Next architectural discussion and separate audits

Before another production correction, distinguish repeated dirty-page writes,
WAL bytes, syncs and automatic/final checkpoint work per publication at the actual
retained Durable100 source. Existing first-party delegated VFS observer provides
a starting point; qualify its current caller/per-unit attribution and keep these
counts separate from native stmt-status counters if a trace observer resets them.
Older [VFS cause results](../302/SQLITE-VFS-CAUSE-RESULTS.md) support this priority
only at their historical1000-file/source scope. They are not current Durable100
byte/sync evidence. Keep existing topology/profile/gates fixed for the proposed
next diagnostic, unless the owner explicitly selects an architecture review.

The systematic-debugging skill requires architectural discussion after three
failed fixes before another one. The owner selected both current writes/sync
tracing and acquisition-placement review. Both investigations proceed under the
existing Durable100 contract; no fourth optimization is selected by this receipt.
No durability relaxation, extra acquisition DB,
profile fallback, lifetime-peak substitution or unqualified PASS follows.

**S7 audit:** incomplete. This is a host Init/Storage component result. Native
runtime assembly, actual full-root operations, context/restart custody and
resource/rate qualification are unchanged. No S7 row is closed by it.

**S9 audit:** incomplete. Canonical Init, cleanup/custody tests and the retained
reservation correction are implemented; Durable100 competitive speed FAIL remains.
Other required cases, greater-than4-GiB native proof, actual capacity/quarantine
through Init, application/daemon assembly and runtime acceptance remain open.
No later R/E/Q/C/P or S10–S13 work is started by this checkpoint.

## Production source size

Exact first-parent/staged/committed comparisons use the unchanged counter SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb,
`git archive <snapshot> core/crates crates` then
`tools/production_loc.py --root <archive> --json`. Shipped SQL and excluded
predecessor production count; tests, inline tests, examples, docs, tooling,
manifests, third-party and generated artifacts do not.

| Local commit | Core | Reference | Combined |
| --- | --- | --- | --- |
|9b74ac035 retained reservation fix|94495→94499 (+4)|65417→65417 (0)|159912→159916 (+4)|
|3d40600be rejected experiment|94499→94497 (-2)|65417→65417 (0)|159916→159914 (-2)|
|ee387232b exact restoration|94497→94499 (+2)|65417→65417 (0)|159914→159916 (+2)|
|593e25971 observer/placement review|94499→94499 (0)|65417→65417 (0)|159916→159916 (0)|

[First correction comparison](checks/durable100-cause-20261006/source-committed-loc.json)
and [experiment comparison](checks/durable100-prerequisites-20261006/source-committed-loc.json)
confirm committed trees. Restoration compares its first parent/final staged tree
before committing and confirms afterward in the primary retained receipt and
handoff; [restoration receipt](checks/durable100-prerequisites-results-20261006/restoration-committed-loc.json)
and [observer receipt](checks/durable100-vfs-20261006/observer-committed-loc.json)
confirm their trees. This is source-size accounting; no legacy retirement or algorithmic
performance claim is inferred. All commits remain local and unpushed.

The owner's subsequent **both** selection is completed in
[current VFS findings](DURABLE100-VFS-RESULTS-20261006.md) and
[placement review](DURABLE100-ACQUISITION-PLACEMENT-REVIEW-20261006.md).
The [next layout proposal](DURABLE100-NEXT-LAYOUT-PROPOSAL-20261006.md) is reviewable
and awaits a scope decision because the current case freezes Monolithic schema4.

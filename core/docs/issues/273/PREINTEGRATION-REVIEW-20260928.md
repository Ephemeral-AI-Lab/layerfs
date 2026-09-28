# #273 → #264: source-bound publication and integration decision

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Reviewed 2026-09-28 UTC in an owned worktree based on
> `0a932faaa04105796252f1e5a78e4888c52d904e` (tree
> `93d600b46c0aa8b057c6f32d4b7a463dee8deecd`). This is an addendum,
> not a change to old receipts or the [order-1 evidence](PREINTEGRATION-ORDER1-SOURCE-AND-EVIDENCE.md).
> No external comment, PR review, merge, benchmark, or combined-source proof
> was submitted by this review. GitHub API reads and Git/receipt inspection
> only; source/test references below are from the named commits.

## 1. Publication: identities and actual evidence (before integration review)

The [order-1 publication](PREINTEGRATION-ORDER1-SOURCE-AND-EVIDENCE.md)
resolves all 29 local links. Its [committed frozen identity](evidence/phase4.5/hot-publication-20260928/FROZEN-CANDIDATE.json)
and [committed archive](evidence/phase4.5/hot-publication-20260928/README.md)
are distinct from local ignored raw iterations. Verified the ignored
`benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-018/`
manifest in the assigned #273 worktree **read-only**: 191/191 SHA-256 entries
match, `RESULTS.json` SHA-256
`96d11c928b3e4a8fc3ec27650f523b52811df107f548e08d07c54125e33d4730`.
Likewise iter-019: 56/56 entries, results SHA-256
`33063f485482f5b57979480260c967af1c3f226f14c167531ab4bf4350fe4abd`.
Neither directory is tracked or has a GitHub raw URL; this document does not
republish its raw files. [Iteration log](CHECKPOINT5-OPTIMIZATION-LOG.md)
and [historical campaign](CHECKPOINT5-LOG.md) preserve the negative attempts.

| Source identity | Admissible observation; exclusions |
| --- | --- |
| Frozen pre-#273 control `48b51e874a41b3e1e6c6661e145316df8b408f07`, tree `64d06dabba1a6d123d1002487c189809ded05cc0` | Checkout exists; newly matched cache/cgroup control **NOT_RUN**. Historical 25-second censored timeouts **FAIL**, not denominators. |
| Hot implementation last changed `a2359620a7966314fbb2a96c98e8da958df72c6c`; frozen proof `4ae36ad3a9c70b32b66c8280ac9496e9f1e345a9`, tree `2292ca5b49d1bd4fdf080929f667cf961566fc35` | **17/17** public functional cases and **42/42** named checks PASS at this source; earlier failed attempts remain FAIL. No speed claim. |
| Nine-row product/telemetry `8801b9c095b9975eaa81fc98c738df6ef9752710`, tree `e116b0dab2d1eee6edcea35204f49cfa867c65cf` | 3 patterns (append/dispersed/repeated overwrite) × 100/512/4,097 public WRITEs: **9/9** independent full bytes, Commit and cleanup PASS; **9/9 numerical INELIGIBLE**. Pack/C1 edit-node counts: append 2/4, 7/4, 52/4; dispersed 2/2028, 7/10532, 209/84982; repeated 1/17 in each tier. Complete raw walls and their 15 s/25 s limits are in order-1 §5, **not** ratios or cache-matched speed. Product-source SHA-256 `c1ac564aba923348be4e773ba8ef9cb8e6f9655398cc1994d9498a890a4cb9e6`, product `5b20276a6a4f661bd2d2f5c8b17fdfeaa7d0ae8a043f615274af8f8d2b3b8f30`, harness `d1906e13f25d80ac8ebabf71438d202f0bbfc88bd3ffdb94faef1fe95e9fab7f`, workload `b8103ddd1e0b2070c2179c03ac7a63f5d3497b22f6b1f1c0ef02d183086c37e4`; no new matched frozen arm. |
| #248 gate at `8801b9c` | Anchored `LFS_C1_EDIT_LOAD v=1 nodes_read=0 stored_nodes_read=0 draft_nodes_read=0` is an explicit **functional** zero; gate numerical status **INELIGIBLE**, historical missing-zero receipts **INCOMPLETE**. Do not substitute SaveFile record counts or C1 save counts. |
| `ee4032e2dd389003cdb37ee781e3937d6b656a24`; later *test-only* `e70b6ab73d0d692813f7870f7c44c48574e83ae7` | Separate 8,192 default-8-MiB-Budget 8,194-byte oracle diagnostic PASS **functionally only**; earlier failures FAIL. e70b6ab 4,097/4,197/8,192 loop ratios are nonregistered diagnostics, **INELIGIBLE** as speed. |

The selected v2 `LFSAHOT2` 64-slot HotDirectory, fenced 17-byte tagged
`Cold(page,epoch)` / `Hot(slot,reuse_epoch)` targets, authenticated verified
pages and atomic selected root/directory/pack watermark prevent a frozen G1
reader from resolving its old slot through G2's live occupant. Capture pins
G1; G2 edits and the same process, fd and inode continue during SaveFile/C1
and charged C5 reconcile. Candidate ownership, old-pin retirement and
verified unlink/refund retain failed or unknown physical custody. v1 private
attachments are not migrated to v2; canonical C1/C2 formats are unchanged.
For authenticated admitted monotone tiny-WRITE frontiers only, balanced
carries yield `W + W/B + W/B² + … = O(W)` **structural** index work; first
admission, eviction, general overlap/Payload, registry lookup, physical I/O,
`O(J*G*log G)` conservative pin retirement and charged `O(E_f)` final-file
Commit scratch remain. No full-CPU O(1), constant RAM, universal latency law
or unlimited #256 capacity is established.

Production LOC (same `tools/production_loc.py` nonblank/noncomment production
Rust/shipped SQL counter, excluding inline legacy tests, external tests, docs,
tools and generated files): `8801b9c` Core **68,031**, reference **65,417**,
combined **133,448**. Its first parent `6f3cd0044` was Core 68,022,
reference 65,417, combined 133,439 → **+9 Core**. Hot continuation
`0513f8a1a`→`4ae36ad3a`: Core **65,136 → 67,158 (+2,022)**;
reference 65,417 unchanged. [Per-commit LOC record](evidence/phase4.5/hot-publication-20260928/phase45-commits-loc.json)
and [iteration log](CHECKPOINT5-OPTIMIZATION-LOG.md) contain intervening
first-parent comparisons. These numbers are not Git diff statistics.

## 2. Live dependency review (API read; no external review submitted)

At this checkpoint GitHub reports #262, #272, #274, #269 and dependency #260
**open draft**; #263 **open, non-draft**. All five requested PRs have zero
formal GitHub reviews as read; GitHub `mergeable=true` is not an approval.
#276 remains **open**, with zero comments/owner rulings. #273 was closed as a
scoped handoff, #270 closed as deferred to #276; #256 is open. Actual PR
head/base (not stale `main`):

| PR | Head / tree | Declared base; review disposition |
| --- | --- | --- |
| #262 | `6bcfa464f74ae9ca3859df31c678985ec69ba098` / `7b073c1c29f5c6540528076eac4b312503a9b76b` | `6af2c5c59a48d0b6c85d656e55aecc353e346728`; **bounded source review, no merge endorsement**: reply-gap and physical custody require owner acceptance. |
| #272 | `48b51e874a41b3e1e6c6661e145316df8b408f07` / `64d06dabba1a6d123d1002487c189809ded05cc0` | `3c3343e0b116f99b1e4abe1a3e9795fff28b3f79` (contains #262); **bounded source review, no speed admission**. |
| #274 | `29fc5747d70eadf01fd8794bf84a50cf57ef3660` / `e5d86d996e8283048c078d7b53d65aa65f284823` | `48b51e874a41b3e1e6c6661e145316df8b408f07`; **BLOCK unqualified admission** pending #276 and broad red tests. |
| #263 | `ef3a310480254774d6e6966004fb5e0a4fa3b94d` / `d33297ed21227f61f7f1efddce928d48dba1c430` | `6af2c5c59a48d0b6c85d656e55aecc353e346728` (#260 head); bounded inherited-rename review; **request combined proofs before integration**. |
| #269 | `6eb7553671d3000160ad023a57b335e52dd81a26` / `40166e52f1ec7f9927e4cf38dc70a0a7c3255bd1` | `ef3a310480254774d6e6966004fb5e0a4fa3b94d`; **REQUEST CHANGE** on duplicate seal below, and block draft integration pending authorization. |

Concrete source findings (line numbers at each PR's **head**; these are
review observations/risks unless marked a reproducible failure):

1. **#262, bounded reply-gap / route, medium integration risk:**
   `core/crates/layerfs-workspace/src/runtime/coherence.rs:110-145,151-169`
   owns one-use mutation and keeps the permit through reply;
   `core/crates/layerfs-fuse/src/adapter.rs:496-570` owns the data as a
   temporary `OwnedPayload` even for 1 byte at #262, and explicitly cannot
   observe kernel reply delivery. The 128-byte tiny path appears only in #274
   (`adapter.rs:542-575`); >128 bytes still use Payload. The covering
   `core/crates/layerfs-workspace/tests/coherence.rs:485-626` probes notifier
   failure; #262's `tests/owner_finalization.rs` checks checked-owner cleanup.
   **Disposition:** no assertion of proven kernel completion or historical
   #261 512 speed; combined reply-gap/tiny/large/published-error check needed.
2. **#272, page-owner/legacy boundary, medium:**
   `core/crates/layerfs-workspace/src/backing/ownership/sponsored.rs:7-50,54-163`
   decodes matching old edges, pins a sponsor and publishes ledger/edge
   progress after physical creation. The sponsor depth cap (4) is not a
   lifetime upper bound; partial physical identity and failing edge completion
   still require charged custody. `tests/owner_finalization.rs` covers a
   focused route, not v2 slot epochs. **Disposition:** compare against its
   `3c3343e` base; don't promote the #271 proposal into a numeric result;
   retain v1/v2 and final-pin/unlink verification on combined source.
3. **#274, old selected view and custody, HIGH admission gate:**
   `core/crates/layerfs-workspace/src/backing/active/hot_directory.rs:77-106,127-185`
   rejects mismatched slot epochs/kinds; `active/resolve.rs:58-93` resolves
   through the *view-selected* directory rather than live state;
   `active/retirement.rs:58-128` puts failed unlink in charged custody.
   `core/crates/layerfs-workspace/src/commit/active_reconcile.rs:26-96`
   precharges captured patch rows, but it does not make all Commit scratch
   constant. `tests/stage.rs:1101-1269` exercises known C1 / local C5
   refusal and G2 continuation; this does not resolve the two broad red C1
   ordering-ceiling tests. **Disposition:** scope functional PASS to sealed
   proof; no aggregate Core/release green or numeric promotion.
4. **#263, inherited never-resident descendants, medium:**
   `core/crates/layerfs-workspace/src/filesystem/rename_paths.rs:60-110`
   checks resident paths and on a growing move recursively lists *effective*
   inherited descendants for path bounds before publication; this is a
   potentially full-subtree walk, not O(1) rename. Stable parent/inode
   `core/crates/layerfs-content/src/filesystem/read.rs:138-160,196-276`
   handles children/readlink. External `core/crates/layerfs-api/sdk/tests/inherited_workspace.rs:301-765`
   covers moved old heads, resident/uncached path refusals and held handles.
   **Disposition:** accept only bounded rename semantics, not #270 C1
   path-local Commit or #256 package scale; retest against #273 G1/G2.
5. **#269, definite redundant finalization, LOW correctness / MEDIUM custody-risk request:**
   `core/crates/layerfs-workspace/src/filesystem/rename.rs:517-518` calls
   `candidate.seal(root,window,deadline)?` *twice* before selecting a rename.
   The second call was introduced over #263. `backing/metadata.rs:798-897`
   shows seal drains temporary pages/custodies and cleanup and returns/refunds
   reservations; a second call is **not** a pure getter. It appears idempotent
   after a successful first call, so a leak or false refund is **not proved**;
   nevertheless an extra fallible cleanup/release pass can reject the
   otherwise prepared rename. Remove or justify the second call and cover
   refusal/cleanup once in #269's owning lane, without inflating limits.
   `runtime/ancestry.rs:5-29` rejects detached parent chains and cycles under
   `rename.rs:520-545`; `namespace_view.rs:69-124,215-238` uses parent/component
   and serial queries. External `core/crates/layerfs-api/sdk/tests/inherited_workspace.rs:304-978`
   challenges pinned ancestors, detached parents, deep lookup and quota.
   **Further risk:** #273 still has path-based Node assumptions; review the
   intersection, not Git mergeability. `core/crates/layerfs-workspace/src/commit/directories.rs:71-165`
   does not prove #270's final-batch C1 cycle/owner-index/old-reader bound.

## 3. #276 requests, mandatory stop, and conditional integration plan

* **§1 SDK:** owner must approve a specific *public same-Workspace* G1 journal
  lease (G1/G2 bytes, 32 pins, cancel, custody, deadlines), **or** grant a
  named-profile waiver with clean and one-edit controls both **NOT_RUN** and
  registry incomplete. Neither choice recorded. Detached Store forbidden.
* **§2 cache/phase memory:** owner-approved common private-container/VM/backend/
  device/host cache capability and verifiable *phase-local* cgroup peak are
  absent. Writing `0` to `memory.peak` left 36,163,584 B versus current
  2,428,928 B. Frozen control **NOT_RUN**, nine numbers **INELIGIBLE**; no
  25-second timeout denominator, repeated unchanged arm or new campaign.
* **§5 red tests:** `layerfs-content/tests/filesystem_ordering.rs` tests
  `a_fresh_build_charges_its_count_array_to_the_ordering_ceiling` and
  `a_high_pending_ceiling_runs_spill_free_to_the_byte_bound` **FAIL** with
  `ObjectLimitExceeded { limit: 18, actual: 19 }`; paired first-parent broad
  run **NOT_RUN**. Owning C1 ordering lane must diagnose/fix or explicitly
  rule. Do not enlarge the ceiling or claim pre-existing failure.
* **§3/#256:** multi-file package capacity and arbitrary frontier NOT_PROVED.
  **§4/#270:** mounted-rename is not path-local C1 directory-move Commit;
  canonical full-base alias/moved-subtree validation remains; NOT_PROVED.

**NO MERGE / NO RELEASE / NO NUMERIC ADMISSION now.** No explicit authorization
for integrating #263 or draft #269, no owner decisions, an unresolved broad
red and the #269 duplicate seal. #273 hot and #264 namespace diverge at
`6af2c5c59a48d0b6c85d656e55aecc353e346728` (181/13 commits at
this checkpoint); neither contains the other. **Combined source: NOT_BUILT.**
If authorized *after* rulings/reviews: create another owned worktree based on
reviewed #273 (not stale main); integrate #263 ancestry then #269 exactly
once, keeping #262/#272/#274 already in #273. Resolve actual conflicts
rather than replay commits, declare #273 as first parent, calculate
`python3 tools/production_loc.py --json --root <first-parent-archive>` and
`--root <final-staged-archive>` before **each** commit, confirm committed
Core/reference/combined LOC, pin updated architecture docs and independent
old/new canonical heads. No conflict list/tree seal exists yet.

**Combined functional and checks: NOT_RUN** (no authorized combined tree).
Required once on clean exact source: external native Linux/ext4 namespace,
resident/never-resident moves, ancestors/held detached/old heads; G1/G2 and
same PID/fd/inode through SaveFile/C5; tiny/Payload/overlap/slot epochs;
notification, old pins, aliases/orphans; quota and Budget refusal, known C1
then local C5 failure, partial/unknown owner, verified unlink/refund; original
#248 4,097-write gate with anchored C1 zero. Run affected locked Core tests,
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`, warning-denying
workspace/all-target Clippy, Core fmt, `core/tools/check_product_boundary.py`,
its self-tests and `git diff --check`, plus owned capable Linux native proofs.
This reviewer has macOS arm64 and no Docker CLI in this shell; no owned ext4
resource or combined executable was used. No binaries, other owners' receipts,
Docker resources or working trees modified. No CI or retired preflight run.

### Subsequent external publication (after the review snapshot)

Using authenticated `gh`, the source-bound [#274 evidence summary](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274#issuecomment-5873088937)
was posted, followed by an explicit [archive-link correction](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274#issuecomment-5873095626):
the archive was committed *after* the `4ae36ad3a` functional-source
commit, so the first comment's archive link at that SHA does not exist.
The corrected link targets the archive at `0a932faaa`. The [#276 owner
request](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5873101805)
asks for §1 same-Workspace lease or named waiver, §2 common cache/phase-cgroup
capability or numerical ineligibility ruling, and §5 ownership/disposition
of the two red tests. These are **requests, not owner approvals**. Neither
comment changes a historical raw receipt, PR review state or combined-tree
NOT_BUILT decision.

### Follow-up source diagnosis and owner-facing PR comments

After rechecking the same live PR heads and open #276, posted a scoped
[#269 change request](https://github.com/Ephemeral-AI-Lab/layerfs/pull/269#issuecomment-5873181294)
for the duplicate seal and a [#263 dependency review comment](https://github.com/Ephemeral-AI-Lab/layerfs/pull/263#issuecomment-5873221907).
These comments are not formal independent approvals or combined-tree results.

Static analysis of the retained two C1 ordering failures identified a more
specific candidate cause, sent to the [#276 owning-lane request](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5873211761).
At #274 `core/crates/layerfs-content/src/filesystem/update.rs:535-549`,
`unreachable_parents` stores **every positive child binding** in `bound`
and refuses when `bound.len() > ordering_bytes/1024`. The high-pending
fixture's calibrated `ordering_bytes = 2*96*rows` can consequently hit
`limit:18,actual:19` while its intended pending-row charge fits. The fresh
count-array fixture uses only `102*16` ordering bytes; the same all-child
check precedes the intended count-array check. Yet the function needs to
identify only new directory *parents* left unbound (`update.rs:553-567`).
This is a **source hypothesis**, not a fix, backtrace, paired parent run or
new test outcome. `git blame` attributes that refusal to shared base
`6af2c5c`; identical source at #272 and #274 does **not** prove a paired
first-parent broad test result. Owning C1 lane must trace, repair or rule
with correct charged storage and both tests, without enlarging the ceiling.
The two FAILs and paired-parent NOT_RUN remain unchanged. #276 has no
owner decision; integration remains NOT_BUILT.

## 4. Owner direction: pursue substantive optimization, not the easy waiver

> **Owner decision in the handoff conversation, 2026-09-28 UTC:** the owner
> accepted the recommendation **1A / 2A / 3 yes / 4 yes**. This is
> prospective work authorization and an explicitly bounded *functional-only*
> integration scope, **not** an implemented SDK contract, proved cache
> capability, corrected test, approved PR merge, speed result or release
> admission. Sections 1–3 retain their original review-time statuses.
> These are new source-planning instructions; the evidence at `8801b9c`,
> frozen `4ae36ad3a` and reviewed `29fc5747d` is not relabelled.

1. **SDK §1 — 1A: design and implement a real public same-Workspace G1
   journal lease; no waiver selected.** First specify a product-supported
   SDK/Bridge selected-view identity, acquisition and release, G1 old-reader
   bytes, continuing G2 process/edit/Commit, at most 32 captures, charged
   admission/refund, cancellation, deadlines and known/unknown canonical
   custody. Review the proposed API/format/ownership contract before
   implementing; do **not** fabricate a method name, use a detached Store or
   introduce a test-only hook. Implement in a separately owned lane with
   independent public byte/pin/refusal and cleanup proofs. Both registered
   clean/one-edit controls remain **NOT_RUN** until a proved public lease and
   frozen shared workload/harness actually allow those selections.
2. **Numerics §2 — 2A: develop an independently verified *common* capability
   before collecting any matched performance arm.** Declare and enforce
   identical private container/VM/backend/device/host cache state and a
   verifiable Exec/Commit-local cgroup memory scope for frozen #271 and the
   candidate without editing frozen #271 product. Common observer/harness
   changes require prospective seals; existing candidate rows are not
   portable to a different identity. If the host cannot provide the
   capability, stop and report it: frozen matched control **NOT_RUN** and
   nine candidate numbers **INELIGIBLE** remain. Only when the capability,
   owner-reviewed shared contract, all twelve registered selections and
   exact row-major control→candidate one sample per case/arm can be met may
   a *new* campaign be proposed. No warm-cache credit, 25-second censored
   denominator, relaxed deadline/Budget or repeat of an unchanged arm.
3. **Repairs — yes: separately owned worktrees are authorized for the C1
   ordering-red diagnosis and #269's duplicate seal.** For C1, verify the
   `unreachable_parents` hypothesis against the exact failing call route,
   prefer charged targeted new-directory-parent membership/a checked join
   over accumulating every unrelated binding, preserve cycle/replacement
   semantics and old-reader behavior, and cover both formerly red cases.
   Do not enlarge `ordering_bytes`, the test ceiling or timeout. A paired
   first-parent broad run is still **NOT_RUN** until actually performed; do
   not call the failure pre-existing by inspection alone. For #269, remove
   or justify the second fallible seal with one-attempt refusal/custody and
   verified refund checks. Changes require source-pinned tests, affected
   architecture documentation and exact first-parent → committed production
   LOC for every commit. No other owner's worktree, raw receipt, binary or
   Docker resource may be modified. Both fixes remain **NOT_RUN** here.
4. **Initial integration scope — yes: #256 many-file/package scale and #270
   path-local C1 move-Commit are deferred, explicitly NOT_PROVED.** Do not
   infer either from mounted rename or #273's one-file proof. Prioritize
   #270 as a *separate* substantive namespace optimization after combined
   functional proof: final-batch unique-parent/cycle and authenticated
   owner-index/legacy old-reader design, actual page/file-save counts and
   charged failure custody remain its own acceptance. Follow with realistic
   many-file #256 workloads on their own changed source. Deferral is **not**
   permission to assert release/package-scale admission.

**Next decision boundary:** report repair and review dispositions plus
actual lease/cache capability progress against then-current source. The owner
may separately authorize *functional-only* integration even while SDK
controls remain NOT_RUN and numerical rows INELIGIBLE; that would not be
complete registry, numeric admission or release approval. No such explicit
integration authorization exists yet, so do not build a combined tree or
merge draft #269. The broad C1 red tests still
**FAIL** in historical receipts; no new checks, matched control or combined
functional gate have run here. **NO MERGE / NO NUMERIC ADMISSION** remains
the current disposition pending those gates; earlier #276 owner-choice
requests are answered as *direction to pursue 1A/2A*, not as evidence that
§1/§2 admission succeeded.

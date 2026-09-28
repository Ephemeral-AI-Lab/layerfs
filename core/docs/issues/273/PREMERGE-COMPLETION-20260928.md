# Phase 4.5 pre-merge completion: lanes, repairs, SDK lease, capability and combined tree

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owned lanes on new worktrees; no other owner's worktree, receipt, binary, target,
> container or measurement was modified. No GitHub PR was merged, no draft PR was
> touched, `main` was not updated, no benchmark limit was changed and no historical
> receipt was rewritten. All historical statuses (frozen control, functional proofs,
> nine-row INELIGIBLE, missing-zero INCOMPLETE, retained FAILs) are unchanged.

## 1. Rechecked live state (2026-09-28T16:05Z)

| PR | State | Head (unchanged from pins) | Base | Disposition after this pass |
| --- | --- | --- | --- | --- |
| #262 | OPEN draft | `6bcfa464f74ae9ca3859df31c678985ec69ba098` | `6af2c5c59a48d0b6c85d656e55aecc353e346728` | Bounded source review stands (reply-gap permit, physical custody); no merge endorsement. |
| #272 | OPEN draft | `48b51e874a41b3e1e6c6661e145316df8b408f07` | `3c3343e0b` (contains #262) | Bounded source review stands; no speed admission. |
| #274 | OPEN draft | `29fc5747d70eadf01fd8794bf84a50cf57ef3660` | `48b51e874` | Basis of this pass's #273-side work; its two ordering reds and the third red are now REPAIRED on `codex/issue273-c1-ordering-repair` (`fbda0f0f1`) with paired first-parent evidence. |
| #263 | OPEN | `ef3a310480254774d6e6966004fb5e0a4fa3b94d` | `6af2c5c59a48d0b6c85d656e55aecc353e346728` | Reviewed and integrated once (merge `022a37211`); its inherited-move semantics were ported onto the active rename. |
| #269 | OPEN draft | `6eb7553671d3000160ad023a57b335e52dd81a26` | `ef3a310480` | Duplicate seal repaired on `codex/issue269-seal-repair` (`05eb5c148`) - and its true origin corrected: introduced by `a9657b85f` (2026-09-23), an ancestor of BOTH #263 and #269, not "over #263". Repaired head integrated once (merge `44379c3f2`). |
| #275 | OPEN | `c39d6085` (new, unrelated statfs quota) | `main` | Out of scope; not reviewed. |

#276 remains OPEN; the latest comment is the owner direction at 2026-09-28T15:40:26Z
(1A/2A/repairs/functional-only), which this pass executed. No newer owner ruling.

## 2. C1 ordering-red repair (owned worktree `issue273-c1-ordering-repair`)

- **Paired first-parent evidence** (required before any pre-existing claim): at
  `29fc5747d`, locked release, `layerfs-content` no-fail-fast = **271 passed /
  3 failed**: the two retained `filesystem_ordering` reds
  (`a_fresh_build_charges_its_count_array_to_the_ordering_ceiling`,
  `a_high_pending_ceiling_runs_spill_free_to_the_byte_bound`) **plus a third red
  the retained evidence had not named**:
  `filesystem_parent_lookup::parent_value_ordering_quota_matrix_is_explicit_and_canonical`
  (`ObjectLimitExceeded { limit: 0, actual: 1 }` at every 96-byte cell).
- **Root cause** (history-pinned): `unreachable_parents` charged EVERY positive
  child binding at `ordering_bytes/1024` (introduced by `ef6bab19d`, 2026-09-17,
  charge added by `6af2c5c`), and validation's declared totals (rows, names,
  prefetch demand) charged at the same 1024-byte rate (also `6af2c5c`), so an
  operation whose count array fit its budget refused in validation first.
- **Repair** (`fbda0f0f1`, first parent `29fc5747d`): targeted charged membership -
  one 1,024-byte entry per declared-new parent other than the root, one binding
  pass joined against that set (no per-binding storage); validation's declared
  totals bounded by the per-serial allowance (`maximum_touched_serials`) while the
  base-record memo and effective-tree walk bounds keep the conservative 1,024-byte
  rate; cycle/reachability proofs split to `validate/cycles.rs` (file back under
  the 999-line ceiling). No `ordering_bytes`, Budget, test ceiling or timeout was
  enlarged; no test expectation was edited; one NEW boundary test added.
- **Verification**: repaired source `layerfs-content` **275/275** (release, this
  host); at repaired source the quota matrix produces its intended byte-granular
  refusals and the four baseline-supported cells return the canonical root
  `ceb15e7f...`. `layerfs-workspace` release suite on this host: 17 passed / 0
  failed (macOS-visible subset); the Linux-gated subsets need the route drivers
  and are covered by the combined-tree run below.
- LOC: Core 68,031 -> 68,041 (+10), combined 133,448 -> 133,458 (+10).

## 3. #269 duplicate-seal repair (owned worktree `issue269-seal-repair`)

- **Correction to the retained review**: the duplicate `candidate.seal` was
  introduced by `a9657b85f` (2026-09-23), already an ancestor of #263 - the
  observation stood, the "introduced over #263" attribution did not.
- **Repair** (`05eb5c148`, first parent `6eb755367`): the second fallible seal
  removed; one-attempt publication documented in place. A second fallible pass
  over a sealed candidate could reject an otherwise prepared rename (closed host
  or poisoned lock between the calls) without doing work the first left behind.
- **Verification** (Linux container, owned ext4 volume): `layerfs-sdk
  inherited_workspace` **11/11 PASS**, including the new focused test proving the
  sealed rename refunds its slots exactly once, retains exactly the fixed
  completion escrow (851,968 bytes) until Commit, releases it exactly once at the
  completion, and closes clean. The broad direct Linux workspace suite cannot run
  outside its registered route drivers in this container (the Native fixture
  needs the route-driver service environment): **NOT_RUN** for the non-route
  suites, stated as such.
- LOC: Core 58,706 -> 58,705 (-1), combined 124,123 -> 124,122 (-1).

## 4. SDK view lease (owned worktree `issue273-sdk-view`)

Contract recorded BEFORE implementation in
[`SDK-VIEW-LEASE-CONTRACT-20260928.md`](SDK-VIEW-LEASE-CONTRACT-20260928.md).
Implementation (`6f18a5f42`, first parent `29fc5747d`): public
`WorkspaceApi::{pin_view, view_lookup, view_list, view_read, view_readlink,
view_status, release_view}` + `WorkspaceViewLease/Entry/DirectoryPage/Read/
Status/Release` types; seven daemon operations (opcodes 21-27, one shared
view-authority grant bit 64 - the only free control bit); native codec; charged
Workspace lease registry bounded to 32 held leases with server-side entry
binding; checked release through the retirement selector; view operations take
the same daemon control slot (Busy during an in-flight Commit, sequential reads
across a finished Commit supported).

**Live proof at final source** (macOS host, Docker sandbox from owned image
`sha256:f35d5fbb5e46...`, daemon `aarch64-unknown-linux-musl` sha256
`18b8960d901c1fa60fa20539331df384de2cb15b7423de0c87ef07adae1fb014`):
pinned root serial 1, generation 1, revision 5, stable across a later Commit;
old G1 bytes through the lease (`g1-note`, `g1-leaf`) and live G2 bytes through
`exec` (`g2-note`, `g2-leaf`) both exact in one attached Workspace; the pinned
G1 listing keeps a name live G2 removed; a released lease, a cross-lease entry,
a malformed-tag token (InvalidInput) and an unregistered well-formed token
(Denied) are each refused; 32 leases admitted, 33rd refused (Capacity) without
pinning; release Completed and the unmount then completes, which a retained pin
would have failed closed. Bridge wire round-trip + malformed-shape refusals
pass. Core suite on this branch: 739 passed / 3 failed (the three then-unrepaired
C1 reds; repaired by the C1 lane, not in this branch). Clippy(-D
warnings)/fmt/boundary(357 files)/self-tests PASS.

Not proved by this pass (honest gaps): deterministic deadline forcing and
Budget-exhaustion refusal beyond the 32-bound on the live route (the 33rd-lease
Capacity refusal is the charged-registry refusal path); a known-C1-then-local-C5
failure with a held lease; an uncertain-release-custody outcome. The registered
clean/one-edit benchmark controls remain **NOT_RUN** - this capability proves
the route exists; the harness identity for those selections is not re-sealed by
this document.

LOC: Core 68,031 -> 70,093 (+2,062), combined 133,448 -> 135,510 (+2,062).

## 5. Cache / phase-cgroup capability probe (no product arm)

Owned privileged observer (`--cgroupns=host`) on Docker Desktop
(kernel `6.12.76-linuxkit`), probing ONLY this pass's own container:

- **`memory.peak` reset falsified again**: write of `0` exits rc=0, peak stays
  2,798,239,744 while `memory.current` fell to ~30 MB. Phase-local peaks are NOT
  available on this host.
- **`memory.reclaim` WORKS as per-container private-cache invalidation**: after
  warming 48 MiB inside the container, writing `memory.reclaim` drove the
  container's `memory.stat` file cache from 808,337,408 to 49,152 bytes
  (`current` 884,494,336 -> 30,314,496). A second write is idempotent at the
  floor. `memory.stat` readback verifies the enforced state - a real, common
  per-arm private-container cache contract candidate.
- **Phase-local memory observation**: `memory.current`/`memory.stat` deltas
  sampled around Exec/Commit control calls give phase-local container memory
  without lifetime-peak lies; per-container cgroups are reachable and writable
  from the privileged observer (`/sys/fs/cgroup/docker/<id>/memory.reclaim`).

**Verdict: PARTIAL.** A verified common method for per-container private cache
invalidation and phase-local current/stat deltas exists on this host. NOT
established: phase-local peaks, VM/backend/device-level cache identity, and the
privileged observer is a harness change requiring prospective seals. No product
performance arm was collected. Numeric rows remain **INELIGIBLE**; matched
frozen control remains **NOT_RUN**.

## 6. Combined tree (owned worktree `issue273-combined`)

Branch `codex/issue273-combined-functional`, first-parent chain
`29fc5747d` -> `6f18a5f42` (SDK) -> `7ff202e06` (merge C1 repair `fbda0f0f`) ->
`022a37211` (merge reviewed #263 `ef3a3104`) -> #269 merge of repaired
`05eb5c148` (final product tree
`cb23d730bd74902aab49055db6b66d5d3d06bb31`). #262/#272/
#274 ancestry already present in #273 was not replayed; no fast-forward claimed.

Per-commit production LOC (first parent -> committed tree,
`tools/production_loc.py --json --root <git archive>`):

| Commit | Core | Reference | Combined | Delta |
| --- | --- | --- | --- | --- |
| `6f18a5f42` (SDK, parent `29fc5747d`) | 68,031 -> 70,093 | 65,417 | 133,448 -> 135,510 | +2,062 |
| C1-repair merge | 70,093 -> 70,113 | 65,417 | 135,510 -> 135,530 | +20 |
| #263 merge | 70,113 -> 70,439 | 65,417 | 135,530 -> 135,856 | +326 |
| #269 merge (final product tree) | 70,439 -> 70,457 | 65,417 | 135,856 -> 135,874 | +18 |

Every tree above was re-measured after the fact with the same counter on
`git archive` of each commit; the two middle merge messages initially carried
mis-transcribed intermediate numbers and the chain was rebuilt (same trees,
same parents, corrected messages) before this ledger was committed. The final
product tree is `cb23d730bd74902aab49055db6b66d5d3d06bb31`; this ledger commit
adds documentation only (delta 0).

Merge-2 conflicts (rename.rs, namespace_view.rs) resolved toward the #273 active
architecture with two #258 semantics **ported into `active_rename.rs`**: the
fail-closed `preflight_rename_paths` (moved directory, resident descendants, and
the never-resident inherited subtree a growing move must walk) and
`ActiveOrigins` entries for every resident descendant so inherited children
under a moved directory still fall back to the canonical base at the base-known
path. No obsolete RootOwner mutation route was resurrected. Merge-3 conflicts
(ten files) resolved the same way; `Inspect::InodeReadlink` and
`BackingStatus.metadata_writes` (with the arena counter that feeds it) were
ported onto the combined tree; the #269 identity-relative rename/namespace
rework, `rename_preflight` and the orphan `ancestry` module were dropped as the
retired route.

**Combined-tree verification (this host + owned Linux/ext4 container):**

- `layerfs-sdk inherited_workspace` (#263-era cases): **7/7 PASS** after the port
  (5/7 before it, with exactly the two ported semantics failing).
- `layerfs-sdk agent_route`: **3/3 PASS**.
- `layerfs-sdk inherited_workspace` (#269-era suite, 11 tests): **4/11 - 7 FAIL**,
  each an identified semantic gap, not a silent skip: #264's identity-relative
  resolution removed the canonical path-length dependency (moves beyond the
  4,096-byte path bound succeed there) while the active view's canonical
  fallback still resolves by path and enforces the bound (2 Capacity failures);
  the active publication holds no metadata escrow (the escrow expectation reads
  0); and the detached-parent refusal shape differs (a lookup the #269 route
  refuses resolves successfully). A complete union needs the active view's
  canonical fallback ported to `ChildAttributes`/`InodeList` resolution.
- `layerfs-workspace tests/readable.rs` (#264 read-only route): **3/9 - 6 FAIL**,
  all `Service(Unsupported)` - the read-only route's service query surface is
  not complete on the union.
- macOS core suite: **741 passed / 6 failed** (the six readable.rs cases above;
  the three C1 ordering reds now PASS on this tree). Clippy (-D warnings), fmt,
  product boundary (359 files), boundary self-tests and `git diff --check` PASS.

**NOT_RUN on the combined tree** (not silently skipped): the original #248
4,097-write functional gate with anchored C1 edit-load zero (needs the
checkpoint-5 harness route on a prospectively sealed combined identity); the
full step-5 proof matrix (G1/G2 same-PID/fd/inode across SaveFile/C5, quota/
custody matrix, aliases/orphans, tiny/Payload/overlap/slot epochs); the
Linux route-driver suites. The SDK view live proof was run at the SDK-lane
source, not re-run on the combined tree (the view stack is unchanged by the
merges; the daemon image for the combined tree was not rebuilt in this pass).

## 7. Numeric campaign

**NOT_RUN / INELIGIBLE**, unchanged. Gates: the SDK route now exists, but the
registered clean/one-edit selections still need a prospectively sealed harness
identity; the cache/phase capability is only PARTIAL (no phase-local peaks, no
VM/device-level identity); the twelve-selection registry is not re-sealed for
any new source; the censored 25-second control walls remain FAIL, not
denominators. No benchmark limit was changed; no unchanged arm was repeated.

## 8. Explicitly NOT_PROVED (deferred)

#256 many-file/package scale and #270 path-local C1 move-Commit remain separate,
explicitly NOT_PROVED follow-ups (owner direction). Nothing in this pass admits
package-scale capacity or a pure-move `O((K+D)h)` Commit.

## 9. Commands, host and cleanup

Host: macOS arm64 (Darwin 25.4.0, 14 cores), rustc/cargo 1.85.1. Owned Linux
container `layerfs-p45-owned` (rust:1.85.1-bookworm, `/dev/fuse`, `SYS_ADMIN`,
ext4 volume `p45-owned-ext4` at `/data`). Owned images: SDK-lane daemon
`sha256:f35d5fbb5e46...` built from `6f18a5f42`. Checks:
`cargo +1.85.1 test --release --manifest-path core/Cargo.toml --locked
[--no-fail-fast] [-p <pkg> --test <t>]`, `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --locked --workspace --all-targets -- -D warnings`,
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`,
`python3 core/tools/check_product_boundary.py`, `python3 -m unittest discover
-s core/tools -p 'test_*.py'`, `git diff --check`, `cargo +1.85.1 zigbuild
--release --target aarch64-unknown-linux-musl -p layerfs-daemon` (repository-root
AEAD profile). No retired preflight, no CI claim, no third-party patch, no
benchmark run. Other lanes' containers were never touched; probes targeted only
this pass's own container cgroup. Worktrees are clean at their commit heads.

## 10. Recommendation

**NO MERGE now.** The repairs and the SDK capability are real and verified at
their sources, and the combined tree is built with an honest partial-union
record - but seven #269-era semantic gaps and six readable-route gaps remain
open on the union, the full combined functional matrix and the #248 gate have
not run, the cache/phase capability is only partial, and every numeric row
stays INELIGIBLE. Merge decisions for #262/#272/#274/#263/#269 remain the
owner's, with the dispositions above as the source-pinned input.

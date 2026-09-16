# Prompt: finish pooling coverage and independent qualification

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

## Copy/paste assignment

Complete the remaining pooling verification and qualifying evidence for
[#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.
Read the [shared continuation rules](stages-3-4-continuation-prompt.md), current
completion report and physical encoding/persistence contracts. Pooling is now
implemented; verify and correct it where evidence warrants. Do not restart E.
Independent pooling correctness/performance does not depend on D's edit algorithm.

You own C2 pooling tests, necessary `encoding/pool/` and `sqlite/pool.rs` fixes and
pool-specific evidence/specification. You are not alone in the codebase: preserve
D's C1/oracle edits and do not revert others. Coordinate shared policy/schema/API
or canonical-grammar changes with the integration owner. No shared-report overwrite
or resource-sensitive measurement/build overlap.

### 1. Exercise the actual index-window boundary directly

Use the existing real public `PoolIndex::note_group` operation with valid 73-byte
values and groups within the production group bound. Generate groups incrementally.
Keep the production window at 131,072; no smaller test window, hash injection or
new test-only API. Test occupancy below, exactly at and across the limit, including
variable group sizes that distinguish whole-group reset from per-entry eviction.
Check count, ordinal progression and bounded state after the next group is admitted.

This direct index test does not require persisting 1,300 leaves first. The aggregate
raw value bytes for 131,072 distinct values are 9,568,256 bytes (9.125 MiB), and can
be generated without retaining that whole input. This is arithmetic, not an asserted
execution-time or total-memory result; count actual B-tree/transient allocation.

Add the focused external test to `tests/metadata_pool_index.rs`. Keep it separate
from the real persisted boundary/reopen test: direct index behavior alone does not
prove catalogue replay or DB integration.

### 2. Verify persisted rollover and reopen with a reusable fixture

Prepare an identity-recorded real Store that crosses the same unchanged window
through the actual production save path. Prepare once, retain its manifest and use
the allowed independent writable clone mechanism for subsequent cases. Do not
regenerate it for every test, mutate a prepared master, or use setup warmth as
performance credit. Fixture reuse cannot remove work that belongs inside the named
save/reopen/synchronization operation.

Verify whole-group chronology/reset, retained-window reuse and evicted-value behavior,
same smallest authenticated ordinal as the reference, catalogue replay and correct
readback of earlier retained leaves. An empty in-memory index after reopen is a
cold index; it is not evidence of a cold OS page cache. Declare those states separately.
If a registered budget cannot be met, record the actual failure/time and remaining
gap; never shrink the window or raise the timeout to manufacture coverage.

### 3. Verify actual chains, not only persisted depth settings

Audit the current metadata default/depth/work policy against the approved reference
contract before testing. Acceptance of 12 or 50 in a policy row is not a verified
chain at that depth. Build small valid changing pooled leaves with explicit eligible
bases through the production path; avoid exact-CAS duplicates or repeated data that
accidentally resets the history. Inspect actual selected bases/edges and records.

Test representative increased depths, the default cap, first excluded edge, and
canonical/encoded/pool work limits independently. Where another limit binds before
the configured depth, prove and report that boundary rather than pretending the
deeper chain was exercised. Do not enlarge byte budgets, alter savings rules or
replace the production selector to force depth 50. Reopen and read retained earlier
results as well as the latest result. More revisions than chain depth must remain
possible through normal eligible FULL selection; depth is not a revision-count cap.

### 4. Handle collision coverage accurately

Search existing reference fixtures first. A real truncated-64-bit fingerprint
collision is expensive to find, not impossible by construction. If a valid pair is
available, retain it as an external fixture and verify that full authenticated bytes
and the smallest exact-match ordinal decide reuse. Do not add a production hash
injection switch, widen visibility solely for testing, or launch an unbounded
collision search. Ordinary reuse/corruption tests do not become collision tests.

If no authentic fixture is available, document the exact branch/source proof and
remaining runtime coverage gap. Continue all other work. Do not mark the gap PASS
or waive a mandatory criterion; final acceptance decides against the existing
contract, and only an explicit owner decision can relax a required gate.

### 5. Qualify the completed component independently

Use supplied canonical metadata inputs through real C2 save-to-ack and authenticated
reads; no file-edit/FUSE/Workspace setup is required. Commit a pool-specific versioned
case addendum before benchmark implementation/collection, using existing harness
mechanisms. Freeze exact inputs/identities/cache/index state, successful operation
boundaries, matched reference semantics and numeric/resource/storage gates.

Cover low/high reuse, rollover, live-index versus reopen, pooled FULL/DELTA and
failure cleanup. Record complete operation costs, SQL/catalogue/base/group reads,
compression work, total retained database/pack/index footprint and scoped memory.
Validate B-tree charging against actual node/transient costs rather than calling
key-payload bytes total memory. Preserve ordinary visibility and one-attempt failures.
No comparison claim when the public operation/semantics/cache state cannot be matched.

Focused checks:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test inode_leaf
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test metadata_pool --test metadata_pool_index --test policy_capacity
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test physical_formats --test pack_locator --test visibility --test persistence_failure
python3 core/tools/check_product_boundary.py
```

Reuse qualifying exact-identity evidence where allowed; do not rerun unchanged passes
by habit. Coordinate builds/checks with active measurements. Preserve every failed
or ineligible case. Later integration may change relevant source/build inputs;
final acceptance must check reuse eligibility and rerun affected cases when required.
Independence of this assignment is not a promise that all evidence survives new seals.

Save `stages-3-4-pooling-report-<UTC>.md` with implemented fixes, boundary/chain
evidence, any authentic collision fixture or explicit gap, component measurements,
source identity, actual LOC and remaining criteria. Do not close #168; hand evidence
to the [final acceptance assignment](stages-3-4-continue-acceptance.md).

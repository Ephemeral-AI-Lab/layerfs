# #130 adversarial foundations review

Read-only audit of the pinned issue and 595-line proposal. No product/proposal
file was edited; no build, test, probe, benchmark or GitHub action was performed.
The counterexamples below test proposed implementation choices, not observed
current product defects. Source identities for the line anchors:

- `issue130.md`: SHA-256 `671d306711a8f05faf4e9e6b939477cd1ab69e5b2766418c1f2a26600dc3f889`
- `README.md`: SHA-256 `8d0a4dbddbac6ef3eea09f4eb94fbe804e9eb7fc43680790c1c424c82ef1618c`
- `overlay-snapshot-rule.md`: SHA-256 `03d93fc5929ba2b6e99c013e4898a454ddf908c81089c41bb87f9d24d54f543e`
- `overlay-snapshot-spec.md`: SHA-256 `1e5913da8454cf0aae597f0d5cf6908ab8ebe8e86e212f966e424df799b356da`

## Verdict

**No explicit lifecycle/FUSE contradiction was found in the requested rule
sections. The proposal is not yet a complete selected mechanism that can be
certified to meet every rule.** Its constraints are largely sound, and it openly
requires later selection and proof. The strongest new gap concerns the ownership
granularity lost when independent file descriptors/ranges share packed pages.
The pager bootstrap and resident-only fsync paths also need selected contracts.

V1 is **not a new ignored blocker in #130**. Issue lines3–16 require completion
of the earlier seven steps and verified completed #124/#125 closure before any
#130 implementation. Lines83–87,164–166,409–411 retain supported-surface capture.
An audit that rejects #130 merely because today's prerequisite V1 is unfinished
would misread its scheduling. The new representation must preserve the actual
provider delivered by that prerequisite; this review did not rerun V1 research.

Severity below means priority for design selection/implementation. A missing
mechanism is not evidence that an existing implementation has violated the rule.

## F1 — P1: packed pages can give a file-only reader unrelated payload ownership

**Classification:** missing ownership mechanism and focused acceptance case;
**not an observed current bug**.

**Anchors:** issue88–98 (compact singleton ranges/correspondence and packed
representation); README97–99,140–155. Rules §5 lines171–178 require independent
owners and reclamation of unrelated superseded state. Spec187–206 requires
indexed ownership and independently reclaimable ranges.

**Counterexample.** Put one-byte file A's singleton range descriptor and a large
file B's descriptor in the same packed inode/descriptor page. Retain a file-only
reader of A, then delete or replace B and release every legitimate B owner. If
A's reader keeps that containing page/root alive as its descriptor lease, the
page's child/external-owner edges can keep B's old range tree and payload alive.
A's one byte then pins B's unrelated large allocation indefinitely. Correct bytes
and an O(1) reader handle do not make that retention correct.

A **full Commit Snapshot legitimately owns its complete captured view**. This
counterexample concerns an independently retained file-only reader/read plan that
promises ownership of A, not a Workspace Snapshot whose scope includes B.

**Smallest repair for singleton state:** while a short page lease is held, copy
only the fixed-size descriptor and acquire the exact referenced payload/token or
immutable base context. Return that independent owner and release the containing
page. Do not let descriptor-address lifetime silently become ownership of every
row's referenced data. Keep the existing exact subrange retention contract.

**Fragmented ranges need an additional restriction.** If many files' range nodes
are packed into a shared index, retaining its whole root repeats the same problem.
Copying all of A's pieces into a RAM vector is not an acceptable repair. Preserve
a per-file/scoped logical root and bounded cursor with independent ownership,
or pack within the existing per-file ownership domain first. Cross-file physical
packing requires a proved distinction between page-byte lifetime and each logical
record's outgoing ownership before enabling it.

**Required narrow check:** hold the A-only reader across B replacement/deletion,
root release and pager eviction; verify A's bytes, B's physical reclamation and
charged backlog. Repeat with A fragmented enough to require several pages and a
bounded cursor. General "held reader survives spill" checks in issue406–408 do
not alone expose this neighbor-retention failure.

## F2 — P2: lazy pager bootstrap does not select its ownership/location protocol

**Classification:** explicitly acknowledged but unresolved selected mechanism.
README154–156 already says a cache diagram is not the proof. This finding does
not treat that honest caveat as a false implementation claim.

**Anchors:** issue75–98; README94,138–156,574–579. Rules §§4–6 lines109–115,
163–174,190–202 and §9 lines295–306. Spec129–148,173,177–191.

**Counterexample.** A small current graph exists only in resident buffers and is
captured as S. Later writes exhaust the resident allowance and spill one of S's
pages; subsequently the live root and its Workspace owner are dropped while a
retained reader still needs S. A bare PageId does not identify where to read after
cache eviction. A transitive Arc<PageBytes> graph avoids loss but pins the graph
in RAM. A complete first-spill/capture conversion creates the forbidden graph
walk, and an always-resident record per historic ID can be unbounded. The text
rejects all these outcomes but does not select the remaining catalog/lease
protocol or its failure-atomic transition.

**Select before implementation:** where pre-spill location and outgoing-reference
state live; how a bounded resident catalog becomes incrementally disk-addressable;
which owner keeps the directory/arena and charges after logical End; what an
in-flight physical read owns while a page moves; and how partial body/catalog
writes leave either the old readable state or an authoritative retry state.
Eviction I/O must not retain a shared pager/service lock across Commit traversal.
A shared pager also needs IDs scoped to their allocation domain/generation so
per-Workspace PageId reuse cannot become a cache ABA problem.

**Smallest restriction:** initially retain the existing owned disk graph/catalog
and defer only payload files and unnecessary buffers. If measurements justify
memory-only metadata, specify the bounded catalog and incremental spill state
machine first. The existing freeze-selected-representation gate (issue482) is
the right place; naming a "shared pager" cannot replace that deliverable.

**Shared-cache and admission subchecks:** a cache entry borrowed by Workspace B
must not lose its charge/file owner when its original Workspace A ends. Shared
immutable cache keys must preserve Store/trust/immutable-object context; live
inode/binding applicability is resolved through the appropriate authoritative
current or captured root, not cached as a permanent "unchanged base" verdict.
Keep recovery buffers/FDs/catalog updates admitted before consuming normal
capacity. Reducing static allowance is not permission to make an evicting reader
wait for the entire Commit to release a shared memory claim. These are concrete
conditions for the new shared/dynamic mechanism, not newly missing V1 research.

## F3 — P2: resident-only data has no explicit fsync transition or acceptance case

**Classification:** missing new consumer integration; no new durability promise
and no observed existing fsync failure asserted.

**Anchors:** issue77–98; README94,117–120,140–146,154–156. README496 names fsync
preservation generally, but the pinned self-contained issue has no fsync route
or explicit resident-only check. Governing rule §5 lines168–170 requires the
actual supported fsync contract to remain; §9 lines301–304 preserves errors and
valid prior state. Released specification lines58–62 separates buffered transfer,
filesystem fsync and canonical/database Commit.

**Counterexample to an incomplete implementation:** acknowledge an ordinary
17-byte write into its newly supported host-owned resident token, call fsync
before any memory-pressure eviction, then encounter a backing-creation/write/sync
error. Reusing only an fd-sync loop with absent lazy FDs treated as clean can
report success without applying the retained filesystem transfer/error contract.
The current `HostOperations` Fsync dispatch to Payload/Index `sync_data` illustrates
the consumer seam; the actual delivered #124 implementation must be reread when
#130 starts, as the issue already requires.

**Smallest repair:** define how explicit fsync/flush reaches relevant resident
state, reserves any necessary backing transition, reports failures and keeps old
readers/current data owned. Reuse the existing explicit filesystem sync path;
add the no-prior-spill case and a transition-error case. Do not add a durable
flush to ordinary mutations, trigger a Commit, silently require the user to
fsync before capture, or invent database/crash durability.

## F4 — P2: the linear scan bound needs a stable-view qualifier

**Classification:** underspecified live cursor mechanism and unqualified cost
bound; not a demonstrated namespace corruption.

**Anchors:** issue191–204 and242–246; README248–261 and299–303. Rules §§2–3
preserve existing live operation ordering and consistent captured roots; §5
requires incremental work. The fixed captured-root case supports the stated
ordered-merge argument.

**Trigger:** consume one live readdir batch, mutate/rename/delete upper entries,
then resume using the old cookie. Keeping the original upper root may change the
promised current-cookie/mutation behavior. Rebinding to the current upper root
requires seeks/revalidation that are absent from the unconditional O(L+K) after
initial-seeks formula; mixing old names with unvalidated new inode state is unsafe.

**Smallest repair:** state O(L+K) for a fixed captured/stable view. Select the live
per-batch root/cookie policy, preserve the existing mutation semantics and add its
actual rebase cost (for example B·H for B reseeks, or an explicitly measured
mutation/revalidation term). The requirement is not to invent a new gate or make
live readdir a whole-Workspace snapshot. Existing issue259–262 tests are a suitable
base, with mutation between batches and cost counters as the focused extension.

## Coverage and attempted falsifications that were rejected

| Governing rule | Relevant issue anchors | Audit outcome |
| --- | --- | --- |
| §1 current state; no Workspace operation history |24–34,119–125,213–218,425–426|Explicitly retained. Superseded state is not made a per-operation journal.|
| §2 non-pausing lifecycle and stable handles |303–346,398–405,419–422|Explicitly retained. A claim that #130 permits a Commit freeze is unsupported. New pager/admission lock scope still needs F2's proof.|
| §3 consistent snapshot; C1/C2; bounded attempt ordering |242–246,273–308,324–376|Explicitly retained. No live-root substitution or new cross-Workspace build lock is authorized.|
| §4 host authority and placement |24–28,127–145,454–458|One host engine is selected; physical services may be shared while fresh mutable identity is preserved. A lazy cache must not become another authority (F2).|
| §5 incremental storage, precise independent ownership, fsync |88–112,191–211,378–394,427–429|Requirements covered, but compact retained-reader ownership needs F1; pager and fsync need F2/F3. Tiny/shared allocations must still preserve the existing4KiB retention and physical-release rules.|
| §6 bounded capture, evictable retained pages |93–98,303–308,421–422|Explicitly retained. The unresolved pre-spill catalog is F2, not permission for bulk conversion or RAM pinning.|
| §9 budgets and supported FUSE visibility |77,392–394,409–429,475–478|V1 inherited as a completed prerequisite, then must be preserved. New cache/dynamic admission still needs exact charge ownership and pressure behavior.|

The following were **not** counted as new contradictions: canonical hardlink
alias/root-domain caveats are stated at issue79–82 and README168–174; deletion
masks survive covered tracking at issue360–365; retry retains the exact candidate
at369–376; physical service reuse does not reset a mutable Workspace at24–28;
and published-page in-place mutation is explicitly excluded at430–431. Replacing
selected dual indexes/cookie/alias layouts already requires an explicit internal
specification revision at99–104. The correctness proof of those mechanisms cannot
be inferred from these statements, but the statements do not authorize violating
the governing rules.

## Is the proposal the simplest implementation?

Not proven. The initial low-intrusion path can remain simple: reuse existing
services with fresh identity, remove measured empty-path construction/admission,
pack within existing ownership boundaries, and defer unused payload backing.
A shared memory-only pager, cross-file logical-node packing, record-inventory
replacement and tiny-source promotion are separate difficult mechanisms. They
should not all become mandatory merely because they occur in an execution list.

Issue114–117 already supplies the right escape hatch: do not require every
exploratory representation if the simpler measured implementation meets the
objective. Apply it expressly to README578–581's pager/packing tranche. For F1,
retaining per-file ownership boundaries first is simpler than a new scoped global
record collector. For F2, preserving the existing disk graph first is simpler
than simultaneously redesigning lifetime, addressing and spill. Measurements can
justify the next change without requiring a second canonical pipeline or a pool
of mutable Workspaces.

These recommendations do not certify an implementation, waive a rule, add a
benchmark threshold or move any mandatory #124/#125 work into #130. The bounded
independent audit is complete; only this report was written.

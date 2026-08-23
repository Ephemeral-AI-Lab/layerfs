# Phase 4 G6 canonical extent-tree research

Status: **RESEARCH/SPECIFICATION COMPLETE / EXECUTION PENDING G5**

Disposition: **`G6_SPEC_READY_PENDING_G5_BASELINE`**

Date: 2026-08-23

This directory answers whether LayerFS should replace its suffix-sensitive
fixed-radix file mapping with one canonical authenticated extent tree and use
one portable resolver for SDK, virtual-filesystem, and native-projection
consumers.

The measured problem is real, and one portable resolver remains the preferred
architecture. The original unconditional algorithmic claim is not yet
supportable:

```text
hard O(log E + local work)
+ deterministic one-raw-byte-content/one-root
+ arbitrary insertion/deletion
```

A conventional persistent B+ tree or rope can provide hard logarithmic
updates, but its topology depends on edit history unless another canonicalizer
is added. A content-defined/prolly tree converges to a history-independent root
and ordinarily localizes changes, but no-cut, every-cut, duplicate, and chosen
boundary streams retain a hard `Theta(E_suffix)` mapping-rebuild case. If raw
FastCDC also fails to rejoin, successful fallback additionally scans the raw
suffix; it is never relabeled logarithmic.

The retained CP-0008 `+1` path inserted one synthetic occurrence; it was not a
raw one-byte FastCDC edit. G6 therefore separates raw byte delta `DeltaB` from
oracle-derived occurrence delta `DeltaE`. Any raw `+/-N` mutation may yield
`DeltaE<0`, `=0`, or `>0`.

The tree itself canonicalizes an ordered occurrence sequence. Raw-byte
one-content/one-root additionally depends on the frozen FastCDC witness; an
arbitrary alternate valid chunk segmentation is not a legal publication
input. Existing legal zero-length occurrences remain identity-bearing mapping
entries in v2 and are not silently reinterpreted. Editable v3 roots require an
exact-root `CanonicalSegmentationWitness`; later migration normalizes through a
complete authenticated FastCDC rebuild, while any lossless occurrence-preserved
conversion would be a distinct read-only profile.

The selected research candidate is therefore narrower and honest:

> A hard-node-bounded, content-defined measured sequence tree over the current
> canonical-v2 `(raw_length, ObjectId)` occurrences, with one portable extent
> resolver, one bounded atomic multi-island `CanonicalReplacement`, expected-
> local updates, bounded resident resources, and an explicit earliest-
> unresolved raw/mapping suffix fallback.

The prior checkpoint disposition `REVISE_EXTENT_TREE_THESIS` is superseded by
this user-authorized variable-size amendment. The design is retained; only the
unsupported hard-logarithmic wording was rejected.

G5 owns terminal `TrustedLocalDev`, position-preserving warm-native behavior,
and honest `FullFallback`. G6 owns arbitrary-size insert/delete,
shorter/longer replacement, multi-splice, dual-coordinate diff, virtual
count-change, TailAppend/TailTruncate, APFS CloneShiftPatch, aligned Linux
routes, explicit fallback, and the magnitude ladder. Under the updated
boundary, even shadow execution waits for honest terminal G5 PASS.

## Documents

| Document | Role | Authority |
|---|---|---|
| [Research and decision](research-and-decision.md) | Current-state audit, alternatives, external sources, architecture decision, migration direction | Evidence-backed research decision |
| [Cost model](cost-model.md) | Analytical topology, metadata, memory, history, read/write amplification, worst cases | Derived/illustrative only unless explicitly marked Observed |
| [G6 candidate specification](../../../implementation-detail/phase-4/g6/g6-canonical-extent-tree-spec.md) | Conditional codec, invariants, resolver, update, concurrency, durability, migration, fault contract | Prospective design; not implemented |
| [Fast benchmark plan](../../../implementation-detail/phase-4/g6/g6-fast-benchmark-plan.md) | Zero-row, `<20 s` shadow/mechanism screen, and `<=150 s` later measured gate | Prospective method; no measured G6 rows |

## Evidence classes

- **Observed**: read directly from source, sealed local evidence, or a primary
  external source.
- **Derived**: arithmetic or complexity consequence with its equation.
- **Estimate**: parameterized illustrative model requiring a shadow or campaign.
- **Inference**: architecture conclusion from observed facts.
- **Unavailable**: not established by the current evidence, with the reason.

No G5 diagnostic number is treated as a sealed G6 control. CP-0008 and CP-0009
remain historical algorithm/workflow evidence. The accepted G4 baseline is a
valid observed protected-operation anchor, with a qualification: G4 v12's
measured terminal remains `REVISE` under its frozen percentage-only rule; the
separate stage PASS used the user-approved sub-1-ms materiality rule and did
not relabel v12. G5-0 H11-v9 is sealed for its narrow history lane. G5-1 v26
and G5-2 v3 remain premeasurement work at this snapshot.

## Snapshot read for this research

Repository and branch:

```text
repository  /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty
branch      codex/empty-worktree
HEAD        d58c5a1307253dfc221fe50de996c183deb9458a
```

The worktree contained active foreign G5 changes. They were read but not
modified. Snapshot hashes used for line-level interpretation were:

```text
canonical_v2.rs                         8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc
content/persistence.rs                  5b7831aa493e84aa77db274c1ac87db70b709a406e8241d7a665c6cefcf287fa
content/mod.rs                          0969881a415f8bd4f4e1574170f8ee869b15145b215fad2c9a86dc0102ad6c9e
cdc/mod.rs                              bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6
cdc/gear.rs                             beb8637ea160f5b61401c0dec2b632927c81be0b491b443142973dc23108edb5
phase4_create_edit_benchmark.rs         596828bda762896511d13b7dea2882ca5e28b3469275f5b0b9d833159cb83e10
phase4_g3_materialization.rs            2b9f197d1dc816f40f02fc10cdeefa0ee12fea3ba6d926aa66a70052120debbb
```

The two benchmark sources are dirty G5 research/product-candidate sources, not
a sealed G6 implementation or control. Line references in these documents are
bound to the hashes above and may drift while the G5 owner continues. The
final research custody snapshot above was taken at `2026-08-23T08:05:56Z`.
The G5-2 source advanced during this audit from the provisional
`47578f2685157774e23299480d2ae8bdb5a43bfa6a06ae3614f57700200109ba`
binding through
`286e7b4ba66d8de7e2ba0e35273ad36cac3a0cab6cff1fad2fd3f7b2d0208353`
to the timestamped snapshot above; its v3 readiness package still reports
`PREMEASUREMENT_REVISE`, so none is a sealed G6 control.

## Selected architecture

```text
Application
    |
    +-- direct SDK
    +-- later FUSE / FSKit / ProjFS adapter
    |
    v
portable extent resolver
    |
    v
canonical bounded content-defined extent tree
    |
    +-- immutable canonical CAS Bytes objects
    +-- SQLite expected-head visible publication
    |
    v
capability-driven physical projection
    +-- virtual read: no native file
    +-- tail append / tail truncate
    +-- APFS whole clone + patch/shift
    +-- Linux range reflink/insert/collapse when supported
    `-- sequential authenticated export fallback
```

Only the tree, resolver, identity validation, and canonical publication are
portable authority. OS cache state, APFS clones, reflinks, native extents,
FUSE modes, FSKit resources, and ProjFS placeholders are adapters or physical
accelerators and never participate in canonical identity.

## Exact compact mutation evidence

The amendment substitutes rows rather than expanding the campaign:

| Stage | Exact population | Added coverage | Total wall |
|---|---:|---|---:|
| Metadata shadow | 43 | Structural occurrence rows plus frozen raw 1/4-KiB, 64-KiB, 1-MiB, tail, net-zero, and atomic mixed manifests | `<20 s` |
| Product screen | 48 | Raw ladder, four virtual cells, five native route cells | `<20 s` |
| Measured gate | 100 | `14 + 14 + 36 + 9 + 24 + 3` | `<=150 s` |

The 100-MiB primary group is exactly 36 rows: six raw one-byte positions with
one pair each, `+/-1 MiB` with three pairs each, and append/truncate/net-zero
with two pairs each. The four virtual and five native route cells are
candidate-only diagnostics; they never manufacture a paired percentage claim.

Claim boundaries remain explicit:

- variable FastCDC chunk sizes/counts and arbitrary streamed replacement size:
  supported by the G6 design;
- bounded atomic multi-island canonical splice: ordinary expected-local,
  adversarial earliest-unresolved raw/mapping suffix fallback;
- virtual/SDK count-changing visibility: zero complete native reconstruction;
- APFS middle insertion/deletion: still `Theta(native suffix + new span)`;
- TailAppend/TailTruncate: shifted suffix zero, but still require exact seed and
  durable native publication;
- Linux extent routes: capability/alignment/preflight dependent;
- FullFallback: correctness route only, never a variable-size fast result;
- cold contiguous export: `Theta(file bytes)`.

## Rejected alternatives

- conventional B+/rope/RRB/finger tree as the canonical identity: efficient
  but history-dependent without a second canonicalization layer;
- piece table or edit overlay as canonical state: history-shaped and requires
  compaction/replay;
- exact Xet 3–9 aggregation: excessive object and metadata count for the local
  LayerFS workload and no hard-local adversarial bound;
- monolithic carrier, pack-first storage, or Git delta chains: physical layout
  variables that do not solve canonical count-change locality;
- separate per-OS canonical maps: duplicate identity and authority;
- projection-only derived tree presented as a canonical Big-O improvement.

## Open questions

1. Does the existing CD32–64 metadata-only candidate reproduce one
   byte-identical root for the same ordered occurrence sequence across fresh
   build and divergent edit histories?
2. What ordinary and adversarial resynchronization spans occur at leaves and
   each parent level?
3. Does adding subtree reference counts keep live/path metadata inside the
   protected limits?
4. For raw rows, which frozen cases yield `DeltaE<0`, `=0`, or `>0`, and does
   work scale with replacement bytes plus unique CDC/tree replay rather than
   base-file suffix?
5. Does the product accept expected locality with a hard suffix worst case, or
   is hard logarithmic edit cost mandatory? The latter would require revising
   raw-byte one-content/one-root or finding a stronger construction.
6. What exact G5 terminal source/executable becomes the G6 adjacent control?

## Eligibility

```text
metadata-only shadow research       specified; waits for terminal G5 PASS
G6 Rust/product implementation      not eligible
G6 measured campaign                not eligible
SDK/VFS/native production work      not eligible
G7/WP5                              not started
```

After terminal G5 PASS, the smallest next step is the `<20 s`, metadata-only
A/B/C shadow in the fast benchmark plan. Passing work then proceeds
sequentially through G6-T canonical tree/resolver, G6-V virtual count-change,
G6-N native variable routes, and one combined protected gate.

The existing Phase-4 top-level indexes are intentionally not edited in this
research checkpoint: they contain active foreign G5 changes, and the governing
scope confines writes to the new G6 paths listed above.

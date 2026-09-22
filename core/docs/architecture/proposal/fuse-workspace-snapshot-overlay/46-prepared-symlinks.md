# Prepared fresh symbolic links

> **Status: implemented and functionally verified for the declared prepared-symlink scope.**
> Implementation parent: `521bcb304c382e53f454eb3eb9007a010c1de486`.
> Product input seal: `edb4b9e3005fb867f7d2cdbe66c33fa7a51e19840b6a899a635ea3e5f7ed27b1`.

This round adds new_symlink_serials (S) to the existing direct prepared filesystem
operation and C5 PreparedChanges. Content and metadata are saved separately through
their existing operations. It introduces no allocator, Workspace symlink mutation
or kernel SYMLINK callback. Existing Workspace requests supply an empty S list.

## Selected admission and encoding

S is strictly sorted, unique, positive, nonroot, within signed63-bit IDs and a kind3
subset of final inode rows I. Fresh-file IDs F remain restricted to kind1. Existing
I versus new-directory N/existing-directory-patch P exclusions remain, with
I+N+P<=128 and F+S<=I. Changed names remain bounded by128 and the complete request
by32768 bytes.

S=0 preserves the existing omitted, v1 and v2 bytes. Nonempty S uses a v3 trailer:
version3, u16 N and24N bytes, u16 P and24P bytes, u16 F and8F bytes, u16 S and8S
bytes. Its size is9+24(N+P)+8(F+S); v3 with S=0 is invalid, but F=0 is permitted.
The decoder checks S<=I-F and available input before allocation. Direct and C5
headers remain103 and195/228 bytes respectively.

## Shared Service and C1 ownership

Declared fresh N/F/S identities must be absent from the base; undeclared missing
identities remain invalid. Fresh F/S values begin with reference count0. C1 derives
the final count from retained name bindings: a fresh symlink must have exactly one
binding. Unbound or duplicate-bound new symlinks fail. A symlink cannot be used as
a directory parent. Its target remains opaque, so dangling/self-referential target
bytes are distinct from namespace directory cycles.

Direct updates reuse the existing symlink content/portable-metadata role validator
for new S rows. C5 already validates every I row and needs no duplicate role pass.
The existing sorted new-inode union becomes N/F/S; the shared filesystem builder
still owns canonical topology and reference counts.

## Actual proof and qualifications

Both native selectors passed on their first attempts, with all nine selected checks:

| Selection | Status | Driver seconds | Complete command seconds |
| --- | --- | ---: | ---: |
| symlinks | PASS | 2.903401625 | 3.007597208 |
| refusals | PASS | 1.508807000 | 1.603271917 |

The healthy selector reserves nine IDs once and assigns distinct triples to direct
candidate save, Stage/CommitStaged and composite Commit. Each triple covers empty,
dangling opaque and literal self targets. Four distinct target objects are saved
because the composite self target is exactly next-self, matching its final name.
Readlink bytes, kind3/mode0777 metadata, timestamps and reference count1 are checked.
The old roots remain readable and exactly two workload Commits occur. A subsequent
independent reservation proves the expected consumed range; there is no hidden
constructor reservation or fixture-based identity bootstrap.

The refusal selector records39 attempts:13 variants across direct, Stage and
composite routes. It covers zero/wrong-kind/excess S, undeclared/missing/existing
identities, wrong content/metadata roles, wrong scope, unbound symlinks, duplicate
bindings and symlink parents. Local NOT_SUBMITTED shape refusals are distinguished
from authenticated Service failures. Every attempt preserves the old namespace,
Branch/history and an acknowledged prior Stage, which is explicitly discarded
afterward. The allocator probe confirms no additional range was consumed.
Base absence is still not reservation-provenance enforcement; that guarantee is
not inferred from a fresh declaration or these callers' correct reservations.

Three Bridge Rust tests check manual legacy/v3 byte expectations on all three
routes, mixed N/P/F/S, v3 F0, malformed counts before allocation, truncated input,
and the exact32768-byte envelope/+1 refusal. Two direct Service tests cover the
same semantic routes and refusals. The healthy Service test additionally reserves
two IDs after its original nine-ID run and one-ID probe, and submits fresh F1+S1
in one direct update. Exact regular-file bytes, symlink Readlink, metadata and
reference counts are checked while Branch/history and the prior root stay unchanged.
That extra direct subcase does not change the native nine-ID workload.

All native input Stores are independent byte copies of the closed master, with
fresh live C5 authority. Clients and Service exit normally, the closed master is
unchanged, and no fixture was regenerated. Complete commands use60-second budgets
and one construction worker. These host-native cases imply no Docker CPU/RSS
isolation. The [functional index](evidence/prepared-symlinks/functional-index.json.gz),
[caller review](evidence/prepared-symlinks/caller-review.json.gz) and
[archive manifest](evidence/prepared-symlinks/archive-manifest.json) retain exact
identities, commands, all client/Service logs, source candidates and cleanup.
No passing sample was rerun. No performance or cold-cache result is claimed.

Locked/offline Rust1.85.1 checks passed: host702 tests/3 ignored, Linux700 tests/149
ignored, both all-target Clippy commands with warnings denied, fmt, host binaries/
examples, host/Linux product checks, the254-file boundary guard and six guard tests.
The [check index](evidence/prepared-symlinks/checks-index.json.gz) records commands.
Docker checks used the pinned image, two CPUs and Cargo jobs2; checks/builds/samples
were serial. CI and the retired aggregate preflight did not run.

## Resources and production source

[Linux compiler layouts](evidence/prepared-symlinks/layout/layout-01.json.gz) show
PreparedChanges320->344, Operation328->352 and Request368->392 bytes; Response112
is unchanged. All alignments are8. S adds one24-byte owned Vec header; the combined
F+S element capacity remains at most128 serials/1024 bytes, with decode enforcing
that bound before S allocation. The merged N/F/S new-inode list remains at most128
IDs, and S does not add another inode row or increase the existing serial-lookup
scratch bound. Service role validation processes one row at a time. These are
source and fixed-layout bounds, not allocator/RSS/cgroup or stack-peak claims.

Production LOC: **112510 ->112586 (delta +76)**; core47093->47169 (+76),
reference65417->65417 (+0). The comparison uses exact first-parent/final-staged
snapshots and unchanged tools/production_loc.py, blob
b5b9617d08204977176302311e0b2c72a811b420, with identical product-only source scope
and exclusions. The [complete inventory](evidence/prepared-symlinks/source-loc.json)
retains254 core and193 reference files. No C1 algorithm or legacy implementation
was replaced. Thirteen existing external test/example files only received empty
or forwarded S fields and the obsolete unknown-version3 oracle changed to4.

Native/kernel SYMLINK, larger full-input admission, the unchanged prepared DSH
upload followed by one explicit full Commit, later incremental Commits, hard memory
qualification and matched R6 remain open. The Round43 capacity failure is retained.

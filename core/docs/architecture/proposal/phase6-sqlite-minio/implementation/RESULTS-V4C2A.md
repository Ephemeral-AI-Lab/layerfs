# V4c2a immutable file-graph certificate result

> Status: Declared small-file real-provider dependency gate passed.
> Full V4c, namespace locality and larger/resource/canonical qualification open.

Specification source `57f032fba8d14fb4d2c8f3d7dbdabbb3fe074511`; runtime
[`d11dda4a97241ac1adb2f94d3e4bb3f72d58e050`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/d11dda4a97241ac1adb2f94d3e4bb3f72d58e050).
[Prospective specification](V4C2A-SPEC.md), [actual identities/receipts](v4c2a-certificate-treatment/),
[external check evidence](v4c2a-file-checks/). Experimental runtime only.

## Actual construction/storage/publication and proof

Ordinary public WorkspaceApi commands run through real kernel FUSE/SQLite3.53.2,
C1 immediate-base construction and C2 packing/MinIO immutable ACK/locator
registration. Global SQLite facts are derived from authenticated canonical bytes
in the same actual locator transaction; no SQL engine is created per Commit.
Existing MEMORY journal/syncOFF/tempMEMORY/cache2MiB, single producer, transport/
object windows,15s child/9.5s separate proof and finite512profile unchanged.

Audit now uses actual file certificates instead of rereading the entire file
graph, while full namespace/portable validation and conditional C5 publication
remain. This gate's two selected files are WholeFile roots: each certificate
visits1new node/0edges/reused0/newly-certified1. Larger chunked graph branches are
covered by external fixtures, not yet by a large-file real-provider gate. Actual
post-daemon proof still rereads both versions from MinIO and checks literal bytes,
mode644, successive C5 head/parent and cleanup.

| Case | Exec | Commit | Dirty inodes/directories/names | Local replacement source bytes |
|---|---:|---:|---|---:|
| 4KiB create |17.004833ms|74.294375ms|2/1/1|4096|
| 4KiB overwrite |17.547584ms|58.109042ms|1/0/0|7|

Child wall6.377585208014352s; SDK lifecycle interval6.062756041s; separate proof
19.570875ms; manual genesis7.621042ms (not public SDK Init). Semantic/heads/parent/
handles0/unmount/delete/provider drain PASS; independent canonical NOT_RUN;
physical resources NOT_RUN. Cache **INELIGIBLE: OS/page cache unknown**. One sample;
no unchanged-arm resampling. Idle session Deadline remains in raw evidence.

Authority GETs first27 and second52: interval25, versus V4c1first28/second54,
interval26. One full-file GET per Commit removed in this small case; all other
namespace/metadata/registration work remains. Timings are essentially unchanged
from V4c1create74.806042ms/overwrite57.480875ms. No speed gain/admission claimed.

Daemon interval counts unchanged: overwrite3PUT/12GET,1328PUT requested/8018GET
received bytes (includes Exec since preceding log, not isolated physical Commit
I/O). Three new packs,7objects,3duplicates, pending10objects/5035canonical bytes.
One metadata connection,11overwrite locator lookups+3registrations+1publication.
C1 namespace/dirty/reference counters remain exact V4c1one-inode/zero-directory
and touched/scanned1. Engine replacement counts exclude C1 base/page reads.

## Typed proof and retained obligations

File facts validate actual C1 root/role/framing/EOF, child summary, source slice,
checked mapping level and root/non-root partition. Only reachable new nodes are
certified after all children pass; exact previously certified immutable summaries
reuse without whole payload reads. Indexed child ordinals and maximum34frames
avoid a resident graph/whole frontier or historical scan. Statement cache uses the
existing SQL connection. Unreachable objects/failing parents are not certified or
adopted; accepted objects remain in custody. Provider acknowledged immutability/
retention is explicit; no durability, GC or stronger credential-confinement claim.

External actual-C1/SQLite fixtures4PASS: deferred missing child then registration,
short payload slice, wrong root/role/trailing EOF, forged child summary and short
nonroot fill, plus new graph reuse64certified payload references/2new nodes.
Cached root visits1/reuses1/edges0. These are correctness/work checks, not a
large-file provider, independent canonical or physical resource proof.

Namespace names/aliases/cycles/references need their own exact-base transition
certificate; file authentication/certification cannot replace it. Complete namespace
walk, repeated C1effective-cycle work, parent index/import/inherited mount, source
retirement and total-population admission remain gates. All named67/270/128,
actual seven families, complete DeepSeek import/generic commands/locality and
canonical/physical/Unknown/concurrency proofs remain open. No #288/release claim.

Next: V4c2b incremental namespace transition architecture against certified exact
base, streaming name/value facts with real sorted COW/refcounts/parent proof and
READY/C5/installation composition; freeze interfaces before validation removal.

## Commands and source size

Single frozen affected run from clean published owned worktree:

```sh
python3 core/benchmark/phase6-live/run.py \
  --output benchmark-results/phase6-integration/v4c2a-certificate-treatment \
  --minio /Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs/benchmark-results/storage-probes/deepseek-full-import-v1/minio-provider/minio \
  --image sha256:c199c3a888765d2f19f14fd5bcf85974fdc3133a0b8aa7e0e683662791cd9c97 \
  --driver benchmark-results/phase6-integration/binary-archive/895ecbbd440b6ff504825534a71c7faa9656a530b24e5bb093c1e8a39edecb31/phase6-live-probe \
  --purpose 'V4c2a typed file-graph certification treatment; one affected sample; cache INELIGIBLE; full namespace validation retained'
```

Cargo+1.85.1 --locked using `--manifest-path core/benchmark/phase6-live/Cargo.toml`:
`test --test file_facts`4PASS; `clippy --all-targets -- -D warnings`PASS;
`fmt --check`PASS; native `build --release`PASS; `zigbuild --release --target
aarch64-unknown-linux-musl`PASS. Initial private-module import corrected via public
reexport; raw failure retained. Test rerun only after actual cached-statement change.
Prior unchanged construction/engine/session/packing checks reused by scope; full
unchanged Core suites/examples and Linux Clippy unrun. No CI/preflight.

Spec and runtime commits each combined135696->135696(+0), reference65417->65417(+0),
Core70279->70279(+0). Counter tools/production_loc.py SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, same Git archive
first-parent/final-staged snapshots with committed-tree confirmation. Tooling/
tests/docs excluded consistently; no shipping product reduction. The result-doc
commit gets its own exact count in its message and issue checkpoint.

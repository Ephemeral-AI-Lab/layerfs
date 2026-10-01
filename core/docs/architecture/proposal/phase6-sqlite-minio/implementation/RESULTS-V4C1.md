# V4c1 indexed immediate-base construction result

> Status: Research correctness dependency complete for the declared two-head gate.
> V4c/full integration and speed/resource/canonical qualification remain open.

Runtime parent `05d4b568ccd8c00a2314b23ed65004b8710a96d2` to
[`1ca095a0e84167a7657445726067ca6f77169618`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/1ca095a0e84167a7657445726067ca6f77169618).
[Prospective scope](V4C1-SPEC.md), [raw identities and receipts](v4c1-indexed-treatment/),
[external checks](v4c1-construction-checks/). Experimental runtime only; no shipped
product change, no larger population profile and no benchmark/release admission.

## Real path and observations

Ordinary commands invoked through actual public WorkspaceApi, kernel FUSE,
SQLite3.53.2 metadata/source files, public C1 construction, actual C2 packing,
immutable local MinIO ACK/locator registration, complete authority audit and
conditional C5 publication. Proof after explicit unmount/delete reads both retained
versions from actual storage and checks literal bytes/mode644, C5 head/parent and
cleanup. One producer and existing15s child/9.5s separate proof limits unchanged.
MEMORY journal/syncOFF/tempMEMORY/cache2MiB remains the actual SQL profile.

| Case | Exec | Commit | Prepared inodes/directories/new | Name changes | File edit rows | Engine construction logical local/base bytes |
|---|---:|---:|---|---:|---:|---|
| 4KiB create |17.426792ms|74.806042ms|2/1/1|1|0|4096/0|
| 4KiB overwrite |17.600875ms|57.480875ms|1/0/0|0|6|7/0|

The six file edit rows are six final one-byte source spans from the generic
`dd bs=1` overwrite, not recursive history. The comparison/construction reads
7 replacement bytes. This engine source counter excludes C1's authenticated
base/page reads; it is neither physical I/O nor a complete Commit resource count.
C1 overwrite directory work is0; inode change keys1; reference touched/scanned1;
base records1. It actually retains the unaffected directory C1 root.

Complete child wall6.8697957919794135s; driver SDK lifecycle interval6.081842708s;
separate proof20.525584ms; manual genesis6.750916ms. Genesis is not public SDK Init.
Semantic proof and cleanup PASS; canonical reference NOT_RUN; physical resources
NOT_RUN. Cache **INELIGIBLE: unknown OS/page cache** for both rows. No repeat sample.

Packing first/new:15 objects,3packs,1647bytes,3duplicates, pending peak18objects/
5674canonical bytes. Overwrite/new:7objects,3packs,1328bytes,3duplicates, pending
peak10objects/5035canonical bytes. One metadata connection; overwrite delta
15calls (11locator batch lookups,3registrations,1publication). Daemon S3 overwrite
interval3PUTs/12GETs,1328PUT requested/8018GET received bytes. Counts include Exec
reads since the previous log; they are not clean Commit-only physical I/O counts.
Authority GET count28->54 (26), still full candidate validation plus registration.
Raw idle metadata Deadline and control close Io observations are retained; proof,
publication and explicit handles0/unmount/delete/provider drain succeed.

## Interpretation and remaining gates

Small-case Commit timing is higher than the preceding packed diagnostic
(V4b create44.687959ms/overwrite41.091458ms). This source adds actual base-backed
reads/COW while the full authority walk remains. The count evidence establishes
changed-row construction; it does not establish a speed improvement. Repeated
base metadata locator/object acquisition is visible in the11overwrite lookups.
Prospective, admitted operation-local read windows and incremental validation
must be evaluated from source/counts, without unchanged-arm resampling.

External real-SQLite checks pass: immediate overlay/truncate/regrow/ordinal EOF,
name deletion/unrelated inode identity,1MiB localized edit/old root preservation,
and large-to-small/small-to-large representation transitions. The large edit
serves under4KiB local replacement data. Fixture canonical objects are in-memory,
not a real-provider large-file speed/resource proof. Exact independent canonical
vectors, physical containment/healthy progress, concurrent captures, failure/
Unknown matrix and retirement remain unqualified.

V4c2 must remove the host's full candidate walk by actual incremental namespace
certification, repair repeated effective-cycle subtree work, implement admitted
retirement, paged population/import and inherited mount. Existing total512/serial
512/handles256 and operation512changed-inode/name-per-directory/edit admissions
remain explicit supported limitations. No global-population or arbitrary-size
claim. All three named cases, actual seven families and complete DeepSeek generic
commands/locality remain open. No #288 qualification is claimed.

## Reproduction and source checks

Run exactly once from the clean published owned worktree using archived sealed
host/Linux binaries and image `sha256:0fe60544a3b770a8eac9243104e180986fe8587b314d2a844053d55f793f93c8`:

```sh
python3 core/benchmark/phase6-live/run.py \
  --output benchmark-results/phase6-integration/v4c1-indexed-treatment \
  --minio /Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs/benchmark-results/storage-probes/deepseek-full-import-v1/minio-provider/minio \
  --image sha256:0fe60544a3b770a8eac9243104e180986fe8587b314d2a844053d55f793f93c8 \
  --driver benchmark-results/phase6-integration/binary-archive/d797cea0f535e9933f3fd21ad48d34707cb25e6932a04a35d395fd6b54b46180/phase6-live-probe \
  --purpose 'V4c1 indexed dirty construction immediate-base treatment; one affected sample; cache INELIGIBLE; full authority audit retained'
```

Exact Cargo+1.85.1 locked commands use `--manifest-path core/benchmark/phase6-live/Cargo.toml`:
`test --test construction --test engine`6PASS; `clippy --all-targets -- -D warnings`
PASS; `fmt --check`PASS; `build --release`PASS; `zigbuild --release --target
aarch64-unknown-linux-musl`PASS. Root ARMv8 build config is pinned in identity.
Initial compile/Clippy corrections are recorded in the append-only log and available
raw check evidence. Unchanged session/packing suites retained from V4a/V4b; full
unchanged Core tests/examples and Linux Clippy not run. No CI/preflight run.

## Production LOC

Runtime commit first-parent/staged/committed tree comparison:
combined135696->135696(+0), reference65417->65417(+0), Core70279->70279(+0).
Counter tools/production_loc.py SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, same Git archive
snapshot method and product classification. Experimental runtime/tests/docs are
tooling excluded on both sides, not a zero-sized product or product LOC reduction.
The result/checklist documentation commit records its own exact comparison in its
commit message and issue checkpoint; no future self-referential SHA is invented.

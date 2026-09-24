# #241 frozen functional positions

This is the prospective **functional** position selection, frozen before any
mounted position check. It is not a latency or cache-admission campaign. The
complete [manifest](position-manifest-v1.tsv) has SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`:
192 pinned SHA-256 band offsets and 72 named edges, 264 cases total. The
[bands-only precursor](position-bands-v1.tsv) has SHA-256
`653a9af543db01b26a114b2455c982171784e57fee80b5000d023de7f57b3fb1`.
The final manifest includes the same 192 rows unchanged.
The [exploratory mounted position receipts](evidence/position-sweep/README.md)
record four historical FAILs and one later single-case PASS; the full sweep
remains unrun.

The 500 MiB case starts from the **524,283,904-byte capped pristine input**.
All other sizes and the fixture SHA-256, canonical file root, and extent count
match the pinned #232 recipe in `core/benchmark/fs-bench-pro/shared/edit_contract.py`.
The test-only [boundary receipt](position-boundaries-v1.tsv) has SHA-256
`04d755bafe54965fdc7f013ea908d9234100cb1568d144c2be7b162d71933a01`.
The receipt was derived from the pinned splitmix64 source bytes and Core's
frozen `build_streaming`/FastCdc implementation. The derivation checked each
source file's full SHA-256, canonical file root and extent count against the
#232 constants before accepting a boundary. This identifies the source bytes;
the live sweep must still use an independent private copy of a validated closed
prepared master. No position case had run when this manifest was frozen.

For each size, the selected actual pristine canonical-chunk boundary is the
interior boundary nearest `floor(N/2)`, breaking a tie toward the lower byte
offset. The selected boundaries are 532,242; 5,250,591; 52,435,851; and
262,138,987 bytes for 1, 10, 100 and capped 500 MiB. Each operation adds
`boundary-1`, `boundary`, and `boundary+1`. Its other named edges are offset
zero, `floor(max_legal_offset/2)`, and the last legal offset, where
`max_legal_offset=N` for insert and `N-4096` for overwrite/delete. The 192
bands use exactly the UTF-8 seed, big-endian SHA-256 integer, and integer band
formula in [SPEC.md](SPEC.md). No offset is aligned or adjusted after results.

The [generator](../../../crates/layerfs-fuse/tests/position_manifest.py) makes
new output with exclusive create. Its reproducibility commands are:

```sh
python3 core/crates/layerfs-fuse/tests/position_manifest.py \
  --out /fresh/path/position-bands-v1.tsv
python3 core/crates/layerfs-fuse/tests/position_manifest.py \
  --boundaries core/docs/issues/241/position-boundaries-v1.tsv \
  --out /fresh/path/position-manifest-v1.tsv
shasum -a 256 /fresh/path/position-manifest-v1.tsv
```

The SDK [functional harness](../../../crates/layerfs-api/sdk/tests/range_position_sweep.rs)
accepts one size (`LAYERFS_POSITION_SIZE=1mib`, `10mib`, `100mib`, or
`500mib-capped`) and optionally one exact `LAYERFS_POSITION_CASE_ID`. It
requires `LAYERFS_POSITION_SOURCE_DIR` containing `<size>.bin` from the pinned
recipe, an immutable `LAYERFS_TEST_IMAGE` carrying the frozen `splice` tool,
a closed, atomically published v3 `master.json` in
`LAYERFS_POSITION_MASTER_RECEIPT`, its declared full file SHA-256 in
`LAYERFS_POSITION_MASTER_SHA256`, the matching
`LAYERFS_HISTORY_CURSOR_KEY`, and a fresh `LAYERFS_POSITION_OUTPUT` directory.
The [test-only copier](../../../crates/layerfs-fuse/tests/position_master.py)
requires the `core-fs-bench-pro-exec-fuse-edit-master-v3` receipt, exactly
`master.json`, `store.sqlite`, and `history.sqlite` in its published directory,
the fixture identity, compatibility-key digest, completed Init, producer
identity, Store/history sizes and SHA-256. It makes one
independent writable byte copy of the closed Store and history per size
invocation and verifies copy digests. The harness opens that copy through the
SDK, forks fresh sibling Branches from the genesis Layer, and creates a fresh
SDK Sandbox for every mounted Workspace view, since one Sandbox admits one
selected Workspace until deletion. Each view records its unmount, SDK delete,
and Sandbox-list absence separately. It creates a baseline Commit for a retained
old-version check and invokes the shared public SDK
Exec/Commit gate once per case. The independent C1/Store oracle streams a
full SHA-256 from the copied Store once for the pristine genesis and for each
new Commit; it compares final size and full bytes to a separate byte-range
model. Each retained old Commit must point to the pinned pristine
content-addressed file root with its length/extent count, so its full payload
is not rehashed for every position. The oracle validates the new C1 file root and extent mapping,
requires a changed root, and records the observed root/count. A localized
chunked edit deliberately retains old chunk boundaries, so its root/count
are not compared with a fresh full-file construction. Public SDK Exec
checks three bounded mounted windows (head, edit seam, tail), `stat` size and
EOF on current, fresh, historical, and pristine-source mounts. Each window
uses a block-positioned `dd bs=4096` for at most two file blocks; byte-granular
trimming happens in the pipe. EOF reads at most one block. The splice
tool itself confirms same-FD `fstat`, STATE and seam readback before its PASS
output. This avoids a 500 MiB no-output FUSE scan in the Exec path. The
harness also checks the published Branch head, route counters and Sandbox
deletion. The oracle invokes only C1/C5 read operations; Core `Store::open`
itself opens the private SQLite copy with its normal read-write connection
profile. No Store mutation API is called by the verifier.

Per-case receipts are exclusive-create TSV files and a failure stops later cases with explicit
`NOT_RUN` receipts. On failure the harness captures available daemon logs with
a read-only Docker observation before SDK Sandbox deletion; it records both a
primary case error and any unmount error. The private Store/history remain for
diagnosis. Ordinary OS cache may be warm; the harness makes no speed
claim. To run all positions, use four **fresh** output directories and one
invocation per size. A one-case smoke uses a case ID from the manifest, such as
`1mib-delete-band00`.

The test-only boundary derivation is opt-in. Set
`LAYERFS_POSITION_SOURCE_DIR` and a fresh
`LAYERFS_POSITION_BOUNDARIES_OUT`, then run
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-sdk
--test range_position_sweep derive_validated_pristine_chunk_boundaries`.
The source directory and output path should be absolute because Cargo sets the
test process working directory to the package directory. The live sweep remains
unrun until the complete source/image identities and output destination are
declared for its own attempt.

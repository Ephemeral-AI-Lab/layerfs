# Full deepseek-harness MinIO + SQLite import

Status: Research; informative and not a product contract.

All captured files were constructed, uploaded and published in the private
SQLite catalog. All file bytes and metadata were ultimately checked. The original
10s exhaustive proof command timed out; subsequent count-driven remainder proofs
completed coverage without repeating the speed arm or changing that timeout.
Numerical results remain cache INELIGIBLE/performance_claim=false. This is a
standalone backend prototype, not SDK Init, canonical filesystem/Commit history,
Workspace bootstrap or release admission.

## Sources and operation

Original construction/upload source:
b76808dee4d78312911abdd4443137f0e4dcb651. Prospective
[contract](EXPERIMENT-DEEPSEEK-FULL-V1.md) was committed at
73d940613d82320e358b9a8dbf1c4848cf979578 before implementation/measurement.
Original receipts and failed proof were preserved at
22974d122e91f3364dcb751d4a5a485beff9555a, with the
[remainder diagnostic contract](DEEPSEEK-RECONSTRUCTION-DIAGNOSTIC-V1.md).
Remainder reader/runner source:
2951bd3300d3ce87c96c6d5bb33dae479277147d. Exact binary/config/tool/provider hashes
are in [original identity](evidence-deepseek-full-v1/source.json),
[build](evidence-deepseek-full-v1/build.json) and
[diagnostic identity](evidence-deepseek-full-v1/diagnostic/identity.json).

Owned branch `codex/issue290-storage-probes`; worktree
`/Users/yifanxu/.codex/worktrees/issue290-storage-probes/layerfs`.
Input `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness` was never edited.
Everything captured: 103,108 regular files, 16,868 directories including root,
10,070 symlinks, 130,046 opaque xattrs and 3,475,776,149 regular-file bytes.
No .git, hidden, ignored or generated-file exclusions. Symlinks were not followed.
No hardlink aliases occurred. Closed independent master acquisition took51.100994s;
manifest SHA256541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af.
Per-file stable reads were checked; no atomic live multi-file snapshot or OS
ACL/ownership-restoration guarantee is claimed.

One construction producer used public C1 construct_stream/default cutoff128KiB,
real CDC8–32KiB for large files and public C2 FULL compression/pack construction.
No delta bases apply to this fresh import. One persistent private SQLite catalog
holds entries/xattrs, file roots, exact indexed CAS descriptors and physical
locators. It is not recreated per file. Prepared statements are reused;
transactions split at256 emitted objects or256 file-root updates, staging
MEMORY/OFF/cache512KiB/mmap0. Actual Rust-linked SQLite was3.51.0. It performs no
WAL writes; Python's verified unchanged official SQLite3.51.3 performs final
WAL/FULL/fullfsync publication. The reader is query-only after final checkpoint.
Native isolated loopback MinIO binary SHA256
b107901fd1afe7b36165c6aa66bb027f96e2ec2b690eebe8c9376746d9a9b0df,
compression off, four HTTP upload connections, eight outstanding-task ceiling,
actual256KiB pack/body ceiling. No retries, daemon, sandbox or mounted filesystem.

## Observed costs

All numbers are one attempt. No cold/physical-cache qualification, no matched
fs-bench-pro comparison and no speed admission. Clone capture and earlier writes
may influence OS residency; cache state is unobserved. RSS or lifetime peaks do
not substitute for physical memory proof. Preparation is paid and visible here,
not hidden inside the upload-only throughput number.

| Stage | Actual wall | Outcome and scope |
| --- | ---: | --- |
| Captured independent master |51.100994s|Setup acquired once; not speed arm|
| C1/C2 preparation complete command |34.543122s|COMPLETE within180s diagnostic bound|
| Constructor internal scope |34.454396s|103,108 files;96.207MiB/s original bytes;2,992.59 files/s|
| Upload + catalog complete command |4.635968s|COMPLETE within declared25s exception|
| Pack transfer internal scope |4.330214s|5,836 ACKs;1,389,188,326 encoded bytes;305.951MiB/s|
| Catalog finalization internal scope |42.286ms|WAL/FULL/fullfsync; READY after all ACKs; final TRUNCATE [0,0,0]|
| Original exhaustive proof command |10.008257s|TIMEOUT; remains failed|
| Original proof download scope |3.783823s|All5,836 exact pack bodies compared and retained|
| Remainder correctness diagnostic |22 commands, total19.244478s|max2.073775s, each <=10s; no performance sample|
| Owned provider cleanup |0.129472s|PASS; provider exited; imported content retained|

Preparation complete command plus upload complete command is39.179090s, an
informative sum of separate scopes. It excludes master acquisition, provider
startup, prepared catalog seal hashing and verification; it is not a measured
full SDK pipeline wall. Logical-equivalent pack-transfer rate765.495MiB/s comes
from deduplication/compression and is not raw network/storage throughput.

The constructor emitted255,544 canonical demands:210,332 unique objects and
45,212 duplicates. Membership is one indexed query per emission, not a historical
population scan. Unique roles:87,696 whole files;119,353 chunks;1,973 extent leaves;
95 extent branches;1,215 file states. Locators update once per unique object;
file roots update103,108 rows;5,836 pack rows are inserted. These operation counts
follow the single exact source path and recorded emission totals; per-operation
CPU/I/O attribution was not instrumented. This run does not establish an empirical
scaling curve or attribute34.5s to SQLite, chunking, compression or source I/O alone.

Encoded pack data is39.9677% of original file bytes, a60.0323% reduction combining
compression and content deduplication. Average pack width238,037.753 bytes. Private
catalog is93,188,096 bytes, including the complete copied opaque metadata.
The counted retained logical files total7,920,713,834 bytes against16GiB admission;
physical block allocation, SQLite internal memory and OS cache remain unobserved.
[Retention counts/seal](evidence-deepseek-full-v1/retained.json).

## Correctness and the failed gate

Original10s proof downloaded every pack and compared exact bytes against the
sealed prepared originals. Full bidirectional entries/xattrs SQL comparisons
passed. Public C2 decoding authenticated each demanded canonical object, public
C1 bounded reads reconstructed file streams, and the independent comparator
checked original fixture bytes, size and exact EOF. Its retained checkpoint is
13,312 files and1,166,388,570 bytes; no later uncheckpointed progress was credited.
See [failed command](evidence-deepseek-full-v1/verification-command.json),
[download proof](evidence-deepseek-full-v1/download.json) and
[retained progress](evidence-deepseek-full-v1/stage/reconstruction-progress.json).

The prospective diagnostic reused that exact pack/metadata/prefix proof, pinned
catalog/master/binary/receipt hashes, then verified the remaining89,796 files in22
keyset batches. Every batch used the same public decoder/reader and independent
original-byte comparator. Cursors remain private; public receipts carry counts
and cursor hashes. Combined coverage is exactly103,108 files and3,475,776,149 bytes.
[Aggregate proof](evidence-deepseek-full-v1/diagnostic/aggregate.json) links the
batch counts; per-batch commands/logs/proofs are alongside it. This establishes
exhaustive content correctness through reused prefix plus bounded remainder;
it does not change the original10s timeout into PASS.

Diagnostic rootv1 was rejected at clean-source admission before any proof command:
one owned CLI route differed from HEAD. Its exact committed line was restored,
the abort retained and the admitted run used fresh rootv2. No speed sample or
proof batch was repeated. [Admission abort](evidence-deepseek-full-v1/diagnostic/admission-abort.json).

## Checks, limits and deliverables

Commands and logs are in [checks](evidence-deepseek-full-v1/checks/selftest.json)
and [diagnostic checks](evidence-deepseek-full-v1/diagnostic/checks/selftest-batch.json):

- `cargo +1.85.1 build --manifest-path core/Cargo.toml --locked --release -p layerfs-storage --example minio_repository_probe`: passed at original and remainder source.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --release -p layerfs-storage --example minio_repository_probe -- -D warnings`: passed at each changed source.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check`: passed. Initial virtual-manifest command without --all found no targets, retained as failed invocation. Initial compile rejected unsigned SQL conversion types; corrected with checked signed conversions, then covering build passed. Initial formatting diff is preserved byte-for-byte, including its trailing EOF blank line.
- `python3 core/tools/check_product_boundary.py`: PASS,272 production Rust/SQL files.
- `python3 -m unittest discover -s core/tools -p 'test_*.py'`:7 tests passed.
- Python compilation checks passed. External real-MinIO selftest passed empty/small/chunked/dedup files, opaque xattr/symlink metadata, exact request/download bytes, READY and decoded bytes/EOF. New bounded selector checked once against the retained selftest fixture and passed.
- Full Core workspace tests/examples and Linux/FUSE/provider containment were not rerun: product source is unchanged;228 owning storage tests from the previous v2 handoff remain their original-source evidence, not new-source proof. No CI or retired preflight ran. No #288 campaign, repeat speed arm, power-loss test, resource/peak proof or cloud-owner/fencing test.

All corpus data, private credentials/catalog and actual MinIO storage remain local:
`benchmark-results/storage-probes/deepseek-full-import-v1/` under the owned
worktree. MinIO process is stopped; data is retained. `stage/catalog.sqlite`
is READY; `stage/packs/` and `minio-provider/data/` hold the constructed/uploaded
packs; `downloaded-packs/` holds the exact checked downloads. Captured master is
`benchmark-results/storage-probes/deepseek-full-master-v2/`. No real namespace
root, layer stack, branch, mounted bootstrap, ACL application or daemon integration
was implemented. A later bootstrap must preserve these metadata/data proofs and
measure its actual restore/lookup path separately.

Per-commit production LOC for b76808dee,22974d122 and2951bd330 is unchanged:
combined117,426 ->117,426 (+0); reference65,417 ->65,417 (+0);
Core52,009 ->52,009 (+0). Same tools/production_loc.py SHA256
c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb,
Git archive first-parent/final staged trees, committed tree confirmation; shipped
SQL included, tests/examples/tools/docs excluded. This report/evidence-only
checkpoint likewise changes no product source; exact comparison is in its commit.

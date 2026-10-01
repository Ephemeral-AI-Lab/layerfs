# JuiceFS DeepSeek repository upload — 2026-10-01

Status: Dated planning checkpoint; not release evidence or a product contract.

Experiment [#292](https://github.com/Ephemeral-AI-Lab/layerfs/issues/292).
Disposition: **TIMEOUT / PARTIAL**. The full public repository upload was attempted
once and did not finish within its frozen25s command limit. Partial data is
retained. Full-corpus correctness and storage totals remain unavailable. All
numerical observations are cache INELIGIBLE, performance_claim=false.

## Source, acquisition and operation

The requested unmodified [fork](https://github.com/Ephemeral-AI-Lab/juicefs) exists.
Its clean local clone is `/Users/yifanxu/Ephemeral-AI-Lab/juicefs`, at upstream
main `adcca1cc61bb4d668a945d64b2e176b44ac8e5b5`. The tested executable is official
JuiceFS Community1.4.1, source release `0b90c7db5a929ae6adc5faad948d108efd2c99f9`,
not a build of unreleased main. Archive SHA256:
`e77739762a94e6ad90d27a97027de214d63516828e90fac86221c88850b82b08`;
binary SHA256 `27b40fe76522ae0b35887a7e7642289de4324a5f974274efa527c1b12312a226`.
No third-party code, dependency or Go source was patched. No Go build was needed.

The [prospective contract](EXPERIMENT-V1.md) was published at
`8012c934c6c86cd62bd75399a21ab5d450e392bd` before execution; runner and measurement
source `a8ed76ea990b33da45e3066643c74da6a43549df` was committed clean before the
sample. Owned worktree `/Users/yifanxu/.codex/worktrees/juicefs-deepseek/layerfs`,
branch `codex/juicefs-deepseek`. Primary, #287 and #291 source/artifacts untouched.

Input is the exact closed independent #291 master:103108 regular files,
16868 directories including root,10070 symlinks,3475776149 regular bytes,
130046 captured opaque xattrs,0 hardlink aliases. No .git, ignored/generated,
hidden-file or symlink exclusions. Master manifest SHA256
`541f55db9814837fd60386e42e56781e80fbdbd6d555d4bb0e3a099e215139af` was checked.
Source repository and master were never edited or recopied.

Native macOS26.4.1 ARM64 M3 Max,14 logical CPUs,36GiB RAM. SQLite metadata and
isolated loopback MinIO are on the host; no Docker, FUSE, mount or sandbox.
Native MinIO binary SHA256
`b107901fd1afe7b36165c6aa66bb027f96e2ec2b690eebe8c9376746d9a9b0df`;
server compression disabled. JuiceFS uses Zstd with default4MiB block size.

The actual whole public command was:

```sh
juicefs sync --threads 1 --list-threads 1 --dirs --perms --links MASTER/tree/ jfs://juiceprobe/
```

The private `juiceprobe` environment variable selected the owned native SQLite
volume. All source enumeration, reading, compression, filesystem metadata,
object upload, startup and exit are inside the performance child timer. Native
source/body hashes are product work; benchmark fixture hashes are outside.
One copy producer; upstream jfs adapter defaults retained: MaxUpload50,
MaxDownload200,MaxRetries10 and300MiB buffer. Actual retries and physical memory
were not instrumented. This is not the same resource/worker profile as#291.
SQLite persisted journal_mode is WAL; its native engine version and connection
synchronous/cache settings are unavailable, not inferred from Python's SQLite.
No configuration was increased after observing the timeout.

## Qualification and observed outcome

Two separate fresh volumes were formatted for full import and small qualification.
The real-provider small qualification uploaded and reconstructed two regular
files (one empty), an empty directory and a symlink. Exact bytes, empty EOF,
symlink target and a regular file's0640 mode passed. It is a configuration proof,
not a throughput sample or full metadata-fidelity proof.

| Scope | Wall / count | Outcome |
| --- | ---: | --- |
| Full volume format |0.081s|COMPLETE; setup|
| Qualification volume format |0.077s|COMPLETE; setup|
| Small qualification upload/readback |0.078s /0.077s|COMPLETE; exact small-fixture proof|
| Full selected upload |25.013558s|TIMEOUT at25s; process group killed and main child reaped|
| Read-only post-attempt inventory |0.662s complete command|COMPLETE; counts only, not byte proof|
| Owned cleanup |0.238841s|PASS within10s; recorded MinIO exit|
| Full-corpus byte proof |—|NOT_RUN because upload incomplete|

Recovered persisted metadata at inventory:11333 regular-file records with
1108298261 bytes of recorded lengths,1303 directory records and1702 symlink
records. These may include internal/transient nodes and do not certify completed
source files or full-byte correctness. Do not interpret them as a successful
upload rate or extrapolate complete-corpus elapsed time.

S3 LIST observed11331 objects for only the full volume prefix, totaling
**691099951 payload bytes** across12 list pages. This is a partial stored-object
inventory, not the complete repository's storage ratio. The post-stop full-volume
MinIO directory used708192KiB of allocated disk blocks (725188608 bytes), including
provider metadata; this is not a phase peak. SQLite inventory file lengths:
main3481600, WAL4161232, SHM32768 bytes, total7675600 bytes.

The private performance log contains two root permission warnings:
`Chown ... operation not permitted` and `Chmod ... operation not permitted`.
No ERROR line was observed. The command still ran until the watchdog; root
ownership/mode fidelity is not established. Opaque Darwin xattrs/flags, ACLs and
ownership are unverified through this sync route and were not supplied by a
benchmark sidecar. [Sanitized native log](evidence-v1/performance-sanitized.log).

The upload command has no successful final acknowledgement. In-flight object or
metadata outcomes were not guessed/adopted or resent. Partial metadata/objects
are retained locally. The bounded inventory does not turn the timeout into PASS.
The [shutdown receipt](evidence-v1/minio-shutdown.json) confirms process exit.

## Interpretation and comparison limits

This one-producer JuiceFS profile **did not meet the25s full-repository command
bound**. The experiment exposes a genuine incomplete outcome; it does not show
that JuiceFS cannot upload the repository or determine how long completion needs.
It is not a fault diagnosis attributing the result to SQLite, Zstd, source I/O or
per-file requests; those contributions were not measured separately.

#291's retained prototype result is34.543s preparation plus4.636s prepared pack
transfer/publication, an informative sum39.179s. Its5,836 pack ACKs and1.389GB
complete encoded data are different boundaries from this public JuiceFS full
sync command. Constructor concurrency, metadata durability, object layout,
opaque metadata preservation, buffer policy and retry semantics also differ.
No qualified speedup ratio or full-corpus storage-efficiency comparison follows.

Cache residency is unknown. Reused closed fixture and OS/client caches may affect
reads. No cold speed, physical cache/I/O, native heap peak, resource containment,
crash durability, cloud failover, mounted POSIX or CAS/CDC/delta proof was run.
Other owners were not interrupted; continuous host isolation is not established.

## Evidence, reproduction and remaining gates

[Manifest](evidence-v1/MANIFEST.json) hashes sanitized aggregate evidence.
[Performance](evidence-v1/performance.json), [inventory](evidence-v1/inventory.json),
[provider](evidence-v1/provider.json), [source](evidence-v1/source.json),
[cleanup](evidence-v1/cleanup.json) and [context](evidence-v1/context.json) retain
exact boundaries, hashes, command/profile and missing fields. Corpus paths,
private credentials/logs, metadata and object bodies stay under ignored output:
`benchmark-results/juicefs-deepseek/full-upload-v1/` in the owned worktree.
Imported data remains there; no owned server remains running.

The owning runner's commands were:

```sh
python3 tools/juicefs_repository_probe.py setup --output benchmark-results/juicefs-deepseek/full-upload-v1
python3 tools/juicefs_repository_probe.py run --output benchmark-results/juicefs-deepseek/full-upload-v1
```

Do not repeat this unchanged arm to choose a better number, increase its deadline,
shrink its corpus or resume its unknown writes as a completed performance result.
A complete import/byte proof or alternative explicitly selected native profile
requires prospective separate scope while retaining this failed gate. FUSE and
full POSIX qualification remain separate. Issue#292 stays open with these gaps.

## Checks and production LOC

Python syntax/CLI grammar, actual release hash/version, closed manifest identity,
real-provider small-fixture proof, public credential exclusion, evidence manifest
and local links passed.
Initial staged whitespace check rejected upstream CLI help output. Exact help
bytes are retained in gzip files, not trimmed; the original failure is linked
in evidence. The final staged whitespace check passed without changing any
provider, sample or raw private output. No product source changed; Core Cargo tests/examples,
fmt/Clippy, CI and retired preflight were not run. Upstream clone remains clean.

Each owned source commit8012c934c and a8ed76ea9: production117426 ->117426 (+0),
reference65417 ->65417 (+0), Core52009 ->52009 (+0). Same counter
`tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, Git archive
first-parent/final staged snapshots, committed tree confirmation; shipped SQL
included and tools/tests/examples/docs excluded. This report/evidence commit's
exact comparison is recorded in its commit message and issue checkpoint.

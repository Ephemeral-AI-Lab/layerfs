# `fs-benchmark-pro`

`fs-benchmark-pro` is the durable single-agent companion to
[`../fs-bench`](../fs-bench). It compares exactly two arms:

- pinned upstream Cloudflare Computer;
- LayerFS V2 Reference placement.

C3, LayerFS Replica, multi-agent behavior, and concurrency are deliberately
excluded. LayerFS Workspace Commit is a reported phase inside the Reference
arm, not a third candidate.

## Registered workload

Both arms consume the same read-only fixture, generated once before either
candidate timer:

| Field | Frozen value |
|---|---|
| Initial bytes | 33,554,432 |
| Generator | AES-256-CTR over zero bytes; key `07` repeated 32 times, IV `03` repeated 16 times |
| Initial SHA-256 | `3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c` |
| Edits | 16 separately durable 10-byte overwrites |
| Edit `i` offset | `((i + 1) * 2654435761) % (33554432 - 10)`, for `i = 0..15` |
| Edit bytes | ASCII `E${String(i + 1).padStart(9, "0")}` |
| Digest after edits | `30e8b6c71ab635057c32f0e509e6e0037b5781f94bf1b4c88fb438f41d76ca26` |
| Prepend | ASCII `PREPEND010` |
| Final bytes | 33,554,442 |
| Final SHA-256 | `7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3` |

The operation matrix is `create`, `edit-01` through `edit-16`, `prepend`, and
`read`. Every acknowledged final state must pass a process-reopen digest and
size check. Fixture generation and final independent verification are outside
candidate timers.

This workload is compatible with the durable volume workload described in
[Agent Infra Book Part III](https://github.com/agent-infra-foundation/agent-infra-book/blob/main/cloudflare/computer/chapters/PART-III.md),
but this package does not include the C3 implementation or compare against it.

## Comparable boundary

The headline table compares each candidate's complete public Workspace
lifecycle in an already-prepared container:

```text
Computer upstream: complete_turn_ns
LayerFS Reference: complete_turn_ns
```

LayerFS also reports:

```text
complete_turn_ns
  = workspace_create_ns
  + authority_checkpoint_ns
  + workspace_end_ns
```

For EDIT16, each arm creates one Workspace/FUSE mount, performs all 16 durable
checkpoints, and ends the Workspace once. Workspace Commit and authority
checkpoint are diagnostic subsets. LayerStack Add is not part of the comparable boundary; if the
LayerFS arm records it, the report keeps it diagnostic and excluded.

## Source pins

The Computer image must carry exact OCI revision and source-tree labels. The
pin cannot be changed with an environment variable.

Formal runs additionally require `dev.layerfs.computer-build-mode=sealed-source-build`.
A locally admitted prebuilt distribution may be labeled
`diagnostic-prebuilt-dist`, but `run.sh` accepts it only for a one-pair smoke.

| Field | Value |
|---|---|
| Repository | `https://github.com/cloudflare/computer` |
| Commit | `de87919a4fd37242e960e13b7b3ba802d1eef0a0` |
| Tree | `4fb409d7e1356e1098439293d77d2fdc2dbf2190` |
| License | MIT; see [`../fs-bench/LICENSE.cloudflare-computer`](../fs-bench/LICENSE.cloudflare-computer) |

The run manifest records the current LayerFS commit and tree plus a SHA-256
source seal covering the workspace manifests, lockfile, production crates,
container Dockerfile, and this benchmark package. Git status, working-tree and
index patches, and untracked-file inventory are retained alongside the seal.

## Build the two images

Build LayerFS from this checkout:

```bash
layerfs_dirty=false
test -z "$(git status --porcelain)" || layerfs_dirty=true
layerfs_seal=$(benchmark/fs-benchmark-pro/run.sh --source-seal)
docker build \
  --build-arg LAYERFS_SOURCE_COMMIT="$(git rev-parse HEAD)" \
  --build-arg LAYERFS_SOURCE_TREE="$(git rev-parse 'HEAD^{tree}')" \
  --build-arg LAYERFS_SOURCE_DIRTY="$layerfs_dirty" \
  --build-arg LAYERFS_SOURCE_SEAL="$layerfs_seal" \
  -f benchmark/fs-benchmark-pro/Dockerfile.layerfs \
  -t layerfs-fs-benchmark-pro:local .
```

Build the sealed Computer image from a clean pinned upstream checkout. The
build context contains only the pinned source archive and the three admitted
Computer-arm files:

```bash
upstream=/absolute/path/to/cloudflare-computer
context=/absolute/path/to/fresh-build-context
computer_commit=de87919a4fd37242e960e13b7b3ba802d1eef0a0

test "$(git -C "$upstream" rev-parse "$computer_commit^{tree}")" = 4fb409d7e1356e1098439293d77d2fdc2dbf2190
git -C "$upstream" archive --format=tar "$computer_commit" >"$context/computer-source.tar"
cp benchmark/fs-benchmark-pro/Dockerfile.computer \
   benchmark/fs-benchmark-pro/computer.mjs \
   benchmark/fs-benchmark-pro/computer.test.mjs \
   benchmark/fs-benchmark-pro/workload.rs \
   "$context/"
docker build -f "$context/Dockerfile.computer" \
  -t layerfs-fs-benchmark-pro-computer:de87919a "$context"
```

Use a newly created empty build-context directory. Do not reuse an old context
that may contain an earlier archive.

## Run

First validate the scripts, neutral workload, fixture digest, and paired
verifier:

```bash
benchmark/fs-benchmark-pro/run.sh --self-check
```

One adjacent pair is the smoke campaign:

```bash
benchmark/fs-benchmark-pro/run.sh smoke \
  layerfs-fs-benchmark-pro-computer:de87919a \
  layerfs-fs-benchmark-pro:local
```

Thirty adjacent randomized pairs are the formal campaign:

```bash
benchmark/fs-benchmark-pro/run.sh formal \
  layerfs-fs-benchmark-pro-computer:de87919a \
  layerfs-fs-benchmark-pro:local
```

An optional fourth argument supplies a safe run ID. Existing run directories
are never overwritten. Each Computer trial is a fresh `docker run`. Each
LayerFS trial runs Store, Workspace Commit, Push, and its FUSE helper together
inside a fresh constrained Linux control container; verification uses a second
fresh container over the retained Store files. The control container mounts
the Docker socket only so the production Workspace placement path can execute
and inspect its own FUSE helper. Both arms use the same Docker daemon,
architecture, and fixed envelope:

```text
--privileged
--network none
--cpus 1
--memory 1g
--memory-swap 1g
--pids-limit 512
--tmpfs /tmp:rw,nosuid,nodev,size=256m
```

The pair order is derived from the run ID, stored before execution, and never
changed after a failure. A failed arm leaves its raw evidence in place and
terminates the campaign; it is not silently rerun.

## Results

All output is under the requested results root:

```text
benchmark-results/fs-bench-plus/runs/<RUN_ID>/
  fixture.bin
  manifest.json
  terminal.json
  environment/
    computer-image-inspect.json
    layerfs-image-inspect.json
    layerfs-source-seal.tsv
    ...host, Docker, and Git evidence...
  pairs/
    001/
      computer-upstream/
        summary.json
        stdout.txt
        stderr.txt
      layerfs-reference/
        summary.json
        layerfs-reference-state.tsv
        ...measure, verify, and container evidence...
  comparison.json
  comparison.md
```

[`compare.py`](compare.py) accepts only the exact two-arm
`fs-benchmark-pro-sample-v2` schema. It validates the complete 19-operation
matrix, every phase and aggregate equation, nonnegative timings, fixture and
final digests, final size, successful process reopen, exact Computer pin, and
the randomized adjacent-pair manifest. It hashes all raw evidence and refuses
to overwrite an existing report.

The report renders:

- complete public Workspace lifecycle latency for both candidates;
- LayerFS Workspace Commit latency as a diagnostic;
- durability and final digest proof;
- physical SQLite/allocation metrics separately from semantic payload;
- a same-retention caveat for every space comparison;
- the actual execution order and per-pair registered totals.

`N/A` means the arm could not expose a measurement; it never means zero.

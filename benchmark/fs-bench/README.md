# `fs-bench`

This package preserves Cloudflare Computer's frozen twelve-scenario real-FUSE
microbenchmark and compares exactly two candidates:

- pinned upstream Cloudflare Computer;
- LayerFS V2 Reference placement.

C3, LayerFS Replica, multi-agent behavior, concurrency, durable edit
checkpointing, and `fs-benchmark-pro` are intentionally excluded.

## Frozen upstream workload

[`fs-bench.sh`](fs-bench.sh) is byte-identical to
`script/fs-bench.sh` in Cloudflare Computer at:

| Identity | Value |
|---|---|
| Repository | `https://github.com/cloudflare/computer` |
| Commit | `de87919a4fd37242e960e13b7b3ba802d1eef0a0` |
| Tree | `4fb409d7e1356e1098439293d77d2fdc2dbf2190` |
| Blob | `338dd9a85dde3ac7dcc8b4873cf29be2c1eb025c` |
| Path | `script/fs-bench.sh` |
| SHA-256 | `0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef` |
| License | MIT; see [`LICENSE.cloudflare-computer`](LICENSE.cloudflare-computer) |

[`verify_fs_bench.py`](verify_fs_bench.py) is the unchanged legacy LayerFS
verifier retained for old evidence. New paired runs use
[`compare.py`](compare.py); the legacy verifier's historical controls and
budgets are not part of a new Computer-versus-LayerFS comparison.

## What is timed

For each scenario the runner creates a fresh directory, performs any declared
fixture preparation, starts the clock, runs the shell workload through an
already mounted filesystem, stops the clock, and removes the directory.

The selected offline scenarios cover 1,000-file create/stat/remove, directory
construction/traversal, zero-filled 64 MiB write/copy/read/overwrite variants,
and `git init` plus an initial 100-file commit. Networked Git, npm, and Go
scenarios are excluded.

The timer does **not** include product initialization, Pull, Fork, Workspace
creation, mount startup, Workspace Commit, Push, LayerStack Add, process
restart/reopen, or persistent-space accounting. The 64 MiB rows exercise
sparse-zero and page-cache behavior, not high-entropy storage throughput.

## Collect a pair

Use one pair ID and identical `REPS`, `WARMUP`, and `RANDOMIZE_TARGETS` for both
arms. `BASE` is rejected so each arm contains exactly the twelve mounted-FUSE
rows.

For Linux-host-visible FUSE mountpoints:

```bash
pair=$(date -u +%Y%m%dT%H%M%SZ)
benchmark/fs-bench/run.sh computer-upstream host /computer-mount "$pair"
benchmark/fs-bench/run.sh layerfs-reference host /layerfs-mount "$pair"
python3 benchmark/fs-bench/compare.py "$pair"
```

For a FUSE mount that exists only inside a running Docker container:

```bash
pair=$(date -u +%Y%m%dT%H%M%SZ)
benchmark/fs-bench/run.sh computer-upstream docker computer-container /workspace "$pair"
benchmark/fs-bench/run.sh layerfs-reference docker layerfs-container /workspace "$pair"
python3 benchmark/fs-bench/compare.py "$pair"
```

The collector requires the supplied path to be the exact FUSE mountpoint in
Linux `/proc/self/mountinfo`. Docker mode copies the frozen script into a
unique `/tmp` path in the selected container, runs it there, copies out the raw
JSON, and removes only those two temporary files.

Host mode records candidate provenance as `unverified`: a caller-selected
mount and commit label do not prove the mounted binary's source. Docker mode
retains container and image inspection evidence. It marks provenance
`verified` only when inspected revision/tree labels exactly match the intended
source; missing or mismatched labels remain `unverified`. An unverified pair is
valid benchmark evidence, but its report states that source attribution is
unverified.

The Computer source pin is fixed at the commit/tree above. It cannot be
overridden through environment metadata.

## Results

All persistent run output is written, without overwriting, below:

```text
benchmark-results/fs-bench/<PAIR_ID>/
  computer-upstream/
    manifest.tsv
    result.json
    stdout.txt
    stderr.txt
    mountinfo.txt
    ...captured provenance evidence...
  layerfs-reference/
    ...same evidence...
  comparison.json
  comparison.md
```

`compare.py` rejects anything other than the exact two candidate arms and
exact twelve-row scenario matrix. It validates matching configs, sample
counts, statistic bounds, exit status, FUSE runner hash, absence of `FAIL`, and
the offline-only scenario selection. It compares scenario medians, hashes every
raw evidence file, and refuses to overwrite an existing report.

Do not combine these results with historical measurements from another host
and call them a same-host pair. The convenience sum of scenario medians is not
the duration of one agent turn.

## Self-check

```bash
benchmark/fs-bench/run.sh --self-check
```

This verifies the frozen script's syntax/hash, the unchanged legacy verifier,
and a synthetic twelve-row paired comparison including overwrite refusal. On
Bash 4+ it also performs one native-directory runner smoke test; this smoke
test validates the workload machinery and is not benchmark evidence.

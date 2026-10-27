<p align="center">
  <a href="https://layerfs.ai/"><img src="docs/assets/readme/layerfs-mark.svg" alt="LayerFS" width="112"></a>
</p>

<h1 align="center">LayerFS</h1>

<p align="center"><strong>Ephemeral Workspaces. Durable Shared History.</strong></p>

<p align="center">
  LayerFS is the substrate under your agents. Each one mounts a complete, writable
  Workspace over shared committed history, does its work, commits what it changed,
  and goes away.
</p>

<p align="center">
  <a href="#get-started"><b>Get started</b></a> ·
  <a href="https://layerfs.ai/"><b>Website</b></a> ·
  <a href="https://layerfs.ai/docs/"><b>Docs</b></a> ·
  <a href="https://layerfs.ai/benchmarks/"><b>Benchmarks</b></a> ·
  <a href="https://layerfs.ai/video/layerfs-v1.0.0-release.mp4"><b>Video</b></a> ·
  <a href="https://github.com/Ephemeral-AI-Lab/dsh-layerfs#readme"><b>LayerFS × DSH</b></a> ·
  <a href="#community"><b>Community</b></a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
  <a href="https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v1.1.0"><img src="https://img.shields.io/badge/version-1.1.0-2f6fde.svg" alt="1.1.0"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20via%20DSH%20%7C%20Windows%20via%20WSL2-555.svg" alt="Linux, macOS through DSH, and Windows through WSL2 on current main">
  <a href="https://discord.gg/8mmAMCDc2"><img src="https://img.shields.io/badge/Discord-join-5865F2.svg?logo=discord&logoColor=white" alt="Discord"></a>
</p>

https://github.com/user-attachments/assets/70c25887-86a1-4d81-bd78-571cf1c09a18

> **Status:** [LayerFS 1.1.0](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v1.1.0),
> released as source: the JavaScript API split into `@layerfs/agent`, `@layerfs/backend`
> and `@layerfs/runtime` under [`api/`](api/README.md), plus the `layerfs` command. It breaks
> code written for 1.0.0; see the [release record](docs/release-notes/1.1.0/README.md).
> The results below are 1.0.0's, measured on `main` at `e5c7aaae05`. The Store runs in its Disposable profile only, with no
> crash or power-loss durability: use LayerFS for agent work and its history, not
> as the only copy of data you cannot rebuild.

Since 1.1.0, LayerFS also supports native Windows dsh Live through WSL2 and its own Linux Docker Engine; see the [Windows setup](https://github.com/Ephemeral-AI-Lab/dsh-layerfs#windows-live).

<a id="get-started"></a>
## Try it with LayerFS × DSH

<p align="center">
  <a href="https://github.com/Ephemeral-AI-Lab/dsh-layerfs#readme"><img src="docs/assets/readme/layerfs-x-dsh.svg" alt="LayerFS × DSH: the LayerFS logo and the DeepSeek Harness logo" width="220"></a>
</p>

LayerFS runs today through **LayerFS × DSH**, the `dsh-layerfs` plugin for
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness). Agents in a dsh
session fork Branches, test them in Sandboxes, merge and publish, and the **Work** and
**History** tabs show every agent's Branch, live.

<p align="center">
  <a href="https://github.com/Ephemeral-AI-Lab/dsh-layerfs#readme"><img src="https://img.shields.io/badge/Try%20LayerFS%20%C3%97%20DSH-install%20guide%20%E2%86%92-7C3AED.svg?style=for-the-badge" alt="Try LayerFS × DSH: install guide"></a>
</p>

## Core ideas

<table>
  <tr>
    <td width="33%" valign="top">
      <a href="https://layerfs.ai/docs/core-ideas/#idea-ephemeral"><img src="docs/assets/readme/idea-01-ephemeral.svg" alt="fs calls go straight into the Store as Commits; each Bash call mounts a Workspace, runs, commits and unmounts" width="100%"></a><br>
      <img src="docs/assets/readme/icon-ephemeral.svg" width="16" height="16" alt=""> <b>01 · Ephemeral Computing</b><br>
      Compute exists only while a command runs.
    </td>
    <td width="33%" valign="top">
      <a href="https://layerfs.ai/docs/core-ideas/#idea-decentralized"><img src="docs/assets/readme/idea-02-decentralized.svg" alt="Four agents, each on its own computer, read and commit to one shared Store" width="100%"></a><br>
      <img src="docs/assets/readme/icon-decentralized.svg" width="16" height="16" alt=""> <b>02 · Decentralized Agents</b><br>
      Agents on different computers share one serverless Store.
    </td>
    <td width="33%" valign="top">
      <a href="https://layerfs.ai/docs/core-ideas/#idea-coordination"><img src="docs/assets/readme/idea-03-coordination.svg" alt="Main with two Branches, three agents on each; a commit to a file that already changed is refused with a Conflict" width="100%"></a><br>
      <img src="docs/assets/readme/icon-coordination.svg" width="16" height="16" alt=""> <b>03 · Multiagent Coordination</b><br>
      One project, many Branches. One Branch, many agents.
    </td>
  </tr>
  <tr>
    <td width="33%" valign="top">
      <a href="https://layerfs.ai/docs/core-ideas/#idea-recursive"><img src="docs/assets/readme/idea-04-recursive.svg" alt="Agents fork instantly into new Branches, recursively; each Branch is scored and the best one is kept" width="100%"></a><br>
      <img src="docs/assets/readme/icon-recursive.svg" width="16" height="16" alt=""> <b>04 · Recursive Filesystem</b><br>
      Fork recursively any Commit, at any depth.
    </td>
    <td width="33%" valign="top">
      <a href="https://layerfs.ai/docs/core-ideas/#idea-storage"><img src="docs/assets/readme/idea-05-storage.svg" alt="Four agents commit the same packages; the first stores every chunk and each later Commit adds only what is new" width="100%"></a><br>
      <img src="docs/assets/readme/icon-storage.svg" width="16" height="16" alt=""> <b>05 · Ultra Storage Optimization</b><br>
      157 snapshots of a real project in 100 MB, 19.9× smaller than Btrfs + zstd.
    </td>
    <td width="33%" valign="top">
      <a href="https://layerfs.ai/docs/core-ideas/#idea-bootstrap"><img src="docs/assets/readme/idea-06-bootstrap.svg" alt="Without ignored files the tests fail until packages are installed; with LayerFS the mount already has them" width="100%"></a><br>
      <img src="docs/assets/readme/icon-bootstrap.svg" width="16" height="16" alt=""> <b>06 · Instant Bootstrap</b><br>
      Dependencies come with every mount. No time lost reinstalling.
    </td>
  </tr>
</table>

**And underneath them**

| | Supporting idea | In one line |
| --- | --- | --- |
| <img src="docs/assets/readme/icon-checkpoints.svg" width="16" height="16" alt=""> | **[07 · Auto Checkpoints](https://layerfs.ai/docs/core-ideas/#idea-checkpoints)** | Every step an agent takes is a checkpoint to return to. |
| <img src="docs/assets/readme/icon-immutable.svg" width="16" height="16" alt=""> | **[08 · Append-only Objects](https://layerfs.ai/docs/core-ideas/#idea-immutable)** | Saved objects are never rewritten. The Store only grows. |
| <img src="docs/assets/readme/icon-lww.svg" width="16" height="16" alt=""> | **[09 · Conflict Rules (`.lww`)](https://layerfs.ai/docs/core-ideas/#idea-lww)** | Name the files that should never conflict. The newest edit wins. |
| <img src="docs/assets/readme/icon-bigfile.svg" width="16" height="16" alt=""> | **[10 · Big File, Small Edit](https://layerfs.ai/docs/core-ideas/#idea-bigfile)** | An edit costs the size of the edit, not the size of the file. |

## See it run

<p align="center">
  <img src="docs/assets/official_release/page3-demo.gif" alt="The LayerFS demo: agents on several Branches run tool calls in Sandboxes and fs, each call becomes a Commit, and the history tree grows" width="100%">
</p>
<p align="center"><sub>Agents fork Branches, work in Workspaces inside Sandboxes, commit, merge and publish to <code>main</code>. The history grows as they go.</sub></p>

## Results

Every suite ran once on `main` at `e5c7aaae05` through the JavaScript SDK. Each
row links to its method, environment and receipts.

| Suite | What it measures | LayerFS 1.0.0 |
| --- | --- | --- |
| [fs-bench](https://layerfs.ai/benchmarks/fs-bench/) | Cloudflare Computer's own twelve-scenario filesystem benchmark, unchanged, same image | **5.20 s vs 8.88 s, 1.71× faster**; faster in 10 of 12 scenarios |
| [dsh-dedup](https://layerfs.ai/benchmarks/dsh-dedup/) | A real project's history, 157 snapshots (4.94 GB of trees), kept as mountable Commits | **100.1 MB vs 1,994.6 MB, 19.9× smaller** than Btrfs + zstd; 156/156 read back |
| [pjdfstest](https://layerfs.ai/benchmarks/pjdfstest/) | POSIX conformance, 8,798 assertions, against kernel overlayfs | **0 unexpected failures**; keeps 76% of overlayfs's passes, every other failure a deliberate format limit |
| [fsx](https://layerfs.ai/benchmarks/fsx/) | Random reads, writes, mapped I/O and truncates, every byte checked | **14/14 pass**, 3.06 M ops, 0 miscompares |
| [fsstress](https://layerfs.ai/benchmarks/fsstress/) | Four concurrent processes, then Commit, remount and compare | **3,755/3,755 entries identical**; 5.21 GB captured by one Commit in 4.9 s |
| [smallfile](https://layerfs.ai/benchmarks/smallfile/) | 100,000 files of 4 KiB, four threads, against libfuse passthrough | **Faster in 9 of 9 operations**, 1.27×–10.71× |
| [mdtest](https://layerfs.ai/benchmarks/mdtest/) | Metadata create, rename and remove, against libfuse passthrough | 0.86–1.28× with 1 process, 1.21–4.22× with 4 |
| [fio](https://layerfs.ai/benchmarks/fio/) | Data I/O bandwidth, against libfuse passthrough | Random read/write mixes 1.49–2.23×; **slower** on sequential write (0.31×), sequential read (0.81×) and random write (0.40×) |

<sub>One sample per case, seed 1, no best-of. Host: arm64 Mac, Docker Desktop
linux/arm64, 8 CPUs, 8.3 GB; host load is recorded per suite. These are release
measurements, not a guarantee for your workload. Earlier fs-bench-pro results:
[v0.1.4](docs/release-notes/0.1.4/benchmark-closeout.md) ·
[v0.1.5](docs/release-notes/0.1.5/benchmark-closeout.md) ·
[v0.1.6](docs/release-notes/0.1.6/benchmark-closeout.md).</sub>

## How it works

```text
agent tool call
 ├─ file tool (read, write, edit, grep …) ── SDK, in process ──────► Store
 └─ Bash ── mount ─► exec ─► commit ─► unmount                       ▲
             └─ Workspace: FUSE view in a Sandbox's daemon ───────────┘
                (committed root + its own overlay)
```

- **Store.** One SQLite file holds every Branch, Commit and content object. Each
  daemon and SDK process opens it directly; there is no database server.
- **Branch, Commit, Layer.** A Branch is a writable line of Commits. Publishing a
  Branch to `main` adds a Layer. Content is chunked and stored once.
- **Workspace.** A mount binds the whole committed tree without copying it. Writes
  go to the Workspace's own overlay until a Commit saves them. Unmount deletes
  the overlay, never history.
- **Conflict.** Two Commits that change the same file are refused with
  `Conflict`, unless a `.lww` rule says the newest edit wins. Publishing a
  stale Branch is different: it may skip newer `main` Layers; merge `main` into
  the Branch first ([publish semantics](.agents/skills/layerfs-js-sdk/references/methods/branch.md)).

## Prior art

LayerFS builds on well-known ideas: layered and union filesystems, copy-on-write
snapshots, content-addressed storage and throwaway execution environments. What
it adds is putting them together for agents: every tool call gets a full
writable tree, and every result becomes a Commit on a Branch in one shared Store.

| Prior work | What it shares with LayerFS | Where LayerFS differs |
| --- | --- | --- |
| **overlayfs, UnionFS, container image layers** | A read-only lower tree with a writable upper layer, which is also where the "layer" name comes from | The upper layer is not just thrown away or kept as a directory: Commit turns it into an immutable snapshot on a Branch, with history, merge and `Conflict`. overlayfs is in the kernel and more complete; LayerFS's FUSE view deliberately omits FIFOs, sockets, device nodes, set-id bits and separate atime/ctime ([pjdfstest](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/data/pjdfstest/README.md)) |
| **Btrfs and ZFS snapshots, reflinks** | Cheap copy-on-write snapshots that share unchanged data | Sharing is by content, not by extent, so a file that is rewritten in full still stores only its new chunks. On a small-file history of 157 snapshots that gives [19.9× less space than Btrfs + zstd](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/data/dsh-dedup/README.md). Btrfs and ZFS also stay cheap for in-place edits of large files, and they are durable, mature kernel filesystems; LayerFS is not ([why Btrfs used more space here](docs/research/layerfs-vs-btrfs-157/REPORT.md#3-why-btrfs-used-more-storage-in-this-workload)) |
| **Git worktrees** | One repository, many working copies and branches | A Workspace holds the whole tree, including `.git`, ignored files, dependencies and build output, so a new mount needs no reinstall. Commits happen per tool call, not when someone runs `git commit` |
| **[try](https://github.com/binpash/try)** | Run a command against an overlay of your system, then inspect, apply or discard what it changed | `try` works on one machine with one overlay at a time. LayerFS keeps every result as a Commit, and lets many agents branch from any of them and share one Store |
| **Apiary and virtual layered file systems** (Potter and Nieh, [USENIX ATC 2010](https://www.usenix.org/conference/usenix-atc-10/apiary-easy-use-desktop-application-fault-containment-commodity-operating), [LISA 2011](https://www.usenix.org/legacy/event/lisa11/tech/full_papers/Potter.pdf)) | Ephemeral containers that start instantly over shared layered filesystems | Apiary isolates desktop applications. LayerFS's target is agent tool calls whose results are kept, branched and merged as history |

**Use something else** if you need crash or power-loss durability (the Store runs
in its Disposable profile only), full POSIX node types, or top sequential I/O
bandwidth ([fio](https://layerfs.ai/benchmarks/fio/)). For a few long-lived
checkouts on one machine, Btrfs snapshots or Git worktrees may be all you need.

## Learn more

| | |
| --- | --- |
| Docs | [LayerFS × DSH guide](https://github.com/Ephemeral-AI-Lab/dsh-layerfs#readme) · [layerfs.ai/docs](https://layerfs.ai/docs/) · [Core ideas](https://layerfs.ai/docs/core-ideas/) · [JavaScript SDK guide](api/README.md) |
| Concepts | [Core concepts](docs/general/concepts.md) · [Documentation index](docs/README.md) |
| Benchmarks | [layerfs.ai/benchmarks](https://layerfs.ai/benchmarks/) |
| Release | [1.1.0 release record](docs/release-notes/1.1.0/README.md) · Previous: [1.0.0 release](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v1.0.0) and its [plan](https://github.com/Ephemeral-AI-Lab/layerfs/blob/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap/1.0/README.md); [0.1.7 release record](docs/release-notes/0.1.7/README.md) and its [manual](docs/versioned/0.1.7/README.md) |

## Community

Questions, ideas and show-and-tell are welcome in either place.

<table>
  <tr>
    <td align="center" valign="top" width="50%">
      <b>Discord</b><br><br>
      <a href="https://discord.gg/8mmAMCDc2"><img src="https://img.shields.io/badge/Discord-join%20the%20server-5865F2.svg?logo=discord&logoColor=white&style=for-the-badge" alt="Join the LayerFS Discord"></a><br><br>
      <a href="https://discord.gg/8mmAMCDc2">discord.gg/8mmAMCDc2</a>
    </td>
    <td align="center" valign="top" width="50%">
      <b>WeChat · LayerFS的好朋友</b><br><br>
      <img src="docs/assets/readme/wechat-group.png" alt="QR code for the LayerFS WeChat group" width="220"><br>
      <sub>Scan in WeChat to join the group.</sub>
    </td>
  </tr>
</table>

## License

LayerFS is licensed under the [MIT License](LICENSE).

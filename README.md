<p align="center">
  <a href="https://layerfs.ai/"><img src="docs/assets/readme/layerfs-mark.svg" alt="LayerFS" width="112"></a>
</p>

<h1 align="center">LayerFS</h1>

<p align="center"><strong>Ephemeral Workspaces. Durable Shared History.</strong></p>

<!-- gitreels:start -->
<p align="center">
  <a href="https://gitreels.com/Ephemeral-AI-Lab/layerfs"><img src="https://gitreels.com/Ephemeral-AI-Lab/layerfs.webp" alt="Ephemeral-AI-Lab/layerfs in 15 seconds" width="100%"></a>
  <br>
  <a href="https://gitreels.com"><img src="https://gitreels.com/badge.svg" alt="Made with GitReels" height="24"></a>
</p>
<!-- gitreels:end -->

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
  <a href="dsh/README.md"><b>LayerFS × DSH</b></a> ·
  <a href="#community"><b>Community</b></a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
  <a href="https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v1.0.0"><img src="https://img.shields.io/badge/version-1.0.0-2f6fde.svg" alt="1.0.0"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20via%20DSH-555.svg" alt="Linux, and macOS through DSH">
  <a href="https://discord.gg/8mmAMCDc2"><img src="https://img.shields.io/badge/Discord-join-5865F2.svg?logo=discord&logoColor=white" alt="Discord"></a>
</p>

https://github.com/user-attachments/assets/70c25887-86a1-4d81-bd78-571cf1c09a18

> **Status:** [LayerFS 1.0.0](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v1.0.0),
> the first official release after the 0.1.x Developer Previews, released as source.
> The results below ran on `main` at `e5c7aaae05`. The Store runs in its Disposable profile only, with no
> crash or power-loss durability: use LayerFS for agent work and its history, not
> as the only copy of data you cannot rebuild.

<a id="get-started"></a>
## Try it with LayerFS × DSH

<p align="center">
  <a href="dsh/README.md"><img src="docs/assets/readme/layerfs-x-dsh.svg" alt="LayerFS × DSH: the LayerFS logo and the DeepSeek Harness logo" width="220"></a>
</p>

LayerFS runs today through **LayerFS × DSH**, the `dsh-layerfs` plugin for
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness). Agents in a dsh
session fork Branches, test them in Sandboxes, merge and publish, and the **Work** and
**History** tabs show every agent's Branch, live.

<p align="center">
  <a href="dsh/README.md"><img src="https://img.shields.io/badge/Try%20LayerFS%20%C3%97%20DSH-install%20guide%20%E2%86%92-7C3AED.svg?style=for-the-badge" alt="Try LayerFS × DSH: install guide"></a>
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
  `Conflict`, unless a `.lww` rule says the newest edit wins.

## Learn more

| | |
| --- | --- |
| Docs | [LayerFS × DSH guide](dsh/README.md) · [layerfs.ai/docs](https://layerfs.ai/docs/) · [Core ideas](https://layerfs.ai/docs/core-ideas/) · [JavaScript SDK guide](sdk/js/README.md) |
| Concepts | [Core concepts](docs/general/concepts.md) · [Documentation index](docs/README.md) |
| Benchmarks | [layerfs.ai/benchmarks](https://layerfs.ai/benchmarks/) |
| Release | [1.0.0 plan](docs/roadmap/1.0/README.md) · Previous: [0.1.7 release record](docs/release-notes/0.1.7/README.md) and its [manual](docs/versioned/0.1.7/README.md) |

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

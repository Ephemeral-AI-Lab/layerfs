# LayerFS documentation

> **Status:** Current documentation entry point; v0.1.7 release documentation.

## LayerFS 1.1.0 (release candidate)

- [Release record, compatibility and the owner waiver](release-notes/1.1.0/README.md)
- [JavaScript packages wiki](../api/README.md) and its [how-to guides](../api/docs/)

## Use LayerFS v0.1.7

- [Manual](versioned/0.1.7/README.md)
- [SDK and terminal UI guide](versioned/0.1.7/sdk-and-ui.md)
- [SDK reference](versioned/0.1.7/sdk-reference.md)
- [Release record, compatibility and waivers](release-notes/0.1.7/README.md)
- Next: [LayerFS 1.0.0 plan](https://github.com/Ephemeral-AI-Lab/layerfs/blob/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap/1.0/README.md) (archived; see [removed roadmap](#removed-roadmap-and-raw-evidence))

## Historical LayerFS 0.1.6

- [Manual](versioned/0.1.6/README.md)
- [Quickstart](versioned/0.1.6/quickstart.md)
- [CLI reference](versioned/0.1.6/cli.md)
- [Rust SDK reference](versioned/0.1.6/sdk.md)
- [Container runtime](versioned/0.1.6/container-runtime.md)
- [Release record and accepted limitations](release-notes/0.1.6/README.md)
- [Owner acceptance, waivers and issue disposition](release-notes/0.1.6/acceptance.md)
- [Sandbox-local state and unchanged Store format](versioned/0.1.6/storage-format.md)
- [Honest limitations](versioned/0.1.6/limitations.md)
- [Every measured v0.1.6 selection](release-notes/0.1.6/benchmark-closeout.md)

## Historical LayerFS 0.1.5

- [Manual](versioned/0.1.5/README.md)
- [Release record](release-notes/0.1.5/README.md)
- [Every v0.1.5 benchmark family and case](release-notes/0.1.5/benchmark-closeout.md)

## Historical LayerFS 0.1.4

- [Manual](versioned/0.1.4/README.md)
- [Release record](release-notes/0.1.4/README.md)
- [Every v0.1.4 benchmark family and case](release-notes/0.1.4/benchmark-closeout.md)

## Historical LayerFS 0.1.3

- [Manual](versioned/0.1.3/README.md)
- [Quickstart](versioned/0.1.3/quickstart.md)
- [Product specification](versioned/0.1.3/specification.md)
- [CLI reference](versioned/0.1.3/cli.md)
- [Rust SDK reference](versioned/0.1.3/sdk.md)
- [Container runtime](versioned/0.1.3/container-runtime.md)
- [Storage format](versioned/0.1.3/storage-format.md)
- [Limitations](versioned/0.1.3/limitations.md)
- [Release record](release-notes/0.1.3/README.md)

## Plan and build

- [Roadmap](https://github.com/Ephemeral-AI-Lab/layerfs/blob/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap/README.md), [0.1.1 history](https://github.com/Ephemeral-AI-Lab/layerfs/blob/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap/0.1/0.1.1/README.md), [0.1.3 completed checkpoint](https://github.com/Ephemeral-AI-Lab/layerfs/blob/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap/0.1/0.1.3/README.md) and [0.1.x development](https://github.com/Ephemeral-AI-Lab/layerfs/blob/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap/0.1/development.md): archived, see [removed roadmap](#removed-roadmap-and-raw-evidence)
- [Complete cluster-two implementation plan](../core/docs/issues/307/IMPLEMENTATION-PLAN-S7-S13-20261007.md): current S7–S13 ordering, iterative exits, optimization decisions, FUSE experiment readiness and expected source structure
- [Cluster-two performance acceptance](../core/docs/issues/307/PERFORMANCE-ACCEPTANCE-S7-S12-20261007.md): existing numerical gates, required work/resource bounds, prospective registration and retained failures
- [Historical S7–S9 continuation prompt](../core/docs/issues/307/HANDOFF-S7-S9.md): retained source and stopping-boundary context; use the complete plan for current remaining-work ordering
- [Core concepts](general/concepts.md)
- [Sandbox cache design](general/sandbox-cache-design.md): cluster-two ownership, branch sharing, source structure and remaining qualification
- [Workspace filesystem view](general/workspace-filesystem-view.md): global Store base, daemon overlay, lazy FUSE read path and ASCII diagrams
- [Workspace queue scheduling](general/workspace-queue-scheduling.md): archived; the shared-database queue and credits removed by #321
- [Optimization guide](general/optimization-guide.md): binding performance-engineering rules
- [Optimization handbook](general/optimization-handbook.md): diagnostic groups, existing counters, scaling checks, procedure and the current candidate list
- [Benchmark rules](general/benchmark_rules.md)
- [Benchmark instruction: the optimization iteration and its metrics](general/benchmark_instruction.md)
- [Release policy](general/release-policy.md)
- [Documentation policy](general/documentation-policy.md)
- [Development workspace cleanup](general/cleanup.md): ownership checks, verified recovery, evidence retention, and disk-space reporting
- [Parallel worktree development](general/parallel-worktrees.md): private Cargo resources, prepared dependencies, pinned SDK builds, and per-run Docker E2E state
- [Plugin listings](plugins/README.md): how a plugin built on LayerFS is listed in `plugins/<plugin-id>/`, with the template to copy and the checker

## Explore

- [Research and historical design material](research/README.md)

The directory classes are deliberately small:

```text
versioned/    immutable released manuals
general/      version-independent concepts and policies
plugins/      the plugin listing guide, its template and checker
research/     informative studies and historical design material
assets/       images used by maintained documentation
```

Release announcements, proof, artifacts, limitations, and performance evidence
remain under [`docs/release-notes/`](release-notes/).

## Removed roadmap and raw evidence

Issue #398 (2026-10-22) removed from `main`, to cut the checkout size:

- all of `docs/roadmap/` (plans, release checklists, proposals, development
  guides, dated checkpoints and the 0.1.x evidence campaigns);
- raw run output (`.json`, `.jsonl`, `.stderr`, `.stdout`, `.log`, `.txt`,
  `.csv`, `.tsv`, `.gz`, `.sqlite`) under `core/docs/issues/`,
  `core/docs/architecture/proposal/` and `core/docs/api/evidence/`. Their `.md`
  records, scripts, patches and checksum files remain. Issue #352's records and
  the three files `benchmark/fs-bench-pro` reads were kept.

Nothing was rewritten in history. Every removed file is at commit
[`792715f00a`](https://github.com/Ephemeral-AI-Lab/layerfs/tree/792715f00a2dbfcf0cb8435ae84862cce972dc90/docs/roadmap),
also tagged `docs-raw-sqlite-final`. Older documents that link into the removed
paths resolve there. To restore a file or directory locally:

```bash
git restore --source=792715f00a -- <path>
```

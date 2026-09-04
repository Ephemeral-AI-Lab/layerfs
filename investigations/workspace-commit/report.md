# Generic Workspace Commit — implementation ledger

Worktree: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-workspace-commit-engine`.
Branch: `codex/workspace-commit-engine`. Implementation base:
`a40b17e05486e5b747b689e7710475d739556a69` (inspected current
`codex/v013-phase1`, includes `fbf32e84` and subsequent functional repairs).
Reference specification: `94bfdf40ff1c6e594bb554510f2d32f4c0072c4e`.
Original investigation base: `7a6e119a`; no experimental product commits promoted.
The untracked bulk-create/delete notes were read from the original checkout;
the reference report also preserves their original copy. Original checkout,
reference branch and other campaign state are untouched.

Status: implementation in progress; no integration or target attainment claim.

## Hypotheses

- A: checked candidate identities remove refresh path rediscovery; retain the
  published outcome across installation/cleanup errors. Expected counters:
  fewer refresh path resolutions; no duplicate logical Commit after failure.
- B: net reference facts recorded before reclamation remove deleted-subtree
  rediscovery. Sorted page merges eliminate repeated point-update emission.
  Expected counters: old-page reads proportional to affected pages, final page
  emissions; bounded reference bytes and measured Exec bookkeeping overhead.
- C: moving owned candidate payloads removes admission copies; carried checked
  batches reduce transactions while retaining collision/receipt/fault checks.

## Simultaneous ownership design

Existing Workspace nodes, overlays, pieces and open backing remain charged to
existing policy/process bounds. Added mutation buffer: at most 42,016 bytes;
private fixed-record reference stream charged with Workspace spool. Commit sort
buffer: at most 256 KiB, 32 run descriptors, fixed merge cursors; input and output
runs reserved simultaneously before growth. Final inode pairs spill using the
same fixed-record sorter. Handoff has bounded records/reader and private spool.
Tree scratch and handoff are reserved in the same final-delta budget, not each
given 8 MiB. Store candidate/index limits remain distinct. No new workers.

Private exploratory containers use 2 CPUs, 2 GiB memory+swap, 256 pids and real
FUSE. Builds/preparation/performance/verification are serialized by the parent.
Shared host interference is recorded; no frozen-profile qualification claim.

Unimplemented counters are unavailable, never zero. All failed runs remain
source-bound. Target status and matched initialization comparison remain pending.

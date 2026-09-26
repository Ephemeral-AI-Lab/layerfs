# #237: native compact-record fresh-builder prototype

> **Status: experimental, not adopted.** The implementation and source checks
> are preserved in isolated commit `27cf06838` at
> `/Users/yifanxu/.codex/worktrees/issue237-bounded-fresh-prototype/layerfs`.
> The main treatment branch keeps the measured direct-inode change.

## What was implemented

Native import writes one replayable C1 record per scanned entry while its
metadata or symlink prerequisite root is emitted. The record contains parent
ordinal, name, kind, content root and metadata root; it contains no file
payload. The old entry vector is dropped before the C1 tree build. C1 checks
the entire stream before tree-object emission: root and serial range,
directory parent, name grammar and order, breadth-first parent groups, and
trailing bytes. It then uses the existing sorted directory and inode page
builders. File, prerequisite, tree and history Saves remain separate. The
same C5 reservation assigns serial `root_serial + ordinal`. The generic C1
path remains used by pathless Init and Workspace updates.

The source scan still accumulates all `PreparedEntry` values, file jobs and
per-directory `read_dir` children. This prototype therefore bounds **C1
replay scratch**, not the entire import pipeline. Its `FileBacking` has a
256-MiB byte ceiling and owns its run until checked cleanup, while C1 has a
4-MiB validation bitset ceiling. C1 replay handled the 4,097-child case,
but the scanner's directory sort remains whole-vector.
These limits prevent a claim that a full bounded fresh builder is finished.

## Functional evidence

Fixed scope and serial tests produced the exact same canonical root and
object set as generic C1 for nested, empty and 4,097-child trees. A bad
non-directory parent was rejected before any tree object was emitted.
Native import Service tests, including the 4,097-entry case, passed. The
one real public SDK 100k-file call passed the separate reopened oracle for
all 101,001 paths and 500,000,000 bytes. Core `test`, `clippy --all-targets
-- -D warnings`, examples build, `fmt --check`, product-boundary check and
tool unit tests all passed on the probe-free isolated source.

## One-shot diagnostic

The prospective [freeze](evidence/native-record-fresh-20260924/freeze.json)
set an 8-MiB conservative reduction target against the retained direct-inode
[control](evidence/c3-inode-fusion-20260924/candidate/receipt.json): candidate
whole-call RSS **plus the entire peak scratch reservation** had to be at
least 8 MiB below control RSS. The [candidate receipt](evidence/native-record-fresh-20260924/candidate/receipt.json)
and [arithmetic](evidence/native-record-fresh-20260924/comparison.json) show:

| Measure | Retained direct-inode control | Native-record prototype | Raw difference |
| --- | ---: | ---: | ---: |
| Whole-call process peak RSS | 127,631,360 B | 116,654,080 B | −10,977,280 B (−10.47 MiB) |
| Peak owned scratch bytes | 0 B for this input run | 8,320,115 B | +8,320,115 B |
| RSS + full scratch reservation | 127,631,360 B | 124,974,195 B | −2,657,165 B (−2.53 MiB) |
| Raw public SDK call | 5.410089250 s | 5.901850000 s | +0.491760750 s |
| Reopened full oracle | PASS | PASS | — |

The input run was recently written by the same call, and its metadata and
page-cache residency were not qualified. Charging **all** owned scratch
bytes against memory is a conservative upper bound, not a measured page-cache
value. It numerically misses the frozen 8-MiB target. The raw time is
`INELIGIBLE` for a speed claim; the run's metadata and scratch cache states
were unqualified. The registered #236 debug SDK selection remains separate.

The candidate's source path was **29 bytes longer** than the retained
control's, and pre-namespace RSS was roughly 3.4 MiB higher. The 100,000
job paths are a likely contributor; this is an inference, not an isolated
allocation measurement. The cross-worktree RSS difference is diagnostic, not a
qualified matched treatment effect. The candidate used zero resident source
payload pages at launch and left no scratch file after completion. A repeat
of the same arm to select a more convenient path or number is not allowed by
the benchmark contract. The exact receipts remain append-only.

## Decision

Do not merge the prototype into the main product branch. Its process RSS
decrease is real for the observed call, but the conservative memory target
was missed, the retained comparison has a path-length confound, and no speed
benefit was established. The conditional file-job frontier stays `NOT_RUN`:
in this candidate the tree Save still set whole-call RSS at 116,654,080 B,
above the 108,773,376-B file-loop high-water. A later full bounded candidate
must remove the whole-vector scanner/job planning, bound high-fanout sorting,
and give the input record run a qualified cache contract while preserving
source-change detection and queue backpressure.

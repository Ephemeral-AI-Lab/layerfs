# Issue 265: 100 mounted writes, tiny payloads and balanced extent leaves

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> All six public latency rows are cache-INELIGIBLE. The original three
> tiny-payload diagnostic receipts remain `INCOMPLETE` because their parser
> refused a Boolean field; count-only postprocessing did not rewrite them.

This child was stacked on #261 draft PR #262 head
`d128d3fa574d4dadce758dd35b9c4af4ecc9c87d`. The [prospective public
workload](../../../../docs/roadmap/0.1/0.1.7/issue265-mounted-write-treatment.md)
and [prospective leaf correction](../../../../docs/roadmap/0.1/0.1.7/issue265-balanced-leaf-treatment.md)
were committed before their runner work and respective samples. The
[append-only evidence](evidence/100-write-v1/) contains the six original
receipts and raw driver/verifier logs, both count-only postprocess reports,
release preparation logs, public manifests, SHA-256s and the setup failure.
The corresponding SQLite clones and binary archives remain in this worktree's
ignored `benchmark-results/fs-bench-pro/`; `EVIDENCE.json` pins their hashes.

## Exact route and source

Each selection used the same closed, independently verified 10 MiB old head,
validated local byte-copy Store/history clones, one public Mount, one Exec of
one generic shell/writer/fd, exactly 100 one-byte FUSE WRITE callbacks, one
explicit public Commit and a separate full old/new-head verifier. The writer
source hash stayed `dc21c66ddb85be7c5d27c164b292cbf19a82f4e5a3352a8197050cfcae15d8f0`.
Locked release SDK, verifier and aarch64 daemon binaries used this worktree's
Cargo target and repository-root AEAD build flags. The baseline source was
`4f644e950` and the balanced source `ebfa8f242`; each case has one attempt
per source, in append/dispersed/repeated order. The same 15 s complete-command,
9 s verifier and one-construction-worker limits applied to both. No cache
preconditioning or cold-state proof was used, so raw time comparisons cannot
pass admission.

The first preparation at `b83215105` stopped **before any build, clone or
sample**: its fail-closed source allowlist omitted the external C1 test file.
`preparation-fail.json` is retained. Commit `4f644e950` added that reviewed
path and a fresh preparation succeeded. The initial tiny-payload sample
runner then parsed `spool_resident=true` as an integer. Each original
receipt remains `INCOMPLETE`; commit `5fbda266b` added a read-only raw-log
postprocessor. Its derived diagnostic status is `COMPLETE` for all three
original rows and does not promote their receipt or performance status. The
balanced receipts are `INELIGIBLE` because their functional and diagnostic
checks passed while cache state remained uncontrolled.

## Count result

The version-2 private format puts one 1..=4,016-byte payload and its 80-byte
identity fields in one aligned 4 KiB page. A 4,017-byte input keeps the
version-1 two-page layout. The focused native ext4 proof checked one byte,
4,016, 4,017 and a multisegment input, arbitrary ranges/EOF, zero padding,
quota refusal, short-input retention, collision refusal and clean reclamation.
Each public case kept 100 accepted one-byte callbacks. The source has one
aligned payload write and 4,096 observed allocated bytes per accepted callback,
predicting **409,600 cumulative payload-write bytes** instead of the prior
819,200. This is a source-derived direct-I/O API count, not measured device
traffic or a latency win. At write 100, append and dispersed each retained
409,600 payload-allocation bytes; repeated retained two 4 KiB payloads after
routine reclamation. Payload ownership and quota charging remained per write.

| Selection | Source | Live metadata pages at writes 25/50/75/100 | Extent leaf / branch writes | Child / custody edges added | Ledger 4 KiB reads / writes |
| --- | --- | --- | ---: | ---: | ---: |
| Append | Tiny baseline | 4 / 4 / 4 / 4 | 100 / 0 | 0 / 5,050 | 2,647 / 1,470 |
| Append | Balanced | 4 / 4 / 4 / 4 | 100 / 0 | 0 / 5,050 | 2,647 / 1,470 |
| Dispersed | Tiny baseline | 4 / 4 / 18 / 37 | 131 / 39 | 699 / 3,859 | 3,328 / 1,929 |
| Dispersed | Balanced | 4 / 4 / 7 / 7 | 101 / 39 | 78 / 3,512 | 3,156 / 1,756 |
| Repeated | Tiny baseline | 4 / 4 / 4 / 4 | 100 / 0 | 0 / 100 | 3,059 / 1,489 |
| Repeated | Balanced | 4 / 4 / 4 / 4 | 100 / 0 | 0 / 100 | 3,059 / 1,489 |

For dispersed writes, balanced touched-leaf packing reduced the final
aggregate live metadata-page count by **30**, leaf writes by **30**, child
edge additions by **621**, custody additions by **347**, ledger reads by
**172** and ledger writes by **173**. Branch writes stayed 39 and total
metadata page reads stayed 703. A source-equivalent page-codec diagnostic
walked the reachable final extent tree: the baseline had 32 leaves plus one
branch for 201 extents, mostly 2–4 records per leaf; balanced packing had
two leaves plus one branch, with 98/103 records. This structural test reads
pages outside the public timed samples. The public `allocated_pages` figure
still covers keyed metadata as well as extent pages, so it is not itself an
exact live extent-page-kind inventory. No extra page read was added to the
public timed path to manufacture that inventory.

The correction met the prospectively declared count gates: append and
repeated had no additional page or ledger work, while dispersed leaf count,
live metadata pages, page/edge churn and ledger I/O all fell. Old roots stayed
readable in the page-codec proof. The same authenticated ledger, temporary
root seal and Local custody edges publish each new page; no page or canonical
Store format changed in this correction.

## Frozen final-run Commit

| Case | Final extent / changed-run count | C1 non-memo loads: stored / draft | `FileInput` record lookups | Replacement reads / bytes |
| --- | ---: | ---: | ---: | ---: |
| Append | 101 / 1 | 2 / 2 | 5 | 1 / 100 |
| Dispersed | 201 / 100 | 2 / 2,026 | 303 | 101 / 101 |
| Repeated | 3 / 1 | 2 / 15 | 6 | 2 / 2 |

These counts were unchanged by balancing. Dispersed Commit's earlier 2,028
`nodes_read` count is **2 stored provider requests plus 2,026 in-memory draft
loads**, not 2,028 Store or physical reads. The 303 `FileInput` lookups are
32-byte spool-record reads. Under the issue's lower-priority rule, no bulk C1
edit algorithm is justified by Store-read work at 100 final runs. C2 SQLite
page-read/write counts and physical-device bytes remain unavailable; Store
file-size growth alone is not a substitute. Exporting C2's counters from
`save/content.rs` would cross this task's assigned source boundary, so that
shared file was left for coordinated follow-up.

## Correctness, raw time and resource limits

All six public routes returned `COMPLETE`, 100 actual FUSE WRITE callbacks,
four upstream Service calls, one explicit Commit, separate independent
old/new-head oracle `PASS`, and SDK Unmount/Delete cleanup `PASS`. The oracle
checked full old and new bytes, exact head parent and path inventory, with
pattern runs 1/100/1. There were zero FUSE READ callbacks in each writer Exec.
The one-byte quota stayed 1 GiB; every checkpoint reported complete backing
and metadata accounting with zero failed payloads. The final balanced
dispersed checkpoint held 409,600 payload bytes and 36,864 metadata bytes;
851,968 bytes were reserved for the existing metadata completion escrow, not
unaccounted allocation. Its Store clone grew 626,688 → 917,504 bytes and
history file stayed 86,016 bytes; these are file sizes, not write counts.

| Case | Baseline Exec / Commit / complete command ms | Balanced Exec / Commit / complete command ms | Baseline / balanced verifier ms |
| --- | ---: | ---: | ---: |
| Append | 386.514 / 27.954 / 2,086.316 | 396.796 / 34.136 / 2,129.294 | 100.148 / 99.852 |
| Dispersed | 461.910 / 64.190 / 1,419.741 | 488.205 / 27.056 / 1,423.901 | 99.657 / 99.546 |
| Repeated | 426.910 / 28.447 / 1,322.137 | 445.027 / 17.743 / 1,318.510 | 97.990 / 95.746 |

Every complete command met 15 s and every independent verifier met 9 s.
The baseline runner/parser and candidate runner were different source seals;
cache residency was uncontrolled in both. These raw walls cannot prove a
speedup or regression. In particular, the dispersed raw Exec observation rose
while its page and ledger counts fell. No rerun was taken to choose a better
number.

## Checks and open gates

- The native tiny-page ext4 test and the source-equivalent 100-dispersed-page
  proof passed. Focused 248/249-page-boundary, 4,097-separated-run and
  level-collapse tests passed, each command under 30 s. The C1 external edit
  test checked `nodes_read = stored + draft` and passed. Warning-denying
  Clippy passed for the three changed packages. The Core product boundary
  guard scanned 318 production files and its nine self-tests passed; the
  Python runner self-check and compile passed.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` still fails
  only on the unchanged `core/crates/layerfs-workspace/src/runtime/state.rs:267`
  attribute wrapping. All changed Rust files passed direct `rustfmt --check`.
  The full Core workspace test suite was not run: this issue's focused-test
  commands have a 30 s ceiling, and the selected native/public proofs cover
  the changed mechanisms. No CI or aggregate preflight exists.
- A direct invocation of the existing `maintenance_frozen` test returned
  `NotPresent` before product execution because it needs a live host Service
  endpoint and keys supplied by its 64 MiB fixture route. That route has a
  separate 60 s bound and was `NOT_RUN` under this issue's under-30 s rule.
  The public Unmount/Delete and absence of a retained-shutdown log are clean,
  but they are not a separate affirmative daemon `workspace closed` or native
  failure-path G1/G2 proof. FLUSH/RELEASE and transport ACK counts remain
  unexported.
- The #248 public 4,097-write gate is `NOT_RUN` at this identity. The separate
  [#266 FUSE EBUSY diagnosis](https://github.com/Ephemeral-AI-Lab/layerfs/issues/266)
  has not established the higher-count refusal fix, and #249 owns the
  30-second product Exec timer. The 4,097 source-equivalent page-codec test
  passed, but it is not the mounted public gate. No timer, benchmark deadline,
  worker count, workload or cache rule was changed to avoid that gate.

## Production LOC and reproduction

Every commit compared its exact first parent and staged tree with unchanged
`tools/production_loc.py` blob `c7dd2b9c6aa9db63393a4ff3ebca9327529d146e`, using
`git archive <revision>` and `python3 tools/production_loc.py --root <snapshot>
--json`. The headline is nonblank/noncomment first-party production source;
tests, docs, runner and generated inputs are excluded. The reference scope
stayed 65,417 and adapter scope 0 throughout; no legacy source was retired.

| Commit | Change | Combined production LOC | Core production LOC |
| --- | --- | ---: | ---: |
| `9a8481d3d` | Prospective workload/format/counters | 123,949 → 123,949 (+0) | 58,532 → 58,532 |
| `8012b4f8c` | Version-2 tiny page and reader | 123,949 → 123,999 (+50) | 58,532 → 58,582 |
| `b83215105` | B+ and C1/FileInput counts, runner | 123,999 → 124,212 (+213) | 58,582 → 58,795 |
| `4f644e950` | Fail-closed preparation allowlist | 124,212 → 124,212 (+0) | 58,795 → 58,795 |
| `5fbda266b` | Raw-log postprocessor and prospective balance rule | 124,212 → 124,212 (+0) | 58,795 → 58,795 |
| `ebfa8f242` | Balanced leaf packing, including branch-packer relocation | 124,212 → 124,233 (+21) | 58,795 → 58,816 |

The result/evidence-only commit has a zero production LOC delta. Recompute
the comparison if any commit is amended or rebased. The exact run and
verification commands, hashes, original statuses and raw stdout/stderr are
in the six retained receipts; the count-only postprocess can be reproduced
without another product sample using `write_patterns.py report` and their
ordered append/dispersed/repeated receipt paths.

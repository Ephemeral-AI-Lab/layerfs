# Handoff prompt — make the #232 1 MiB Exec/FUSE edit crossing-lean (v0.1.7 Core)

You are taking over a **measured** diagnosis, not a hunch. Read this whole file
before touching anything. Then read, in this order:
`AGENTS.md`, `benchmark/AGENTS.md`, `core/AGENTS.md`,
`core/benchmark/fs-bench-pro/AGENTS.md`, `docs/general/benchmark_rules.md`,
`core/docs/benchmark/fs-bench-pro/exec2edit.md`, `core/docs/issues/232/SPEC.md`,
`core/docs/issues/232/exec-fuse-edit-v2-baseline.md`.

## 0. Mission, restated honestly

Owner ask: *"iterate recursively on the 1 MiB case to ensure v0.1.7 is optimized
and no round trips."*

"No round trips" is not reachable and must not be chased literally: the declared
operation is *one public `WorkspaceApi::exec` of a POSIX edit through FUSE, then
`WorkspaceApi::commit`*, and that contract requires a host→sandbox control call,
a process inside the sandbox, and an upstream publication path. What you must
drive to its **floor** is the number of crossings and the work each one causes.

**Frozen crossing budget for `overwrite-middle-4k-on-1mib-ops-1-exec-v2`:**

```
                                                        today   floor   lever
  control  host→sandbox  WorkspaceExec                      1       1    required
  control  host→sandbox  WorkspaceCommit                    1       1    required
  upstream sandbox→host  Inspect (baseline refresh)         1       0    R2
  upstream sandbox→host  EditFile   (content save)  ┐       1  ┐
  upstream sandbox→host  UpdatePortableMetadata     ┘       1  ┘ 1    R1 (merge)
  upstream sandbox→host  HistoryCommand (composite)         1       1    required
  upstream sandbox→host  ReadFile (base blocks)             0       0    n/a (overwrite)
  --------------------------------------------------------------------------
  crossings inside the timed window                         6       4
```
For the 20 shift cases add `1 per declared 128 KiB block` (today **2.0** — see §2).

**Honest ceiling:** in the frozen 1 MiB overwrite, crossings account for 16.92 ms
of 34.37 ms (49.2 %); the other 17.45 ms is sandbox-side and unmeasured. Fixing
the two redundant crossings is worth ≈ 5.4 ms (→ ≈ 29 ms). Do not declare victory
at 29 ms, and do not declare the sandbox share out of scope: instrument it (§4).

**Do not** present any of this as a matched v0.1.6/v0.1.7 regression, and do not
change the registered target (`g2_target_ms: 5.63`, `TARGET_MISS` as recorded).

## 1. Frozen baseline you must reproduce before changing anything

Case `overwrite-middle-4k-on-1mib-ops-1-exec-v2`, retained LFT1
(`benchmark-results/fs-bench-pro/sdk-exec-fuse/edit_length_preserving/<case>/telemetry.lft1`):

```
sdk.edit_commit.fuse            34.37 ms   (driver edit_commit_ns 34 369 375)
├─ edit (WorkspaceApi::exec)    15.55 ms   ── Inspect 2.78 ms + UNASSIGNED 12.77 ms
└─ commit (WorkspaceApi::commit)18.82 ms   ── EditFile 5.53 + UPM 3.71 + HistoryCommand 4.90
                                              = 14.14 ms named, UNASSIGNED 4.67 ms
   within EditFile: service.begin_save 1.57 / service.edit 1.47 / service.finish 1.78
   within UPM:      service.begin_save 0.53 / service.metadata 1.19 / service.finish 1.65
   within HistCmd:  history.role_file 0.52 / begin_save 0.49 / validate 0.09 / inodes 0.09
                    / history.finish 1.50 / unassigned 2.20
status: INELIGIBLE (fuse-backing domain not invalidatable) — this does not change.
```
1 MiB tier for context: 10 non-structural rows 33.19–44.12 ms, all with exactly
4 crossings; 5 structural rows 117–274 ms with 11–20 crossings.

Reproduce every count with the packaged analyzer (evidence re-reading, no workload):

```
python3 <this dir>/analyze_roundtrips.py \
  --repo /path/to/checkout --results benchmark-results/fs-bench-pro/sdk-exec-fuse --case 1mib
```
It also prints the identity that makes the counts trustworthy: **the product's own
`upstream_calls` counter equals the number of upstream Service ops inside the timed
window, 46/46 completed cases.** Use it as your round-trip meter.

## 2. Mechanisms already pinned (do not re-derive)

| lever | mechanism, with source | measured prize |
|---|---|---|
| **R1** merge content+metadata save | daemon commit issues two upstream ops per dirty file: `core/crates/layerfs-workspace/src/commit/save.rs:98-120` (`EditFile`/`ConstructFile`) then `:155-175` (`UpdatePortableMetadata`/`ConstructPortableMetadata`), then one composite at `commit/operation.rs:36-50`. Dispatch table has no combined op: `core/crates/layerfs-server/src/service/handler.rs:225-271` | 2.18 ms (UPM `begin_save` 0.53 + `finish` 1.65) + 0.45 ms gap ≈ **2.6 ms**, and it is per file |
| **R3** fetch once per block, not once per request | tool does 1 `read_exact_at` + 1 `write_all_at` per declared block, `SHIFT_BLOCK_BYTES == MAX_READ_BYTES == 128 KiB` with the comment *"one block is one projection request"* (`core/benchmark/fs-bench-pro/workload/src/main.rs:29-32,180-205`; `core/crates/layerfs-workspace/src/types.rs:6`). Measured: **≈2.0 projection requests per declared block**, and `FUSE read == upstream ReadFile` **1:1** in every completed structural case. So the multiplier is between the tool's transfer and the projection request (kernel split/readahead or short-read retry), and the daemon then pays one upstream fetch per request | up to ~30 ms of the 105.85 ms 1 MiB-insert exec; up to ~1.4 s of the 2.73 s 10 MiB-insert edit |
| **R2** no extra round trip for the baseline refresh | the mutation path refreshes the original before writing: `core/crates/layerfs-workspace/src/filesystem/write.rs:179` → `serial_original` → `filesystem/original.rs:88-115` (`inspect_view` → `Operation::Inspect`); the read path has the same refresh at `filesystem/read.rs:72-90`. Exactly **1 Inspect per mutated file**, 57/57 — it is not a stray `getattr` | 2.78 ms host span (lower bound) |

## 3. Non-goals and hard rules (violating any of these voids the iteration)

- Do **not** move the mutation off FUSE (no SDK range edit, no host-side write, no
  temp-copy/rename substitution, no `docker exec` as an edit path). The row must
  stay a Workspace Exec/FUSE claim.
- Do **not** skip the baseline refresh unless the independent verifier still proves
  the exact canonical result — correctness first, counts second.
- Do **not** rewrite, relabel, move or "fix" the frozen v2 receipts, the baseline
  report, the registry targets or the #232 spec. New evidence goes to new paths.
- Do **not** close #232, raise budgets or timeouts, change worker counts, warm
  caches, or turn `INELIGIBLE` into `PASS` by priming anything.
- **One performance sample per source identity per case**, fresh `--output`,
  `--verification skipped` during collection plus a separate `verify-edit`.
  Never rerun an arm for a nicer wall; keep every `FAIL`/`INELIGIBLE`/`NOT_RUN`.
- Diagnostic runs are allowed and encouraged, but they must be **labelled**,
  **count-driven**, and their wall time must never be promoted or compared.
- No telemetry writes into the mounted workspace while the timer runs (that would
  perturb the measured operation). No benchmark-only branch in product code
  (`core/AGENTS.md`): counters and spans are ordinary product telemetry.
- No third-party patches/vendoring; `--locked` builds; aarch64 AEAD flags come
  from the repository-root `.cargo/config.toml`; production LOC accounting
  (`Production LOC: <before> -> <after> (delta …)`) in every commit; core checks
  (`cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked`,
  `python3 core/tools/check_product_boundary.py`,
  `python3 -m unittest discover -s core/tools -p 'test_*.py'`). No `preflight.sh`,
  no CI claim.

Work in a **fresh worktree/branch** (leave
`/Users/yifanxu/.codex/worktrees/bb50/layerfs` at `b0bd10422`, clean). Suggested:
`codex/issue232-roundtrip-1mib` from the current HEAD.

## 4. The recursive loop (this is the whole method)

```
        ┌──────────────────────────────────────────────────────────────┐
        │  iterate ONLY on counts; walls are gate-only                 │
        └──────────────────────────────────────────────────────────────┘
 (a) INSTRUMENT   add/extend the counter or span for the term you are about to
                  claim, on the public route (status / telemetry), never inside
                  the timer's hot path beyond what the product already pays
 (b) ATTRIBUTE    run the case ONCE as a labelled diagnostic; re-read counts;
                  if a term is still unattributed, go back to (a)
 (c) CHANGE       exactly one lever, smallest change, no unrelated edits
 (d) VERIFY       counts moved as predicted AND the independent verifier passes
 (e) DECIDE       keep or revert; write the counts before/after; next term
        └──────────── repeat until the crossing budget (§0) is met or you hit a
                      stop condition, then freeze and take the gate sample
```

Stop conditions: crossing budget met *and* no single named term above ~3 ms;
or five consecutive iterations with no count improvement; or any verification
failure; or a change that would require breaking §3. Report the stop reason.

**Instruments, in this order:**

1. **D3 — projection request/byte counter (cheapest, settles R3).** Extend
   `ProjectionCounters` (`core/crates/layerfs-workspace/src/filesystem/projection_counters.rs:70-100`)
   with per-op request counts *already there* plus **byte totals and a request-size
   histogram**, and surface them through the existing post-timer public
   `WorkspaceApi::status` (`core/crates/layerfs-api/sdk/src/workspace.rs:122-158`).
   Answer: is one 128 KiB transfer split into 2 requests by size, and what are the
   sizes? `SPEC.md` §2 already declares byte totals a product prerequisite.
2. **D1 — retain the daemon's `role=2` LFT1** (biggest unknown: 12.77 ms exec +
   the sandbox share of every fetch). The daemon already emits
   (`core/crates/layerfs-daemon/src/config.rs:11-16`, `execution.rs:61,86`,
   `control.rs:141-152`); the records are lost because `Forward` writes to that
   process's stderr (`core/crates/layerfs-telemetry/src/output/queue.rs:322-334`)
   and `SandboxApi::delete` removes the container. Preferred fix: capture the
   container's stderr **before** delete, in the runner (a new harness identity, so
   it needs its own prospective note — it is an instrument, not a product change).
   Answers `daemon.exec_spawn` vs `daemon.exec_output` and per-request proxy spans.
3. **D2 — commit phase spans.** The labels already exist
   (`core/crates/layerfs-workspace/src/commit_types.rs:13-19`); add timing around
   `commit/operation.rs:28,36`, `commit/save.rs:98,155,283`,
   `commit/completion.rs:241,286,291` and publish them. Dissolves the 4.67 ms
   commit gap into named phases.

**Levers, in this order (each with its own acceptance criterion):**

```
 R1  upstream requests per dirty file      2 -> 1
     host save work for the pair           9.24 ms -> <= 9.24 ms (no regression)
 R3  projection read requests per block    ~2.0 -> 1.0, with FUSE read == ReadFile
     upstream base fetches (shift cases)   2/blk -> 1/blk ; bytes verified unchanged
 R2  Inspect per mutated file              1 -> 0 additional round trips
     canonical result                       verified identical by the oracle
 R6  (separate, product) 5 s silent-wait  turns 11 FAILs into measurable rows;
     not a latency fix and not a benchmark tune — its own decision + evidence
```

## 5. Running the case (frozen harness contract)

```
python3 core/benchmark/fs-bench-pro/runner.py run \
  --case overwrite-middle-4k-on-1mib-ops-1-exec-v2 \
  --out benchmark-results/fs-bench-pro/<new-evidence-dir>/<case> --verification skipped
python3 core/benchmark/fs-bench-pro/runner.py verify-edit \
  --run benchmark-results/fs-bench-pro/<new-evidence-dir>/<case>
python3 core/benchmark/fs-bench-pro/runner.py report-edit \
  --runs benchmark-results/fs-bench-pro/<new-evidence-dir> \
  --out benchmark-results/fs-bench-pro/<new-evidence-dir>
```
The exact driver invocation that produced the frozen row is in the case receipt's
`command` array — reuse it. Release driver, release image, 2 CPU / 512 MiB sandbox,
`LAYERFS_CONSTRUCTION_WORKERS=1`, complete-command budget ≤ 15 s, verifier < 10 s.
Per-iteration evidence dir must hold `perf.jsonl`, `telemetry.lft1`,
`driver-receipt.json`, `receipt.json`, the verifier output, and a short note with
the crossing table before/after and the identity seals.

## 6. Reporting and completion

- Append-only ledger entry per iteration: counts, identities, command, arithmetic,
  every non-passing line. Report `FAIL`/`INELIGIBLE`/`TARGET_MISS` as plainly as `PASS`.
- Final gate: one performance sample at the frozen identity + separate
  identity-matched verification + `report-edit`. The row stays `INELIGIBLE` unless
  the *cache contract* changes, which is a scenario-version decision, not yours.
- State plainly what improved in **counts** and what did not improve in **wall**.
  If `edit_commit_ns` stays above 5.63 ms, that is `TARGET_MISS` as recorded — do
  not move the target, do not re-label, do not pool with G2.
- Hand back: crossing table old→new, the final waterfall with unknown spans marked,
  the diagnostic limits (single sample, uncontrolled cache, identity differences),
  and the smallest next lever.

## 7. Traps that will waste your time

1. Chasing "6 → 4 crossings" and stopping: that is ~5.4 ms of ~34 ms. The sandbox
   share is bigger and needs D1.
2. Treating the `Inspect` as a cache miss: it is the mutation path's required
   baseline refresh; removing it changes correctness, so prove it with the oracle.
3. Explaining the 2× per block as "the daemon fetches twice": it does not —
   `FUSE read == upstream ReadFile` 1:1. The splitter is upstream of the projection
   request. Measure it (D3) before "fixing" the daemon's fetch path.
4. Writing telemetry or scratch files into the mounted workspace: it perturbs the
   measured operation.
5. Using a diagnostic wall as a result, or running the arm twice for a better one.
6. Assuming the frozen receipts are committed: `benchmark-results` is in
   `.git/info/exclude`. Copy anything a report depends on into a committed
   evidence directory, or it will be lost the way the v0.1.6 G2 receipts were.


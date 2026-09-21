# Benchmark hosting

## Scoped v0.1.6 replacement experiment

For the owner-directed `v016-local-snapshot-experiment-v1` only, follow
`docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md` ahead of conflicting
legacy hosting/sampling text below or in the general guide/quick start. The
candidate may own mutable Workspace metadata, snapshot generations and temporary
payload backing in the sandbox. SQLite, the benchmark/SDK coordinator, canonical
construction and publication remain on the macOS host. Use **one** Commit compute
worker for commit, capture and snapshot, and one performance sample per case/arm; no
n3/repeated-sample campaign. The released small-content path used four workers
(`SMALL_CONTENT_WORKERS = 4`) and `construction_worker_limit()` still defaults to
available parallelism, so a **performance drop against v0.1.5 is expected**: it is
accepted by the bounded acceptance rule (sub-50 % or sub-10 ms), never repaired by
adding workers. Single-worker must become the product default during this campaign,
not merely an exported variable.
**`init_namespace` is the one case allowed multiple workers/threads** — its
initialization path (`initialize_layerstack`, `prepare_parallel_root_directories`)
keeps its parallelism and its 2.7 s cold Init target; every other family's measured
work must be single-worker. This exception does not change unrelated benchmark or
release contracts.

## Existing profiles

- SQLite, the SDK/benchmark coordinator, canonical construction/Commit publication, and physical spool backing must run on the macOS host.
- The approved #49 rewrite may place the live Workspace operation core with the Linux daemon/FUSE runtime. This is execution-side filesystem state, not a container-side SQLite Store, benchmark coordinator, or canonical publication service.
- Docker runs only the Linux daemon, FUSE, and workload helper. Never run or restore Docker-owned SQLite, prepared Store images, or a container-side benchmark coordinator.
- Migrate unsupported families to host execution; never add a Docker fallback or use a historical revision to bypass this prohibition.
- Historical Docker results remain unchanged and apply only to their recorded topology.
- Use the current fs-bench-pro family entrypoints and follow `docs/general/benchmark_rules.md` and `fs-bench-pro/QUICKSTART.md`.

## Measurement cache discipline

Reuse preparation proactively; never let reuse or residual warmth credit a
measured phase. `../AGENTS.md` §1–2 states the rule; this section names the
mechanics that exist in this tree.

Setup reuse is `--setup clone`; verification reuse is `--reuse-pass`; builds and
images reuse through their seals. There is no bare `--reuse` flag.

Reuse this (do, and say so in the report):

- `--setup clone` — the default and required choice for every
  post-initialization case. The runner takes the closed, validated prepared
  master and hands the sample an independent writable byte copy
  (`closed_store_copy`; deliberately a byte copy, not an APFS clone), so no sample
  re-pays preparation. `--setup fresh` exists only for initialization and
  fresh-output cases, where the runner rejects `clone` outright. Preparation runs
  automatically on a cache miss: never run a family's `setup.sh` before every
  sample, never clear protected caches routinely, never reuse a mutated sample.
  Paired arms use the identical qualified Store artifact and each mutation sample
  gets its own fresh writable copy (hard links to the master are forbidden).
- `--reuse-pass <verification.json>` — accept one identity-matched
  `status=PASS`/cleanup-`PASS` verification instead of re-running it. It fails
  closed on schema, identity, hard-limit or wall mismatch, and records
  `reused_proof_identities` plus an explicit omission.
- incremental host builds, the shared Cargo target, image layers keyed by the
  compilation seal, immutable `binary-archive/<sha256>/` executables, and
  `--prune-builds KEEP` for retention. A host-only Python/shell change may reuse
  an image whose compilation seal still matches, but needs a new host identity.

A clone is setup reuse, **not** a cold claim: the receipt records
`clone_method: closed-quiescent-byte-copy` and `master_unchanged`, and the
QUICKSTART states plainly that clone is not an APFS clone and not a cold-OS-cache
claim. Declare the method, treat ordinary OS-cache effects consistently, never
pool clone and fresh rows, and never let the master's or the clone's warmed pages
credit a timed phase.

Never: warm starts, replaying an old receipt as a new sample, moving cold product
work into setup, priming the paths a timed phase will read, dropping caches for
one arm only, or pooling cold and warm rows. Anything reused must be visible in
the receipt (`build_mode`, `dependency_reuse`, `clone_method`,
`reused_proof_identities`, `cache_contract`, `swap_current_bytes`-style domains).

### Budgets

- Prepared inputs are acquired **once per campaign and reused** with identity
  checks: no per-selection fixture or image rebuild, no repeated setup before a
  sample.
- A performance selection's **complete command** (product timer + container
  lifecycle + cleanup) is **≤ 15 s**; a small declared exception list may take up to
  **25 s** (the 500 MiB and 25k members are the usual candidates). Declare the
  exception with the measured wall time in the report — no sign-off blocks the run.
- Verification stays **under 10 s**, and typically takes a fraction of a second.
  There is no 60 s allowance to grow into.
- A selection that cannot fit is either reused from a qualifying receipt with its
  evidence cited, or recorded as `NOT_RUN` with its measured wall time and the reason.
  Never move work outside the timer, enlarge a timeout, or reduce the workload to fit.

What the harness already enforces, and where an agent must not fight it:

- `shared/cold.py` (`namespace-100000-cold-v2`,
  `darwin-shared-mmap-invalidate-mincore-v1`) invalidates source data pages
  before the timed sample and verifies whole-input residency: any resident page
  makes the row `INELIGIBLE`. Acquisition time is reported outside the operation
  timer. That is the model to follow for a family that needs a declared cache
  stance; an `INELIGIBLE` row keeps its raw timing and is not admission evidence.
- Memory must stay domain-separated: cgroup anonymous, file cache, dirty,
  writeback, shmem, kernel/slab and sockets, plus spool and Store disk as
  separate fields. A cgroup `memory.peak` is a lifetime total unless it was
  verifiably reset after setup; without a fresh or resettable cgroup with a
  verified reset/readback the phase peak is *unavailable for the gate*, so the
  row is `INCOMPLETE` — not `PASS` and not `FAIL`. Bounded process heap never
  excuses file-size-proportional spool or cgroup page-cache growth.
- Per-sample checklist: one sample per case/arm; fresh `--output`; declare the
  cache stance; pin every identity; verify separately with those identities; and
  append the exact numbers, limits, arithmetic and reproduction command to the
  active ledger, including every non-passing line.
- **Do not sample.** No repeat of an arm to confirm stability, characterise spread
  or replace an inconvenient number, and no iterative verify/test loop. Diagnose an
  anomaly from the receipts already taken, or with a labelled diagnostic that
  measures the cause on a count-driven instrument. See root `AGENTS.md` §3.1.

Worked example: `#151` B2 (ledger L18) — a 500 MiB payload written through FUSE
was read back inside the measured Commit, so the transfer looked like 19 GB/s and
the sandbox's reported memory looked like the payload itself. The spool now keeps
a bounded resident window (≤2.6 MiB measured), the transfer pays a storage read,
and six identical runs produced container lifetime peaks from 24.6 MB to 189.8 MB
— which is why a lifetime cgroup number cannot decide a memory gate.

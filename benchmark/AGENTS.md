# Benchmark hosting

## Scoped v0.1.6 replacement experiment

For the owner-directed `v016-local-snapshot-experiment-v1` only, follow
`docs/roadmap/0.1/0.1.6/sandbox-local-snapshot-spec-and-plan.md` ahead of conflicting
legacy hosting/sampling text below or in the general guide/quick start. The
candidate may own mutable Workspace metadata, snapshot generations and temporary
payload backing in the sandbox. SQLite, the benchmark/SDK coordinator, canonical
construction and publication remain on the macOS host. Use one Commit compute
worker and one performance sample per case/arm; no n3/repeated-sample campaign.
This exception does not change unrelated benchmark or release contracts.

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

There is no bare `--reuse` flag: setup reuse is automatic (the prepared-input
cache prepares on a cache miss and is keyed by content/format compatibility), and
`--reuse-pass` reuses one identity-matched verification PASS. Builds and images
reuse through their seals.

Reuse this (do, and say so in the report):

- `--reuse-pass <verification.json>` — accept one identity-matched
  `status=PASS`/cleanup-`PASS` verification instead of re-running it. It fails
  closed on schema, identity, hard-limit or wall mismatch, and records
  `reused_proof_identities` plus an explicit omission.
- prepared immutable inputs — `_host_acquire` prepares automatically on a cache
  miss; never repeat setup before every sample, never clear protected caches
  routinely, never reuse a mutated sample. Each mutation sample gets its own
  fresh writable copy, and paired arms use the identical qualified Store
  artifact.
- incremental host builds and the shared Cargo target, image layers keyed by the
  compilation seal, immutable `binary-archive/<sha256>/` executables, and
  `--prune-builds KEEP` for retention. A host-only Python/shell change may reuse
  an image whose compilation seal still matches, but needs a new host identity.

Never: warm starts, replaying an old receipt as a new sample, moving cold product
work into setup, priming the paths a timed phase will read, dropping caches for
one arm only, or pooling cold and warm rows. Anything reused must be visible in
the receipt (`build_mode`, `dependency_reuse`, `clone_method`,
`reused_proof_identities`, `cache_contract`, `swap_current_bytes`-style domains).

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

Worked example: `#151` B2 (ledger L18) — a 500 MiB payload written through FUSE
was read back inside the measured Commit, so the transfer looked like 19 GB/s and
the sandbox's reported memory looked like the payload itself. The spool now keeps
a bounded resident window (≤2.6 MiB measured), the transfer pays a storage read,
and six identical runs produced container lifetime peaks from 24.6 MB to 189.8 MB
— which is why a lifetime cgroup number cannot decide a memory gate.

# AGENTS.md

Repo-wide rules for coding agents working in `layerfs`. This file routes; it does
not replace the normative documents below, and where they disagree with this page,
they win.

Read before touching measurement, benchmark or release work:

- [`docs/general/benchmark_rules.md`](docs/general/benchmark_rules.md) — the measurement contract
- [`benchmark/AGENTS.md`](benchmark/AGENTS.md) — benchmark-tree specifics and the v0.1.6 exception
- [`benchmark/fs-bench-pro/QUICKSTART.md`](benchmark/fs-bench-pro/QUICKSTART.md) — build, reuse and run mechanics
- [`docs/general/release-policy.md`](docs/general/release-policy.md), [`docs/general/documentation-policy.md`](docs/general/documentation-policy.md)

## 1. A warm cache must never credit a measured phase

The harness runs timers; the OS, the Store and the page cache are not part of the
product's work. Two rules follow, and they are absolute:

**A measured phase pays for its own work from a declared cache state.** If the
machine had just booted and no page cache held this process's data, would the
work still have to happen inside the timed phase? If yes, the phase must pay for
it — every run, in every arm.

Forbidden, without exception:

- letting pages left resident by setup, preparation, an earlier sample, another
  arm or another phase serve a timed phase (a transfer that reads its own recent
  writes out of cache is the canonical case: it measured 19 GB/s instead of the
  2.1 GiB/s the same bytes cost from storage);
- pre-touching, priming, warming or "just checking" the paths a timed phase will
  read, or priming selected ranges with expected-result data;
- measuring arm A warm and arm B cold, or invalidating caches for one arm only —
  cache state MUST be declared and enforced equally, and cold and warm rows MUST
  NOT be pooled;
- re-running a case until a warm variant produces a passing number and reporting
  only that run;
- quoting a lifetime counter as a phase number (cgroup `memory.peak` is a
  lifetime total unless it was verifiably reset after setup);
- excusing file-size-proportional spool or cgroup page-cache growth because the
  process heap is bounded.

Allowed, and expected: untimed, deterministic, recorded preconditioning;
preparing pristine fixtures once outside the measured child; using the harness's
own cold contract — for the families it covers, `shared/cold.py` invalidates
source data pages before the timed sample and checks whole-input residency, so a
row with resident pages is reported `INELIGIBLE` rather than quietly fast.

If a phase's cache state is undeclared, unknown, or different between arms, its
number is `INCOMPLETE` or `INELIGIBLE` — never `PASS`. Say so in the receipt and
in the report.

## 2. Reuse preparation proactively; never reuse measurement

Agents MUST reuse preparation rather than repeat it, and MUST NOT let that reuse
reach inside a timed phase. The test is where the saved work lives: reuse that
removes work **outside** the timers is required; reuse that removes work **inside**
a timed phase is cheating.

Reuse this, proactively:

- `--reuse-pass <verification.json>` — accept one `status=PASS`, cleanup-`PASS`,
  identity-matched verification receipt instead of re-running verification. It
  fails closed on any identity, schema, hard-limit or wall mismatch and records
  `reused_proof_identities` plus an explicit omission.
- prepared immutable inputs — acquisition runs automatically on a cache miss.
  Do not repeat setup before every sample, do not clear protected caches
  routinely, and never reuse a mutated sample: each mutation sample gets its own
  fresh writable copy, and paired arms use the identical qualified Store
  artifact.
- incremental host builds, the shared Cargo target, image layers keyed by the
  compilation seal, and immutable `binary-archive/<sha256>/` executables
  (a host-only Python/shell change may reuse an image whose compilation seal
  still matches; `--prune-builds` retains owned targets).

Never: warm starts, replaying a previous receipt as a new sample, moving cold
product work into setup, or treating a cached acquisition as evidence about the
measured operation. Anything reused MUST be visible in the receipt
(`build_mode`, `dependency_reuse`, `clone_method`, `reused_proof_identities`,
`cache_contract`) and stated in the report.

## 3. Running a measurement

1. One sample per case per arm unless an owner-approved campaign says otherwise.
   No n3 and no best-of selection. Diagnostics are allowed and MUST be labelled
   as diagnostics and reported alongside the gate sample.
2. Fresh `--output` path per run; receipts are append-only evidence and are never
   overwritten. Failures, `INELIGIBLE` rows and discarded attempts stay on disk.
3. Pin identities: source commit/seal/tree, product, compilation and dependency
   seals, image ID, harness identity, workload-source hash. A rebuilt artifact
   needs a rebuilt matched arm; a harness change invalidates the pair.
4. Verify separately, in verification mode, with the exact identities from the
   performance receipt. A performance PASS is not release admission.
5. Respect the measurement lock: never overlap resource-sensitive work, never
   interrupt another owner's run.
6. Record it: append an entry to the active ledger
   (`docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md` and its
   successors) with exact numbers, limits, the arithmetic, the identities, the
   reproduction command, and every non-passing line. Report FAIL, INCOMPLETE and
   unrun work as plainly as PASS.

## 4. Code, build and docs

- CI-exact before pushing: `cargo +1.96.0 fmt --all --check` and
  `cargo +1.96.0 clippy --workspace --locked -- -D warnings`; run the affected
  tests. Keep the tree clean for sealed builds — a dirty source seal is recorded
  and cannot be compared against a sealed arm.
- No new dependencies when an existing crate already provides the capability;
  keep platform-specific code `cfg`-gated and behind a no-op fallback.
- Never claim durability the contract does not provide: no `fsync`/`fdatasync`/
  `sync_data`/`sync_all` on Workspace backing, and memory hints are hints.
- Documentation states measured facts, limits and open rulings; roadmap READMEs
  link to the ledger rather than paraphrasing numbers.

## 5. Why these rules exist (worked example)

`#151`'s B2 case wrote a 500 MiB payload through FUSE into the sandbox, which
then read it back during the measured Commit. Because the workload's own writes
had left that payload resident, the transfer looked like 0.026 s (19 GB/s) and the
sandbox's memory looked like the payload itself (563 MB container peak, of which
524 MB was `file` cache and 4.6 MB anonymous). Both readings violated this page:
one was cache-credited, the other was page-cache growth proportional to file size.
After the spool was given a bounded resident window, the sandbox's own residency
fell to ≤ 2.6 MiB and the same transfer cost a storage read — the honest price,
which then showed up as a Commit-phase FAIL against a limit derived from a
cache-served control. Six identical runs also produced container lifetime peaks
from 24.6 MB to 189.8 MB, which is why a lifetime cgroup number cannot decide a
memory gate. See
[`issue151-experiment-ledger.md`](docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md)
L18.

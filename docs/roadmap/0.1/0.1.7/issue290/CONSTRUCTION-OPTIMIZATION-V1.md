# Construction optimization: statement reuse and known-size small input

Status: Research; informative and not a product contract.

Owner2026-10-01 requests concrete time optimization following attribution.
Parent8c295e99f7bee3bb2915a5c205a8e5d7f64e0ad4, issue#291. This prospective
mechanism comparison is separate from the completed attribution diagnostic and
original full-import receipts; those sources/results remain unchanged. No
full-corpus speed arm, upload, codec/profile/worker increase or product changes.

## Registered comparisons

One attempt per case/arm. Commit this spec before implementation and freeze clean
release binaries before collection. These external prototype mechanisms use public
C1/C2/rusqlite APIs, one producer, current formats/limits and private MEMORY/OFF,
cache512KiB/mmap0, no fsync, no retries. No throughput or cold admission because
OS/file cache residency cannot be enforced/observed. Report numerical rows as
INELIGIBLE/performance_claim=false, including both passing/failing command results.

- O-root-update-uncached-v1 / O-root-update-cached-v1: all103,108 original file
  roots applied once into independently prepared catalogs. SQL calls are exactly
  the same UPDATE/binds/256-file COMMIT+BEGIN cadence; only statement preparation
  changes (Connection.execute versus prepare_cached). Stream paths/roots from
  immutable verified reference, no population vector. This isolates the root
  UPDATE mechanism; original interleaved821 object transactions/codec/file reads
  are excluded in both arms.15s each complete command. Measure row enumeration,
  UPDATE time/calls, transaction time/calls, complete wall, user/system CPU.
- O-small-stream-v1 / O-small-sized-v1: every101,494 nonempty regular file below
  128KiB,523,127,919 bytes, from the original closed master. Each arm gets one
  independently copied pristine tree with the same relative paths; no APFS clone
  or source hardlinks. Acquire both trees together once, validate SHA256 against
  independent captured manifest while copying, preserve acquisition time.
  Stream arm calls public construct_stream with current128KiB threshold probe;
  sized arm allocates exactly known file size, read_exact then one-byte exact EOF
  check, and calls public construct_bytes. One file buffer <=131071 bytes; input
  is the certified closed fixture, not mutable live Workspace metadata. Every
  byte/EOF is paid inside the arm, no pre-reading/hash/body priming before timers.
  Both arms use a bounded discarding consumer and compare every resulting root
  against the independently reconstructed reference root. No compression, CAS
  SQL writes or packs in either arm; this isolates acquisition/C1 small-file work.
 25s each complete command, prospectively declared large-count exception to15s.

Record input calls/bytes, C1 inclusive, consumer span, source open/close,
whole arm, errors, exact root cardinality and bounded requested capacity:
stream cutoff131072 per file versus sized actual file length. Cumulative
requested capacities are allocator demand, not actual memory/peak or speed.
No allocator hooks. Timing instrumentation same between source arms. EOF check
must fail on trailing or short input; no trusting a file size to skip correctness.
The sized API preserves canonical roots; no implicit platform/workload shortcut
in production. Statement reuse is applied to the external import example's
normal root update, retaining an explicit baseline only in this experiment.

## Proof, custody and interpretation

Fresh output paths and per-worktree lock. Setup is not a cold claim: independent
copies may leave pages resident, and inter-arm cache equality is unverified.
Metadata/reference identity checks happen before commands and can warm metadata;
source bodies are not checked again immediately before timed arms. No quantitative
speed PASS; measured mechanism outcomes plus structural count reduction inform
which treatment to pursue. Never rerun an unchanged arm for a nicer result.

SQL proof separately <=10s per arm: every root and original metadata row equals
reference in both directions, no missing/unexpected entries, exact103,108 updates.
Small-source arm records root equality for every file and an untimed command
receipt validates101,494/523,127,919 totals; exact EOF/read errors retained. Source
hash acquisition/reference reconstruction are independent byte oracles. Tiny
external correctness fixture covers sized path boundaries plus short/trailing
refusal; no product test hooks or inline tests. No full unchanged reconstruction.

Require8GiB extra free disk; new fixtures total1,046,255,838 logical file bytes,
catalogs bounded separately and retained privately; no unrelated artifacts removed.
Close all workers/connections; no server started. Preserve every attempt, commands,
source/build/corpus/reference seals, cache limits, raw metrics and LOC per commit.
No future-sha guesses. Final outcome distinguishes mechanical benefit from an
unrun full-import performance qualification. A >25s full import remains outside
this comparison, not silently resized or rerun under the180s diagnostic deadline.

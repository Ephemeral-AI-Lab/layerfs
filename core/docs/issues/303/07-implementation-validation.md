# 07 — Implementation and validation plan

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. This is a plan: no code was written and no build, test or
> benchmark was run. "Measured" below means counted by the repository's LOC
> counter on that commit. Every "after" size is an estimate and is labelled as
> one. Claim labels are defined in the [entry point](README.md#claim-labels).

## 1. Starting point

[implemented and source-verified]

- `main` at `f96d97651be5299f153ccde2bc8d921dd58807ad`.
- The cluster two crates (`layerfs-workspace`, `-fuse`, `-daemon`, `-bridge`,
  `-sandbox`, both `layerfs-api` packages) and `layerfs-server` are in `exclude`
  in `core/Cargo.toml:19-28`. They are byte-identical to `7edddbdb8`, have no lock
  entries, and are not built or tested by the prescribed
  `--manifest-path core/Cargo.toml` commands.
- `layerfs-server` and the SDK import `layerfs_storage::Store` and
  `layerfs_history::sqlite`, which cluster one no longer exports. This comes
  from reading imports and exports; nothing was compiled.
- Production LOC, `python3 -B tools/production_loc.py --detail` (counter
  SHA-256 `c0fe7f36…624adb`): core 74,000 in 449 files; reference 65,417;
  combined 139,417.

| Crate | Production LOC (measured) |
| --- | ---: |
| `layerfs-workspace` | 26,835 |
| `layerfs-bridge` | 6,834 |
| `layerfs-server` | 3,996 |
| `layerfs-daemon` | 2,151 |
| `layerfs-fuse` | 1,447 |
| `layerfs-sandbox` | 1,106 |
| `layerfs-api` (two packages) | 796 |
| **Cluster two scope** | **43,165** |

## 2. Build structure

[proposed design]

- New and rewritten crates join the `members` of `core/Cargo.toml` when they
  build, one at a time, so the prescribed core commands cover them.
- `layerfs-overlay` and the semantic part of `layerfs-workspace` contain no
  Linux-only code. They build and test on the macOS host, linked against the
  system SQLite like `layerfs-persistence`. Only the daemon image enables
  `rusqlite`'s `bundled` feature, as a Linux-target dependency. Slice S0 must
  confirm that this does not change the features of the host build; if it
  does, the daemon image builds from its own manifest.
- `layerfs-fuse` and `layerfs-daemon` keep their Linux code behind
  `cfg(target_os = "linux")`, as today.
- The dormant `layerfs-workspace` is moved to `layerfs-workspace-legacy` in S1
  so the replacement can be built at its final path. That move is relocation:
  production LOC delta 0, labelled as such. The legacy crate stays excluded and
  is deleted whole in S11.
- Every production file stays under 1,000 physical lines and every `lib.rs`
  and `mod.rs` under 200. Product code only under `src/`; tests under `tests/`.

## 3. Slices

[proposed design] Each slice is the smallest complete vertical piece. A slice
is done when its exit condition holds and its commit carries the production LOC
line. "Core checks" means, from the repository root:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

| Slice | What changes | Needs | Verification | Exit |
| --- | --- | --- | --- | --- |
| **S0 Contracts** | Owner answers to O-1 … O-5. A cluster one follow-up issue for prerequisites P1, P3–P7 of [06 §6](06-cluster-one-integration.md#6-prerequisites-outside-cluster-two). Two build probes, as labelled preparation: `layerfs-content` and a bundled-SQLite crate for `aarch64-unknown-linux-musl` | — | The two probes compile `--locked`; host feature set unchanged | Placement (K2) and profile (K3) confirmed or replaced by their stated fallbacks |
| **S1 Scaffolding** | Move the dormant Workspace crate to `-legacy`. Add `layerfs-overlay` with its real profile and schema code, not a placeholder | S0 | Core checks; a settings test in which every `PRAGMA` reads back and an unsupported one fails the open | LOC: relocation 26,835 → 26,835, delta 0 |
| **S2 Generations and names** | `ws`, `inode`, `dentry` rows; newest-row lookups; capture, install, retire at the row level | S1 | Row-level tests of the visibility, write and capture rules | The rules of [02 §4](02-base-overlay.md#4-generations-and-visibility) hold by test |
| **S3 Base reads** | The base client, `BaseCache`, and base lookups in `layerfs-workspace` over an in-memory `AuthenticatedObjects` provider under `tests/`, with trees built by `build_filesystem` | S1 | Lookup, listing, readlink and range reads over an empty and a non-empty base | An empty base is a real cluster one root; the Workspace has no "empty" branch |
| **S4 Namespace operations** | lookup, getattr, create, mkdir, symlink, link, unlink, rmdir, rename, readdir, chmod, utimens; mutation-time refusals | S2, S3 | The namespace cases of the legacy external tests, ported; a 100,000-entry listing with bounded memory | Every operation is one transaction |
| **S5 Bytes** | Extents, the read plan, truncate, holes, the append hint | S4 | The write, resize and readable cases, ported; scattered-overwrite and sparse-file cases; a 5 GiB sparse file | A write never calls the base client, by counter |
| **S6 Lifetimes and maintenance** | Open counts, open-unlinked files, pins; reclaim and retirement steps; the pressure rule; quota | S5 | Unlinked-open across install; backlog bound under truncate-and-regrow cycles; `ENOSPC` only when live data exceeds the quota | [03 §7](03-mutation-hot-path.md#7-maintenance) holds by test |
| **S7 Count diagnostic** | Counters for statements, transactions, pages written and base calls per operation class. Freeze extent size and page size from the candidates | S6 | A **labelled diagnostic**, count-driven, on the host; not a benchmark sample | Targets T1–T3 of [03 §8](03-mutation-hot-path.md#8-proposed-targets) met or reported as missed with the counts |
| **S8 Mount and daemon** | `layerfs-fuse` on the new Workspace with the target profile; registry, several Workspaces and Execs; concurrent control calls; the command identity; event-driven drain | S6; O-8 | Mounted tests in the container route; the coherence cases of [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design); four Execs with zero `EBUSY`; trust probes | Generic commands run on an empty base through the real mount |
| **S9 Store host and adapters** | `layerfs-server` rewritten over `Handles` and `Storage`: the owner thread, `ReadObjects`, `Attributes`, `GetPolicy`, the Save session with its admission check, history calls. Bridge contract updated | S0; P1 | Host-side integration tests against a real Store; two interleaved Save sessions; the daemon's object authentication | A Workspace opens on a real committed base and reads it |
| **S10 Commit** | Construction, Save, stage, transition, install; every outcome of [04 §6](04-concurrency-commit.md#6-outcomes); fold | S8, S9 | Committed bytes, metadata and history checked independently; the overlap witness (§5.2); conflict, refusal, cancellation and each uncertain phase | A Commit while commands write publishes exactly the captured state |
| **S11 Retirement** | Delete `layerfs-workspace-legacy`, the bridge contracts of host-side construction, the daemon's old upstream path, the duplicate Init code in the server | S10 | Core checks | Removal totals recorded per commit (§4) |
| **S12 Qualification** | Prospectively frozen specifications, then the runs | O-1 | §5.4 | The seven families have terminal dispositions |

Up to S8 nothing is needed from the store host. S9 can proceed in parallel with
S4–S8 once S0 is settled.

## 4. Source removal, relocation and estimates

### 4.1 Removal (measured at `f96d97651`)

| Removed | LOC | Slice | Kind |
| --- | ---: | --- | --- |
| `layerfs-workspace`, all 101 files | 26,835 | S11 | Replacement of a mechanism. `backing/` (13,866 per the #303 ledger) is deleted outright. `filesystem/` and `runtime/` implement behaviour that is kept and rewritten, so their removal is not a simplification claim |
| Bridge files of host-side construction: `contract/prepared_stream.rs` 387, `contract/metadata.rs` 23, `contract/source.rs` 29, `adapters/native/protocol/metadata.rs` 945, `adapters/native/protocol/prepared.rs` 48 | 1,432 | S11 | Deletion, to be confirmed file by file when the slice opens them |
| Daemon `transport.rs` 88, `headless.rs` 133 | 221 | S11 | Replaced by the upstream pool |
| `layerfs-server`, all 31 files | 3,996 | S9, S11 | Rewritten. Of this, `service/save/import/` (645) and `service/init_project.rs` (60) duplicate what `layerfs-project` already provides (799 LOC on `main`): retiring them is removal of a duplicate, not a simplification |

Under placement option C the bridge's history contracts
(`contract/history.rs` 349, `protocol/history_failure.rs` 163) are **kept**:
history calls cross the bridge. The prepared plan deleted them.

### 4.2 Relocation

| Move | LOC | Delta |
| --- | ---: | ---: |
| `layerfs-workspace` → `layerfs-workspace-legacy` (S1) | 26,835 | 0 |

No other relocation is planned. Init already lives in `layerfs-project`.

### 4.3 Estimated future size

> **Estimate. Not a measurement and not a commitment.** It must never be
> quoted as a production-LOC result. The per-commit comparison from
> `tools/production_loc.py` is the only record of actual size.

| Crate | Now (measured) | After (estimate) | Basis |
| --- | ---: | ---: | --- |
| `layerfs-overlay` | — | 1,500–2,500 | Eleven small files of typed row operations and one schema |
| `layerfs-workspace` | 26,835 | 6,000–9,000 | The old `filesystem/` covered the same operations in 4,909; add the base client, Commit construction, fold and maintenance |
| `layerfs-fuse` | 1,447 | 1,300–1,600 | Same adapter shape |
| `layerfs-daemon` | 2,151 | 2,200–2,800 | Registry, upstream pool and concurrent control added; 221 removed |
| `layerfs-bridge` | 6,834 | 5,000–5,800 | 5,402 after the whole-file deletions (measured); object and Save operations added; content operations removed from four files |
| `layerfs-server` | 3,996 | 1,200–2,000 | An owner thread and five operation families |
| `layerfs-sandbox` | 1,106 | 1,100–1,300 | Identity and environment changes |
| `layerfs-api` | 796 | 750–900 | Commit outcome type; Init through `layerfs-project` |
| **Cluster two scope** | **43,165** | **19,050–25,900** | |

If `layerfs-workspace` passes the top of its range before S10, that is reported
as a finding, not absorbed.

### 4.4 File ownership

[proposed design] One line per production file. Names are proposals; a file is
created in the slice that first needs it.

```text
core/crates/layerfs-overlay/                   knows SQLite; knows nothing else
  sql/schema.sql            the five tables and one index
  src/lib.rs                declarations
  src/profile.rs            open; apply and read back every setting; quota
  src/error.rs              OverlayError and its classes
  src/generation.rs         the ws row: view numbers, capture, install, clear
  src/inode.rs              inode rows: newest, upsert, delete, relabel, pin
  src/dentry.rs             name rows: newest, upsert, whiteout, delete, listing page
  src/extent.rs             overlap queries, insert, in-place write, inline append, range delete
  src/scan.rs               keyset pages of a generation
  src/reclaim.rs            the reclaim queue; bounded delete steps; page accounting
  src/counters.rs           statements, transactions, pages (product telemetry)

core/crates/layerfs-workspace/                 knows semantics; no SQL text, no kernel, no framing
  src/lib.rs                declarations
  src/types.rs, error.rs    public types; mapping from overlay and content errors
  src/ports.rs              what the daemon supplies: object fetch, Save session, history calls
  src/workspace.rs          open, close; Core, its mutex, busy flags
  src/base/client.rs        AuthenticatedObjects over the object-fetch port
  src/base/cache.rs         BaseCache and the attribute cache
  src/base/view.rs          child, inode, listing page, link target, attributes, file range
  src/view.rs               newest state of a name or inode; the read plan
  src/ops/                  lookup, open, read, write, resize, attr, create, remove, rename, readdir
  src/orphan.rs             open counts; unlinked-open records; pins
  src/maintenance.rs        retirement, reclaim steps, the pressure rule
  src/commit/slot.rs        admission and state
  src/commit/capture.rs     capture and install
  src/commit/file.rs        captured extents to EditSequence, EditSource or a stream
  src/commit/namespace.rs   captured rows to PreparedRows; the scratch database
  src/commit/publish.rs     Save finish, stage, transition, outcome classification
  src/commit/fold.rs        drain the captured generation into the active one
  src/commit/outcome.rs     result types

core/crates/layerfs-fuse/                      adapter.rs, mount.rs, replies.rs changed; trace.rs removed
core/crates/layerfs-daemon/
  src/registry.rs           new: Workspaces and Execs, configured limits
  src/upstream.rs           new: connection pool; implements the Workspace ports
  src/control*.rs           changed: concurrent calls; no slot lock
  src/execution.rs          changed: command identity, cleared environment, event-driven exit
  src/transport.rs, headless.rs   deleted
core/crates/layerfs-bridge/
  src/contract/object.rs    new: ReadObjects, Attributes, GetPolicy
  src/contract/save.rs      new: the Save session
  src/contract/request.rs, history.rs, outcome.rs   changed
  (deleted files: §4.1)
core/crates/layerfs-server/
  src/host/                 acceptor, assembly, config, run: rebound to Handles and Storage
  src/service/owner.rs      new: the store owner thread and its two-class queue
  src/service/objects.rs    new: ReadObjects, Attributes, GetPolicy
  src/service/save.rs       new: Save sessions
  src/service/admission.rs  new: identity, role and reference check
  src/service/history.rs    rebound history calls
  (the construction and import service is deleted)
```

`layerfs-workspace/src/ports.rs` holds the only traits introduced: they sit on
a process boundary. No interface is created per algorithm.

## 5. Validation

### 5.1 Correctness precedes any timing

[proposed design]

| Requirement | Test |
| --- | --- |
| Accepted-write semantics | A write is readable by another Exec as soon as it returns; a failed write leaves no partial row (fault injected by quota, through the public API) |
| Exact capture | Rows of a request are never split across generations: a writer loop and a capture loop, then a row audit |
| No edit-count limit | 10,240 and 100,000 writes to one file, appended, dispersed and repeated; Commit of each |
| No whole-file or whole-Workspace work | The S7 counters, flat against file size, edit index and Workspace size |
| Repeated edits across Commit | The first interleaving of [04 §9](04-concurrency-commit.md#9-worked-interleavings), asserted on stored rows |
| Open-unlinked lifetime | Across capture, install, retirement and fold |
| Truncate | Shrink, regrow, read zeros; shrink racing a write |
| Rename of a base directory | No descendant row; children resolve; Commit publishes the move |
| Enumeration | Entries present throughout appear exactly once under concurrent rename and unlink; resume at a partially consumed reply |
| Two Workspaces | One fills its quota; the other is unaffected. Both commit to one Branch: one published, one conflict, stage discarded |
| Stat identity | Identical `ino`, `size`, `mtime`, `ctime`, `mode` across two mounts and across an install |
| Coherence | One test per row of [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design) |
| Mutation-time refusals | Each row of the refusal table in [02 §6](02-base-overlay.md#6-names-and-inodes) |
| Outcomes | Each row of [04 §6](04-concurrency-commit.md#6-outcomes), driven through a test implementation of the ports under `tests/` |

Tests use the public API of the production library. No test-only branch, hook
or feature is added under `src/`.

### 5.2 The overlap witness

[proposed design] A deterministic proof that commands keep writing while a
Commit runs and that the Commit publishes exactly the captured state:

1. Write state S1. Start a Commit through a Save-session port, implemented
   under `tests/`, that blocks at its first accepted object.
2. While it is blocked: overwrite a captured block, truncate a captured file,
   rename a captured name, create a file, unlink an open file. Assert each
   returns without waiting for the Commit.
3. Release the port. Assert the published root equals S1 exactly, by an
   independent read of the Store.
4. Assert the mount shows S1 plus the step-2 changes, and that a second Commit
   publishes them.

### 5.3 Count diagnostics

[proposed design] Count-driven instruments, reproducible across windows,
labelled as diagnostics and never as samples of a benchmark arm (root
`AGENTS.md` §3.1): statements, transactions and pages written per operation
class; base calls per operation class; bridge calls and bytes per Commit;
objects emitted, reused and inserted per Commit (`WriteOutcome`); lock-wait
maximum per phase; garbage pages over time.

### 5.4 Qualification

[proposed design, under the owner's recorded acceptance]

**Before any run.** Read `benchmark_agent_report.md` and
`docs/general/benchmark_rules.md`. The following must exist first:

1. The owner's amendment of the hosting rule (O-1). Until then no measurement
   of this design is admissible.
2. A committed specification per family that freezes case identities, order,
   limits, the cache contract and the verifier before sampling.
3. A declared cache contract that names every cache of
   [02 §8](02-base-overlay.md#8-caches-owner-key-visibility-invalidation):
   `BaseCache`, the attribute cache, the overlay's page cache, the guest page
   cache and the host Store's caches. A warm cache is setup reuse, never a cold
   claim; an undeclared state is `INCOMPLETE` or `INELIGIBLE`.
4. Release binaries with pinned identities; one sample per case per arm; a
   fresh output path; one construction worker.

**Acceptance.** The owner's acceptance for this cluster is the seven
`fs-bench-pro` families on the integrated product (#303). A family is complete
when, at one frozen integrated source, every registered selection has a
terminal status recorded in the applicable table of
`benchmark_agent_report.md`: a functional `PASS` with its command budget,
independent verifier and cleanup, or a non-`PASS` status reported as plainly.
Numeric latency stays `INELIGIBLE` unless the cache contract is declared and
enforced equally.

| Family | Disposition in this plan |
| --- | --- |
| 1 `init_namespace`, 2 `history_retention` | Cluster one families. Reused from qualifying receipts by identity if the product, compilation and binary scope they cover is unchanged; otherwise only the affected checkpoint is run (root `AGENTS.md` §3.4). The retained Durable Init rows are `FAIL` on the relative speed ceiling and stay so |
| 3 `workspace_write` | All nine cells of the write matrix. The Repeated cells are the registered home of target T3. The 10,240-write case deferred under #276 is re-opened as a prospectively registered case |
| 4 `workspace_commit`, 5 `workspace_namespace`, 6 `workspace_mutations`, 7 `workspace_shell_package` | One row per registered case. A selection whose subject is a removed mechanism (private-backing custody, funds, page credit) is `NOT_RUN — mechanism removed`, shown beside its prospectively registered successor; that disposition needs the owner's confirmation, as the prepared plan already asked |

No historical receipt is relabelled: the Phase B dirty-discard `FAIL` and the
recorded deferrals stay as recorded.

**What #305 and #306 do and do not contribute.** They are diagnostics on a
passthrough and a throwaway prototype
([05 §1](05-fuse-assessment.md#1-what-was-measured-and-what-was-not)). They
motivate the mount profile and the request-count targets. They are not arms of
any qualification of this design and no row of theirs is reused as a sample.

## 6. Rules for every commit

- The production LOC line, from the first parent and the staged tree, with the
  same counter: `Production LOC: <before> -> <after> (delta <signed>)`, with
  reference and core subtotals.
- An architecture document under `core/docs/architecture/` is updated in the
  same commit as any change to a boundary, format, algorithm or named bound.
- No new dependency where an existing crate provides the capability; no
  patched, vendored or forked dependency; locked builds.
- The exact checks that ran, the ones that did not, and why. There is no CI and
  no aggregate gate to cite.

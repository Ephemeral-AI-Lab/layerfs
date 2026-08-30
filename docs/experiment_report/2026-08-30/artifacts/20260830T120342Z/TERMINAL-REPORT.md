# LayerFS V2 terminal report

Disposition: **PASS**

Source base: `b498b7253bb94ec4e24840a15b825fe00334c7dd`

Exact source seal:
`e15168bcd0ba19fbfc814e512ac764d38f4f6c230565cd6d945876e65d2ef4bb`

## Reconciled architecture

The product has exactly two durable databases:

- `LayerStackStore`: authoritative named LayerStacks, Layers, pushed Branch
  candidates, immutable Commits, and complete canonical objects.
- `BranchStore`: pulled LayerStack prefixes, local/remote Branch rollout
  histories, immutable Commits, local/replicated canonical objects, placement
  scopes, and truthful complete-root receipts.

Workspace is ephemeral and database-free. One SDK `Client` binds one
LayerStackStore endpoint, one BranchStore with one immutable parent identity,
one Monitor, and one worker per active Workspace UUID. There is no connection
vector or active Store selector.

Pull has exact through-boundary semantics. Layer Pull imports the full prefix;
Branch Pull imports full visible inherited history plus every required prefix.
Reference is local-first exact-missing fallback. Replica and every receipted
root are local-only and report `Integrity` on a promised miss. Fork is
zero-copy/local-only and Push sends only the locally owned suffix.

## Exact schemas

- LayerStackStore: schema v3, exactly 6 tables / 25 columns.
- BranchStore: schema v3, exactly 9 tables / 33 columns.
- Both use exact normalized DDL checking, SQLite `STRICT`, foreign keys, WAL,
  synchronous `FULL`, the frozen index set, and cold rejection of any altered
  DDL/index/CHECK or earlier schema.
- `EntityName` is immutable and validated at domain, wire, admission, and SQL
  boundaries. LayerStack names are authority-wide unique; Branch names are
  unique within `LayerStackId`.
- Receiver-local scopes and complete-root receipts never enter immutable fact
  identity. Facts use canonical `signing_bytes`; V2 does not claim an in-repo
  cryptographic signature or network transport.

## SDK and CLI

Implemented SDK families:

- Store/context create, connect, and immutable pair binding.
- named LayerStack initialize, prefix Pull, and Add.
- Branch history Pull, named local Fork, suffix-only Push.
- all three supported paged Diff forms.
- Workspace create, conflicts, resolve, Commit, End, Exec, Shell, Output, Stop.
- bounded typed Query pages, passive Monitor snapshot, explicit dedup analysis.

Implemented CLI families are `db`, `context`, `layerstack`, `branch`,
`workspace`, `monitor`, and `query`, with JSON schema v3, name-aware plans and
completion, nonzero failure exit status, and bounded streaming of all Query and
Diff pages. The exact public grammar is documented in
`docs/v2/sdk-cli-operation-families.md`.

## Destructive convergence

Deleted rather than retained as parallel architectures:

- V1 `layerfs-layer-store` and `layerfs-stack-store` crates and compatibility
  APIs.
- head-only/isolated Pull, remote placement inside Fork, generic merge, Layer
  Push, Branch advance, and Branch-to-Branch Diff surfaces.
- tracked `layerfs-tui`, installer, TUI docs/image, Ratatui, and Crossterm.

No commit, staging operation, push, or reset was performed. Unrelated dirty
worktree content was preserved.

## Audit corrections

Independent audits found and drove fixes for:

- endpoint key/full-record validation and inert fact visibility;
- older/equal Pull classification without unnecessary remote enumeration;
- Branch Pull publication of required LayerStack prefixes;
- retained-receipt and per-root Reference/Replica policy;
- BranchStore-routed Layer Diff and suffix-only complete Push;
- exact Type/Directory/HardLink namespace reconciliation and alias graphs;
- three-root reconciliation protection against authority-masked corruption;
- preservation of `Integrity` and `Unavailable` through mutation builders;
- bounded union traversal, exact transfer bytes, postorder admission, and one
  authority verification Seen domain across multiple pages;
- post-CAS Workspace state, long-lived quiescence, projection refresh, atomic
  materialization publication, cleanup quarantine, and retry-owned FUSE
  unmount;
- CLI process status, full pagination, output follow, JSON controls, semantic
  receipts, and cached immutable Workspace query identity.

The final independent post-fix audit found no remaining P0/P1/P2 issue.

## Focused proof

Current tests prove, among other invariants:

- historical Layer objects deleted from later roots remain offline under
  Replica;
- every selected historical Branch Commit and required Layer root is offline
  readable under Replica;
- interruption leaves admitted facts/objects inert until scope publication;
- older/equal/newer boundaries and Reference/Replica transitions are exact;
- incremental Pull transfers only the missing suffix, and Push never sends
  pulled ancestry back;
- local Fork copies zero canonical objects and performs no endpoint call;
- complete/receipted roots cannot fall back to authority even in a mixed
  three-root reconciliation;
- Content, Type, Directory, and HardLink choices produce exact canonical roots,
  including source-only parents and complete alias graphs;
- a separate-producer Reference reconciliation remains physically incomplete,
  refreshes to the committed root, preserves unrelated edits, and becomes
  read-only;
- missing-only equations remain exact for every fact kind and canonical
  objects, including partial reuse bytes;
- object membership is payload-free, pages and transactions are bounded, and
  no network/hash/history enumeration occurs inside publication transactions;
- same Branch name across different LayerStacks is isolated while same scoped
  name conflicts are typed;
- partial materialization never reaches the final path, failed cleanup retains
  a quarantine/retry target, and Linux FUSE retains a canonical mount token.

## Terminal gates

All passed against the sealed source:

```text
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
cargo check -p layerfs-workspace --all-features --target x86_64-unknown-linux-gnu
```

The full workspace run includes Content canonical/model/oracle suites, Storage,
LayerStackStore, BranchStore, FUSE proxy, materialization, Monitor, SDK, CLI,
Workspace, and doc tests. Live-gated host tests compile in the workspace run;
the real Docker/FUSE benchmark below executes the live container path.

Production Rust physical LOC, excluding inline test modules and external test
targets, is `34,985`. Every handwritten non-SQL/schema production file is
under 1,500 lines; SQL/schema is the explicit documented exception.

## Transfer, transaction, and memory evidence

- History/fact pages: bounded keyset pagination with truthful continuations.
- Membership: at most 512 IDs, payload-free existence/length query.
- Payload batches: at most 128 objects and 4 MiB, with a 34 MiB total active +
  staged ceiling.
- Facts: at most 128 records / 64 KiB per batch.
- Large/deep valid input is streamed or rejected at the explicit bound rather
  than growing memory without limit.
- Replica verification uses a spillable Seen set and records complete roots
  before scope publication.
- SQL-trace tests place expensive verification before write transactions and
  final scope/receipt publication after durable admission.

## FUSE benchmark

The exact-source `linux/arm64` helper
`3d85d937321d31cfff78e8988b872ea3f078c43745752ee5a85edb16d72115b0`
ran through Linux kernel FUSE in a 1-CPU, 3-GiB Docker container.

| Placement | LayerFS sum | Cloudflare sum | Speedup | Wins |
|---|---:|---:|---:|---:|
| Reference | 2,837.322 ms | 7,617.910 ms | 2.68x | 12/12 |
| Replica | 2,860.041 ms | 7,617.910 ms | 2.66x | 12/12 |

Both frozen verifiers returned `PASS_OPTIMIZED`; OOM and OOM-kill counters were
zero, and `/workspace` was unmounted after the run. Only current-source LayerFS
measurements populate these totals.

## Raw evidence

- `source/source-SHA256SUMS`
- `verification/01-fmt.txt`
- `verification/02-workspace-tests.txt`
- `verification/03-clippy.txt`
- `verification/04-git-diff-check.txt`
- `verification/05-linux-cross-check.txt`
- `verification/production-loc.txt`
- `build/linux-helper-release-build.txt`
- `build/image-payload-sha256.txt`
- `bench/cargo-test.txt`
- `bench/reference.json`
- `bench/reference.verification.json`
- `bench/replica.json`
- `bench/replica.verification.json`
- `bench/comparison.json`
- `live/container-inspect-before.json`
- `live/container-inspect-after.json`
- `live/runtime-before.txt`
- `live/runtime-after.txt`

## Explicit deferrals

- OverlayFS projection: deferred; FUSE and explicit materialization are the V2
  projections.
- Automatic GC or implicit object deletion on Replica→Reference: deferred and
  forbidden in the current transition.
- Rename operations for immutable LayerStacks/Branches: deferred.
- TUI: removed, not deferred as an active product surface.

# LayerFS core (replacement product workspace)

> **Status:** Current SQLite-only replacement workspace under qualification.

The active workspace has eleven packages: six cluster-one libraries (content,
storage, history, persistence, Project Init and telemetry), plus Overlay,
Workspace, Daemon, SDK and Bridge. Storage/history are engine-independent domains.
The global persistence adapter owns the current host-local macOS SQLite Store;
Objects/Metadata and History share it with separate bounded acknowledgements.
Daemon Overlay owns separate disposable backing with no synchronization guarantee.

The [cluster-two design](docs/issues/303/README.md), current
[implementation tracker](docs/issues/307/PROGRESS.md) and separate S7/S8/S9 audits
own unfinished integration. SDK now supplies authenticated local object/Save/history
handlers and a bounded fair typed service. Native Bridge authentication is active;
logical runtime/control codecs, complete client delivery and owning Sandbox/API-core
assembly remain open. Native FUSE replacement is excluded. Excluded predecessors,
server and root reference are retained source, without dependency or fallback use.

Project Init includes ignored/dependency/cache/output/.git paths and opaque symlink
targets, and has removed the inherited4GiB refusal. Collection diagnostics still
expose input-sized retention and native hard-link identity remains incomplete;
complete bounded import acceptance is open. Component checks do not complete S7
resource gates or integrated qualification. Historical cluster-one evidence below
retains its source/profile/cache pins and original verdicts.

The source/LOC, locked dependency and verification rules remain in
[AGENTS.md](AGENTS.md). No CI or aggregate preflight gate is used.

## Architecture

[`docs/architecture/`](docs/architecture/) describes what this workspace's
packages actually do: the C1/C2 boundary and its two traits, the canonical object
model, the construction/edit/filesystem algorithms, the storage schema and
physical formats, and every declared limit with the check that enforces it. It is
a source-backed description pinned to a commit — descriptive, not a contract, and
carrying no performance or qualification claim.

Start with the [index](docs/architecture/README.md), which maps the descriptive
papers and the `proposal/` and `deferred/` subtrees beside them. It describes
**this** workspace; the reference tree under the repository-root `crates/` is a
different product with different identifiers and formats.

## Commands

Build selected packages first using `cargo +1.85.1 test --manifest-path
core/Cargo.toml --locked -p <package> --all-targets --no-run`. Run each selected
actual test command with an explicit wall timeout at most120s; retain/diagnose
any timeout as FAILED before a source-driven repair. Package/target selection
keeps unrelated campaigns separate. Required changed-scope checks also include:

```sh
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p <package> --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 -B core/tools/check_product_boundary.py
```

Core/tool self-tests require the same explicit wall timeout. See the current
[check contract](AGENTS.md#checks-and-completion) for exact scope and ARM64 flags.

### Real component runs

`examples/measure_components.rs` runs one real sample per mode against an
explicit input and a fresh output path. These smoke runs prove wiring and
readback; they are not release-admissible benchmarks.

```sh
run="$(mktemp -d /tmp/layerfs-stage02.XXXXXX)"
python3 -c 'from pathlib import Path; import sys; Path(sys.argv[1]).write_bytes(bytes(range(256)) * 64)' "$run/input.bin"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c1 --input "$run/input.bin" --timings "$run/c1.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode c2 --input "$run/input.bin" --store "$run/c2.sqlite" --timings "$run/c2.json"
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked -p layerfs-storage --example measure_components -- --mode pipeline --input "$run/input.bin" --store "$run/pipeline.sqlite" --timings "$run/pipeline.json"
```

The two workspaces are checked separately: the commands above use the core manifest,
and the reference workspace uses its own. There is no aggregate repository gate —
`tools/preflight.sh` is permanently retired by owner decision (ledger L32), and it
runs no checks. Do not restore it.

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

The reference workspace keeps its own commands and lockfile:

```sh
cargo test --manifest-path Cargo.toml --workspace --locked
```

Design and migration: [`docs/roadmap/0.1/0.1.7/component-decoupling/`](../docs/roadmap/0.1/0.1.7/component-decoupling/README.md).

Stages 3-4 status: the batch (#168 / #169 under #165) is closed out against the
independent review of the pinned snapshot; the completion gate table, the evidence
round and the two open owner decisions are in
[`stages-3-4-closeout-report.md`](../docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-closeout-report.md).
The three packages are C1 `layerfs-content`, C2 `layerfs-storage` and
`layerfs-telemetry`. Three earlier figures are on record and **none of them
describes the current tree**: the closeout packet reported 10,983 for the snapshot
it measured (`aa4b5a9e4`), the batch tip recorded 11,058, and the architecture
set's authoring pin (`1884e3eca`) counted 18,792 across 116 files. At `9f35c49ad`
the counter reports **20,116 production lines across 121 files**
(`layerfs-content` 12,512; `layerfs-storage` 6,841; `layerfs-telemetry` 763),
alongside the unchanged reference tree at 65,417 and a combined 85,533. Counted
with `tools/production_loc.py`, which excludes comments, blanks, tests, examples,
docs and manifests, and includes shipped runtime SQL; a physical `wc -l` total is
a different measure (26,831 at `9f35c49ad`) and must not be quoted as this one.

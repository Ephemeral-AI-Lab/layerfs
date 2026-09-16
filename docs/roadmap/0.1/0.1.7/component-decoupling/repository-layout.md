# Repository layout and side-by-side v0.1.7 migration

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Related: [discussion index](README.md), [shared proposal](proposal.md),
[content/storage co-design](content-storage-co-design.md),
[timer design](telemetry.md),
[design issue #160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

## Recommendation

Put the complete LayerFS implementation and its build workspace under `core/`.
Put application integrations under sibling `adapters/`. Keep existing root
`crates/` and its workspace available as the reference during migration; remove
them after the v0.1.7 replacement is qualified. This is a proposed layout and
migration sequence, not a request to move the live source immediately.

Repository root becomes the home of the project family. The LayerFS engine,
runtime, SDK and CLI have one product home; Claude Code, Codex and dsh integrations
consume its public surface. The folder marks the reusable product boundary,
independently of deployment roles such as host, server, container or cloud.
Here core/ means the complete reusable LayerFS product, including SDK, CLI and
runtime integration. This name replaces the earlier layerfs/ proposal and avoids
repeating the repository name in the product directory.

## Target layout

```text
<repository>/
  README.md
  AGENTS.md
  LICENSE
  core/
    AGENTS.md                     # product-source and entry-file rules
    README.md
    Cargo.toml
    Cargo.lock
    crates/
      layerfs-telemetry/           # implemented
      layerfs-content/             # selected C1 home; create with Stage 1 code
      layerfs-storage/             # selected C2 home; create with Stage 2 code
      ...                         # later runtime/application boundaries undecided
    benchmark/                    # maintained product harness/workloads
    containers/                   # maintained product image definitions
    tools/                        # product build, tests and evaluation
  adapters/
    layerfs-claude-code/           # future, create when implementing
    layerfs-codex/                # future
    layerfs-dsh/                  # future
  docs/                           # shared design, maintained guides, history
  release-notes/                   # existing versioned release evidence
  web/                            # existing website
  tools/                          # only any needed repository-wide entry points
  benchmark-results/              # retained existing evidence, not relocated
```

The [Stages 0–2 handoff](stages-0-2-handoff.md) selects layerfs-content and
layerfs-storage alongside implemented layerfs-telemetry. Later package names,
counts and boundaries remain open. Existing crates
and candidate clusters describe the current implementation and design discussions;
neither is a template for a one-to-one replacement package inventory. Decide each
additional crate when its component design is settled. Adapter names above are
future integration projects, not approved Rust crate boundaries.

The [core agent rules](../../../../../core/AGENTS.md) require production-only
src/ trees, external tests/examples and lib.rs/mod.rs files no longer than 200
physical lines with declaration/delegation responsibilities only. Every other
production file must stay under 1,000 physical lines (maximum 999); large components
use focused responsibility folders. The [implementation plan](implementation-plan.md#1-proposed-folders-and-implementation-rules)
shows the selected C1/C2 package homes; create no empty scaffolding ahead of code.
The source
boundary guard runs from local preflight; semantic responsibility review is also
required. Policy/check tooling is present before the first product crate.

Do not create a crate per algorithm, duplicate a cluster hierarchy in directories,
or add empty placeholder crates/adapter projects. Tests stay in actual Cargo
packages; do not assume a tests/ folder at a virtual workspace root is automatically
a test target.

`core/` owns everything needed to build/test the product. Shared documentation
and historical evidence can stay at repository root without becoming build-time
dependencies. Move maintained product tooling/image definitions as they are
adopted; do not carry obsolete harnesses into the candidate automatically.

### Application integrations versus internal runtime adapters

```text
adapters/layerfs-claude-code --+
adapters/layerfs-codex -------+--> public SDK / CLI --> LayerFS components
adapters/layerfs-dsh ---------+
```

- Application adapters own tool-specific hooks, configuration, command mapping,
  session lifecycle integration and their own packaging/tests.
- They use supported SDK/CLI entry points. No direct SQL, private Store handles,
  internal crate imports or accesses to the legacy source tree.
- FUSE, daemon transport and execution binding belong to the product under
  core/. They implement filesystem/runtime behavior, not a particular agent
  application's integration policy.
- LayerFS never depends on Claude Code, Codex or dsh adapter packages. A native
  adapter may use the public Rust SDK; another language may invoke the public CLI.
  Introduce a new service protocol only for a concrete need.
- Each application adapter can use its appropriate packaging/build tool. Decide
  shared Rust integration tooling when an actual Rust adapter exists; do not build
  a multi-language build framework or plugin registry now.

## Two independent workspaces during migration

```text
<repository>/
  Cargo.toml                 existing reference workspace (excludes core/)
  Cargo.lock
  crates/                    existing reference product
  tools/ and benchmark/      reference build/test tooling

  core/
    Cargo.toml               replacement workspace
    Cargo.lock               replacement dependency resolution
    crates/                  components as they are ported
```

The candidate has its own [workspace] and explicit member list. Explicitly exclude
core/ from the legacy root workspace when introducing it. Do not list both
copies of same-named packages in one workspace or use a shared lockfile that
silently selects one implementation. Preserve required public package contracts;
this does not settle internal crate names or require each old crate to survive.
Independent workspaces permit the same package name where its later design needs it.

Cargo workspaces own their shared lockfile and default target directory. Member
selection, exclusion and inherited package metadata must refer to the intended
workspace. See the [Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html).

Proposed explicit commands after the candidate workspace exists:

```sh
# Reference workspace.
cargo test --manifest-path Cargo.toml --workspace --locked

# Candidate workspace.
cargo test --manifest-path core/Cargo.toml --workspace --locked
```

The candidate uses its own target/build namespace; keep that separation in harness
target overrides, immutable binary archives and image seals too. Reuse sealed
builds and dependency acquisition where valid; do not routinely clear caches.
Seed the new workspace with the existing third-party version pins, update its
lockfile deliberately for its actual first-party graph, and build --locked.
This is not authorization to update, patch, vendor or fork dependencies.

Candidate product dependencies must not resolve into root crates/, old build
outputs, or legacy binaries. Port/reuse needed first-party code into the new
ownership boundary, or define the component's small explicit contract. Treat
legacy as a reference, not a dependency or a runtime fallback. Required old-code
fixes must be explicit and must not silently move the pinned comparison baseline.

## Migration sequence

1. **Pin the reference.** Record the exact baseline commit, product/dependency
   identities and required behavior/format contracts. Keep old code available
   for reading and qualified comparison. A dirty working tree is not a frozen
   baseline; use the actual recorded artifacts/source when comparing.
2. **Establish the replacement workspace with the first real component.**
   **Started:** `core/Cargo.toml`, `core/Cargo.lock` and
   `core/crates/layerfs-telemetry/` exist, `core/` is excluded from the reference
   workspace, and local preflight runs the candidate workspace checks. The
   telemetry crate has no product dependencies and was the first structural
   proof. No other crate has been created and no future module was scaffolded.
3. **Design and implement the next component boundaries.** Continue the content/
   storage and other responsibility discussions before choosing their crate homes.
   Derive package boundaries and implementation order from those decisions; this
   layout does not prescribe a crate inventory or a crate-by-crate port of the old
   tree. Keep useful algorithms/tests and replace unwanted internal couplings.
   Each selected component gets independent correctness and measurement entry
   points. New code must compile without old product crates.
4. **Integrate an end-to-end candidate path.** Exercise the public SDK/CLI and real
   FUSE/runtime path. No fallback to the reference for missing behavior. Compare
   canonical identities, formats, errors, recovery and public results; qualify
   performance/resources under the existing measurement contract.
5. **Switch maintained tools and documentation.** Update manifests, package paths,
   build contexts, executable discovery, source seals, local preflight and current
   usage docs together. Existing root tools/preflight.sh may remain a thin entry
   point to the new product checks. Preserve append-only historical evidence.
6. **Retire the reference after v0.1.7 qualification.** Remove root crates/, its
   old Cargo.toml/Cargo.lock and old-only product tooling in a dedicated cleanup
   change. Do not keep a permanent legacy/ source copy. Git history and pinned
   baseline artifacts retain the reference. Root cargo commands are replaced by
   the documented product-directory/manifest commands; deployed CLI names and
   public behavior remain stable.

During the transition, test routing must make reference/candidate selection
explicit. Root preflight currently checks both the reference and the candidate
telemetry workspace; those passes do not prove future components. Extend candidate
coverage as real members land and switch the product default only after qualification.

## Source-backed migration work

Inspected at 7fd667f1f8cd011f462abc27cf7be525f5a0bc6b. The working tree also contains
planning changes; these observations are not a sealed performance baseline.

| Current location | Assumption to update in the relevant migration slice |
| --- | --- |
| [Root Cargo manifest](../../../../../Cargo.toml) | Product crates, evaluator and benchmark share the root workspace |
| [Local preflight](../../../../../tools/preflight.sh) and [fast test entry](../../../../../tools/test-fast.sh) | Root cwd/manifest and root tools paths select the current implementation |
| [Benchmark manifest](../../../../../benchmark/fs-bench-pro/Cargo.toml) | Relative dependencies point to root crates/ |
| [Evaluator manifest](../../../../../tools/layerfs-eval/Cargo.toml) | SDK path points to root crates/ |
| [Benchmark runner](../../../../../benchmark/fs-bench-pro/shared/runner.py#L246) | Compilation/product seals enumerate root crates/, tools/, manifests and lockfile |
| [Benchmark image](../../../../../benchmark/fs-bench-pro/Dockerfile.layerfs) | Docker COPY, build workdir and target cache assume the current root layout |

Separate repository root (Git identity, shared docs/evidence) from product root
(manifest, crates, maintained product tools) in tooling that needs both. Resolve
them explicitly; do not infer candidate/reference by whichever binary happens to
be in PATH. Include all actual build inputs in source/product/compilation seals.
Layout and harness identity changes invalidate reuse where identities no longer
match; do not re-label old receipts as candidate evidence. Compare both arms with
the same qualified comparison harness/workload contract and declare their source
identities. Keep setup reuse outside timers and preserve worker/cache/limit rules.

Do not relocate or rewrite existing benchmark-results or historical release
receipts. Maintained architecture links to reference source should become pinned
commit permalinks where needed before removing old paths; leave immutable historical
evidence intact. Update .dockerignore, ignore rules, development commands and
active tooling tests as part of the concrete move.

## Completion gates

- [x] Candidate workspace metadata/build/test graph contains no legacy product
      paths, binary fallback or application adapter projects; local preflight runs
      the candidate checks. (Verified for the telemetry package; recheck as more
      components land.)
- [ ] New components are independently testable and measurable under declared
      inputs, with no hidden setup/DB/runtime dependence claimed away by timers.
- [ ] SDK/CLI, daemon protocol, canonical identity and Store compatibility meet
      the v0.1.7 boundary; cleanup preserves ownership and operations enforce the
      [single-attempt rule](physical-encoding-and-packing.md#one-attempt-no-retries).
- [ ] Source seals, image/build selection and benchmark attribution name the
      implementation actually executed; performance meets the agreed gates.
- [ ] Local preflight covers the candidate, current docs/links match it, and no
      active manifest/tool path resolves to the retired reference.
- [ ] Remove the reference source after the qualified replacement is complete;
      preserve baseline identities and immutable historical evidence.

The first implementation step has landed: the independent `core/` workspace with
`layerfs-telemetry` as its only member, the root-workspace exclusion and the
candidate checks in `tools/preflight.sh`. No reference source was moved, deleted
or rebuilt by this proposal, and no benchmark run was performed. The first handoff
now selects layerfs-content/layerfs-storage, but neither is implemented yet. Later
runtime/application package choices remain open.

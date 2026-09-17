# Independent verification: R2-F17 (encoder context) and R2-F18 (pre-work policy validation)

Verifier: subagent, read-only. Repository frozen at `99743b2cf3a869b7d8897a1f16b82d742aeedc40`
(`git rev-parse HEAD` = `99743b2cff2470e6634874d7ee14b9d37d0ba16e`, verified first).
Round-3 commit `2fe2a4642` ("fix(core): bound the read wave, right-size the value limit,
validate policies") confirmed an ancestor of HEAD (`git merge-base --is-ancestor` → 0).
No roadmap report, suite result, commit message or interface is treated as evidence;
every row is reproduced from source or a fresh run with path:line citations. This
verifier's only write is this file. Working-tree note: `git status --short` was clean at
session start; during the session a sibling agent sharing this workspace modified
`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md` and created
`verify-R2-F12-F13.md` / `per-commit-loc-reread-2.log` — neither is this verifier's work.

## Verdicts

| Row | Claim | Verdict |
| --- | --- | --- |
| R2-F17 | `encode_node` takes the page's context (root vs non-root); short non-root page (< MIN_ENTRIES = 64) refused by the encoder; page builder, edit tree and attribute value pass their own context; two-sided test exists | **PASS** |
| R2-F18 | Every C1 entry point (`construct_bytes`, `construct_stream`, `apply_edits`) calls `ConstructionPolicy::validated()` before any work; negative test exists; README "before work begins" claim now true on C1 | **PASS** (with one coverage nuance, see UNVERIFIED #1) |

## Commands run (from repository root; exit codes)

| # | Command | Exit |
| --- | --- | --- |
| C1 | `git rev-parse HEAD && git status --short` | 0 (HEAD = `99743b2cff2470e6634874d7ee14b9d37d0ba16e`; tree clean at start) |
| C2 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test file_read a_short_page_is_accepted_as_a_root_and_refused_as_a_non_root -- --nocapture` | 0 (1 passed, 0 failed) |
| C3 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test file_complete unsupported_policy_values_are_rejected_before_work -- --nocapture` | 0 (1 passed, 0 failed) |
| C4 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content` (full crate suite) | 0 (33 targets, **227 passed, 0 failed**) |
| C5 | `git log --oneline -3 2fe2a4642` ; `git merge-base --is-ancestor 2fe2a4642 HEAD` | 0 / 0 (ancestor confirmed) |
| C6 | Read-only searches: `encode_node` callers (`grep -rn`), `policy: ConstructionPolicy` parameters, `emit_node(`/`commit_node(` call sites, `decode_node_with_context(` call sites, `ConstructionPolicy::new(` in tests, `pub fn` enumeration in `src/file/`, README `UnsupportedPolicy`/`validated` | 0 |
| C7 | `git status --porcelain` (end-of-session cleanliness check) | 0 (only the sibling agent's drift described above) |

Build artifacts went to `core/target/` only (C2–C4). No tracked file was modified by this
verifier; nothing was staged, committed, pushed, checked out or reset.

## Findings — R2-F17

**Encoder signature and validation.**
- `core/crates/layerfs-content/src/file/mapping/codec.rs:107` — `pub fn encode_node(node: &ExtentNode, root: bool) -> ContentResult<Vec<u8>>`. Doc at codec.rs:100-106: "`root` is the context the page will be decoded in: a non-root page must satisfy the canonical partition, so a short non-root page that this function accepted would be publishable and unreadable... the encoder asks for it instead of assuming every page it sees is a root."
- codec.rs:108 — `node.validate(root)?` is the **first statement**, before any byte is written (length math, header, entries all follow at codec.rs:109-162).
- `core/crates/layerfs-content/src/file/mapping/types.rs:171-175` — `pub fn validate(&self, root: bool)`: `if count > MAX_ENTRIES || (!root && count < MIN_ENTRIES) { return Err(ContentError::NonCanonicalPagePartition); }`. `MIN_ENTRIES: usize = 64` at types.rs:13; root branches additionally require `children.len() >= MINIMUM_ROOT_ENTRIES` (= 2, types.rs:19) at types.rs:216. README.md:26 corroborates: "`<= 128` entries, `>= 64` for non-root pages".

**Callers pass their own context (all production callers, complete list from grep).**
- Page builder: `core/crates/layerfs-content/src/file/mapping/build.rs:256-261` `emit_prefix(..., root: bool)`; build.rs:298 forwards it to `emit_node`; build.rs:383 `encode_node(node, root)?`. The builder decides the context per page: build.rs:241 emits the final top page with `true`; build.rs:245 emits a page pushed to a higher level with `false`. `emit_empty_leaf` (build.rs:335-348) passes `true` — the defined empty mapping **root** page (README.md:22).
- Edit tree: `core/crates/layerfs-content/src/file/edit/tree.rs:394-399` `fn commit_node(..., root: bool, ...)`; tree.rs:449 `encode_node(&node, root)?`. `finish`/`commit` pass `true` for the mapping root (tree.rs:367, 385); children are committed recursively with `false` (tree.rs:421, 441). Decode side symmetric: tree.rs:415 `decode_node_with_context(object.canonical(), root)?` and tree.rs:153-161 `load_node(..., root: bool)`.
- Attribute value: `core/crates/layerfs-content/src/filesystem/attributes/value.rs:37-43` — `encode_node(&ExtentNode::Leaf { ... }, true)`. This `true` **is** the page's own context: the emitted leaf is the single-page root of the attribute-value file — the `FileState` at value.rs:49-56 points directly at `leaf_id` (`mapping_root: leaf_id`, `tree_level: 0`). Not blind root-passing.
- Read-side symmetry (the "decoded in" half of the claim): `core/crates/layerfs-content/src/file/mapping/read.rs:244` frontier starts `root: true` for `state.mapping_root`; read.rs:271 `decode_node_with_context(canonical, node.root)?`; read.rs:305 every child frontier `root: false`. `decode_node_with_context` revalidates with the same rule (codec.rs:268 `node.validate(root)?`), so a short page published as a child would indeed be unreadable — the "publishable but unreadable" failure mode the fix closes.

**Two-sided test (C2, passes).** `core/crates/layerfs-content/tests/file_read.rs:522-559`
`a_short_page_is_accepted_as_a_root_and_refused_as_a_non_root`:
- :527-534 — an empty `ExtentNode::Leaf` (shortest possible page): `assert!(encode_node(&short, true).is_ok(), "an empty leaf is the canonical root of an empty mapping")`.
- :535-539 — `encode_node(&short, false)` must be `Err(ContentError::NonCanonicalPagePartition)` ("a page below the partition minimum is refused in child context").
- :541-558 — the boundary itself: `encode_node(&page(MIN_ENTRIES - 1), false)` → `Err(NonCanonicalPagePartition)` (:551-554); `encode_node(&page(MIN_ENTRIES), false).is_ok()` ("a page at the partition minimum is a legal child", :555-558).
The test's doc comment (file_read.rs:516-521) states the history: "The encoder used to validate every page it was given as a **root**... this case pins both sides of it - the same short page is accepted as a root and refused as a child."

**Full-suite corroboration (C4).** 227 passed / 0 failed across all 33 layerfs-content
targets — the builder, edit-tree and attribute paths build real trees through these
exact calls, so a wrongly-passed context (e.g. children encoded as root) would fail
round-trip reads elsewhere in the suite.

## Findings — R2-F18

**Entry points call `validated()` before any work.** The grep for
`policy: ConstructionPolicy` in `core/crates/layerfs-content/src` returns exactly four
hits: the three public entry points plus the private inner `construct_bytes_in`
(content.rs:165) — there is no other policy-taking function in the crate.
- `core/crates/layerfs-content/src/file/content.rs:150-162` `construct_bytes`: comment at :157-159 ("A policy this profile does not support is refused before any work"), `policy.validated()?` at **:160**, then `scope.run(...)` at :161 — validation is the body's first statement; no read, probe or emission precedes it.
- `content.rs:201-209` `construct_stream`: `policy.validated()?` at **:208**, `scope.run(...)` (threshold probe, buffering, construction) at :209.
- `core/crates/layerfs-content/src/file/edit/apply.rs:38-47` `apply_edits`: `policy.validated()?` at **:46**, `scope.run(...)` (FileView open, edit dispatch) at :47.

**`validated()` semantics.** `core/crates/layerfs-content/src/policy.rs:81-102` — rejects a cutoff that is not a power of two outside `131072..=1048576` with `ContentError::UnsupportedPolicy { field: "small_file_threshold_bytes" }` (:87-89), and either depth above `MAXIMUM_DELTA_MAX_DEPTH` (50) with its own field name (:91-99).

**Negative test (C3, passes).** `core/crates/layerfs-content/tests/file_complete.rs:273-332`
`unsupported_policy_values_are_rejected_before_work`:
- :278-301 — `validated()` directly refuses cutoffs `1_048_577`, `65_536`, `196_608` and depths `51`/`51`, each as the exact `UnsupportedPolicy` field error.
- :307-318 — `construct_bytes(ConstructionPolicy::new(1_048_577, 8, 4), ...)` into a `MemoryStore`.
- :319-325 — `assert_eq!(outcome, Err(ContentError::UnsupportedPolicy { field: "small_file_threshold_bytes" }), "construction refuses an unsupported policy before emitting anything")`.
- :326-330 — `assert!(consumer.is_empty(), "a refused policy emits no object: {}", consumer.len())` — **nothing is emitted**.
The test's comment (:302-306) states the README link: "the README says an unsupported value fails 'before work begins', which was only true on the storage bridge."

**README claim now true on C1.** `core/crates/layerfs-content/README.md:32` — "Any other
value fails with `ContentError::UnsupportedPolicy` before work begins." Since the only
C1 entry points that accept a policy are the three above and each validates first
(source-verified), the sentence is true on C1. `lib.rs:26-30` re-exports exactly
`apply_edits, construct_bytes, construct_stream` as the crate's construction surface.

**Falsification attempts (all negative — no entry point skips validation).**
- Only four `policy: ConstructionPolicy` parameters exist in the crate (three entry points + the private `construct_bytes_in`); every other public construction-ish function takes no policy at all: `encode_whole_file` (content.rs:82-85, takes `capacities` only), `encode_whole_file_payload` (content.rs:117), `emit_value` (value.rs:22, no policy), `build_filesystem`/`update_filesystem` (lib.rs:32-36, no policy), read paths (read.rs, view.rs, mapping/read.rs, no policy). `ExtentBuilder::new` (build.rs:74) takes capacities and is reached only through the validated entry points.
- `decode_node` (codec.rs:166-168) defaults to `root: true` — checked whether any non-root page is decoded through it: production callers all use `decode_node_with_context` with an explicit context (read.rs:271, tree.rs:157/160/415); tree.rs:208/273 decode **root** branch pages (`true`). No mismatch found.
- Searched the whole crate for `encode_node` callers: only build.rs:383, tree.rs:449, value.rs:37 (all quoted above) — none passes a constant context where the page may be non-root.

## UNVERIFIED

1. **Per-entry-point negative coverage.** The negative entry-point test exercises only
   `construct_bytes` end-to-end (file_complete.rs:307-330). `construct_stream` and
   `apply_edits` are never called with an unsupported policy in any test — every test
   call site uses a valid policy (all `ConstructionPolicy::new(` uses in tests are
   edit_transitions.rs:21, 258 and file_complete.rs:279-281, 291, 297, 307). Their
   pre-work validation is therefore **source-verified only** (content.rs:208,
   apply.rs:46). The claim's wording ("A negative test exists", singular) is satisfied,
   but a strict "each entry point is negatively tested" reading would not be.
2. **C2 (layerfs-storage) not examined.** The claim says the README sentence "was only
   true on the storage bridge" before and is "now true on C1 too"; the storage bridge's
   own pre-work validation was outside this scope and was not checked.
3. **No clippy/fmt/boundary-guard run.** Not required to reproduce these claims; the
   locked layerfs-content test suite (C4) is the relevant check. `cargo clippy` and
   `python3 core/tools/check_product_boundary.py` were not run by this verifier.
4. **Sibling workspace drift.** `stage-5-report.md` (modified) and
   `verify-R2-F12-F13.md` / `per-commit-loc-reread-2.log` (untracked) appeared during
   the session from a sibling agent; this verifier did not inspect or author them and
   cannot vouch for their contents.

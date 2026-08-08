# M6.1.2 verification receipt — strict codec, paths, bounds, and errors

```text
DOCUMENT_ROLE: NORMATIVE_M6_1_2_IMPLEMENTATION_EVIDENCE
RECEIPT_STATE: FINAL_PASS
VERIFIED_UTC: 2026-08-06T20:04:51Z
PRODUCT_PATH: /Users/yifanxu/Ephemeral-AI-Lab/ephemeral-sandbox-v2-worktrees/ephemeral-sandbox-v2
PRODUCT_BRANCH: ephemeral-sandbox-v2
PRODUCT_REMOTE: https://github.com/Ephemeral-AI-Lab/ephemeral-sandbox.git
PRODUCT_HEAD: b22862550e0a7cb4fe61ce581831e9244cc492b5
OFFICIAL_V1_V0_1_4_ANCESTRY_EXIT: 0
M6_1_2_CRITERIA_PASS: 6_OF_6
M6_1_IMPLEMENTATION_CHECKED_AFTER_TRACKER_UPDATE: 17_OF_48
M6_1_PROGRESS_PERCENT_AFTER_TRACKER_UPDATE: 35.42
P0_OPEN: 0
P1_OPEN: 0
M6_1_3: NOT_STARTED_STOP_AFTER_THIS_RECEIPT
M6_2_AND_LATER: STOP
BENCHMARK_E2E_LIVE_DOCKER: NOT_RUN
WORKSPACE_MATERIALIZATION: NOT_STARTED
PHASE_2: STOP
```

## 1. Governing authority and frozen custody

The M61-BLOCK-001 authority transition remains exact and unchanged:

| Artifact | SHA-256 |
|---|---|
| `M6_1_CONTRACT_AMENDMENT_001_EFFECTIVE_SET.md` | `3cf3ad012263fd47627354ad765956de339a11508edc2ab3aeefaa4c1d3b1d88` |
| `M6_1_SPEC.md` | `ab4797e90743554f9e4e30f00e0bd4bcb8519b3216aab88c7dae8f8e6a49db32` |
| `M6_1_CONTRACT_AMENDMENT_001.md` | `9d167b8ee8450cc8efb8be3db7c1b4da0dbda264e22bc6abfeaa2612d5d3e266` |
| `M6_1_CONTRACT_AMENDMENT_001_SEAL_RECEIPT.md` | `487d370b3dc882b6e169265a3a5724797e775bdf8a4f354fac56d171924d433b` |

The three frozen M6.0 artifacts were rehashed after the final M6.1.2 test and
review cycle and remain byte-for-byte unchanged:

| Frozen artifact | SHA-256 |
|---|---|
| `M6_0_GOLDEN_VECTORS.md` | `7e0ae75e32bd38111f9271c551efacc580a42287a9f31b6999157e1da3710d7a` |
| `m6-vectors/src/main.rs` | `431ce15864631f5b17b634ade4f6562946937e8c0a4d67be107d2a19c24172a7` |
| `m6-vectors/README.md` | `fd249cef99b3feb4f6d1569dba8ac80502317c7fd0719ed0ead3b1ab9a2c7bcd` |

No frozen authority was edited or resealed by M6.1.2.

## 2. Exact product snapshot and allowlist

The product status contains exactly the nine authorized paths:

```text
 M Cargo.lock
 M Cargo.toml
 M crates/sandbox-runtime/layerstack/Cargo.toml
?? crates/sandbox-runtime/layerstack-core/Cargo.toml
?? crates/sandbox-runtime/layerstack-core/src/codec.rs
?? crates/sandbox-runtime/layerstack-core/src/error.rs
?? crates/sandbox-runtime/layerstack-core/src/lib.rs
?? crates/sandbox-runtime/layerstack-core/src/path.rs
?? crates/sandbox-runtime/layerstack-core/tests/hostile_and_properties.rs
```

| Product file | SHA-256 |
|---|---|
| `Cargo.toml` | `aac679e5dae7c5abb31d165e4553d8e299f1913c6ce1ef136ab34b2ceec80bf7` |
| `Cargo.lock` | `d9f91287587c8326861d2aa465c7062a92d6cb773c5662a7c93b6f45fb269071` |
| `crates/sandbox-runtime/layerstack/Cargo.toml` | `00bb6c10ac5e415b53bc7c0207d486111a516bcb7287509e8426ea645cc6a292` |
| `crates/sandbox-runtime/layerstack-core/Cargo.toml` | `5f6a1a5ec84d0ce091244b132c709aa66af98b35be456b4bbdf40e87755c55e9` |
| `crates/sandbox-runtime/layerstack-core/src/lib.rs` | `ac1b42d656935d68061ca7d7eb2ef92d8102ced2906b9a6c7fe030719e4cd372` |
| `crates/sandbox-runtime/layerstack-core/src/codec.rs` | `17d5b6ab3851898581c309948ac16ffbb00d208ba21fa1b12b218668afe865ac` |
| `crates/sandbox-runtime/layerstack-core/src/error.rs` | `ca25c0c628e687e9b7ad9a3bcd5775f18abff120b3dca06ba7a6e62ba1b0baad` |
| `crates/sandbox-runtime/layerstack-core/src/path.rs` | `f126637e456c8e8842562cf149b6fe010b3373bc0f896d5880a5ec7a3fefff4d` |
| `crates/sandbox-runtime/layerstack-core/tests/hostile_and_properties.rs` | `c25b49fe7b6981f656153a17aaf74ee0f19f1db1e94588a85f66310a2d6928f8` |

`git diff --name-status` reported only the first three tracked Cargo files;
`git ls-files --others --exclude-standard` reported only the six new core
files. `git diff --check` returned exit `0`. No out-of-allowlist path exists.

## 3. Exact commands and exits

All commands ran in the canonical product checkout. The targeted test and
verification gate completed at `2026-08-06T20:04:51Z` UTC.

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --all -- --check` | `0` | formatting clean |
| `cargo check -p sandbox-runtime-layerstack-core` | `0` | core crate checks |
| `cargo test -p sandbox-runtime-layerstack-core --test hostile_and_properties` | `0` | 17 passed; 0 failed |
| `cargo test -p sandbox-runtime-layerstack-core` | `0` | 17 integration tests plus unit/doc targets pass |
| `cargo clippy -p sandbox-runtime-layerstack-core --all-targets -- -D warnings` | `0` | zero warnings |
| `cargo tree -p sandbox-runtime-layerstack-core --edges normal,build,dev --locked --offline` | `0` | printed only the root core package |
| `cargo metadata --format-version 1 --locked >/dev/null` | `0` | manifests and lockfile resolve exactly |
| `git diff --check` | `0` | no whitespace/error marker |
| `git status --short --untracked-files=all` | `0` | exact nine-path allowlist in section 2 |
| `git diff --stat` | `0` | tracked Cargo delta: 3 files, 8 insertions |

No workspace-wide qualification, benchmark, E2E, live Docker, stress,
challenge, or materialization command was run. Those are not M6.1.2 evidence.

## 4. Mechanical M6.1.2 criterion audit

| Criterion | Stable source and test evidence | Result |
|---|---|---|
| `M61-0201` | `codec.rs:223-315,348-374,396-408`; literal LE/BE and checked-length tests `hostile_and_properties.rs:28-129` | `PASS` |
| `M61-0202` | `codec.rs:158-188,223-315,410-512`; fixed-prefix, declared-bound, truncation, discriminator, reserved, flags, and EOF tests `hostile_and_properties.rs:28-240,515-612` | `PASS` |
| `M61-0203` | `path.rs:7-253`; component/path/target, UTF-8, separator, dot/dotdot, NUL, depth, exact/one-over, and first/later reset tests `hostile_and_properties.rs:277-458` | `PASS` |
| `M61-0204` | `codec.rs:22-197`, `path.rs:256-298`; mode/sentinel, unknown-kind, unsigned name/path order, duplicate and descending tests `hostile_and_properties.rs:241-276,459-558` | `PASS` |
| `M61-0205` | capacity-first cursor/sink and checked bound primitives `codec.rs:223-270,377-451`; exact frozen cap validators `codec.rs:430-512`; hostile pre-effect and exact/one-over tests `hostile_and_properties.rs:71-210,559-612` | `PASS` |
| `M61-0206` | exact 32-row code/string inventory and exhaustive no-wildcard mappings `error.rs:5-166`; uniqueness/base-preservation/extension test `hostile_and_properties.rs:614-797` | `PASS` |

The tree boundary repair is explicitly contextual: Leaf fanout `1..=192`,
Index fanout `1..=96`, Directory depth `0..=2`, Leaf depth `0`, and Index
depth `1..=2`, with every lower/upper hostile boundary covered at
`hostile_and_properties.rs:579-611`.

## 5. Policy and resource self-audit

The exact core package has empty normal and development dependency tables and
the dependency tree contains no child package. Targeted `rg` scans returned
exit `1`, meaning zero matches, for:

- concrete hash libraries, filesystem/host/runtime/provider/serde/FFI/unsafe
  behavior, and DB packages;
- reflink/clone/copy-offload APIs and FUSE/virtiofs/9p mechanisms;
- production `Vec`/`String`/`Box`/collection/reserve/allocation APIs; and
- named historical POC source or fixture imports.

The core `src/` inventory is exactly `lib.rs`, `codec.rs`, `error.rs`, and
`path.rs`; no `identity.rs`, `port.rs`, `object.rs`, CDC, store, pack, ledger,
cache, benchmark, or later-scope module exists.

```text
CORE_HASH_LIBRARY_DEPENDENCIES: 0
CORE_FILESYSTEM_RUNTIME_PROVIDER_SERDE_FFI_UNSAFE_IMPORTS: 0
REFLINK_CLONE_COPY_OFFLOAD_SYMBOLS_IN_M6_1_DIFF: 0
FUSE_VIRTIOFS_9P_SYMBOLS_OR_PACKAGES_IN_M6_1_DIFF: 0
DB_LOOSE_SIDECAR_CACHE_TRUTH_IN_M6_1_DIFF: 0
HISTORICAL_POC_IMPORTS_OR_FIXTURE_COPIES: 0
OUT_OF_ALLOWLIST_PRODUCT_FILES: 0
BENCHMARK_E2E_LIVE_RUNTIME_RUNS: 0
```

The inherited root-workspace `rusqlite` dependency and the four inherited
`std::fs::copy` sites recorded in
`M6_0_SOURCE_OWNERSHIP_AND_POLICY_RECEIPT.md` remain outside the core closure
and outside the M6.1 diff. None was edited.

## 6. Independent reviews

Two bounded, independent, read-only reviewers assessed the final repaired
source snapshot. Neither edited a file.

| Focus | Report | Report SHA-256 | Verdict |
|---|---|---|---|
| Codec/path/error correctness | `M6_1_2_CORRECTNESS_REVIEW.md` | `e88d90b86520c8acfafc6cd98f6a059e7c93ffb0d91553c22429a662ba576256` | unconditional `PASS`; P0=0; P1=0 |
| Scope/resource/policy compliance | `M6_1_2_POLICY_REVIEW.md` | `949bbe94bde6b8aec60b238bbf464310648ba6db8f13b4911731eda92ffdda5a` | unconditional `PASS`; P0=0; P1=0 |

The correctness review first found one P0 on a superseded source snapshot.
The tracker remained at 11/48; the precise contextual tree-bound defect was
repaired; all scoped commands were rerun; and both reviewers then issued the
final PASS verdicts against the identical four implementation/test hashes in
section 2. No P2/editorial issue reopened the sealed contract.

## 7. Disposition

```text
M61_0201_THROUGH_M61_0206: AUTHORIZED_TO_CHECK
M6_1_2: COMPLETE_AFTER_MECHANICAL_TRACKER_UPDATE
M6_1_IMPLEMENTATION_CHECKED: 17_OF_48
M6_1_PROGRESS_PERCENT: 35.42
P0_OPEN: 0
P1_OPEN: 0
M6_1_3: STOP_NOT_ENTERED_IN_THIS_TRANSITION
M6_2_AND_LATER: STOP
```

This receipt closes only M6.1.2. It grants no authority to begin M6.1.3 or any
later milestone in the same transition.

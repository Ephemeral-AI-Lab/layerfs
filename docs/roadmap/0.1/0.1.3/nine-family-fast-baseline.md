# Nine-family unified fast baseline

Status: PASS for the user-authorized nine-family fast baseline: 118 single performance samples and 27 selected independent proofs. All other families are deferred to #39. This is explicitly sampled verification, not exhaustive byte or statistical qualification.

The user superseded the 581-performance/119-proof inherited replay with the existing fast-path model: one complete selected current-source sample, reusable preparation, separate quick verification, and no automatic baseline/candidate or repetition multiplication. This new baseline records observations for subsequent comparison; one sample is not a five-sample median or evidence of size-parity distributions. Historical results and failures remain immutable.

## Shared execution contract

- macOS owns SDK, Workspace capture/spool and embedded SQLite. Linux containers run the daemon, real FUSE, and applicable workload helpers through the existing authenticated binding. No Docker data-sharing mounts.
- Same container defaults: 2 CPUs, 2 GiB memory/no swap, 256 PIDs. Host CPU is not cgroup-capped; host and container memory/CPU remain separate. Product work stays below 15 seconds. A selected verifier retains 45 seconds work/59 seconds hard end-to-end; quick warm verification is the target, not a promise inferred from small inputs.
- One sample by default. `--perf-samples` remains explicit and never changes a fixed edit input. Prepared inputs, expected plans, and source/build artifacts are reused. No reuse of mutated sample Stores.
- Host Store snapshots are closed/quiescent independent writable copies with full master-integrity checks. Directory-import families reuse owned prepared source trees and initialize fresh output Stores. Warm native-input reuse validates ownership and the prepared recipe instead of repeatedly hashing the entire tree. Its receipt says `owned-prepared-recipe`; modifying this disposable cache requires recreating it. Preparation compatibility is distinct from producer/executor provenance.
- Every route declares its product timer. Missing timing is a failure; command wall time cannot substitute for product time. Physical Store accounting remains separately observable.
- One shared verification launcher accepts one or more compatible timing-row bindings, not exactly five. Every receipt identifies its actual coverage; no materialization, fresh FUSE reopen, or conformance proof is claimed unless executed or explicitly bound.

## Scope

| Family | Registered performance cases | Preparation | Timer |
|---|---:|---|---|
| payload_create_read | 8 | Prepared Store | pure_call_sum_ns |
| dedup_workspace_reuse | 14 | Prepared Store | pure_call_sum_ns |
| dedup_cross_file | 10 | Directory input, fresh Store | pure_call_sum_ns (initialize) |
| dedup_cdc_locality | 20 | Directory input, fresh Store | pure_call_sum_ns (initialize) |
| edit_length_preserving | 12 | Shared SDK Store size | edit_commit_ns |
| edit_length_changing | 32 | Shared SDK Store size | edit_commit_ns |
| edit_canonical_chunk_count | 12 | Shared SDK Store size | edit_commit_ns |
| init_namespace | 4 | Directory input, fresh Store | layerstack_init_ns |
| store_footprint | 6 (3 main + 3 compact controls) | Directory input, fresh Store | product_call_sum_ns |
| Total | 118 | | |

The CDC boundary proof is proof-only and is not a performance sample. The extra compact controls remain identifiable; they do not replace large inputs or become misleading scaling points. Independent proof selection must cover all edit operation/outcome shapes and changed execution routes while avoiding repetition of unchanged passing proofs.

## Simplification and correctness

Namespace verification checks import file/byte counts and persisted root equality after Store reconnect. Real FUSE checks at most 11 files: first/last files in the first/middle/last directories plus the first representative of each file class. Each selected file checks type, exact length, mode/mtime, parent metadata and up to 64 KiB of independently generated prefix bytes. Coverage is explicitly sampled; it does not verify every namespace entry or every file byte. The former full-tree FUSE proof timed out at 45.050 seconds for 100,000 files and is retained as incomplete. No unrelated edit/Commit preparation sequence is added. Initialization performance still performs the public native import with actual byte/file counters.

Store-footprint retains its initial import, storage accounting, and required post-edit Commit state; Create/Exec/Commit/visibility/End are explicitly timed. Verification checks the published root after reconnect and compares canonical/FUSE bytes around the edit with the prepared input plus the edit marker. It omits full-tree FUSE digest replay and labels the bounded edit coverage. Accounting and outer process wall are not renamed product-call time.

SDK verification checks prequalified canonical roots, exact lengths and chunk counts, inode and untouched payload retention, and no-amplification observations. Canonical and FUSE reads cover the replacement plus up to 64 KiB on either side (at most 192 KiB), compared with an independent boundary oracle. Receipts explicitly label boundary hashes and report that full-file bytes were not verified. There is no full-byte option. Materialization is removed. The FUSE read occurs after Commit in the current projection; Store reconnect is separate and is not mislabeled a fresh FUSE mount.

The SDK families share six pristine Store lengths: 1/10/100/500 MiB and two shortened deterministic prefixes (524283904 and 524285952 bytes). Five growing 500 MiB rows have explicit `500mib-result-capped-v2` IDs and end at exactly 524288000 bytes. Their source/oracle/registry identities are versioned; historical oversized rows are not silently relabeled. New length-changing registry hash: `f59a1a68a19b95e1dcc8e6e5e273c7279c69fca373adca9980a9a9ffbd9fa517`; combined registry: `46d45fb445e2f1cd721c523ff8f4602c425364786f43b4ea2f81c0b4a55557e4`.

Source sealing now includes SQL files as well as Rust, Python, shell, manifests and lockfile inputs. New seal values therefore differ from historical seals even though the product implementation is unchanged. Prior evidence keeps its original identity scheme.

## Fast-only SDK verification check

Source `72f408b467840389be75dc14bd21495a07f6c39b6f2164d0ad6c7aea985e11d9` and matching Linux image passed all 14 edit operation/outcome shapes at 1 MiB, with one timing sample and one independent proof each. Proof wall times were 1.475–1.661 seconds. The capped 500 MiB insert also passed: 5.119 ms edit/Commit and 4.370 seconds proof wall, versus the retained 16.277-second whole-file proof. Its independent, canonical, and FUSE boundary digests match over 135168 bytes; exact prepared roots/counts, inode preservation, zero captured bytes and clean End passed. This is a verifier-work reduction, not a claimed product speedup or a full-file byte proof. Evidence is under `benchmark-results/nine-family-fast-baseline/72f408b467840389/`.

## Current-source performance baseline

All 118 registered performance cases passed once on the source above and Linux image `sha256:e3846fe4352aa9d90a5dcede742de39ae35300b9ff95263609dd404e3980be3d`. The assessment checks exact registry coverage, one sample per case, declared product timer below 15 seconds, matching source/image identities, identical observed container settings, no data mounts, unchanged prepared masters, and successful cleanup. Raw timings and preparation/cache observations are preserved in `performance-assessment.json` alongside the raw receipts. These ranges span different cases/sizes; they are not confidence intervals or same-case distributions.

| Family | Samples | Product time range (ms) |
|---|---:|---:|
| payload_create_read | 8 | 18.123–3049.052 |
| dedup_workspace_reuse | 14 | 26.300–4003.826 |
| dedup_cross_file | 10 | 4.282–542.101 |
| dedup_cdc_locality | 20 | 4.090–472.644 |
| edit_length_preserving | 12 | 3.479–6.471 |
| edit_length_changing | 32 | 3.174–6.276 |
| edit_canonical_chunk_count | 12 | 3.456–7.001 |
| init_namespace | 4 | 11.409–2850.586 |
| store_footprint | 6 | 29.321–4816.533 |

## Final selected verification

Final verifier source `aa907534bb9f760048739de4205a29c0c7630ad7de6a7b3c21b5e50e52b66cb3` removes the redundant namespace/storage full-tree reads and repeated native-cache hashing. Product source and measured operations are unchanged from the completed `72f408b467840389` baseline; that baseline is preserved under its actual identity rather than replayed or relabeled. The 15 passing SDK proofs remain under their producing source because this follow-up did not change their verification path. The remaining 12 proofs use the final verifier source and matching Linux image. This source difference is a verifier/setup change, not evidence of a product speedup.

| Coverage | Proofs | Proof wall range (s) |
|---|---:|---:|
| SDK length-preserving: all 3 edit positions at 1 MiB | 3 | 1.483–1.529 |
| SDK length-changing: all 8 operations at 1 MiB + capped 500 MiB insert | 9 | 1.475–4.370 |
| SDK canonical count: preserve/increase/decrease at 1 MiB | 3 | 1.486–1.572 |
| Namespace: all 4 sizes | 4 | 1.518–5.615 |
| Storage footprint: all 3 main controls | 3 | 2.663–7.287 |
| Payload create 500 MiB | 1 | 7.313 |
| Workspace unique reuse 500 MiB | 1 | 14.571 |
| Cross-file mixed 500 MiB | 1 | 8.240 |
| CDC insert 500 MiB + proof-only boundaries | 2 | 1.892–9.904 |
| Total | 27 | 94.268 seconds combined |

The 100,000-file namespace receipt checks 100,000 imported files/500,000,000 bytes, persisted root equality, and 10 FUSE paths/132,793 sampled bytes. Its reopen/sample check takes 219.860 ms; total proof wall is 5.615 seconds, including initialization and runtime setup. Warm preparation inside that proof fell from 9.162 to 0.839 seconds after removing redundant native-input rehashes. The old 45.050-second incomplete receipt remains untouched, including its post-timeout integrity-check cleanup failure. Final inventory confirms no owned sample containers or host sample directories remain.

Preparation misses remain a separate reusable stage; their receipts are retained in `proof-preparation/` and are not included in the 94.268-second proof sum. The final terminal assessment is `benchmark-results/nine-family-fast-baseline/aa907534bb9f7600/terminal-assessment.json`. Host/Linux builds and strict benchmark Clippy passed; all 22 shared tests passed before the final cache simplification and its affected 11 runner tests passed afterward. No new full-byte option or dedicated full-byte test was added.

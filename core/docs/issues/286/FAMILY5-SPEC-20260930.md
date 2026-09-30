# Family 5 frozen namespace selection v1

Planning/functional benchmark registration, 2026-09-30; not release admission.
Owner direction is to continue Family5 using the fast path, reuse unaffected
earlier proofs and leave the 10240-file architecture work deferred under #276.
Product implementation, storage codecs, workers, Budget, quotas and deadlines
are unchanged. This selection uses `workspace_namespace.py`, the existing
`benchmark_shell` SDK example, independent `verify_checkpoint5` and the existing
external native SDK fixture. No second Init runner is introduced.

The order is prospectively fixed: the ten `NATIVE` registry entries, then the
five `SDK` entries, exactly in insertion order. Every case has one child/arm;
there is no frozen matched public control, relative speed gate, median or retry.
Original source-pinned #273 profiles and Phase A receipts retain their status.

| Native component selection, ordered | Work/oracle | Functional child command bound |
| --- | --- | ---: |
| uncached descendants3 | same-depth growing-prefix move; full old/new/pinned tree and bytes, unchanged inode IDs | 15s |
| uncached descendants67 | same operation with64 extra inherited child files | 15s |
| resident descendants3 | explicitly lookup all source descendants before rename inside the functional command | 15s |
| resident descendants67 | same with64 extra resident files | 15s |
| unrelated resident256 | source stays3 descendants; lookup256 unrelated files before rename | 15s |
| deep4097 uncached | inherited15×250-byte components; depth-increasing move, component read,4096 rename, move-back, full canonical/pinned oracles | 15s |
| deep4097 resident | same with source descendants already resolved inside the command | 15s |
| retained live G1/G2 | deterministic held canonical reply; move G1 then later G2 write, full old/G1/G2 and old pinned namespace | 15s |
| exact rename refund | known Commit, completion escrow and slots refunded once, checked clean close | 15s |
| atomic rename refusal | independent baseline/target clones; baseline+8192-byte quota; refusal preserves revision/names and clean close | 15s |

Native route is Linux Workspace + production in-process Server using the existing
functional control topology. This is not public SDK latency or host SQLite
performance. Docker container/volume acquisition, closed prepared fixture cloning,
hash checks and ownership preconditioning are recorded outside the native child;
owned container/volume removal is checked separately. Public native operations and
all asserted reads/Commit/lease release/clean-close remain in the child command.
New native controls require prepared fixtures explicitly and are ignored by
ordinary unit-test discovery. The existing refusal control now checks its baseline
close as well. No private product source is compiled into tests.

| Public SDK/FUSE selection, ordered | Work/oracle | Complete launch-to-exit bound | Separate full verifier |
| --- | --- | ---: | ---: |
| move/replace descendants3 | inherited subtree move, child rename/move-back, source move-back, final move, grand file replacement and listing | 15s | <9s |
| move/replace descendants67 | same with64 extra one-byte files; every file preserved | 15s | <9s |
| inherited deep4097 | component directory descriptors, depth-increasing move, full leaf read and replacement at4097 bytes | 25s exception | <9s |
| components270 |270 successive mkdir/chdir by directory descriptor; leaf bytes; canonical serial traversal | 25s exception | <9s |
| retained G1 replacement | same mounted Workspace: move, pin G1, known prelude Commit, later replacement/known Commit, full held10-byte old file and checked release | 15s | <9s |

SDK timers include driver startup, host Server/SQLite, Sandbox creation, Mount,
arbitrary ordinary Exec, every declared Commit and Pin, Status, lease release,
unmount, Sandbox deletion, logs and shutdown. Report Exec, Commit and prelude
intervals separately; verification never enters a speed comparison. The two
deep SDK rows are prospectively declared25s exceptions, not enlarged retries.

The independent read-only verifier checks exact complete directory listings,
all modes and every file byte at old/new heads, known new Commit and parentage.
Family5 opts into public C1 inode-serial listing/portable metadata beyond4096
bytes or256 components; existing path-based verification defaults remain.
It checks every moved subtree inode identity, with the replaced leaf required
to receive a different inode serial. The270-component row checks every
pre-existing inode identity and the complete newly created tree. The retained
row independently checks the prelude's parent and both retained heads; the SDK
also checks the held old file digest. Native retained controls check the full
pinned namespace and bytes. Symlinks and broader mutations belong to Family6.

Reuse once-prepared closed C2/C5 masters: small(default prior Family4 master),
wide(extra64), deep15×250 and unrelated256. Each native sample receives an
independent writable byte copy with host/Linux SHA256 checks and recorded0:0
Linux ownership. SDK masters are acquired once from these immutable namespaces,
receive one untimed public SDK `.marker` Commit, close and seal; samples byte-copy
them. This preparation creates a retained starting head, not any case operation.
Public SDK C1/C2/C5 processing runs on the host and daemon/FUSE/workload on Linux.
Store/image/build reuse is explicit. Never reuse a mutated sample.

Cache is uncontrolled in both domains; cloning is not a cold claim. Every timing
is diagnostic, `numeric_latency_status=INELIGIBLE`, with no admission PASS or
invented comparison to a NOT_RUN baseline. Native counters record operation
intervals, resident nodes, upstream calls, private metadata reads/writes/pages and
accounted memory delta. Resident scans, descendant visits and internal C1 visits
remain UNAVAILABLE where the public surface does not supply them; upstream calls
must not be relabelled as internal visits. Status/cgroup lifetime memory is not a
phase-memory gate. No observer warms a row to give it numeric credit.

Run the SDK group through the fast performance lane and prove retained Stores
once separately with sealed verifier/artifact identities. Keep all failed,
INELIGIBLE and NOT_RUN rows and cleanup refusals in fresh immutable raw paths.
Do not rerun unchanged earlier-family arms. The shared helpers/examples changed,
but no product mechanism changed: prior F1–4 product proofs are reused explicitly,
including Family2's compound storage proof. No compression work is added.

Reproduction from the owned worktree:

```sh
python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-namespace-native --out benchmark-results/fs-bench-pro/issue286-workspace-namespace-native-r064
python3 core/benchmark/fs-bench-pro/runner.py run --family workspace-namespace-sdk --out benchmark-results/fs-bench-pro/issue286-workspace-namespace-sdk-r065
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue286-workspace-namespace-sdk-r065 --out benchmark-results/fs-bench-pro/issue286-workspace-namespace-sdk-proof-r065
```

Record all raw times/verdicts, compilation/source/dependency/image/fixture/oracle
seals, command and separate verifier bounds, explicit reuse, unavailability and
production LOC using `benchmark_agent_report.md`. The next family is ordinary
mixed `workspace_mutations`; this document does not pre-qualify that work.

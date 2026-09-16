# LayerFS 0.1.6 limitations

> **Status:** LayerFS 0.1.6 Developer Preview manual.

LayerFS is for local evaluation and development. Keep independent copies of
important data; it is not a backup service or a hostile-code security boundary.

## Storage and compatibility

- One Client binds one local Store, with one live local Store authority and
  exclusive SQLite ownership. Cross-host synchronization and automatic repair
  are outside this release.
- Publication is committed and readable from the same live local Store process.
  Process-crash, OS-crash, power-loss and disaster-recovery guarantees are outside
  the MEMORY-journal/synchronous-OFF profile. Filesystem fsync and bounded fsync
  batching do not upgrade database durability.
- New Stores use schema 10. Supported schema-6/7/8/9 Stores retain their own
  schema and construction policy. Published 0.1.3/schema-5 Stores are rejected.
  `LayerStackStore::upgrade_format` promotes a closed schema-7/8/9 Store to
  schema 9 only; there is no in-place promotion to schema 10 and no downgrade.
  See [compatibility](storage-format.md#compatibility-and-migration).
- **The sandbox owns live mutable state in v0.1.6.** A v0.1.6 sandbox owner and a
  v0.1.5 host do not share a mutable workspace; mixed-version live sessions are
  unsupported. The mutable namespace, file data and private packed payload backing
  live in the container for the life of the Workspace, and only Commit-time
  mutable-state transfer crosses to the host.
- **Kernel-dirty shared mmap is still not captured by a Commit.** A write made
  through a shared `mmap` that the kernel has not written back is not part of the
  published snapshot. The boundary is isolated to exact byte level and no
  registered benchmark selection is affected; it remains the specification's open
  obligation.
- **One construction worker is mandatory**, so construction-heavy cases are slower
  than v0.1.5's four-way small-content path: six material regressions
  (dedup/CDC construction, 1.50–1.65×) are recorded and owner-accepted, with the
  direct diagnostic in [the changelog](../../../docs/releases/v0.1.6/CHANGELOG.md).
  `init_namespace` keeps its multi-worker initialization exception.
- **The host-side FUSE write-spool metric is dead on the sandbox route** and is no
  longer a gate; re-wiring it needs a new cross-boundary counter. The container-side
  counterpart shows one bounded behavioural difference from v0.1.5 (a failed append
  leaves its reserved range packed in the current segment, retired with it).
- **Sandbox process memory is not measurable on the frozen harness**; only
  container-scoped numbers (quota, current, lifetime peak) can be compared.
- **Time comparisons are cache-stance and host-load sensitive** beyond the declared
  allowance: every v0.1.6 benchmark row carries its host load, and a contended
  sample is not pooled with a quiet one.
- Explicit compaction is removed. Previously compacted Stores remain readable
  through the retained authenticated `LFCNT1`/version-107 path, which is read
  compatibility only. There is no product compaction command, export or
  maintenance mode in this release.
- Use matching-release SDK, CLI owner, daemon and runtime components. A protocol
  version field is not evidence that 0.1.4 and 0.1.5 components interoperate.
- Automatic object garbage collection and deletion are outside this release.
  LayerStack and Branch names remain immutable.

## Live filesystem boundaries

- Managed execution requires a compatible local container runtime. Real Linux
  FUSE requires `/dev/fuse` and the documented privileges; see [container
  runtime](container-runtime.md).
- Each execution starts its own process. A live Workspace and mount may survive
  multiple executions and Commit cuts, but there is no persistent shell-state
  guarantee across independent executions.
- Commit can capture a cut while a command continues. Applications requiring an
  atomic multi-syscall update must coordinate that update themselves.
- Ordinary writes are acknowledged into live buffered ownership; backing flush,
  filesystem fsync and canonical Commit are distinct operations. An acknowledged
  write is not a crash-durable database publication.
- The bounded pending representation coalesces an equal-length overwrite of
  committed content into a base root plus a splice descriptor that is converted
  at Commit. Pending capacity is bounded by the edit budget, not unlimited; other
  edit shapes still retain per-piece state.
- A newly created FUSE file uses direct I/O for its create handle. mmap on that
  handle is unsupported (`ENODEV`); close/reopen restores the normal cached-open
  path. Tested dirty-mapping/SDK coherence does not imply unrestricted POSIX
  compatibility for every program or filesystem feature.
- SDK range-edit batches must be non-empty and target one regular file in one
  Workspace. Namespace and metadata changes remain filesystem operations.
- Clean End refuses dirty state; Discard must be explicit. End does not Commit.
  Applications remain responsible for releasing Workspaces, executions and
  containers they create.
- Retained stage and presentation recovery operate within the documented live
  ownership model; they are not restoration of arbitrary active commands after
  a host crash. OverlayFS projection is outside this release.

## Scale and evidence

Resource limits apply to their named buffers or policies, not automatically to
the whole host process. Initialization fast paths require eligible input shapes
and retain canonical fallbacks. Ordinary Commit and every initialization route
do not share identical scheduling or admission behavior.

The [v0.1.6 release record](../../../release-notes/0.1.6/README.md) states its own
exceptions and non-passing rows. The earlier [v0.1.5 release record](../../../release-notes/0.1.5/README.md) inherits the
#120 finalization campaign on the measured candidate (`c55daf13…` at
`1ff1f2ddd`): 198/198 registered performance selections terminal — **182 fresh
(56 PASS / 125 WARN / 1 FAIL) + 16 reused** — and 29/29 proof-only selections
terminal (**28 PASS + 1 NOT_RUN_OPTIONAL**). These are fixed-seed, source-bound
single samples with the declared verification coverage, not latency
distributions, paired comparisons or crash qualification. v0.1.3 references have
an undeclared cache profile, so their ratios are historical context only.

**Measured regressions, published as measured:**

- `dedup-history-unrelated-500-mixed-v2` measured **16.107 s** against the
  registered `unrelated-history500 < 15 s` gate: a **FAIL**, dispositioned by an
  explicit owner [waiver](../../../release-notes/0.1.6/waivers.md) whose residual
  risk is that the worst-case unrelated-history tier runs about 7 % over its
  historical gate.
- `namespace-100000` cold Init measured **4.3975 s** against a **2.7 s** target
  that the owner waived in #118; the waiver covers that target only.
- **125 WARN cells.** Worst ratios against v0.1.3: `dedup-cdc-scattered-100`
  3.45×, `dedup-cross-file-unique-100` 3.35×, `namespace-100` 3.22×.
- **139 of 198** comparable cells are ≥15 % slower than published v0.1.3, median
  ratio **1.34×**, total added time **+33.115 s**. This is campaign context for
  future target selection, never an acceptance gate.
- The #116 bounded pending representation and #107 pack coalescing add fixed
  per-iteration costs of about **+2.49 ms/exec and +1.46 ms/commit** (#116) and
  **≈+0.4 ms/phase** (#107) on unrelated-history shapes.
- `store-footprint-unique-100000` construction timer is **5.397 s** (+36 % versus
  the fsync-qualified treatment): the #107 pack-row UPDATE cost on one giant
  commit, against a **−1.9 %** allocated-footprint reduction.
- Ordinary full157 allocated **83,951,616 B** versus a Git157 control live
  **56,197,120 B** — an accepted residual of the ordinary-write route, retained
  with its allocation-layout WARN.
- `dedup-cdc-scattered-100` leaves an unattributed **≈452 ms** gap between its
  308.33 ms timer and its 760.6 ms command window on this treatment; the
  `slab_send_blocked_ns` mechanism remains unattributed because that counter is
  absent from the ordinary-path receipts.
- The default-budget K32000 route reports a harness `TARGET_MISS` against its
  historical 15 s *family* target; the harness itself labels that target
  reporting-only, and the route's independent verification passed.
- **Endurance is not qualified:** `workspace-sustained-600s-compact-v2-proof` was
  `NOT_RUN_OPTIONAL` under the frozen campaign declaration (optional long test).
  The other five extended reliability members and all 27 runnable reliability
  proofs passed.

**Measured improvements (same evidence base):** against the earlier #104
baseline the candidate's median ratio is **0.993×** with 38 fresh cells ≥15 %
faster — `workspace-distributed-sdk-edit-500` 4.131 → 0.666 s (0.16×) and `-100`
0.45× (the #116 win), unrelated-history cells 0.43×–0.68×; store footprint
allocated −1.9 % on unique-100000 (530,358,272 → 520,142,848 B) with pack rows
3,457 → 1,058 and −43 % pack overflow slack on full157 (#107); `payload-create-*`
0.79–1.02× and `dedup-workspace exact/local-100` 0.62–0.66× v0.1.3.

Owner acceptance retires optimization scope; it does not relabel the FAIL, the
WARNs or the #108/#112/#114 unmet gates as passes. Host CPU/storage scope differs
from container limits; a successful execution or proof does not establish a
performance pass. Full measurement details, immutable source/binary/image
identities and the exact unmet gates remain in the linked release record and
[issue #120 report](../../roadmap/0.1/0.1.5/issue120/final-report.md).

## Distribution

Build instructions are in the [quickstart](quickstart.md). Prebuilt binaries,
crates.io packages and runtime images are official release artifacts only when
the release record explicitly lists them with immutable identities. Do not infer
artifact publication from a repository tag or an example image name.

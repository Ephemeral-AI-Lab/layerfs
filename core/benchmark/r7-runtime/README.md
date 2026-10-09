# R7 real daemon runtime harness

> **Status:** Current general guide.

This independent harness workspace uses only existing first-party dependencies.
It is exploratory, not admission eligible, and has no product source membership.
The lead runs every build or invocation under the checkout lock and a declared
wall stop. The harness performs one attempted operation and retains a container
on original failure. It never removes containers, volumes, fixtures or evidence.

`provision` performs host `ProjectApi::init` on an independent source copy,
including consuming seal, creates the real daemon with `SandboxApi`, and streams
the sealed file once through SDK installation into an existing named VM volume.
It writes the installed manifest as a private binary record and explicitly stops
the setup container. It retains the container, sealed master and volume. The
copy's root uid/gid must match the ordinary command identity. Init preserves
metadata; the harness does not rewrite ownership or weaken modes.

```sh
layerfs-r7-runtime provision \
  --socket /Users/yifanxu/.docker/run/docker.sock \
  --volume OWNED_NEW_VOLUME \
  --daemon /ABS/core/target/r7-linux-release/release/layerfs-daemon \
  --source-copy /tmp/INDEPENDENT_COPY \
  --sealed /tmp/NEW_SEALED.sqlite \
  --manifest /tmp/NEW_INSTALLED.manifest \
  --receipt /tmp/NEW_PROVISION.events.jsonl \
  --uid 501 --gid 20 --init-seconds 300
```

The image is fixed to
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
The daemon executable is uploaded through the SDK, with no image rebuild or
network dependency. Release identity and ARM flags must be checked by the caller.
Provisioning is setup and never a mounted timing sample.

The prospectively frozen resource selection uses `OwnerConfig::default()`:
8 MiB aggregate owner credits, 64 KiB Lifecycle reserve, 16 namespaces,
16 ordinary and 2 Lifecycle slots (Source has its separate 16 slots). The cache
allowance is the current S8 8 MiB logical charge. There are 2 Store readers,
4 ordinary controls and a 1024 KiB overlay pager suggestion. These settings never
increase in response to a workload, failure or measurement.

`serve` starts a real daemon from an explicitly already installed manifest and
keeps its Store sessions, immutable cache and overlay alive across explicit
commands. The host remains control-only. Mutable sample Stores require a fresh
independent byte-copy volume; the manifest describes authority and locator and
does not authorize reuse of mutated input.

```sh
layerfs-r7-runtime serve \
  --socket /Users/yifanxu/.docker/run/docker.sock \
  --volume OWNED_VOLUME_WITH_INSTALLED_STORE \
  --daemon /ABS/core/target/r7-linux-release/release/layerfs-daemon \
  --manifest /tmp/INSTALLED.manifest \
  --receipt /tmp/NEW_SERVE.events.jsonl --uid 501 --gid 20
```

Each stdin command is one line with **literal tabs** between fields. Scripts live
in files outside the product and are read before the command timer. Workspace
keys are 64 hexadecimal digits supplied by the controlling harness as authority
identities; they must be fresh for every Mount. Each command emits a newline JSON
event on stdout and in the append-only receipt. Wait for that event before the
next dependent command. EOF is an error that retains ownership; explicit `stop`
requires every Workspace already unmounted.

| Command | Fields | Operation and receipt |
| --- | --- | --- |
| `mount` | key | `WorkspaceApi::mount`, including Bind plus Attach to Ready |
| `command` | key, script filename | Ordinary `/bin/bash -o pipefail -c`, via runtime only; no filesystem Exec registration |
| `commit` | key | Exactly one `WorkspaceApi::commit`; records original typed history outcome |
| `snapshot` | key | SDK Status plus external backing/process observer; outside product timers |
| `observe` | none | External backing/process observer only |
| `verify` | key, oracle filename | Separate ordinary command, labelled verifier; caller enforces independent budget |
| `unmount` | key | Exactly one normal terminal `WorkspaceApi::unmount` to its reply |
| `cleanup` | original closed key | One read-only `WorkspaceApi::cleanup`, after routing removal; records actual Live/Held/Queued/Gone |
| `stop` | none | One EndSession and explicit container stop; no deletion |

A key `native:/ABSOLUTE/NATIVE/ROOT` is permitted for `command` and `verify`.
It selects the container's own filesystem and the identical Bash, image, runtime
and nonroot identity. Prepare that independent native fixture outside timing;
neither the harness nor a mounted measurement materializes the immutable root to
implement the native arm. `docker cp -a` or equivalent declared byte-copy setup
can preserve original copy metadata. Native mount/unmount are N/A. A real product
daemon remains idle in this topology and its setup/resource cost is explicitly
separate from the native command.

Each fresh receipt exclusively creates a sibling artifact directory by replacing
its final extension with `.artifacts`. Existing directories and receipt files
are refused before any Init, installation or runtime effect. Command output
files are session-local within that directory, so separate receipts in `/tmp`
cannot collide at the same event sequence. Historical shared-parent output
files remain untouched.

An external runtime command creates output files before the timer and streams
stdout/stderr directly into those files. It records observed stream EOF separately
from runtime exit observation. The timer covers create, start, stream drain and
status observation. A nonzero exit remains a failure with files retained; it
never infers that published filesystem changes disappeared. Scripts that expect
a nonzero exit must declare the expected result in a future additive protocol
before use; current commands require exit zero.

For the initial changed-binary survival proof, use `mount`, an ordinary script
that writes a known file, `commit`, `unmount`, `mount` with a new authority key,
`verify` with an independent exact-byte script, then `unmount` and `stop`. The
fresh mount uses the same installed Branch with its acknowledged head.

Observers read `/proc/1/status` (including daemon `VmHWM`) and `stat` for Store,
its sidecars, overlay and overlay journal. Block counts times block bytes are
allocated bytes. `VmHWM` is a lifetime high-water mark, never a phase peak. The
daemon is PID 1 under the checked SDK container topology. Root observer commands
do no mutation, no cache eviction and no filesystem mount access.

Ordinary control replies retain their original outcomes. The generic observed
facade adds caller-selected numeric diagnostic records for existing owner,
statement, Store, reader, cache, native and Commit counters. The harness retains
these original daemon records separately and validates their bounded framing.
An explicit read-only Cleanup call observes the original closed namespace after
terminal unmount; Unmounted alone never claims Gone. Source availability is not
runtime proof, and missing/failed diagnostics remain unavailable. Per-file
residency, cache-class eligibility, external identity seals, sample uniqueness,
complete-command wall budgets, reuse and full/scoped oracles belong to the
prospective registry and runner. No cold-state mismatch becomes a pass and no
numerical performance threshold is assigned here.

Build this manifest from the repository root so the ARM64 Cargo config is found.
The lead generates its lockfile offline once from the repository's existing
dependency set, then all builds use `cargo +1.85.1 --locked`. Set a worktree-local
target. No build may overlap a Docker, test or measurement command.

The prospective selection runner is invoked from the repository root under the
checkout lock, with an already prepared, sealed configuration:

```sh
PYTHONPATH=core/benchmark/fs-bench-pro python3 -B -m r7.runner \
  --config /tmp/SEALED_CELL_CONFIG.json --selection E02:A:L \
  --output /tmp/NEW_EXCLUSIVE_SAMPLE --claims /tmp/OWNED_ONCE_CLAIMS
```

`--all` is the explicit complete-matrix option. Its configuration uses a
`selections` mapping from every exact selection ID to independently prepared
cell inputs. Missing inputs remain `NOT_RUN`; mutated sample volumes are never
silently reused. The runner creates no fixture, Store, image or volume and makes
no automatic retry. An exclusive claim binds each L selection to the full
identity inventory. N/P claims exclude unrelated product source changes while
retaining their own binary, image, harness, workload, fixture and oracle seals.

Each cell configuration requires `runtime_binary`, `runtime_sha256`,
`daemon_binary`, `daemon_sha256`, `socket`, `volume`, `manifest`,
`manifest_sha256`, `identities`, `build_profile=release`, `uid`, `gid`,
`setup_method`, `declared_interference`, `fixture_kind`,
`cache_observer_inside`, `native_root`, `container_setup`, and
`source_clean=true`, and `source_dirty=false`. `identities` contains source commit/tree, product,
compilation, dependency, binary SHA-256, image ID, harness, workload, fixture,
oracle and report-generator seals. A writable clone additionally requires
`volume_clone_identity`; qualified shared binding requires
`shared_binding_identity_proof`. The controlling lead verifies these seals;
configuration booleans alone are not independent build or source proof.

Each selection creates a new SDK container. After its acknowledged identity and
protocol readiness, the runner performs the prospectively sealed untimed
deployment before any Mount, warmup or cache predicate. A boolean cannot replace
this deployment. `container_setup` contains `manifest` (an owned `/tmp` JSON
file), `manifest_sha256`, `implementation_sha256` (the actual
`r7/deployment.py` SHA-256), and `wall_stop_seconds=300`, selected by staging
plan 91. Its separate setup receipt records each original action, source and
container identities, output custody, verification and setup resource domain.
Neither setup time nor setup warmth qualifies a performance phase.

The deployment manifest has `schema=r7-container-deployment-v1`, `image_id`,
`uid`, `gid`, and explicit `assets`, `native`, and `passthrough_mounts` lists.
Each asset/native tree contains `source`, `target`, `inventory`,
`inventory_sha256`, and `content_metadata_set_sha256`. Sources and inventories
must resolve into this stage's owned `/tmp/layerfs-r7-*` preparation paths.
Asset targets are `/code` and `/replay`; native targets are `/native` or distinct
`/native-*` roots; P mountpoints are `/p` or `/p-*`. Store, Overlay, Workspaces,
fusectl and system paths are protected. L must have an empty `native` list and
never materializes its Workspace root. Every copy goes directly into the
acknowledged container's own filesystem, using the declared byte-copy setup;
there is no image rebuild, network fetch or transport replay.

Prepare a tree inventory with the standalone sealed helper:

```sh
python3 -B core/benchmark/fs-bench-pro/r7/deployment.py seal-tree \
  --root /tmp/layerfs-r7-OWNED_INPUT \
  --output /tmp/layerfs-r7-NEW_INPUT.inventory.jsonl
```

Its printed SHA-256 and content/metadata set SHA-256 populate that tree's
deployment fields. The retained full fixture can reuse its closed copy
projection through `--fixture-rows`, `--fixture-rows-sha256`, and
`--expected-set-sha256`. Projection checks current owned-copy metadata and names
and reuses qualifying content hashes; it never opens the protected original or
rereads host payloads. This alone cannot detect a same-size rewrite with restored
mtime. Mandatory actual-container full-byte verification catches that mismatch
before a sample. Other newly sealed owned assets read their complete payloads
in bounded windows within the setup domain.

`/code` includes `r7_deployment.py` copied from the exact sealed implementation,
`workload.py`, `oracle.py`, `changes.py`, `residency.py`, `stream_manifest.py`,
and `generate_manifest.py`. Case-specific static inputs include
`tracked-paths.json`, `node-roots.json`, and `largest-path` where selected.
P's sealed binary also lives under `/code`. The runner validates deployed helper
bytes before executing its verification helper, and verifies complete native
contents, names, modes, ownership, symlink targets and internal hardlink
relations. It explicitly reconciles then checks supported nanosecond mtime;
archive preservation is not assumed. Ctime and equality of host/Linux inode
numbers are not claimed. Every metadata action is attempted once, and failure
retains the exact container for lead-owned disposition.

After actual final native copies pass verification, their inode-ordered manifests
are generated and retained. The observed `native_file_manifest_inside` and
`native_file_manifest_sha256` are bound into the effective configuration before
cache A; they cannot be known before fresh Linux inodes exist.
This keeps hundreds of thousands of names out of Docker CLI argv. Per-file raw
observations are transferred once into `cache.files.jsonl` outside timing.

Linux native symlinks expose mode 0777. A sealed input that requires another
symlink mode is rejected during manifest preflight with
`UNSUPPORTED_LINUX_SYMLINK_MODE`, before any container/copy attempt. The original
111 transfer demonstrated source mode0755 versus actual0777 with other supported
fields matching. The strict verifier remains unchanged; full controls with that
representation limit remain unrun. An explicitly declared replay payload cut
can omit physical symlink nodes unused by the original workload while retaining
every original master.json row and target. This does not normalize native inputs.

E12/E13 class A additionally pays for cold `/replay/F` and master.json input.
Staging generates a separate actual `/replay` regular-file manifest. The runner
enforces a separate per-file hint/mincore predicate, retains its raw inventory,
and requires every replay component to have zero resident pages before an
attempt. These inputs never replace the main Store/Overlay or native predicates.
Timed semantic `/code` data such as E14's node-roots.json receives its own cold
file predicate. B/C and no-class Commit selections retain an explicit account of
setup/full-byte-verification and identical-call warmth. Common interpreter and
helper executable warmth is consistently declared separately from input data.
Declared alias handling is explicit through `declared_native_aliases`. P additionally
requires `passthrough_inside`, `passthrough_mount`, and
`matched_profile_identity`; the mountpoint and backing directory must already
be prepared. C09–C11 require `preconditioned_empty_root_receipt`; E19 requires
`related_root_preparation_receipt`. A reduced fixture requires its explicit
`reduced_fixture_cut_identity` and remains labelled in the sample receipt.

Native and passthrough W01 use exactly two already prepared independently
writable full roots in `native_peer_roots`, with the first equal to
`native_root`. W02 retains one root. Both controls require
`peer_preparation_identity` and a sealed scoped `oracle_script`. These inputs
are supplied by the lead; the runner does no source-directory copy. Untimed
ordinary nonroot metadata/permission checks reject actual aliased or overlapping
roots. P additionally uses corresponding distinct prepared mountpoints in
`passthrough_peer_mounts`, the first equal to `passthrough_mount`. W01 starts
two real sessions, validates both original two-loop readiness events and matched
negotiated profiles, warms the identical schedule on those same mounts, and
retains them for the measured calls. W02 uses one retained mount. An ordinary
Bash parent launches the two exact registered bodies concurrently in their
respective directories and explicitly waits for each child. Both original child
PID/exit records must be present. Each P mount gets one plain detach and its
original clean two-loop join/handle-drain check. Failed peer custody is retained
separately. Missing preparations keep the selection `NOT_RUN`.

The runner writes the canonical registered environment to a session-local file
and passes `--environment-file`; its format is one `NAME=value` per line with
unique names and explicit `LAYERFS_CONSTRUCTION_WORKERS=1`. Absence of that
option retains the older functional-proof minimum environment, and must not be
used for a registered workload comparison.

`oracle_recipe={bundle,closed_sha256}` selects a closed reference-author bundle.
The loader requires the registered body/environment, current helper/source seals,
actual prepared Store and installed-manifest hashes, original native-reference
root sets, and the exact case-specific tree checkpoint/role coverage. The only
stdout-only tree exclusions are E05/E06. Each comparison script must equal the
canonical author body; its expected files and helpers must match both the sealed
deployment inventory and the actual staged bytes. The original verifier event's
body SHA-256 must match that script. An arbitrary known-zero script cannot
establish verification PASS.

`oracle_fixture_identity` includes `prepared_store_sha256` and
`installed_manifest_sha256`; `prepared_store_file` names the closed host master.
L also requires `base_fixture_inventory={path,sha256,
content_metadata_set_sha256}`. N/P use their ordered native deployment sets.
The bundle records the same original inputs and complete native verification
receipts. Supplemental reduced fixtures retain their separate identities.
`oracle_prepare.py` authors untimed native expectations on independently staged
reference roots; it is never a reused N performance sample. Its per-body and
per-observer setup stops do not demonstrate the complete K performance budget.

Every verification has one separate 9.5-second stop shared by the selected
filesystem comparison and original host stdout comparison. K verifies every
checkpoint after its known Commit; a selected fresh-mount proof uses the final
checkpoint before another mutation. Verification failures are excluded from the
performance clock and retained. Cleanup and explicit stop follow the terminal
performance boundary. Exact runtime exit time remains unavailable:
the harness records transport EOF, original inspection and inner Create/Start/
stream/status spans, plus the broader runtime interval that includes ID-custody
publication. These intervals must not be summed as overlapping independent
phases.

Each completed Start, stream-EOF and inspection phase is published before the
next potentially blocking operation. An interrupted command therefore retains
its last completed phase. Missing stream EOF does not establish when the root
process exited; an independent retained Engine event may supply that separate
timestamp without replacing the original runtime verdict.

The separate `r7-git-index-scoped-v2` registry variant preserves original
registered bodies and the full row population. E04/E10/E11/E18/C12 compare the selected
generic `.git` tree and every semantic index entry/TREE record, retaining raw
index bytes and cached stat fields as separate observations. The Git recipe
binds the actual binary/version/config queries, qualified default policy,
semantic index, generic tree, shared comparison pin and all nested witness
files to their declared hashes. Standalone Git assets include `git_queries.py`,
`git_index_oracle.py` and `workloads.py`. The actual verifier independently
qualifies its comparison-time context and uses the sealed paired pin; it does
not claim to have observed the actual context before the measured command.
E18 class C remains `NOT_RUN` in all arms: identical-command warmup can refresh
the required unrefreshed index. No reset or normalization is authorized.
The separate exact L raw-index Commit/fresh-mount survival proof remains a
distinct prerequisite and cannot be supplied by semantic cross-arm equality.
Only E04/E18 query with their registered fsmonitor override. Mutable Git cases
query the configuration their original body uses and refuse unsupported values.
E10/E11 retain the pre-body query context; C12 requires a declared and actually
verified empty root, runs its whole registered body once, then queries the
created Git context. E11 additionally seals its tracked-path operand. E19
remains unavailable until its exact related-root preparation and stdout evidence
exist; this author does not invent a refreshed-index fast path.

`core/benchmark/r7-tools/reference_prepare.py` controls untimed native expectation
authoring through one fresh SDK container. Its configuration supplies the closed
host `prepared_store_file`, `base_fixture_inventory`, exact runtime/daemon and
manifest hashes, an independent volume-clone receipt and a sealed deployment.
It stages once, retains the exact acknowledged inside-inventory paths, declares
the original reference input/spec, and invokes the root setup author once.
Every original workload and query drops to501:20; root setup access does not
change private inventory permissions or measured command identity. The controller
compares the original author stdout with copied closed.json, binds every expected
operand, retains all witnesses, and closes its owners once. Only success sends
EndSession/explicit stop; failure retains exact container custody. The lead
enforces its prospectively declared660-second setup-only stop, with separate
300-second stage/author allowances and15-second other actions. This is no
performance or independent-verifier allowance. Authored roots are never reused
as mutated N/P performance inputs.

The harness emits acknowledged container/Exec IDs before Start and blocking
stream drain. A host watchdog or event timeout only fences the host controller;
the receipt retains Docker-owned command/container custody and establishes no
filesystem drain. The lead explicitly cancels/stops only its own retained
container when required. Root process/backing observers use an external
`docker exec --user 0:0` against that exact ID: the ordinary SDK admission guard
remains unchanged. All Store and overlay sidecar absences are explicit.

Workspace calls opt into the generic observed facade with a nonzero fresh
32-byte scope, supplied by `--observation-scope` or generated for this session.
Numeric record identity is daemon instance + caller scope + call ID; an admitted
slot is attribution, not a session identity. Duplicate complete identities fail
validation. Numeric daemon records
are retained through one log read and checked against their original bounded
headers/footers. Cumulative snapshots remain labelled cumulative unless an
exact supported delta is derived. The runner does not parse Rust Debug as a
typed outcome or turn unavailable counters into zero. Missing instrument or
scope attribution leaves the row `INCOMPLETE`, even after functional success.
Cleanup uses distinct read-only observations of the original closed token until
`Gone` or its declared readiness stop; no Unmount or maintenance turn is replayed.

Successful observed SDK lifecycle events expose actual numeric control-send
record bounds. The fresh serve channel establishes exactly one Hello, and
successful Mount has two original observed calls (Bind and Attach); Commit,
Status, Unmount and Cleanup each have one. The runner validates every numeric
daemon/scope/call group against those ranges. Failed calls receive no invented
successful range. Status additionally emits one known Lifecycle State job only
when its original typed local result is present; unavailable local results and
older receipts keep that attribution unavailable.

`r7.counts` decodes the original source field order and accepts explicit original
before/end identities. It subtracts only the exact section-30 Resources observer
Read jobs and Startup StatementWork within that interval, retaining every raw
operand. A separate count-only floor view can also remove typed known Status
State jobs; their SQL VM work remains unavailable. Cumulative gauges, lifetime
peaks and all time stay absolute. Instrumented SDK spans include diagnostic
acquisition, formatting and transport. A configured `phase_count_endpoints`
list has `label`, `before_identity`, and `end_identity` per explicit interval;
`exclusive_observer_scope=true` is required. The runner does not insert a Status
baseline after a cold predicate or guess endpoint pairs. The standalone decoder
accepts `--numeric-validation`, explicit `--before-identity`/`--end-identity`,
`--exclusive-observer-scope`, `--label`, and a fresh `--output`; optional
`--control-correlation` supplies the original typed Status operands.

Configurations can instead declare `phase_count_selectors`, each with `label`,
`before`, and `end` logical endpoint labels. They resolve only through successful
correlated original event sequences and first/last calls. Available labels are
`measured_mount_bind`, `measured_mount_attach`, `command_end_status`, `command:0`, `commit:0`
(and later original command/Commit ordinals), `after_verifier:checkpoint-1`
(and later checkpoint ordinals; fresh verification uses a `fresh_` prefix),
and `unmount:0` (and later terminal
ordinals). B also retains `warmup_unmount`. C retains
`retained_mount_attach_before_warmup` and `warmup_end_status`; the latter is one
declared observer Status after identical warmup and before the C predicate/start,
outside performance. It has real State/Resources work in the receipt. A/B gets
no extra post-predicate baseline. Selecting a pre-warmup endpoint explicitly
includes warmup work; it cannot masquerade as a measured-command-only delta.
Missing or ambiguous bindings stay unavailable. Absolute explicit identity pairs
remain supported.

The actual-byte source inventory is mandatory in `source_inventory={path,
sha256}`. After committing relevant sources, seal it once for each selected arm:

```sh
PYTHONPATH=core/benchmark/fs-bench-pro python3 -B -m r7.source_inventory \
  --arm L --output /tmp/layerfs-r7-SOURCE-L.json
```

The `r7-source-inventory-v1` artifact records arm, scope policy, commit/tree,
explicit source roots, source-set SHA-256 and sorted files with path, complete
byte SHA-256, byte count and mode. Verification independently enumerates current
membership and reads every relevant file's complete bytes, rejecting untracked
and dirty inputs. L includes core product inputs, Cargo, configuration and the
authorized core/vendor/fuser-0.18.0 source. N/P include their R7 workload/cache/
runtime/harness inputs; P also includes its implementation and fuser source.
Unrelated live LayerFS product edits do not change N/P seals. Their already
compiled SDK/daemon dependencies retain their immutable compilation/dependency/
binary provenance; this is not a claim that changed product dependencies were
rebuilt. Source-set hashes exclude unrelated Git commit changes for controls.

Completion is derived from actual availability. A complete selected L diagnostic
can be `DIAGNOSTIC` while admission eligibility remains false. Missing B object
demands and C warmup gaps never become zero. E01 uses actual lifecycle ownership
(typed Ready, known ordinary command result, detached/joined/drained native
session and original-token Gone) rather than an unselected tree oracle. P's
batch/default kernel opcode completeness and native command/backing resource
scopes remain explicit gaps; idle daemon gauges are not native process metrics.
Current gauges and lifetime high-water observations never become phase peaks or
a resource-admission PASS.

The once-only claim uses actual verified source-set and execution binary hashes,
image/user, canonical workload/environment, input sets, master/manifest bytes,
selected closed oracle digest, and the enforced P profile. Display identity labels
and fresh container/Workspace names cannot manufacture another sample. Independent
clone/setup receipts and compilation provenance remain required; a hash alone does
not prove that a binary was compiled from its declared sources. K cumulative daemon
snapshots include checkpoint verifier work. Use `command:i -> commit:i` for an
isolated Commit interval; cross-checkpoint deltas cannot establish Commit-only
producer ratios. Raw cumulative snapshots and every verifier event remain retained.

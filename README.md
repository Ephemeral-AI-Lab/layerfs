# LayerFS

A restart-durable, content-addressed filesystem for ephemeral compute, coding
agents, containers, and branchable workspaces.

LayerFS exposes an ordinary POSIX workspace through Linux FUSE while keeping
the authoritative namespace, file metadata, content extents, history, and
accepted roots in a SQLite-backed object store.

Applications see normal files:

~~~text
/workspace/
├── package.json
├── src/
│   ├── main.ts
│   └── server.ts
└── tests/
    └── server.test.ts
~~~

LayerFS does not require a second materialized backing directory for that
workspace. The visible tree is resolved directly from the LayerFS Store.

> **Current status: local Linux/Docker qualification passed**
>
> LayerFS has passed its local correctness, restart-durability, resource-safety,
> cleanup, and live-mount performance gates on native Linux ARM64 under Docker.
> The evidence classification is <code>PASS_LOCAL_ONLY</code>.
>
> This does not claim deployed-cloud durability, Kubernetes support, or
> cross-platform production qualification.

## Why LayerFS

Ephemeral environments frequently create, modify, branch, and discard large
workspaces. A conventional workspace pipeline often looks like this:

~~~text
materialize complete directory tree
→ run tools
→ scan complete directory tree
→ capture changes
→ publish another snapshot
~~~

That becomes expensive when workspaces are large, histories are retained, or
many agents branch from related states.

LayerFS instead makes filesystem state the durable model:

~~~text
accepted root
→ immutable namespace and inode graph
→ persistent file extent structures
→ content-addressed objects
→ guarded SQLite publication
~~~

Linux FUSE turns that model into an ordinary mounted workspace. Bash, Git,
editors, compilers, package managers, tests, and servers continue to use normal
paths and POSIX operations.

LayerFS provides:

- a real Linux FUSE filesystem mounted at <code>/workspace</code>;
- immutable accepted roots with guarded mutable references;
- restart-durable publication through explicit checkpoints;
- content-addressed payload reuse;
- structural sharing between workspace versions;
- bounded request memory and dirty-operation accounting;
- a disk-backed spool for dirty bytes beyond the memory envelope;
- exact Verified reopen after process or container failure;
- fork and rollback through accepted roots; and
- no mandatory materialize-edit-capture cycle for direct mounted work.

LayerFS does not claim that every application edit is automatically sublinear.
A POSIX overwrite is an overwrite, and an application that saves a complete
replacement still presents a complete replacement. The explicit splice path
can preserve unchanged extent ranges when insertion or deletion intent is
known.

## Quick start

### Requirements

- Docker with Linux containers
- <code>/dev/fuse</code>
- permission to add <code>CAP_SYS_ADMIN</code>
- Git
- a native ARM64 or AMD64 Docker environment

The deepest sealed qualification currently covers native Linux ARM64 under
Docker Desktop on Apple Silicon. The image build supports ARM64 and AMD64;
AMD64 still needs an equally deep sealed end-to-end campaign.

### Build

~~~bash
git clone https://github.com/Ephemeral-AI-Lab/layerfs.git
cd layerfs

SOURCE_COMMIT="$(git rev-parse HEAD)"
SOURCE_TREE="$(git rev-parse 'HEAD^{tree}')"

docker build \
  --build-arg LAYERFS_SOURCE_COMMIT="$SOURCE_COMMIT" \
  --build-arg LAYERFS_SOURCE_TREE="$SOURCE_TREE" \
  -f containers/layerfs-fuse/Dockerfile \
  -t layerfs:local \
  .
~~~

The Docker build runs the Linux FUSE crate tests and Clippy checks before
creating the runtime image.

### Run

Create the persistent Store:

~~~bash
docker volume create layerfs-store
~~~

Start LayerFS:

~~~bash
docker run -d \
  --name layerfs \
  --init \
  --device /dev/fuse:rwm \
  --cap-add SYS_ADMIN \
  --mount type=volume,src=layerfs-store,dst=/var/lib/layerfs \
  -p 3000:3000 \
  layerfs:local \
  --store /var/lib/layerfs/store.sqlite \
  --mount /workspace \
  --spool /var/tmp/layerfs-owned/spool \
  --receipt /var/tmp/layerfs-owned/terminal.json \
  --ref main \
  --integrity verified \
  --uid 0 \
  --gid 0
~~~

Confirm that <code>/workspace</code> is a real FUSE mount:

~~~bash
docker exec layerfs mountpoint /workspace
docker exec layerfs findmnt -T /workspace
~~~

Use it as a normal workspace:

~~~bash
docker exec -it layerfs bash
~~~

Inside the container:

~~~bash
cd /workspace
mkdir -p src tests
printf 'fn main() { println!("hello from LayerFS"); }\n' > src/main.rs
cat src/main.rs
find .
git init
~~~

Any tool installed in the container can work against <code>/workspace</code>.
LayerFS provides the filesystem; it does not replace the language runtime or
toolchain.

To host a server from the mounted workspace:

~~~bash
docker exec -d layerfs \
  python3 -m http.server 3000 --directory /workspace
~~~

Open <http://localhost:3000>.

### Stop cleanly

~~~bash
docker stop layerfs
docker cp layerfs:/var/tmp/layerfs-owned/terminal.json ./layerfs-terminal.json
docker rm layerfs
~~~

The <code>layerfs-store</code> volume remains authoritative and can be reused
by a new container. Removing it deletes the local workspace authority:

~~~bash
docker volume rm layerfs-store
~~~

## Architecture

~~~mermaid
flowchart TB
    Programs["Bash · Git · editors · builds · tests · servers"]
    Kernel["Linux VFS"]
    Fuse["layerfs-fuse<br/>callbacks, errno, invalidation, lifecycle"]
    Vfs["layerfs-vfs<br/>MountedWorkspace and POSIX semantics"]
    Engine["layerfs-engine<br/>transactions, integrity, refs, publication"]
    Core["layerfs-core<br/>objects, identities, namespaces, extents, COW"]
    Store[("SQLite LayerFS Store<br/>authoritative accepted state")]
    Spool[("Bounded disk spool<br/>non-authoritative dirty bytes")]

    Programs --> Kernel
    Kernel --> Fuse
    Fuse --> Vfs
    Vfs --> Engine
    Vfs --> Spool
    Engine --> Core
    Engine --> Store
~~~

The dependency direction is one-way:

~~~text
layerfs-core
    │
    ▼
layerfs-engine
    │
    ▼
layerfs-vfs
    │
    ▼
layerfs-fuse
~~~

The Linux adapter translates kernel requests into the portable mounted VFS. It
does not own a second filesystem model. Platform names, runtime handles, mount
paths, container IDs, and VM IDs never enter canonical LayerFS identities.

### Runtime layout

~~~text
container /
├── workspace/                        direct LayerFS FUSE mount
├── var/lib/layerfs/
│   └── store.sqlite                  authoritative Store
└── var/tmp/layerfs-owned/
    └── spool                         bounded dirty spool
~~~

Docker's OverlayFS may implement the container operating-system root. It does
not represent <code>/workspace</code>.

A visible path such as <code>/workspace/src/main.ts</code> does not correspond
to a second <code>/backing-workspace/src/main.ts</code>. Lookup resolves
through the accepted namespace, inode, extent, and content-object graph.

## Durability

A successful write updates mounted dirty state. It does not necessarily mean
the accepted root has advanced.

~~~text
POSIX operation
→ mounted dirty state
→ bounded request/Q accounting
→ owned memory or disk spool
→ checkpoint
→ SQLite transaction
→ object and namespace publication
→ accepted generation/root
→ durable acknowledgement
~~~

LayerFS checkpoints admitted dirty state at explicit synchronization boundaries
and during admitted graceful shutdown or successful external unmount.

The integrity mode must be explicit:

~~~text
--integrity verified
--integrity trusted
~~~

<code>verified</code> authenticates accepted state and is recommended for
persistent workspaces. <code>trusted</code> is an explicit local-development
class and is never silently selected.

## Performance

LayerFS reports live-mount and persistence-inclusive performance separately.

The sealed results below bind implementation commit
<code>7e82abcd7320f6a214be336d82488ba0527b6025</code> and its retained image.
Later canonical-release changes are documentation, CI, licensing, and image
source metadata only; they do not replace the sealed source/image identities in
the evidence.

### Matched local live-mount comparison

The accepted cross-product live comparison uses the same unchanged upstream
<code>fs-bench.sh</code> for LayerFS and a locally reproduced Cloudflare
Computer FUSE configuration.

Both populations used native Linux ARM64, real FUSE, one CPU, a 512 MiB hard
memory limit, no network, one warmup, three measured repetitions, the same 12
scenarios, and the same benchmark hash.

| Control | LayerFS FUSE median sum | Cloudflare FUSE median sum | Cloudflare ÷ LayerFS |
|---|---:|---:|---:|
| <code>/var/tmp</code> | 3.361 s | 7.260 s | **2.160×** |
| <code>/tmp</code> | 3.299 s | 7.449 s | **2.258×** |

LayerFS won all 12 retained live operations in both populations.

| Control | Median sum | Ratio of sums | Geometric mean | Spread |
|---|---:|---:|---:|---:|
| <code>/var/tmp</code> | 3.361 s | 2.193 | 3.372 | 1.058 |
| <code>/tmp</code> | 3.299 s | 2.133 | 3.569 | 1.021 |
| Acceptance | ≤4.500 s | ≤2.850 / 3.100 | ≤7.000 / 7.750 | ≤1.150 |

These values describe live mounted operations. They do not include checkpoint
latency.

### Persistence-inclusive qualification

The sealed durable campaign used 12 scenarios, one warmup per scenario, three
measured samples per scenario, 48 fresh Stores, Verified integrity, two
crash/reopen chains per sample, and exact accepted-root and recursive-inventory
verification.

| Measurement | Sum of per-scenario medians |
|---|---:|
| Command time | 3.898 s |
| Checkpoint time | 4.337 s |
| Command-to-durable time | **8.229 s** |

All 48 samples passed restart and exact Verified reopen. A focused high-entropy
64 MiB write reached durable acknowledgement in 926.499 ms and reopened with
exact bytes after immediate <code>SIGKILL</code>.

A later local persistence-aware Cloudflare population remains diagnostic:
Cloudflare exceeded the preregistered 5% CFS throttle gate and the two products
did not run under identical enforced memory limits. LayerFS therefore does not
publish a durable cross-product speed ratio from that population.

### Resource qualification

Across the 36 measured durable samples:

| Resource | Observed | Gate |
|---|---:|---:|
| Aggregate CPU throttling | 0.478% | ≤5% |
| Mount-lock wait / callback wall | 2.609% | ≤10% |
| Maximum daemon RSS upper-bound increase | 9.03 MiB | ≤64 MiB |
| Maximum whole-cgroup peak | 271.8 MiB | ≤512 MiB |
| Daemon threads | 7 | ≤8 |
| FD growth | 6 | ≤64 |
| OOM / OOM kill | 0 / 0 | 0 / 0 |
| Terminal operation Q | 0 | 0 |
| Terminal Store connections | 0 | 0 |
| Terminal owned runtime residue | 0 | 0 |

The largest admitted FUSE request is bounded at 1 MiB. Large dirty payloads use
the disk spool rather than workspace-sized resident memory.

### Evidence

- [Stage 2 specification](poc/19-stage2-docker-linux-fuse.md)
- [Stage 2P performance specification](poc/23-stage2-fuse-performance-optimization.md)
- [Candidate 015 evidence index](poc/evidence/stage2-freeze-candidate-015/README.md)
- [Candidate 015 summary](poc/evidence/stage2-freeze-candidate-015/summary.json)
- [Live verification](poc/evidence/stage2-freeze-candidate-015/live-current/verification.json)
- [Durable verification](poc/evidence/stage2-freeze-candidate-015/durable/verification05.json)
- [Persistence-aware comparison 016](poc/evidence/stage2-local-durable-comparison-016/README.md)

## Platform and integration status

| Platform or integration | Status |
|---|---|
| Linux FUSE, ARM64 | **Validated** |
| Docker Desktop on Apple Silicon | **Validated Linux envelope** |
| Linux FUSE, AMD64 | Build-supported; equivalent sealed qualification pending |
| macOS native APFS projection | PoC complete; not a host mount |
| OCI import/export | Planned next |
| OverlayFS compatibility | Planned next |
| Firecracker guest workspace profile | High-priority roadmap |
| containerd snapshotter | Planned |
| Remote immutable object/ref transport | Planned |
| Direct macOS FSKit | Later evaluation |
| Windows WinFsp | Later evaluation |
| Kubernetes | Deferred; not a current priority |

## Roadmap

~~~text
canonical release
→ OCI import/export
→ OverlayFS compatibility
→ Firecracker guest workspace profile
→ container runtime integration
→ remote object/ref transport
→ retention and garbage collection
~~~

Direct FUSE remains the authoritative, high-performance workspace path. OCI is
the interchange format. OverlayFS is a compatibility mode. Firecracker is an
isolated execution target whose adapter must not enter LayerFS identities.

Kubernetes is deliberately deferred. If added later, it will reuse completed
OCI, runtime, Firecracker, and remote-publication contracts rather than shaping
the LayerFS core prematurely.

See [ROADMAP.md](ROADMAP.md) for milestones and acceptance gates.

## Repository layout

~~~text
layerfs/
├── crates/
│   ├── layerfs-core/       canonical objects, namespaces, extents and COW
│   ├── layerfs-engine/     Store, transactions, integrity and publication
│   ├── layerfs-vfs/        mounted filesystem semantics
│   ├── layerfs-fuse/       Linux FUSE adapter and daemon
│   ├── layerfs-os/         native OS projection adapters
│   └── layerfs-sdk/        public programmatic API
├── containers/
│   └── layerfs-fuse/       Linux/Docker image
├── tools/
│   └── layerfs-eval/       evaluation CLI
├── docs/                   architecture documentation
├── poc/                    specifications and sealed evidence
├── implementation-detail/ retained implementation records
├── research/               candidate algorithms
└── eval/                   retained evaluation summaries
~~~

## Current limitations

The validated scope does not yet include:

- OCI import or export;
- OverlayFS commit/capture integration;
- Firecracker guest or snapshot integration;
- containerd snapshotter support;
- deployed remote authority;
- online background garbage collection;
- Kubernetes orchestration;
- hardened hostile multi-user mounts;
- direct macOS host mounting;
- Windows mounting;
- device nodes, sockets, FIFOs, or paging files;
- an indefinitely stable on-disk format;
- hardware power-loss qualification; or
- published crates.io or binary releases.

The current Linux mount is intended for controlled local development,
ephemeral compute, and coding-agent environments.

## Development

The workspace uses Rust 1.85.

~~~bash
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
~~~

Real FUSE end-to-end tests require Linux, <code>/dev/fuse</code>, and the
minimum mount capability.

## Documentation

- [Architecture overview](docs/architecture/README.md)
- [LayerFS specification](SPEC.md)
- [Architecture document](architecture.md)
- [Linux/FUSE specification](poc/19-stage2-docker-linux-fuse.md)
- [Performance specification](poc/23-stage2-fuse-performance-optimization.md)
- [PoC and evidence index](poc/README.md)
- [Roadmap](ROADMAP.md)

Research documents describe candidate designs and are not automatically product
authority.

## License

LayerFS is licensed under the [MIT License](LICENSE).

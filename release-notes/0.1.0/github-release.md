# LayerFS 0.1.0 Developer Preview

LayerFS is a SQLite-backed, content-addressed filesystem engine for ephemeral
agent Workspaces and durable shared history. CAS, content-defined chunking, and
copy-on-write reuse unchanged bytes and filesystem structure, so parallel
Branches and Workspaces do not multiply the shared base.

## Highlights

- One SQLite Store for LayerStacks, immutable Layers, named Branches,
  immutable Commits, and canonical objects.
- Zero-copy Branch Fork from an immutable Layer or eligible Commit.
- Ephemeral materialized or real-FUSE Workspaces with explicit Commit and End.
- Fresh-process command execution with bounded, paged output.
- Public Rust SDK and CLI for storage, Workspace, history, monitoring, and
  managed-container lifecycle operations.
- Store-wide deduplication with authenticated canonical reads, CDC for
  localized file edits, and structural COW for changed paths.

## Measured results

In the final seven-pair public-SDK, real-FUSE campaign against Cloudflare
Computer, LayerFS measured:

| Lifecycle | LayerFS | Computer | LayerFS speedup |
| --- | ---: | ---: | ---: |
| Cold create 32 MiB | 161.231 ms | 1,660.321 ms | 10.07× |
| EDIT16 | 169.133 ms | 2,631.062 ms | 15.80× |
| Prepend 10 bytes | 232.394 ms | 2,484.210 ms | 10.48× |
| Read 32 MiB | 119.154 ms | 780.946 ms | 6.53× |
| Registered total | 690.196 ms | 7,579.414 ms | 10.76× |

Incremental semantic storage was 97.19% lower for EDIT16 and 99.92% lower for
a ten-byte prepend to a 32 MiB file. See the
[full benchmark report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.0/release-notes/0.1.0/benchmark-results.md)
for methodology, environment, source seals, acknowledgement policy, and raw
evidence references.

## Start here

LayerFS 0.1.0 is distributed as source. Build the CLI with Rust 1.85 or newer:

```bash
git clone --branch v0.1.0 --depth 1 https://github.com/Ephemeral-AI-Lab/layerfs.git
cd layerfs
cargo build --release -p layerfs-cli
./target/release/layerfs --version
```

Continue with the
[0.1.0 quickstart](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.0/docs/versioned/0.1.0/quickstart.md)
and review the
[current limitations](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.0/docs/versioned/0.1.0/limitations.md).

## Developer Preview boundary

This is a pre-release for evaluation, integration, benchmark reproduction, and
design research. It is not production storage, does not claim crash- or
power-loss durability at every acknowledgement point, and is not a hardened
hostile-code sandbox. Keep an independent copy of important data.

Prebuilt executables, crates.io packages, and runtime images are not published
for 0.1.0. The release contains deterministic source archives, `Cargo.lock`,
the MIT license, and `SHA256SUMS`.

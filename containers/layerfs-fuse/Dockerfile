FROM rust:1.85.1-bookworm AS build

ARG TARGETARCH
ARG LAYERFS_SOURCE_COMMIT
ARG LAYERFS_SOURCE_TREE
ENV LAYERFS_SOURCE_COMMIT="$LAYERFS_SOURCE_COMMIT" \
    LAYERFS_SOURCE_TREE="$LAYERFS_SOURCE_TREE"
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY tools ./tools
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    test "${#LAYERFS_SOURCE_COMMIT}" -eq 40 \
 && test "${#LAYERFS_SOURCE_TREE}" -eq 40 \
 && case "$LAYERFS_SOURCE_COMMIT$LAYERFS_SOURCE_TREE" in *[!0-9a-f]*) exit 1 ;; esac \
 && case "$TARGETARCH" in \
      arm64) rustup target add aarch64-unknown-linux-gnu; target=aarch64-unknown-linux-gnu ;; \
      amd64) rustup target add x86_64-unknown-linux-gnu; target=x86_64-unknown-linux-gnu ;; \
      *) exit 1 ;; \
    esac \
 && rustup component add clippy \
 && cargo test --locked --target "$target" -p layerfs-fuse --no-default-features --features proxy \
 && cargo clippy --locked --target "$target" -p layerfs-fuse --no-default-features --features proxy --all-targets -- -D warnings \
 && cargo build --locked --release --target "$target" -p layerfs-fuse --no-default-features --features proxy \
 && cp "target/$target/release/layerfs-fuse" /layerfs-fuse

FROM rust:1.85.1-bookworm

ARG LAYERFS_SOURCE_COMMIT
ARG LAYERFS_SOURCE_TREE
LABEL org.opencontainers.image.source="https://github.com/Ephemeral-AI-Lab/layerfs" \
      org.opencontainers.image.revision="$LAYERFS_SOURCE_COMMIT" \
      org.opencontainers.image.layerfs.source-commit="$LAYERFS_SOURCE_COMMIT" \
      org.opencontainers.image.layerfs.source-tree="$LAYERFS_SOURCE_TREE" \
      org.opencontainers.image.layerfs.fs-bench-sha256="0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"
ENV LAYERFS_SOURCE_COMMIT="$LAYERFS_SOURCE_COMMIT" \
    LAYERFS_SOURCE_TREE="$LAYERFS_SOURCE_TREE" \
    LAYERFS_FS_BENCH_SHA256="0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef"

COPY --from=build /layerfs-fuse /usr/local/bin/layerfs-fuse
COPY benchmark/fs-bench/fs-bench.sh /usr/local/bin/fs-bench.sh
COPY benchmark/fs-bench/verify_fs_bench.py /usr/local/bin/verify_fs_bench.py
RUN echo "$LAYERFS_FS_BENCH_SHA256  /usr/local/bin/fs-bench.sh" | sha256sum -c - \
 && python3 /usr/local/bin/verify_fs_bench.py --self-test \
 && chmod 0555 /usr/local/bin/fs-bench.sh /usr/local/bin/verify_fs_bench.py \
 && mkdir -p /workspace /var/lib/layerfs /var/tmp/layerfs-owned

ENTRYPOINT ["/usr/local/bin/layerfs-fuse"]

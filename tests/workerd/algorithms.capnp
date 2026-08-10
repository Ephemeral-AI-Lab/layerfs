using Workerd = import "/workerd/workerd.capnp";

const config :Workerd.Config = (
  services = [(name = "main", worker = .worker)],
  sockets = [(name = "http", address = "127.0.0.1:18799", http = (), service = "main")]
);

const worker :Workerd.Worker = (
  compatibilityDate = "2026-08-10",
  modules = [
    (name = "worker.mjs", esModule = embed "algorithms-worker.mjs"),
    (name = "cas/bytes.js", esModule = embed "../../packages/fs/dist/cas/bytes.js"),
    (name = "cas/sha256.js", esModule = embed "../../packages/fs/dist/cas/sha256.js"),
    (name = "cdc/fastcdc.js", esModule = embed "../../packages/fs/dist/cdc/fastcdc.js"),
    (name = "cow/pages.js", esModule = embed "../../packages/fs/dist/cow/pages.js"),
    (name = "manifests/builder.js", esModule = embed "../../packages/fs/dist/manifests/builder.js"),
    (name = "manifests/binary.js", esModule = embed "../../packages/fs/dist/manifests/binary.js"),
    (name = "manifests/codec.js", esModule = embed "../../packages/fs/dist/manifests/codec.js"),
    (name = "manifests/cursor.js", esModule = embed "../../packages/fs/dist/manifests/cursor.js"),
    (name = "manifests/grouping.js", esModule = embed "../../packages/fs/dist/manifests/grouping.js"),
    (name = "operations/full-rebuild.js", esModule = embed "../../packages/fs/dist/operations/full-rebuild.js"),
    (name = "operations/local-rebuild.js", esModule = embed "../../packages/fs/dist/operations/local-rebuild.js"),
    (name = "operations/streamed-rebuild.js", esModule = embed "../../packages/fs/dist/operations/streamed-rebuild.js"),
    (name = "patches/patches.js", esModule = embed "../../packages/fs/dist/patches/patches.js"),
    (name = "resources/limits.js", esModule = embed "../../packages/fs/dist/resources/limits.js"),
    (name = "resources/byte-capacity.js", esModule = embed "../../packages/fs/dist/resources/byte-capacity.js"),
    (name = "resources/safe-integers.js", esModule = embed "../../packages/fs/dist/resources/safe-integers.js")
  ]
);

using Workerd = import "/workerd/workerd.capnp";

const config :Workerd.Config = (
  services = [(name = "main", worker = .worker)],
  sockets = [(name = "http", address = "127.0.0.1:18799", http = (), service = "main")]
);

const worker :Workerd.Worker = (
  compatibilityDate = "2026-08-10",
  modules = [
    (name = "worker.mjs", esModule = embed "algorithms-worker.mjs"),
    (name = "cas/sha256.js", esModule = embed "../../packages/fs/dist/cas/sha256.js"),
    (name = "cdc/fastcdc.js", esModule = embed "../../packages/fs/dist/cdc/fastcdc.js"),
    (name = "manifests/builder.js", esModule = embed "../../packages/fs/dist/manifests/builder.js"),
    (name = "manifests/codec.js", esModule = embed "../../packages/fs/dist/manifests/codec.js"),
    (name = "operations/local-rebuild.js", esModule = embed "../../packages/fs/dist/operations/local-rebuild.js"),
    (name = "utils/bytes.js", esModule = embed "../../packages/fs/dist/utils/bytes.js")
  ]
);

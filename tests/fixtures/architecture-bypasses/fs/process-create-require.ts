const moduleApi = process.getBuiltinModule("node:module");
const load = moduleApi.createRequire(import.meta.url);

load("@ephemeralai/fs-sqlite-node");

// Build the addon on its actual host. Cargo discovers the repository's ARM64
// profile from cwd; explicit RUSTFLAGS remain the caller's declared input.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, renameSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const suffix = { darwin: "dylib", linux: "so" }[process.platform];
if (!suffix) throw new Error(`native addon builds require macOS or Linux, got ${process.platform}`);
const flags = process.argv.slice(2);
if (flags.some((flag) => flag !== "--debug")) throw new Error("usage: build-native.mjs [--debug]");
const profile = flags.includes("--debug") ? "debug" : "release";
const configured = process.env.CARGO_TARGET_DIR;
const target = configured ? resolve(root, configured) : join(root, "target");
const outside = relative(root, target);
if (outside.startsWith("..") || isAbsolute(outside)) throw new Error("Cargo target must stay in this worktree");
if (process.env.CARGO_BUILD_TARGET) throw new Error("build on the actual macOS/Linux host without CARGO_BUILD_TARGET");
const result = spawnSync("cargo", [
  "+1.85.1", "build", "--manifest-path", "Cargo.toml", "--offline", "--locked",
  ...(profile === "release" ? ["--release"] : []), "-p", "layerfs-node",
], { cwd: root, stdio: "inherit", env: { ...process.env, CARGO_TARGET_DIR: target } });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const artifact = join(target, profile, `liblayerfs_node.${suffix}`);
const output = process.env.LAYERFS_NATIVE_OUTPUT
  ? resolve(root, process.env.LAYERFS_NATIVE_OUTPUT)
  : fileURLToPath(new URL("../../engine/crates/layerfs-node/layerfs.node", import.meta.url));
mkdirSync(dirname(output), { recursive: true });
const staged = `${output}.${process.pid}.new`;
copyFileSync(artifact, staged);
renameSync(staged, output);
console.log(`Built ${process.platform}/${process.arch}: ${output}`);

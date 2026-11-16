// Build the addon on its actual host. Cargo discovers the repository's ARM64
// profile from cwd; explicit RUSTFLAGS remain the caller's declared input.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, renameSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const suffix = { darwin: "dylib", linux: "so", win32: "dll" }[process.platform];
if (!suffix) throw new Error(`native addon builds require macOS or Linux, got ${process.platform}`);
const usage = "usage: build-native.mjs [--debug] [--features postgresql,tls]";
const flags = process.argv.slice(2);
const featuresAt = flags.indexOf("--features");
const features = featuresAt < 0 ? [] : (flags[featuresAt + 1] ?? "").split(",");
if (featuresAt >= 0) flags.splice(featuresAt, 2);
if (flags.some((flag) => flag !== "--debug") || features.some((name) => !["postgresql", "tls"].includes(name))) throw new Error(usage);
const profile = flags.includes("--debug") ? "debug" : "release";
const configured = process.env.CARGO_TARGET_DIR;
const target = configured ? resolve(root, configured) : join(root, "target");
const outside = relative(root, target);
if (outside.startsWith("..") || isAbsolute(outside)) throw new Error("Cargo target must stay in this worktree");
if (process.env.CARGO_BUILD_TARGET) throw new Error("build on the actual macOS/Linux host without CARGO_BUILD_TARGET");
const existingRustflags = process.env.RUSTFLAGS ? [process.env.RUSTFLAGS] : [];
if (process.arch === "arm64") {
  // Preserve target flags from .cargo/config.toml if overridden by RUSTFLAGS
  for (const flag of ["--cfg aes_armv8", "--cfg polyval_armv8", "--cfg chacha20_force_neon", "-C target-feature=+aes,+sha2"]) {
    if (!existingRustflags.join(" ").includes(flag)) existingRustflags.push(flag);
  }
}
const remapFlags = [`--remap-path-prefix ${root}=/`, `--remap-path-prefix ${process.env.HOME || "/root"}=/root-home`];
for (const flag of remapFlags) {
  if (!existingRustflags.join(" ").includes(flag)) existingRustflags.push(flag);
}
const rustflags = existingRustflags.join(" ");
const result = spawnSync("cargo", [
  "+1.85.1", "build", "--manifest-path", "Cargo.toml", "--offline", "--locked",
  ...(profile === "release" ? ["--release"] : []), "-p", "layerfs-node",
  ...(features.length ? ["--features", features.join(",")] : []),
], { cwd: root, stdio: "inherit", env: { ...process.env, CARGO_TARGET_DIR: target, RUSTFLAGS: rustflags } });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
// Windows MSVC puts no lib prefix on a cdylib; unix does.
const stem = process.platform === "win32" ? "layerfs_node" : "liblayerfs_node";
const artifact = join(target, profile, `${stem}.${suffix}`);
const output = process.env.LAYERFS_NATIVE_OUTPUT
  ? resolve(root, process.env.LAYERFS_NATIVE_OUTPUT)
  : fileURLToPath(new URL("../node/layerfs.node", import.meta.url));
mkdirSync(dirname(output), { recursive: true });
const staged = `${output}.${process.pid}.new`;
copyFileSync(artifact, staged);
renameSync(staged, output);
console.log(`Built ${process.platform}/${process.arch}: ${output}`);
if (process.env.LAYERFS_NATIVE_OUTPUT) console.log(`The package loads it only when LAYERFS_NATIVE=${output}`);

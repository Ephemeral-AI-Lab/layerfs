// Build a reproducible release artifact of the CLI for x86_64-unknown-linux-gnu.
// Produces layerfs-<version>-x86_64-unknown-linux-gnu.tar.gz and SHA256SUMS.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
if (process.platform !== "linux" || process.arch !== "x64") {
  throw new Error(`build-cli-dist.mjs requires linux x64, got ${process.platform}/${process.arch}`);
}

const usage = "usage: build-cli-dist.mjs <output-dir>";
const args = process.argv.slice(2);
if (args.length !== 1 || args[0].startsWith("-")) {
  console.error(usage);
  process.exit(1);
}

const outDir = resolve(process.cwd(), args[0]);
mkdirSync(outDir, { recursive: true });

// Read version from root Cargo.toml workspace package version
const cargoToml = readFileSync(join(root, "Cargo.toml"), "utf8");
const versionMatch = cargoToml.match(/\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"/);
if (!versionMatch) {
  throw new Error("cannot determine version from root Cargo.toml");
}
const version = versionMatch[1];

const configured = process.env.CARGO_TARGET_DIR;
const target = configured ? resolve(root, configured) : join(root, "target");
const outside = relative(root, target);
if (outside.startsWith("..") || isAbsolute(outside)) {
  throw new Error("Cargo target must stay in this worktree");
}

const existingRustflags = process.env.RUSTFLAGS ? [process.env.RUSTFLAGS] : [];
if (process.arch === "arm64") {
  for (const flag of ["--cfg aes_armv8", "--cfg polyval_armv8", "--cfg chacha20_force_neon", "-C target-feature=+aes,+sha2"]) {
    if (!existingRustflags.join(" ").includes(flag)) existingRustflags.push(flag);
  }
}
const remapFlags = [`--remap-path-prefix ${root}=/`, `--remap-path-prefix ${process.env.HOME || "/root"}=/root-home`];
for (const flag of remapFlags) {
  if (!existingRustflags.join(" ").includes(flag)) existingRustflags.push(flag);
}
const rustflags = existingRustflags.join(" ");

console.log(`Building release layerfs binary (v${version})...`);
const buildResult = spawnSync("cargo", [
  "+1.85.1", "build", "--release", "--locked", "--offline", "-p", "layerfs-cli",
], { cwd: root, stdio: "inherit", env: { ...process.env, CARGO_TARGET_DIR: target, RUSTFLAGS: rustflags } });

if (buildResult.error) throw buildResult.error;
if (buildResult.status !== 0) process.exit(buildResult.status ?? 1);

const binary = join(target, "release", "layerfs");

// Stage files in a temporary directory before creating tar.gz
const stageDir = join(target, `cli-dist-stage-${process.pid}`);
rmSync(stageDir, { recursive: true, force: true });
mkdirSync(stageDir, { recursive: true });

copyFileSync(binary, join(stageDir, "layerfs"));
// Make binary executable
spawnSync("chmod", ["755", join(stageDir, "layerfs")]);
copyFileSync(join(root, "LICENSE"), join(stageDir, "LICENSE"));

const readmeContent = `# LayerFS CLI

Version: ${version}
Platform: x86_64-unknown-linux-gnu

The \`layerfs\` command: a versioned filesystem for AI agents.

## Usage

\`\`\`bash
./layerfs help
./layerfs --version
\`\`\`
`;
writeFileSync(join(stageDir, "README.md"), readmeContent);

const tarballName = `layerfs-${version}-x86_64-unknown-linux-gnu.tar.gz`;
const tarballPath = join(outDir, tarballName);

console.log(`Creating ${tarballName}...`);
const tarResult = spawnSync("tar", [
  "-czf", tarballPath,
  "-C", stageDir,
  "layerfs", "LICENSE", "README.md",
]);
if (tarResult.error) throw tarResult.error;
if (tarResult.status !== 0) {
  console.error("tar failed:", tarResult.stderr.toString());
  process.exit(tarResult.status ?? 1);
}
rmSync(stageDir, { recursive: true, force: true });

// Compute sha256 checksum
const tarballBytes = readFileSync(tarballPath);
const hash = createHash("sha256").update(tarballBytes).digest("hex");
const sumsPath = join(outDir, "SHA256SUMS");
writeFileSync(sumsPath, `${hash}  ${tarballName}\n`);

console.log(`Artifacts produced in ${outDir}:`);
console.log(`  ${tarballName} (${tarballBytes.length} bytes, sha256: ${hash})`);
console.log(`  SHA256SUMS`);

#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join, posix, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const api = fileURLToPath(new URL("../", import.meta.url));
const root = resolve(api, "..");
const npm = process.platform === "win32" ? "cmd.exe" : "npm";
const npmArgs = (args) => process.platform === "win32" ? ["/d", "/s", "/c", "npm", ...args] : args;
const packages = ["agent", "backend", "runtime", "cli"];
const mainExports = { agent: "LayerFS", backend: "LayerFSBackend", runtime: "LayerFSRuntime", cli: "main" };
const repository = "https://github.com/Ephemeral-AI-Lab/layerfs.git";

const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { cwd: api, encoding: "utf8", stdio: "inherit", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} exited ${result.status}`);
};

const invoke = (command, args, options = {}) => {
  const result = spawnSync(command, args, { cwd: api, encoding: "utf8", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} exited ${result.status}\n${result.stderr}`);
  return result.stdout;
};

const sourceFiles = (directory, prefix = "src") => readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
  const path = posix.join(prefix, entry.name);
  return entry.isDirectory() ? sourceFiles(join(directory, entry.name), path) : [path];
});

const temporary = mkdtempSync(join(tmpdir(), "layerfs-npm-prepare-"));
try {
  const packDirectory = join(temporary, "pack");
  const installDirectory = join(temporary, "install");
  mkdirSync(packDirectory);
  mkdirSync(installDirectory);
  const versions = new Set();
  const tarballs = [];
  const manifests = [];

  for (const name of packages) {
    const directory = join(api, name);
    const manifest = JSON.parse(readFileSync(join(directory, "package.json"), "utf8"));
    manifests.push(manifest);
    versions.add(manifest.version);
    assert.equal(manifest.name, `@layerfs/${name}`);
    assert.notEqual(manifest.private, true);
    assert.equal(manifest.license, "MIT");
    assert.equal(manifest.repository?.url, repository);
    assert.equal(manifest.repository?.directory, `api/${name}`);
    assert.deepEqual(readFileSync(join(directory, "LICENSE"), "utf8"), readFileSync(join(root, "LICENSE"), "utf8"));
    if (name !== "agent") assert.equal(manifest.dependencies?.["@layerfs/agent"], manifest.version);

    const output = invoke(npm, npmArgs(["pack", "--offline", "--json", "--ignore-scripts", "--pack-destination", packDirectory, `./${name}`]));
    const [result] = JSON.parse(output);
    const packedFiles = new Set(result.files.map(({ path }) => path));
    const entries = [manifest.main, ...Object.values(manifest.exports ?? {}), ...Object.values(manifest.bin ?? {})].filter((entry) => entry !== undefined);
    const required = new Set(["package.json", "README.md", "LICENSE", ...sourceFiles(join(directory, "src"))]);
    for (const entry of entries) {
      assert.equal(typeof entry, "string", `${manifest.name}: entrypoint must be a path string`);
      required.add(entry.replace(/^\.\//, ""));
    }
    for (const path of required) assert.ok(packedFiles.has(path), `${manifest.name}: tarball is missing ${path}`);
    assert.ok(![...packedFiles].some((path) => path.endsWith(".node")), `${manifest.name}: native addons must stay external`);
    tarballs.push(resolve(packDirectory, result.filename));
    console.log(`PACK ${manifest.name}@${manifest.version}: ${packedFiles.size} files; all ${required.size} required files present`);
  }
  assert.equal(versions.size, 1, "all @layerfs/* packages must have one exact set version");

  run(npm, npmArgs(["install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--package-lock=false", ...tarballs]), { cwd: installDirectory });
  for (const manifest of manifests) {
    const exportName = mainExports[manifest.name.slice("@layerfs/".length)];
    const check = `import assert from 'node:assert/strict'; const pkg = await import(${JSON.stringify(manifest.name)}); assert.equal(typeof pkg[${JSON.stringify(exportName)}], 'function');`;
    run(process.execPath, ["--input-type=module", "-e", check], { cwd: installDirectory });
    console.log(`IMPORT ${manifest.name}: main entry imported from offline tarball install`);
  }
  const binDirectory = join(installDirectory, "node_modules", ".bin");
  const cliEnvironment = { ...process.env, PATH: `${binDirectory}${delimiter}${process.env.PATH ?? ""}` };
  const cliCommand = process.platform === "win32" ? "cmd.exe" : "layerfs";
  const cliArgs = (args) => process.platform === "win32" ? ["/d", "/s", "/c", "layerfs", ...args] : args;
  assert.equal(invoke(cliCommand, cliArgs(["--version"]), { cwd: installDirectory, env: cliEnvironment }).trim(), [...versions][0]);
  assert.match(invoke(cliCommand, cliArgs(["help"]), { cwd: installDirectory, env: cliEnvironment }), /usage:\s+layerfs /);
  console.log(`CLI layerfs --version: ${[...versions][0]}; layerfs help passed from tarball install`);
  console.log("NATIVE no .node files packed; Linux uses LAYERFS_NATIVE, macOS/Windows use the external sealed runtime bundle");
} finally {
  rmSync(temporary, { recursive: true, force: true });
}

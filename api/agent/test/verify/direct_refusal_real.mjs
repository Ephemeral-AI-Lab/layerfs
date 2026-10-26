// Refusal (#352 F-020 step 5.2): the direct route opens a Store only on a
// local Linux filesystem whose locks reach every opener (ext4, xfs, btrfs).
// In the runtime container, a Store path on a bind mount from the Docker host
// (Docker Desktop's shared filesystem) and on tmpfs are each refused
// `Unsupported` before any write: the file's bytes are unchanged and no
// overlay directory is left. macOS is covered by
// engine/crates/layerfs-store-service/tests/kernel_guard.rs.
//
//   node api/agent/test/verify/direct_refusal_real.mjs outer <daemon> <addon> <image digest>
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { docker, inRuntime, REPO } from "./container.mjs";

async function inner([bound]) {
  const { LayerFS } = await import("@layerfs/agent");
  const work = mkdtempSync(join(tmpdir(), "layerfs-refusal-"));
  const source = join(work, "site");
  mkdirSync(source);
  writeFileSync(join(source, "README.md"), "base\n");
  process.env.LAYERFS_HOME = join(work, "home");
  // A project record, so the refusal is the filesystem's and not a missing project.
  await (await LayerFS.init("site", source)).close();
  const shm = mkdtempSync("/dev/shm/layerfs-refusal-");
  const lines = [];
  for (const [label, dir] of [["bind mount", bound], ["tmpfs", shm]]) {
    const store = join(dir, "store.sqlite");
    writeFileSync(store, "not a Store, never opened\n");
    const before = readFileSync(store);
    const overlays = () => readdirSync(tmpdir()).filter((n) => n.startsWith(`layerfs-store-${process.pid}-`));
    // the refusal is the Store open's, at LayerFS.open
    let refused = null;
    try {
      process.env.LAYERFS_STORE_PATH = store;
      await (await LayerFS.open("site")).close();
    } catch (e) {
      refused = e;
    }
    assert.equal(refused?.code, "Unsupported", `${label}: ${refused?.code} ${refused?.message}`);
    assert.ok(readFileSync(store).equals(before), `${label}: the Store file's bytes changed`);
    assert.deepEqual(overlays(), [], `${label}: an overlay directory was left`);
    lines.push(`${label}: Unsupported (${refused.message.slice(0, 140)})`);
  }
  rmSync(shm, { recursive: true, force: true });
  rmSync(work, { recursive: true, force: true });
  console.log(`REFUSAL\n${lines.join("\n")}`);
}

function outer([daemon, addon, image]) {
  assert.ok(daemon && addon && image, "usage: outer <daemon> <addon> <image digest>");
  const stamp = `f020-refusal-${Date.now().toString(36)}`;
  const volume = `layerfs-issue352-${stamp}`;
  const bound = join(REPO, "target/f020", stamp);
  mkdirSync(bound, { recursive: true });
  execFileSync("docker", ["volume", "create", volume], { stdio: "ignore" });
  let run;
  try {
    run = inRuntime({ script: fileURLToPath(import.meta.url), args: [bound], daemon, addon, image, volume, prefix: `${stamp}-`, timeout: 60_000 });
  } finally {
    docker(["volume", "rm", volume]);
    rmSync(bound, { recursive: true, force: true });
  }
  console.log(run.out.split("\n").slice(-8).join("\n"));
  assert.equal(run.status, 0, `inner run failed (signal ${run.signal})`);
  console.log("direct refusal: ok");
}

const [mode, ...rest] = process.argv.slice(2);
if (mode === "inner") await inner(rest);
else outer(rest);

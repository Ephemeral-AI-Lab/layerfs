// Two processes sharing one stateDir keep each other's bindings:
//   node dsh/packages/host/tests/bindings-shared.mjs
import assert from "node:assert/strict";
import { spawnSync, spawn } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const BINDING = new URL("../src/binding.js", import.meta.url).href;
const { BindingStore } = await import(BINDING);
const dir = mkdtempSync(join(tmpdir(), "dsh-layerfs-bindings-"));

// in one process: two stores opened before either changes anything
const web = new BindingStore(dir), headless = new BindingStore(dir);
web.addProject("/w/app", { name: "app", source: "/w/app", volume: "v-app", created: 1 });
headless.setAgent("s-1", "s-1", "main");
assert.equal(headless.byName("app")?.volume, "v-app", "a read sees the other store's project");
headless.lock("s-1", "app", "/w/app");
web.remove("app");
const disk = JSON.parse(readFileSync(join(dir, "bindings.json"), "utf8"));
assert.deepEqual(Object.keys(disk.projects), [], "remove kept");
assert.equal(disk.agents["s-1"].name, "main", "the other store's agent kept");
assert.deepEqual(disk.sessions["s-1"], { project: null, locked: true, folder: "/w/app" }, "the other store's lock kept, its project cleared by remove");

// across processes: 4 writers × 40 agents each, all at once
const child = `const { BindingStore } = await import(${JSON.stringify(BINDING)}); const b = new BindingStore(process.argv[1]); for (let i = 0; i < 40; i++) b.setAgent(process.argv[2] + "-" + i, "r", "n" + i);`;
await Promise.all([0, 1, 2, 3].map((w) => new Promise((resolve, reject) => {
  const p = spawn(process.execPath, ["--input-type=module", "-e", child, dir, `w${w}`], { stdio: "inherit" });
  p.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`writer ${w} exited ${code}`))));
})));
const agents = JSON.parse(readFileSync(join(dir, "bindings.json"), "utf8")).agents;
assert.equal(Object.keys(agents).filter((k) => k.startsWith("w")).length, 160, "every concurrent writer's agents survive");
console.log("bindings-shared: ok");

// The layerfs command against the real SDK, Store, Docker and daemon (#428):
// init, fork, fs write and read, sandbox create, workspace mount, exec (JSON
// and --stream), commit, the committed file read back, unmount and sandbox
// close, each one `layerfs` command run once. Disposable Store only. Under
// the E2E launcher (its Store volume, deployment and sealed Linux bundle for
// this commit):
//   python3 tools/worktree.py run --resources <resources.json> --bundle <sealed bundle> --timeout 110 -- node cli/test/verify/cli_real.mjs inner
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

if (process.argv[2] !== "inner") throw new Error("usage: cli_real.mjs inner (under tools/worktree.py run)");
const CLI = fileURLToPath(new URL("../../src/layerfs.js", import.meta.url));
const work = mkdtempSync(join(tmpdir(), "layerfs-cli-real-"));
const store = join(dirname(dirname(process.env.LAYERFS_STORE_PATH)), `cli-${process.pid}`);
const project = `cli-${Date.now().toString(36)}`;
const env = { ...process.env, LAYERFS_HOME: join(work, "home"), LAYERFS_STORE_PATH: join(store, "global", "store.sqlite"), LAYERFS_PROJECT: project };

/** One layerfs command; its exit code must be `code`. */
function layerfs(args, { input = "", code = 0 } = {}) {
  const started = Date.now();
  const r = spawnSync(process.execPath, [CLI, ...args], { input, env, timeout: 60_000 });
  const out = r.stdout.toString();
  console.log(`layerfs ${args.join(" ")}  exit ${r.status}  ${Date.now() - started} ms`);
  assert.equal(r.status, code, `layerfs ${args.join(" ")}: ${r.stderr}`);
  return { out, err: r.stderr.toString() };
}
const json = (args, options) => JSON.parse(layerfs(args, options).out);

const source = join(work, "source");
mkdirSync(source);
writeFileSync(join(source, "README.md"), "# Shop\n");
mkdirSync(store);
let sandbox = null;
let ws = null;
try {
  assert.equal(json(["init", project, source]).kind, "Initialized");
  json(["branch", "fork", "cart"]);
  assert.equal(json(["fs", "write", "cart", "/notes.txt", "-m", "notes"], { input: "one\n" }).kind, "Committed");
  assert.equal(json(["fs", "read", "cart", "/notes.txt"]).text, "one\n");

  sandbox = json(["sandbox", "create"]).sandbox;
  ws = json(["workspace", "mount", sandbox, "cart"]).ws;
  const ran = json(["workspace", "exec", ws, "cat notes.txt && echo two > out.txt", "-m", "step 1"]);
  assert.equal([ran.exitCode, ran.stdout].join("|"), "0|one\n");
  const streamed = layerfs(["workspace", "exec", ws, "cat out.txt; exit 3", "--stream"], { code: 3 });
  assert.equal(streamed.out, "two\n");
  assert.equal(json(["workspace", "commit", ws, "-m", "step 1"]).kind, "Committed");
  assert.equal(json(["fs", "read", "cart", "/out.txt"]).text, "two\n");
  json(["workspace", "unmount", ws]);
  ws = null;
  json(["sandbox", "close", sandbox]);
  sandbox = null;
  console.log("cli_real: PASS");
} finally {
  // one attempt each, reported, never retried
  if (ws) console.log("left mounted:", spawnSync(process.execPath, [CLI, "workspace", "unmount", ws, "--discard"], { env }).status);
  if (sandbox) console.log("left running:", spawnSync(process.execPath, [CLI, "sandbox", "close", sandbox], { env }).status);
  rmSync(work, { recursive: true, force: true });
  rmSync(store, { recursive: true, force: true });
}

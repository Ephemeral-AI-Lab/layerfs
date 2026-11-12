// The `layerfs` command and the `layerfs` package give the same JSON for
// the same call (#533 step 15): every call of the table, refused in both
// front ends by the same library check, and a live Store read both ways.
// The command is the repository's debug build (target/debug/layerfs, or
// under CARGO_TARGET_DIR); build it first with cargo build -p layerfs-cli.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, chmodSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { client, table, LayerFSError } from "../src/index.js";
import { decode } from "../src/values.js";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const target = process.env.CARGO_TARGET_DIR ? resolve(root, process.env.CARGO_TARGET_DIR) : join(root, "target");
const command = join(target, "debug", "layerfs");

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-shell-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};

/** One shell call: `{ status, value }` or `{ status, refused }`. */
function shell(home, words, input) {
  const run = spawnSync(command, words, {
    env: { ...process.env, LAYERFS_HOME: home },
    input: input ?? "",
    timeout: 60_000,
  });
  assert.equal(run.error, undefined, `${words.join(" ")}: ${run.error}`);
  if (run.status === 0) return { status: 0, value: decode(run.stdout.toString()) };
  const lines = run.stderr.toString().trim().split("\n");
  return { status: run.status, refused: decode(lines[lines.length - 1]) };
}

/** The same call in Node: `{ value }` or `{ refused }`. */
async function node(lfs, family, call, positionals) {
  try {
    return { value: await lfs[family][call](...positionals) };
  } catch (error) {
    assert.ok(error instanceof LayerFSError, `${family}.${call}: ${error}`);
    return { refused: { code: error.code, message: error.message, detail: error.detail } };
  }
}

/** The two front ends' outcomes are the same: one refusal, or one value. */
function same(name, fromShell, fromNode) {
  if (fromNode.refused) {
    assert.equal(fromShell.status, 125, `${name}: exit ${fromShell.status}`);
    assert.deepEqual(fromShell.refused, fromNode.refused, name);
  } else {
    assert.equal(fromShell.status, 0, `${name}: ${JSON.stringify(fromShell.refused)}`);
    assert.deepEqual(fromShell.value, fromNode.value, name);
  }
}

/** A value of `kind` that every call refuses before any effect. */
const refusing = { project: "nosuch/x", name: "Bad!", text: "/x", json: [], content: "x", int: 1, size: "1G" };

test("every call is refused the same way by the command and the package", { skip: !existsSync(command) }, async () => {
  const home = temp("refused");
  const lfs = client({ home });
  for (const spec of table) {
    // the first reset clears what the second would count
    if (`${spec.family}.${spec.call}` === "logs.reset") continue;
    const args = spec.args.filter((a) => !a.optional);
    const positionals = args.map((a) => refusing[a.kind]);
    const words = [spec.family, spec.call];
    let input;
    for (const [i, arg] of args.entries()) {
      if (arg.kind === "content") input = positionals[i];
      else if (arg.kind === "json") input = JSON.stringify(positionals[i]);
      else words.push(String(positionals[i]));
    }
    const fromNode = await node(lfs, spec.family, spec.call, positionals);
    same(`${spec.family}.${spec.call}`, shell(home, words, input), fromNode);
  }
  // a wrong command line is exit 2 and calls nothing
  const wrong = spawnSync(command, ["file", "write", "a"], { env: { ...process.env, LAYERFS_HOME: home } });
  assert.equal(wrong.status, 2);
  assert.equal(spawnSync(command, ["store", "get", "s", "--bogus"]).status, 2);
  rmSync(home, { recursive: true, force: true });
});

test("a live Store gives the same answers both ways", { skip: !existsSync(command) }, async () => {
  const home = temp("live");
  const folder = temp("folder");
  writeFileSync(join(folder, "README.md"), "hello shell\n");
  const lfs = client({ home });
  assert.equal(shell(home, ["store", "create", "shop"]).status, 0);
  const progress = spawnSync(command, ["project", "init", "shop/api", folder, "--progress"], {
    env: { ...process.env, LAYERFS_HOME: home },
  });
  assert.equal(progress.status, 0, progress.stderr.toString());
  // progress, when Init reports any, is one JSON line per update on stderr
  for (const line of progress.stderr.toString().split("\n").filter(Boolean)) {
    assert.ok(decode(line).progress !== undefined, line);
  }
  await lfs.branch.fork("shop/api", "agent", "main");
  // content from stdin, and bytes that are not UTF-8
  assert.equal(shell(home, ["file", "write", "shop/api", "agent", "/a.txt", "-m", "from the shell"], "piped\n").status, 0);
  assert.equal(shell(home, ["file", "write", "shop/api", "agent", "/b.bin"], Buffer.from([0, 255, 1])).status, 0);
  const reads = [
    ["store", "get", "shop"],
    ["store", "list"],
    ["project", "get", "shop/api"],
    ["branch", "list", "shop/api"],
    ["commit", "list", "shop/api", "agent"],
    ["file", "read", "shop/api", "agent", "/a.txt"],
    ["file", "read", "shop/api", "agent", "/b.bin"],
    ["file", "stat", "shop/api", "agent", "/README.md"],
    ["file", "list", "shop/api", "agent", "/"],
    ["file", "grep", "shop/api", "agent", "/", "hello"],
    ["branch", "diff", "shop/api", "main", "agent"],
  ];
  for (const [family, call, ...positionals] of reads) {
    const fromNode = await node(lfs, family, call, positionals);
    const fromShell = shell(home, [family, call, ...positionals]);
    if (family === "store" && call === "list") {
      // a Store's state counts the leases of other processes: this one
      // keeps the Store open since the fork, so the shell sees it in use
      // and this process does not
      assert.equal(fromShell.value.items[0].state, "in_use");
      assert.equal(fromNode.value.items[0].state, "idle");
      for (const item of [...fromShell.value.items, ...fromNode.value.items]) delete item.state;
    }
    same(`${family}.${call}`, fromShell, fromNode);
  }
  assert.equal((await lfs.file.read("shop/api", "agent", "/a.txt")).text, "piped\n");
  assert.equal((await lfs.file.stat("shop/api", "agent", "/b.bin")).size, 3);
  // a folder scope on the command line
  const scoped = shell(home, ["file", "stat", "shop/api", "main", "/README.md", "--scope", "/web", "--scope-project", "shop/api"]);
  assert.equal(scoped.status, 125);
  assert.match(scoped.refused.message, /out of scope: path/);
  // a scope names its project; another call is out of scope
  const usage = spawnSync(command, ["store", "list", "--scope", "/web"], { env: { ...process.env, LAYERFS_HOME: home }, timeout: 60_000 });
  assert.equal(usage.status, 2);
  assert.match(usage.stderr.toString(), /--scope-project/);
  const store = shell(home, ["store", "remove", "shop", "--scope", "/web", "--scope-project", "shop/api"]);
  assert.equal(store.status, 125);
  assert.match(store.refused.message, /out of scope: store.remove/);
  rmSync(home, { recursive: true, force: true });
  rmSync(folder, { recursive: true, force: true });
});

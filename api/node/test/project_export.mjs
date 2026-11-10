// LP53b: the real JS shell over a synthetic native table, without an addon.
// Requires Node >=22.15 (registerHooks); no production module is rewritten.
// node --test api/node/test/project_export.mjs
// After LP53a: LP53_EXPORT_NATIVE=1 enables addon integration (fails if absent).
// LAYERFS_EXPORT_CLI=/absolute/path/to/layerfs enables real CLI integration.
// Run every invocation under an external wall timeout <=120s.
import { test } from "node:test";
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync,
  lstatSync, readlinkSync, symlinkSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const spec = {
  family: "project", call: "export", help: "export a committed Branch snapshot",
  args: [{ name: "project", kind: "project", optional: false },
    { name: "destination", kind: "text", optional: false }],
  options: [{ name: "branch", kind: "name", optional: true },
    { name: "on_progress", kind: "function", optional: true }],
};
const result = {
  project: "shop/api", store: "shop", name: "api", kind: "sqlite", machine: "native",
  branch: "main", snapshot: "main@0123456789ab", node: "01".repeat(33),
  root: "02".repeat(32), destination: "/exports/api", files: 2, directories: 1,
  symlinks: 1, bytes: 9,
};
const progress = [
  { phase: "prepare", files: 0, directories: 0, symlinks: 0, bytes: 0, done: null, total: null },
  { phase: "export", files: 0, directories: 0, symlinks: 0, bytes: 0, done: 0, total: null },
  { phase: "export", files: 1, directories: 1, symlinks: 0, bytes: 4, done: 4, total: null },
  { phase: "export", files: 2, directories: 1, symlinks: 1, bytes: 9, done: 9, total: null },
  { phase: "finalize", files: 2, directories: 1, symlinks: 1, bytes: 9, done: 9, total: null },
];
let calls = [];
let answer = { value: result };
const stub = {
  table: () => JSON.stringify([spec]),
  async call(name, args, home, scope, onEvent) {
    calls.push({ name, args: JSON.parse(args), home, scope, onEvent });
    // Deliberately resolve the native reply before the queued events arrive.
    if (onEvent) setImmediate(() => {
      for (const event of progress) onEvent(JSON.stringify({ progress: event }));
    });
    return JSON.stringify({ events: onEvent ? progress.length : 0, answer });
  },
};
const key = Symbol.for("layerfs.lp53b.export.native");
globalThis[key] = stub;
const nativeUrl = new URL("../src/native.js", import.meta.url).href;
const hooks = registerHooks({
  resolve(url, context, next) {
    if (new URL(url, context.parentURL).href === nativeUrl) {
      return { url: "lp53b:export-native", shortCircuit: true };
    }
    return next(url, context);
  },
  load(url, context, next) {
    if (url === "lp53b:export-native") return {
      format: "module", shortCircuit: true,
      source: 'export const native = globalThis[Symbol.for("layerfs.lp53b.export.native")];',
    };
    return next(url, context);
  },
});
let sdk;
try {
  sdk = await import("../src/index.js?lp53b-stub");
} finally {
  hooks.deregister();
  delete globalThis[key];
}

function checkProgress(events, exported) {
  assert.ok(events.length >= 3, "prepare, export and finalize must be observed");
  assert.equal(events[0].phase, "prepare");
  assert.equal(events.at(-1).phase, "finalize");
  let previous = { files: 0, directories: 0, symlinks: 0, bytes: 0 };
  let phase = 0;
  for (const event of events) {
    const next = ["prepare", "export", "finalize"].indexOf(event.phase);
    assert.ok(next >= phase && next >= 0, JSON.stringify(event));
    phase = next;
    assert.equal(event.total, null);
    for (const name of ["files", "directories", "symlinks", "bytes"]) {
      assert.ok(Number.isSafeInteger(event[name]) && event[name] >= previous[name], name);
    }
    if (event.phase === "prepare") {
      assert.equal(event.done, null);
    } else {
      assert.equal(event.done, event.bytes);
    }
    previous = event;
  }
  assert.ok(events.some((e) => e.phase === "export"));
  assert.equal(events.at(-1).files, exported.files);
  assert.equal(events.at(-1).directories, exported.directories);
  assert.equal(events.at(-1).symlinks, exported.symlinks);
  assert.equal(events.at(-1).bytes, exported.bytes);
}

test("export is generated; omission leaves the main default to the library", async () => {
  calls = [];
  const value = await sdk.default.project.export("shop/api", "/exports/api");
  assert.deepEqual(value, result);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].args, { project: "shop/api", destination: "/exports/api" });
  assert.equal(calls[0].name, "project.export");
  assert.equal(calls[0].home, null);
  assert.equal(calls[0].onEvent, undefined);
});

test("branch, home and scope are forwarded once; callbacks stay out of JSON", async () => {
  calls = [];
  const scope = { project: "shop/api", folder: "/src", branch: "agent" };
  const events = [];
  const lfs = sdk.client({ home: "/data/lfs", scope });
  await lfs.project.export("shop/api", "/exports/api", {
    branch: "agent", on_progress: (e) => events.push(e),
  });
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].args, { project: "shop/api", destination: "/exports/api", branch: "agent" });
  assert.equal(calls[0].home, "/data/lfs");
  assert.deepEqual(JSON.parse(calls[0].scope), scope);
  assert.deepEqual(events, progress);
});

test("all late native events reach the handler before the Promise settles", async () => {
  const events = [];
  const exported = await sdk.project.export("shop/api", "/exports/api", { on_progress: (e) => events.push(e) });
  checkProgress(events, exported);
  const settled = events.slice();
  await new Promise(setImmediate);
  assert.deepEqual(events, settled);
});

test("commit, extra positionals and nonfunction handlers fail before native dispatch", async () => {
  calls = [];
  for (const args of [
    ["shop/api", "/exports/api", { commit: "01".repeat(33) }],
    ["shop/api", "/exports/api", { on_progress: null }],
    ["shop/api", "/exports/api", { on_progress: "print" }],
    ["shop/api", "/exports/api", "extra", {}],
  ]) {
    await assert.rejects(sdk.project.export(...args), (e) => e.code === "InvalidInput");
  }
  assert.equal(calls.length, 0);
});

test("a throwing progress handler warns, drains every event and keeps the result", async (t) => {
  const warnings = [];
  t.mock.method(process, "emitWarning", (message, type) => warnings.push({ message, type }));
  const seen = [];
  const exported = await sdk.project.export("shop/api", "/exports/api", {
    on_progress: (event) => { seen.push(event); throw new Error("observer failed"); },
  });
  assert.deepEqual(exported, result);
  assert.deepEqual(seen, progress);
  assert.equal(warnings.length, progress.length);
  assert.ok(warnings.every((w) => w.type === "LayerFSHandlerWarning" && /observer failed/.test(w.message)));
});

test("a refusal after finalize stays a refusal with its detail and is never replayed", async () => {
  calls = [];
  answer = { refused: { code: "NotEmpty", message: "destination changed", detail: { phase: "finalize" } } };
  const events = [];
  try {
    await assert.rejects(sdk.project.export("shop/api", "/exports/api", { on_progress: (e) => events.push(e) }), (e) => {
      assert.equal(e.code, "NotEmpty");
      assert.deepEqual(e.detail, { phase: "finalize" });
      return true;
    });
    assert.equal(events.at(-1).phase, "finalize");
    assert.equal(calls.length, 1);
  } finally {
    answer = { value: result };
  }
});

function fixture() {
  const parent = mkdtempSync(join(tmpdir(), "layerfs-lp53b-"));
  const home = join(parent, "home");
  const source = join(parent, "source");
  mkdirSync(home, { mode: 0o700 });
  mkdirSync(source);
  mkdirSync(join(source, "empty"));
  mkdirSync(join(source, ".git"));
  writeFileSync(join(source, ".git", "config"), "[core]\n");
  writeFileSync(join(source, ".gitignore"), "ignored.bin\n");
  writeFileSync(join(source, "ignored.bin"), Buffer.from([0, 255, 128, 1]));
  writeFileSync(join(source, "empty.bin"), Buffer.alloc(0));
  writeFileSync(join(source, "héllo ☃.txt"), "unicode ☃\n");
  writeFileSync(join(source, "run.sh"), "#!/bin/sh\nexit 0\n");
  chmodSync(join(source, "run.sh"), 0o755);
  symlinkSync("ignored.bin", join(source, "link"));
  return { parent, home, source };
}
function checkTree(destination) {
  assert.deepEqual(readFileSync(join(destination, "ignored.bin")), Buffer.from([0, 255, 128, 1]));
  assert.equal(readFileSync(join(destination, "empty.bin")).length, 0);
  assert.equal(readFileSync(join(destination, "héllo ☃.txt"), "utf8"), "unicode ☃\n");
  assert.equal(readFileSync(join(destination, ".git", "config"), "utf8"), "[core]\n");
  assert.deepEqual(readdirSync(join(destination, "empty")), []);
  assert.equal(readlinkSync(join(destination, "link")), "ignored.bin");
  assert.equal(lstatSync(join(destination, "run.sh")).mode & 0o777, 0o755);
}

test("native Node export: default main, selected Branch, callbacks, refusals and scope", {
  timeout: 60_000, skip: process.env.LP53_EXPORT_NATIVE !== "1" && "NOT_RUN: set LP53_EXPORT_NATIVE=1 after LP53a",
}, async () => {
  const real = await import("../src/index.js");
  const f = fixture();
  const lfs = real.client({ home: f.home });
  try {
    assert.equal(typeof lfs.project.export, "function", "LP53a table entry required");
    await lfs.store.create("shop"); // public create selects the sole Disposable profile
    assert.equal((await lfs.store.get("shop")).profile, "disposable");
    await lfs.project.init("shop/api", f.source);
    await lfs.branch.fork("shop/api", "agent", "main");
    await lfs.file.write("shop/api", "agent", "/branch.txt", "agent\n");
    const events = [];
    const dest = join(f.parent, "main");
    const out = await lfs.project.export("shop/api", dest, { on_progress: (e) => events.push(e) });
    assert.equal(out.branch, "main");
    assert.equal(out.destination, dest);
    for (const field of Object.keys(result)) assert.ok(Object.hasOwn(out, field), field);
    checkTree(dest);
    checkProgress(events, out);
    const branchDest = join(f.parent, "agent");
    mkdirSync(branchDest); // existing empty destination is supported
    let seen = 0;
    await lfs.project.export("shop/api", branchDest, {
      branch: "agent", on_progress: () => { seen++; throw new Error("native observer"); },
    });
    assert.ok(seen >= 3);
    assert.equal(readFileSync(join(branchDest, "branch.txt"), "utf8"), "agent\n");
    await assert.rejects(lfs.project.export("shop/api", dest), (e) => e.code === "NotEmpty");
    checkTree(dest);
    const plainFile = join(f.parent, "plain-file");
    writeFileSync(plainFile, "not a folder");
    await assert.rejects(lfs.project.export("shop/api", plainFile), (e) => ["Exists", "NotDirectory"].includes(e.code));
    assert.equal(readFileSync(plainFile, "utf8"), "not a folder");
    await assert.rejects(lfs.project.export("shop/api", join(f.parent, "missing"), { branch: "absent" }), (e) => e.code === "NoSuchBranch");
    const scoped = real.client({ home: f.home, scope: { project: "shop/api", folder: "/", branch: "agent" } });
    await assert.rejects(scoped.project.export("shop/api", join(f.parent, "scoped")), /out of scope: project.export/);
    assert.ok(!readdirSync(f.parent).some((n) => n.startsWith(".layerfs-export-")));
  } finally {
    real.close();
    rmSync(f.parent, { recursive: true, force: true });
  }
});

const command = process.env.LAYERFS_EXPORT_CLI;
function shell(home, words) {
  const run = spawnSync(command, words, {
    env: { ...process.env, LAYERFS_HOME: home }, encoding: "utf8", timeout: 15_000,
  });
  assert.ifError(run.error);
  return run;
}
const cliSkip = !command && "NOT_RUN: set LAYERFS_EXPORT_CLI after building LP53a";
test("CLI export help and parser: two positionals, branch/progress, no commit", { timeout: 30_000, skip: cliSkip }, () => {
  const f = fixture();
  try {
    const help = shell(f.home, ["help", "project", "export"]);
    assert.equal(help.status, 0, help.stderr);
    assert.match(help.stdout, /project export <project> <destination>/);
    assert.match(help.stdout, /\[--branch <branch>\].*\[--progress\]/);
    assert.doesNotMatch(help.stdout, /--commit/);
    for (const words of [["project", "export", "shop/api"],
      ["project", "export", "shop/api", "/exports/api", "--commit", "abc"],
      ["project", "export", "shop/api", "/exports/api", "--branch"]]) {
      const run = shell(f.home, words);
      assert.equal(run.status, 2, run.stderr);
      assert.equal(run.stdout, "");
    }
  } finally { rmSync(f.parent, { recursive: true, force: true }); }
});

test("CLI export: stdout JSON/text, stderr progress JSONL, default/selected Branch, exit 125", {
  timeout: 60_000, skip: cliSkip,
}, () => {
  const f = fixture();
  try {
    for (const words of [["store", "create", "shop"], ["project", "init", "shop/api", f.source],
      ["branch", "fork", "shop/api", "agent", "main"]]) {
      const run = shell(f.home, words);
      assert.equal(run.status, 0, run.stderr);
    }
    assert.equal(JSON.parse(shell(f.home, ["store", "get", "shop"]).stdout).profile, "disposable");
    const dest = join(f.parent, "out");
    const run = shell(f.home, ["project", "export", "shop/api", dest, "--progress"]);
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stdout.trim().split("\n").length, 1);
    const out = JSON.parse(run.stdout);
    assert.equal(out.branch, "main");
    assert.equal(out.destination, dest);
    for (const field of Object.keys(result)) assert.ok(Object.hasOwn(out, field), field);
    const events = run.stderr.trim().split("\n").map((line) => {
      const event = JSON.parse(line);
      assert.deepEqual(Object.keys(event), ["progress"]);
      return event.progress;
    });
    checkProgress(events, out);
    checkTree(dest);
    const text = shell(f.home, ["project", "export", "shop/api", join(f.parent, "text"), "--branch=agent", "--text"]);
    assert.equal(text.status, 0, text.stderr);
    assert.equal(text.stderr, "");
    assert.match(text.stdout, /branch\s+agent/);
    assert.match(text.stdout, /destination\s+/);
    const refused = shell(f.home, ["project", "export", "shop/api", dest]);
    assert.equal(refused.status, 125);
    assert.equal(refused.stdout, "");
    assert.equal(JSON.parse(refused.stderr).code, "NotEmpty");
    checkTree(dest);
    assert.ok(!readdirSync(f.parent).some((n) => n.startsWith(".layerfs-export-")));
  } finally { rmSync(f.parent, { recursive: true, force: true }); }
});

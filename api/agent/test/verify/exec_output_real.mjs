// #352 fix 056: a Sandbox exec's output arrives complete at any size.
//
//   node api/agent/test/verify/exec_output_real.mjs outer <daemon> <addon> <image> [check|time]
//
// `outer` (on the Docker host) makes a fresh Store volume and runs `inner` in
// the runtime image, which is also the Sandbox image (it has Node). `check`:
// a small exec's result is exactly the old shape; 64 MiB of every byte value
// on each of stdout and stderr, in interleaved chunks of 1 B..64 KiB, goes
// through sinks first, from a small resident base: SHA-256 equal, the
// client's resident growth stays under 32 MiB, and its RSS after gc() is
// printed; then 1, 8 and 64 MiB of UTF-8 per stream arrive as complete text,
// byte-for-byte; no spool file is left in /tmp.
// `time` (one sample per arm): p50 of 30 small execs after one unmeasured
// exec, and one 8 MiB-per-stream exec. One attempt per step; nothing retried.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmodSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Writable } from "node:stream";
import { fileURLToPath } from "node:url";
import { docker, inRuntime, REPO, STORE_PATH } from "./container.mjs";

const MiB = 1 << 20;
/** Writes `n` bytes to each of stdout and stderr in interleaved chunks of
 * 1 B..64 KiB from a fixed seed; `raw` covers every byte value, `text` is
 * UTF-8 with 3-byte characters across chunk edges, in 16-byte units so a
 * whole MiB ends on a character. Run here for the expected bytes and in the
 * Sandbox as `node -e`. */
const GEN = `
const n = Number(process.argv[1]), raw = process.argv[2] === "raw";
const emit = globalThis.EMIT ?? ((s, b) => (s ? process.stderr : process.stdout).write(b));
const units = [Buffer.from("out ✓ € abc\\n"), Buffer.from("err ✓ € abc\\n")];
const at = [0, 0];
let x = 12345;
while (at[0] < n || at[1] < n) {
  x = (Math.imul(x, 1103515245) + 12345) >>> 0;
  let s = (x >>> 16) & 1;
  if (at[s] >= n) s = 1 - s;
  const len = Math.min(1 + (x % 65536), n - at[s]);
  const b = Buffer.allocUnsafe(len);
  for (let i = 0, o = at[s]; i < len; i++, o++) b[i] = raw ? (o * 131 + s * 17 + (o >>> 13)) & 255 : units[s][o % units[s].length];
  at[s] += len;
  emit(s, b);
}`;
const command = (n, mode) => `node -e "$(echo ${Buffer.from(GEN).toString("base64")} | base64 -d)" ${n} ${mode}`;
function expected(n, mode) {
  const parts = [[], []];
  globalThis.EMIT = (s, b) => parts[s].push(b);
  new Function("process", GEN)({ argv: ["node", String(n), mode] });
  delete globalThis.EMIT;
  return parts.map((p) => Buffer.concat(p));
}
class Hash extends Writable {
  constructor() { super({ highWaterMark: 64 * 1024 }); this.hash = createHash("sha256"); this.bytes = 0; }
  _write(chunk, _encoding, done) { this.hash.update(chunk); this.bytes += chunk.length; setImmediate(done); }
}
const sha = (b) => createHash("sha256").update(b).digest("hex");
const spools = () => readdirSync(tmpdir()).filter((f) => f.startsWith("layerfs-exec-"));
const mib = (n) => (n / MiB).toFixed(1);

async function check(layerfs, ws) {
  const small = await layerfs.workspace.exec(ws, "printf 'hello\\n'; printf 'warn\\n' >&2; exit 3", "Run");
  assert.deepEqual(Object.entries(small), [["kind", "exit:3"], ["exitCode", 3], ["stdout", "hello\n"], ["stderr", "warn\n"], ["message", "Run"], ["truncated", false]]);
  const empty = await layerfs.workspace.exec(ws, "true");
  assert.deepEqual(Object.entries(empty), [["kind", "exit:0"], ["exitCode", 0], ["stdout", ""], ["stderr", ""], ["message", null], ["truncated", false]]);
  console.log("SMALL ok");

  const sinks = { stdout: new Hash(), stderr: new Hash() };
  global.gc();
  const base = process.memoryUsage().rss;
  let peak = base;
  const sampler = setInterval(() => { peak = Math.max(peak, process.memoryUsage().rss); }, 5);
  const began = performance.now();
  const r = await layerfs.workspace.exec(ws, command(64 * MiB, "raw"), undefined, sinks);
  const ms = performance.now() - began;
  clearInterval(sampler);
  peak = Math.max(peak, process.memoryUsage().rss);
  global.gc();
  await new Promise((resolve) => setTimeout(resolve, 100)); // deferred finalizers
  global.gc();
  const after = process.memoryUsage().rss;
  assert.equal(r.exitCode, 0);
  assert.deepEqual([r.stdout, r.stderr, r.stdoutBytes, r.stderrBytes], [null, null, 64 * MiB, 64 * MiB]);
  assert.deepEqual([sinks.stdout.bytes, sinks.stderr.bytes], [64 * MiB, 64 * MiB]);
  console.log(`SINK 64 MiB per stream raw in ${ms.toFixed(0)} ms; rss ${mib(base)} -> peak ${mib(peak)} MiB (growth ${mib(peak - base)}), after gc ${mib(after)} MiB`);
  const [out, err] = expected(64 * MiB, "raw").map(sha);
  assert.equal(sinks.stdout.hash.digest("hex"), out, "stdout bytes equal");
  assert.equal(sinks.stderr.hash.digest("hex"), err, "stderr bytes equal");
  assert.ok(peak - base < 32 * MiB, `resident growth ${mib(peak - base)} MiB under 32 MiB`);

  for (const n of [MiB, 8 * MiB, 64 * MiB]) {
    const [out, err] = expected(n, "text");
    const began = performance.now();
    const r = await layerfs.workspace.exec(ws, command(n, "text"));
    const ms = performance.now() - began;
    assert.equal(r.exitCode, 0, r.stderr.slice(0, 300));
    assert.equal(r.truncated, false);
    // exactly the 1 MiB inline window stays inline, with the old shape
    assert.deepEqual([r.stdoutBytes, r.stderrBytes], n > MiB ? [n, n] : [undefined, undefined]);
    assert.ok(r.stdout === out.toString("utf8"), `stdout of ${n} complete`);
    assert.ok(r.stderr === err.toString("utf8"), `stderr of ${n} complete`);
    assert.ok(Buffer.from(r.stdout).equals(out) && Buffer.from(r.stderr).equals(err), "byte-for-byte");
    console.log(`TEXT ${n / MiB} MiB per stream ok in ${ms.toFixed(0)} ms`);
  }
  assert.deepEqual(spools(), [], "no spool file is left");
}

async function time(layerfs, ws) {
  await layerfs.workspace.exec(ws, "echo warm"); // unmeasured, both arms
  const ms = [];
  for (let i = 0; i < 30; i++) {
    const began = performance.now();
    const r = await layerfs.workspace.exec(ws, "echo hi");
    ms.push(performance.now() - began);
    assert.equal(r.stdout, "hi\n");
  }
  ms.sort((a, b) => a - b);
  const began = performance.now();
  const r = await layerfs.workspace.exec(ws, command(8 * MiB, "text"));
  const big = performance.now() - began;
  console.log(`TIME ${JSON.stringify({ smallP50: (ms[14] + ms[15]) / 2, smallMin: ms[0], smallMax: ms[29], small: ms.map((v) => +v.toFixed(2)), big8MiB: big, stdoutChars: r.stdout.length, stderrChars: r.stderr.length, truncated: r.truncated })}`);
}

async function inner([phase]) {
  const { LayerFS } = await import("@layerfs/agent");
  let layerfs;
  const source = mkdtempSync(join(tmpdir(), "layerfs-f056-"));
  writeFileSync(join(source, "README.md"), "workspace file\n");
  process.env.LAYERFS_STORE_PATH = STORE_PATH;
  layerfs = await LayerFS.init("site", source);
  rmSync(source, { recursive: true, force: true });
  await layerfs.branch.fork("work", "main");
  const ws = await layerfs.workspace.mount((await layerfs.sandbox.create()).sandbox, "work");
  try {
    await (phase === "time" ? time : check)(layerfs, ws);
  } finally {
    await layerfs.workspace.unmount(ws, true);
    await layerfs.close();
  }
  console.log("MAIN ok");
}

function outer([daemon, addon, image, phase = "check"]) {
  assert.ok(daemon && addon && image, "usage: outer <daemon> <addon> <image digest> [check|time]");
  const stamp = `f056-${Date.now().toString(36)}`;
  const volume = `layerfs-issue352-${stamp}`;
  const home = join(REPO, "target/f056", `${stamp}-home`);
  mkdirSync(home, { recursive: true });
  chmodSync(home, 0o700); // LAYERFS_HOME must be private
  let r;
  try {
    assert.equal(docker(["volume", "create", volume]).status, 0);
    r = inRuntime({ script: fileURLToPath(import.meta.url), args: [phase], daemon, addon, image, volume, prefix: `${stamp}-`, env: { LAYERFS_HOME: home, NODE_OPTIONS: "--expose-gc" }, timeout: 110_000 });
  } finally {
    const left = docker(["ps", "-aq", "--filter", `name=^${stamp}-`]).stdout.trim().split("\n").filter(Boolean);
    if (left.length) docker(["rm", "-f", ...left]);
    docker(["volume", "rm", volume]);
    rmSync(home, { recursive: true, force: true });
  }
  console.log(`== exit ${r.status}${r.signal ? ` signal ${r.signal}` : ""}\n${r.out.split("\n").slice(-30).join("\n")}`);
  assert.equal(r.status, 0, "the run passes");
  assert.match(r.out, /MAIN ok/);
  console.log(`exec output ${phase}: ok`);
}

const [mode, ...rest] = process.argv.slice(2);
if (mode === "inner") await inner(rest);
else outer(rest);

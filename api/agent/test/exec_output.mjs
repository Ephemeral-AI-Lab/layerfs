// workspace.exec output delivery over a stand-in binding (no Docker):
//   node api/agent/test/exec_output.mjs
// A small result is returned as the binding gives it; a spooled stream is
// read back into one reused window of at most 1 MiB, complete, then
// released; sinks get raw bytes, one page after the last is written; a failed
// page still releases the spool.
import assert from "node:assert/strict";
import { Writable } from "node:stream";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const FAKE = fileURLToPath(new URL("./fixture/spool-native.cjs", import.meta.url));
Object.assign(process.env, {
  LAYERFS_NATIVE: FAKE, LAYERFS_HOME: "/private/exec-home", LAYERFS_DOCKER: "/fake.sock",
  LAYERFS_IMAGE: "fake-image", LAYERFS_VOLUME: "fake-volume", LAYERFS_DAEMON: "/fake/daemon",
  LAYERFS_STORE_PATH: "/fake/store.sqlite",
});
for (const name of ["TRACE", "TRACE_STEP", "TRACE_START"]) delete process.env[`LAYERFS_${name}`];
const fake = createRequire(import.meta.url)(FAKE);
const { LayerFS } = await import("@layerfs/agent");
const client = await LayerFS.open("site");
const MiB = 1 << 20;
const WS = { sandbox: "1", path: "/w" };

/** Exactly `n` bytes of UTF-8 text whose 3-byte characters straddle page edges. */
const text = (n, seed) => {
  const unit = Buffer.from(`${seed}€\n`);
  const whole = Math.floor(n / unit.length);
  return Buffer.concat([...Array(whole).fill(unit), Buffer.alloc(n - whole * unit.length, 0x2e)]);
};
const pages = () => fake.calls.filter(([op]) => op === "execOutput").length;

// Small output: the binding's value, unchanged in content and field order.
fake.outputs.small = { stdout: Buffer.from("hello\n"), stderr: Buffer.from("warn\n") };
const small = await client.workspace.exec(WS, "small", "Run");
assert.deepEqual(Object.entries(small), [["kind", "exit:0"], ["exitCode", 0], ["stdout", "hello\n"], ["stderr", "warn\n"], ["message", "Run"], ["truncated", false]]);
assert.equal(pages(), 0, "no page reads for a small result");
// Exactly the inline window still stays inline.
fake.outputs.edge = { stdout: Buffer.alloc(MiB, 0x61), stderr: Buffer.alloc(0) };
const edge = await client.workspace.exec(WS, "edge");
assert.equal(edge.stdout.length, MiB);
assert.equal(edge.output, undefined);
assert.equal(pages(), 0);

// Large output: complete text, pages one at a time, spool released.
for (const [stdoutBytes, stderrBytes] of [[MiB + 1, 10], [3 * MiB + 17, 2 * MiB + 5]]) {
  const stdout = text(stdoutBytes, "o");
  const stderr = text(stderrBytes, "e");
  fake.outputs.large = { stdout, stderr };
  fake.calls.length = 0;
  const large = await client.workspace.exec(WS, "large");
  assert.equal(large.stdout, stdout.toString("utf8"), "stdout complete");
  assert.equal(large.stderr, stderr.toString("utf8"), "stderr complete");
  assert.equal(large.truncated, false);
  assert.equal(large.output, undefined, "the spool id is not returned");
  assert.equal(large.stdoutBytes, stdout.length);
  assert.equal(large.stderrBytes, stderr.length);
  const reads = fake.calls.filter(([op]) => op === "execOutput").map(([, a]) => a);
  assert.ok(reads.every((a) => a.page >= 1 && a.page <= MiB), "pages are at most 1 MiB");
  assert.equal(reads.length, Math.ceil(stdout.length / MiB) + (stderr.length > MiB ? Math.ceil(stderr.length / MiB) : 0));
  assert.equal(fake.calls.at(-1)[0], "workspace.exec_release");
  assert.equal(fake.spooled.size, 0, "released");
}

// Invalid UTF-8 decodes as it does inline: one U+FFFD per bad sequence.
fake.outputs.binary = { stdout: Buffer.concat([Buffer.alloc(MiB, 0x62), Buffer.from([0xff, 0xe2, 0x82]), Buffer.from("z")]), stderr: Buffer.alloc(0) };
assert.equal((await client.workspace.exec(WS, "binary")).stdout, `${"b".repeat(MiB)}��z`);

// Sinks: raw bytes, waits for drain, field null, sizes reported.
class Slow extends Writable {
  constructor() { super({ highWaterMark: 64 * 1024 }); this.parts = []; this.pending = 0; this.overlap = 0; }
  write(chunk, ...rest) { if (this.pending++ > 0) this.overlap += 1; return super.write(chunk, ...rest); }
  _write(chunk, _encoding, done) { this.parts.push(Buffer.from(chunk)); setImmediate(() => { this.pending -= 1; done(); }); }
}
const bytes = Buffer.concat([Buffer.from([0, 0xff, 0xfe]), text(5 * MiB, "s")]);
fake.outputs.sink = { stdout: bytes, stderr: Buffer.from("small err") };
const sink = new Slow();
fake.calls.length = 0;
const piped = await client.workspace.exec(WS, "sink", undefined, { stdout: sink });
assert.equal(fake.calls[0][1].spool, true, "sinks ask the binding to spool");
assert.ok(Buffer.concat(sink.parts).equals(bytes), "sink got the exact bytes");
assert.equal(sink.overlap, 0, "each page is written once the last one is");
assert.ok(sink.parts.length >= 6, `${sink.parts.length} pages`);
assert.equal(piped.stdout, null);
assert.equal(piped.stderr, "small err", "a stream without a sink is text");
assert.equal(piped.stdoutBytes, bytes.length);
assert.equal(fake.spooled.size, 0);
// Empty output with a sink: nothing spooled, nothing written.
fake.outputs.none = { stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) };
const quiet = new Slow();
const none = await client.workspace.exec(WS, "none", undefined, { stdout: quiet, stderr: quiet });
assert.deepEqual([none.stdout, none.stderr, none.stdoutBytes, none.stderrBytes, quiet.parts.length], [null, null, 0, 0, 0]);

// A failed page is the call's refusal; the spool is still released.
fake.outputs.fail = { stdout: text(2 * MiB, "f"), stderr: Buffer.alloc(0) };
fake.failPage = true;
await assert.rejects(client.workspace.exec(WS, "fail"), (e) => e.code === "Failed");
assert.equal(fake.spooled.size, 0, "released after a failed page");

// Sinks must be writable streams.
await assert.rejects(client.workspace.exec(WS, "small", undefined, { stdout: "file.txt" }), (e) => e.code === "InvalidInput");
console.log("exec output: ok");

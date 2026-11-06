// Values across the addon: whole numbers exactly, bytes, and what is refused
// before any call. The encode/decode boundary is tested on its own; two live
// calls then show the same through the real addon. No call of the table
// answers a whole number beyond 2^53 (its numbers are counts and
// millisecond times), so decoding is tested on the library's own JSON text.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { client, LayerFSError } from "../src/index.js";
import { encode, decode } from "../src/values.js";

const I64_MAX = 2n ** 63n - 1n;
const I64_MIN = -(2n ** 63n);

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-node-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};

const refused = (value, pattern) =>
  assert.throws(() => encode(value), (error) => {
    assert.ok(error instanceof LayerFSError, String(error));
    assert.equal(error.code, "InvalidInput");
    assert.match(error.message, pattern);
    return true;
  });

test("a bigint is encoded as its exact digits", () => {
  assert.equal(encode({ n: 2n ** 60n + 1n }), '{"n":1152921504606846977}');
  assert.equal(encode([I64_MAX, I64_MIN, -(2n ** 53n) - 1n, 0n, 7n]), "[9223372036854775807,-9223372036854775808,-9007199254740993,0,7]");
  assert.equal(encode({ deep: { list: [{ n: 2n ** 53n + 1n }] } }), '{"deep":{"list":[{"n":9007199254740993}]}}');
  assert.equal(encode(2n ** 100n), "1267650600228229401496703205376", "beyond i64 is left to the library, which refuses it");
});

test("safe integers stay numbers; unsafe numbers, NaN and Infinity are refused", () => {
  assert.equal(encode({ n: Number.MAX_SAFE_INTEGER, m: Number.MIN_SAFE_INTEGER, z: 0, f: 1.5 }), '{"n":9007199254740991,"m":-9007199254740991,"z":0,"f":1.5}');
  refused({ n: Number.MAX_SAFE_INTEGER + 1 }, /"n": 9007199254740992 is not a safe integer/);
  refused({ n: 2 ** 53 + 2 }, /not a safe integer/);
  refused([-(2 ** 60)], /not a safe integer/);
  refused({ n: 1e21 }, /not a safe integer/);
  refused({ n: NaN }, /"n": NaN/);
  refused([Infinity], /Infinity/);
  refused(-Infinity, /-Infinity/);
});

test("unsupported values are refused, never dropped or emptied", () => {
  refused({ m: new Map([["a", 1]]) }, /"m": Map cannot be sent/);
  refused({ s: new Set([1]) }, /Set cannot be sent/);
  refused({ b: new ArrayBuffer(2) }, /ArrayBuffer cannot be sent/);
  refused({ b: new SharedArrayBuffer(2) }, /SharedArrayBuffer cannot be sent/);
  refused({ b: new Uint16Array(2) }, /Uint16Array cannot be sent/);
  refused({ b: new Float64Array(1) }, /Float64Array cannot be sent/);
  refused({ b: new DataView(new ArrayBuffer(1)) }, /DataView cannot be sent/);
  refused({ f: () => 1 }, /"f": Function cannot be sent/);
  refused({ s: Symbol("x") }, /Symbol cannot be sent|symbol cannot be sent/);
  refused({ s: "a\ud800b" }, /"s": text with a lone surrogate/);
  refused(["\udc00"], /lone surrogate/);
  refused({ ["k\ud800"]: 1 }, /a name with a lone surrogate/);
  const cycle = {};
  cycle.self = cycle;
  refused(cycle, /cannot be sent/);
  // well-formed surrogate pairs are text
  assert.equal(decode(encode({ s: "𝄞 😀" })).s, "𝄞 😀");
});

test("bytes: a Uint8Array or Buffer is a { base64 } value, any view of it, empty too", () => {
  assert.equal(encode({ b: new Uint8Array([0, 1, 255]) }), '{"b":{"base64":"AAH/"}}');
  assert.equal(encode(Buffer.from("héllo 世界")), '{"base64":"aMOpbGxvIOS4lueVjA=="}');
  assert.equal(encode({ b: Buffer.alloc(0) }), '{"b":{"base64":""}}');
  assert.equal(encode({ b: new Uint8Array([1, 2, 3, 4]).subarray(1, 3) }), '{"b":{"base64":"AgM="}}');
  // a plain object that looks like Buffer.toJSON() is a map, not bytes
  assert.equal(encode({ type: "Buffer", data: [1, 2] }), '{"type":"Buffer","data":[1,2]}');
  const back = decode(encode({ b: new Uint8Array([0, 1, 255]), e: Buffer.alloc(0), nested: [{ x: Buffer.from("é") }] }));
  assert.ok(Buffer.isBuffer(back.b) && back.b.equals(Buffer.from([0, 1, 255])));
  assert.ok(Buffer.isBuffer(back.e) && back.e.length === 0);
  assert.ok(Buffer.isBuffer(back.nested[0].x) && back.nested[0].x.toString() === "é");
});

test("a one-key map { base64: text } is read as bytes; two keys or other values are a map", () => {
  // the documented ambiguity: the reserved key makes a map into bytes, both ways
  const asBytes = decode('{"base64":"QUJD"}');
  assert.ok(Buffer.isBuffer(asBytes) && asBytes.toString() === "ABC");
  assert.ok(Buffer.isBuffer(decode(encode({ base64: "QUJD" }))), "a user map with the key comes back as bytes");
  assert.deepEqual(decode('{"base64":"QUJD","x":1}'), { base64: "QUJD", x: 1 });
  assert.deepEqual(decode('{"base64":5}'), { base64: 5 });
  assert.deepEqual(decode('{"base64":"not base64!"}'), { base64: "not base64!" }, "text that is not base64 stays a map, as in the library");
  assert.deepEqual(decode('{"other":"QUJD"}'), { other: "QUJD" });
  assert.deepEqual(decode('{"a":{"base64":1}}'), { a: { base64: 1 } });
});

test("decoding: safe integers are numbers, whole numbers beyond them are exact bigints", () => {
  const got = decode(`{"max":${I64_MAX},"min":${I64_MIN},"p":9007199254740993,"q":9007199254740992,"s":9007199254740991,"neg":-9007199254740993,"ns":-9007199254740991,"z":0,"two60":${2n ** 60n + 1n},"list":[9007199254740993,1],"deep":{"x":{"y":${I64_MAX}}},"f":1.5,"e":1e3,"fz":9007199254740993.0}`);
  assert.equal(got.max, I64_MAX);
  assert.equal(got.min, I64_MIN);
  assert.equal(got.p, 9007199254740993n);
  assert.equal(got.q, 9007199254740992n, "2^53 is the first bigint");
  assert.equal(got.s, 9007199254740991);
  assert.equal(got.neg, -9007199254740993n);
  assert.equal(got.ns, -9007199254740991);
  assert.equal(got.z, 0);
  assert.equal(got.two60, 2n ** 60n + 1n);
  assert.deepEqual(got.list, [9007199254740993n, 1]);
  assert.equal(got.deep.x.y, I64_MAX);
  assert.equal(got.f, 1.5);
  assert.equal(got.e, 1000, "a number written with an exponent is a float, so a number");
  assert.equal(typeof got.fz, "number", "a number written with a fraction is a float, so a number");
  assert.equal(decode("-0"), -0);
});

test("a bigint survives encode then decode, in arrays and objects", () => {
  const value = { a: [I64_MAX, I64_MIN, 2n ** 60n + 1n, 9007199254740993n, -9007199254740993n, 5], o: { n: 2n ** 53n } };
  assert.deepEqual(decode(encode(value)), { a: [I64_MAX, I64_MIN, 2n ** 60n + 1n, 9007199254740993n, -9007199254740993n, 5], o: { n: 2n ** 53n } });
  assert.deepEqual(decode(encode([Number.MAX_SAFE_INTEGER, 2n ** 53n - 1n])), [Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER], "a bigint inside the safe range comes back as a number");
});

test("through the library: i64 max arrives exactly, an unsafe number is refused before the call", async () => {
  const home = temp("values");
  const folder = temp("values-folder");
  writeFileSync(join(folder, "README.md"), "hello\n");
  const lfs = client({ home });
  await lfs.store.create("shop");
  await lfs.project.init("shop/api", folder);
  // The library reads the line as an i64. i64 max is whole text it accepts and
  // then refuses as a line number ("positive safe integer"); rounded to a
  // Number it would be 2^63, which the library cannot read at all
  // ("number out of range"). So the message tells exact from rounded.
  await assert.rejects(lfs.file.read("shop/api", "main", "/README.md", { start_line: I64_MAX }), /startLine must be a positive safe integer/);
  await assert.rejects(lfs.file.read("shop/api", "main", "/README.md", { start_line: I64_MAX + 1n }), /number out of range/);
  await assert.rejects(lfs.file.read("shop/api", "main", "/README.md", { start_line: I64_MIN }), /positive safe integer/);
  await assert.rejects(lfs.file.read("shop/api", "main", "/README.md", { start_line: Number(2n ** 53n) + 2 }), (error) => {
    assert.ok(error instanceof LayerFSError);
    assert.equal(error.code, "InvalidInput");
    assert.match(error.message, /not a safe integer/, "refused by the package, not the library");
    return true;
  });
  assert.equal((await lfs.file.read("shop/api", "main", "/README.md", { start_line: 1n })).text, "hello\n");
  await assert.rejects(lfs.file.apply("shop/api", "main", [{ op: "write", path: "/m", content: new Map() }]), /"content": Map cannot be sent/);
  rmSync(home, { recursive: true, force: true });
  rmSync(folder, { recursive: true, force: true });
});

test("through the library: empty, binary, Unicode and one-key-map content", async () => {
  const home = temp("bytes");
  const folder = temp("bytes-folder");
  writeFileSync(join(folder, "README.md"), "hello\n");
  const lfs = client({ home });
  await lfs.store.create("shop");
  await lfs.project.init("shop/api", folder);
  await lfs.branch.fork("shop/api", "agent", "main");
  const size = async (path) => (await lfs.file.stat("shop/api", "agent", path)).size;
  const text = async (path) => (await lfs.file.read("shop/api", "agent", path)).text;
  await lfs.file.write("shop/api", "agent", "/empty.txt", "");
  await lfs.file.write("shop/api", "agent", "/empty.bin", Buffer.alloc(0));
  await lfs.file.write("shop/api", "agent", "/bin", Buffer.from([0, 1, 255, 254]));
  await lfs.file.write("shop/api", "agent", "/uni.txt", "héllo 世界 😀\n");
  await lfs.file.write("shop/api", "agent", "/uni.bin", Buffer.from("héllo 世界 😀\n"));
  // Inside a JSON value a one-key map with the reserved key is bytes: the
  // three bytes "ABC". (As the `content` positional of file.write it is taken
  // for the options object, which does not know "base64".)
  await lfs.file.apply("shop/api", "agent", [{ op: "write", path: "/map", content: { base64: "QUJD" } }, { op: "write", path: "/buf", content: Buffer.from("ABC") }, { op: "write", path: "/u8", content: new Uint8Array([0, 255]) }]);
  await assert.rejects(lfs.file.write("shop/api", "agent", "/map", { base64: "QUJD" }), /unknown option "base64"/);
  assert.equal(await size("/empty.txt"), 0);
  assert.equal(await size("/empty.bin"), 0);
  assert.equal(await size("/bin"), 4);
  assert.equal(await size("/u8"), 2);
  assert.equal(await text("/uni.txt"), "héllo 世界 😀\n");
  assert.equal(await text("/uni.bin"), "héllo 世界 😀\n", "bytes that are UTF-8 read back as text");
  assert.equal(await text("/map"), "ABC");
  assert.equal(await text("/buf"), "ABC");
  // a map with another key besides is a map, which is not content
  await assert.rejects(lfs.file.apply("shop/api", "agent", [{ op: "write", path: "/map2", content: { base64: "QUJD", x: 1 } }]), (error) => error instanceof LayerFSError && error.code === "InvalidInput");
  rmSync(home, { recursive: true, force: true });
  rmSync(folder, { recursive: true, force: true });
});

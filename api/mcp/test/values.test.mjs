// M4: bytes and whole numbers are lossless across the MCP boundary.
// bytes: base64 in, Buffer in the library, base64 out; the binary file is
// exported and compared byte for byte (file.read itself is text-only in the
// 2.0 call table and refuses binary with `Binary`, which this test asserts).
// integers: the exact wrapper at the ±(2^53-1) boundary, and a decimal string
// argument kept as BigInt; a non-safe JSON number is refused, never rounded.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { table } from "layerfs";
import { call, connect, value, workspace } from "./helpers.mjs";
import { InvalidArguments, encodeResult, readArguments } from "../src/values.js";

const BINARY = Buffer.from([0x00, 0x01, 0x02, 0xff, 0xfe, 0x80, 0x7f]);

test("a binary file round-trips byte for byte through file_write and export", async () => {
  const box = workspace("values", { "seed.txt": "seed\n" });
  const server = await connect({ home: box.home, profile: "admin" });
  try {
    const run = async (name, args) => value((await call(server.client, name, args)).result);
    await run("store_create", { name: "shop" });
    await run("project_init", { project: "shop/api", folder: box.source });
    await run("branch_fork", { project: "shop/api", name: "feat", from_branch: "main" });

    const write = await run("file_write", {
      project: "shop/api",
      branch: "feat",
      path: "/bin.dat",
      content: { base64: BINARY.toString("base64") },
      message: "binary",
    });
    assert.equal(write.kind, "Committed");

    const stat = await run("file_stat", { project: "shop/api", branch: "feat", path: "/bin.dat" });
    assert.equal(stat.size, BINARY.length);

    // file.read is text only (D2): binary is a refusal, not silent mojibake
    const refused = (await call(server.client, "file_read", { project: "shop/api", branch: "feat", path: "/bin.dat" })).result;
    assert.equal(refused.isError, true);
    assert.equal(JSON.parse(refused.content[0].text).code, "Binary");

    // the bytes themselves come back out through export, byte for byte
    const exported = await call(server.client, "project_export", {
      project: "shop/api",
      destination: box.out,
      branch: "feat",
    });
    assert.equal(value(exported.result).files, 2);
    assert.deepEqual(readFileSync(join(box.out, "bin.dat")), BINARY);

    // and a UTF-8 Buffer is still read back as text
    await run("file_write", {
      project: "shop/api",
      branch: "feat",
      path: "/utf8.txt",
      content: { base64: Buffer.from("héllo\n", "utf8").toString("base64") },
    });
    const text = await run("file_read", { project: "shop/api", branch: "feat", path: "/utf8.txt" });
    assert.equal(text.text, "héllo\n");

    await assert.rejects(
      call(server.client, "file_write", {
        project: "shop/api",
        branch: "feat",
        path: "/bad.bin",
        content: { base64: "not base64!" },
      }),
      (error) => error.code === -32602 && /not base64/.test(error.message),
    );
  } finally {
    await server.close();
    box.remove();
  }
});

test("a whole number beyond ±(2^53-1) is a typed exact string, never a rounded number", () => {
  const max = 9223372036854775807n;
  const min = -9223372036854775808n;
  assert.deepEqual(encodeResult({ max, min, nested: [max] }), {
    max: { $layerfs: "int", value: "9223372036854775807" },
    min: { $layerfs: "int", value: "-9223372036854775808" },
    nested: [{ $layerfs: "int", value: "9223372036854775807" }],
  });
  // and the text survives a JSON round trip without a double in between
  const text = JSON.stringify(encodeResult({ max }));
  assert.equal(JSON.parse(text).max.value, "9223372036854775807");

  assert.deepEqual(encodeResult({ bytes: Buffer.from([0, 0xff]), list: [Buffer.from([1, 2])] }), {
    bytes: { base64: "AP8=" },
    list: [{ base64: "AQI=" }],
  });

  const spec = table.find((entry) => entry.family === "file" && entry.call === "read");
  const read = readArguments(spec, { project: "shop/api", branch: "feat", path: "/x", start_line: "9223372036854775807" });
  assert.equal(read.options.start_line, 9223372036854775807n);
  const safe = readArguments(spec, { project: "shop/api", branch: "feat", path: "/x", start_line: 3 });
  assert.equal(safe.options.start_line, 3);
  assert.throws(
    () => readArguments(spec, { project: "shop/api", branch: "feat", path: "/x", start_line: 9223372036854775807 }),
    InvalidArguments,
  );
});

test("a decimal string reaches a real call and a non-safe JSON number is refused", async () => {
  const box = workspace("ints", { "lines.txt": "one\ntwo\nthree\n" });
  const server = await connect({ home: box.home, profile: "admin" });
  try {
    const run = async (name, args) => value((await call(server.client, name, args)).result);
    await run("store_create", { name: "shop" });
    await run("project_init", { project: "shop/api", folder: box.source });
    const read = await run("file_read", {
      project: "shop/api",
      branch: "main",
      path: "/lines.txt",
      start_line: "2",
      end_line: "2",
    });
    assert.equal(read.text, "two\n");

    await assert.rejects(
      call(server.client, "project_list", { page_size: 9223372036854775807 }),
      (error) => error.code === -32602 && /safe integer/.test(error.message),
    );
  } finally {
    await server.close();
    box.remove();
  }
});

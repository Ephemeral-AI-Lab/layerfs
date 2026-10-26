// Binary write bodies (#413): fs.write and fs.apply take a Buffer, any typed
// array, a DataView or an ArrayBuffer as the bytes they view, and refuse
// shared, detached and non-binary bodies before the call. Reads stay text
// only. Real native binding, Disposable Store, no Docker or daemon; Linux,
// with LAYERFS_STORE_PATH on a local volume (the E2E launcher's):
//   node api/agent/test/binary_write.mjs
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const work = mkdtempSync(join(tmpdir(), "layerfs-binary-"));
const store = join(dirname(dirname(process.env.LAYERFS_STORE_PATH)), `binary-${process.pid}`);
try {
  const source = join(work, "website");
  mkdirSync(source);
  writeFileSync(join(source, "README.md"), "Hello\n");
  process.env.LAYERFS_HOME = join(work, "home");
  mkdirSync(store);
  const { LayerFS } = await import("@layerfs/agent");
  process.env.LAYERFS_STORE_PATH = join(store, "global", "store.sqlite");
  const layerfs = await LayerFS.init("website", source);
  assert.equal(layerfs.created.kind, "Initialized");
  await layerfs.branch.fork("feat");

  const ascii = () => new TextEncoder().encode("xxABCDyy").buffer; // "ABCD" at offset 2
  const text = async (path) => (await layerfs.fs.read("feat", path)).text;
  const size = async (path) => (await layerfs.fs.stat("feat", path)).size;

  // Each body form writes exactly the bytes it views.
  const forms = {
    "/arraybuffer": new TextEncoder().encode("ABCD").buffer,
    "/subarray": new Uint8Array(ascii()).subarray(2, 6),
    "/uint16": new Uint16Array([0x4241, 0x4443]), // little-endian "ABCD"
    "/dataview": new DataView(ascii(), 2, 4),
  };
  for (const [path, body] of Object.entries(forms)) {
    assert.equal((await layerfs.fs.write("feat", path, body, "bytes")).kind, "Committed", path);
    assert.equal(await text(path), "ABCD", path);
  }

  // Bytes that are not text are stored whole and read back as Binary.
  await layerfs.fs.write("feat", "/blob", Buffer.from([0, 0xff, 0xfe, 1]));
  assert.equal(await size("/blob"), 4);
  await assert.rejects(layerfs.fs.read("feat", "/blob"), { code: "Binary" });

  // fs.apply: `bytes` in any binary form, and binary `content` sent as bytes.
  const applied = await layerfs.fs.apply("feat", [
    { op: "write", path: "/apply-bytes", bytes: new TextEncoder().encode("ABCD").buffer },
    { op: "write", path: "/apply-content", content: new Uint8Array([0, 1, 2]) },
    { op: "write", path: "/apply-text", content: "text\n" },
  ]);
  assert.equal(applied.kind, "Committed");
  assert.equal(await text("/apply-bytes"), "ABCD");
  assert.equal(await size("/apply-content"), 3);
  assert.equal(await text("/apply-text"), "text\n");

  // Refused before the call: shared memory, a detached buffer, a number array.
  const view = new Uint8Array(4);
  const detached = view.buffer;
  structuredClone(detached, { transfer: [detached] });
  const refusals = {
    shared: new Uint8Array(new SharedArrayBuffer(4)),
    detached,
    "detached view": view,
    numbers: [1, 2, 3],
  };
  for (const [name, body] of Object.entries(refusals)) {
    await assert.rejects(layerfs.fs.write("feat", "/refused", body), { code: "InvalidInput" }, name);
    await assert.rejects(layerfs.fs.apply("feat", [{ op: "write", path: "/refused", bytes: body }]), { code: "InvalidInput" }, name);
  }
  await assert.rejects(layerfs.fs.stat("feat", "/refused"), { code: "NoSuchPath" });
  await layerfs.close();
} finally {
  rmSync(work, { recursive: true, force: true });
  rmSync(store, { recursive: true, force: true });
}
console.log("binary_write: ok");

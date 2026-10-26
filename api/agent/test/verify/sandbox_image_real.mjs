// Sandboxes of one project on different images (#373; no placement since
// 2026-10-23): sandbox.create({ image }) starts one on that image (else the
// deployment's), a mount into it says its image, and the listing says which
// image each runs. A stopped Sandbox (as its idle close leaves it) is left
// out of the listing and reclaimed by the next sandbox.create, which then
// reuses its number. The second image is the deployment's with one label
// (its own digest), built here through the Engine and deleted at the end.
// Under the E2E launcher (its Store volume, home and deployment):
//   node api/agent/test/verify/sandbox_image_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: sandbox_image_real.mjs inner (under tools/worktree.py run)");

/** One Engine exchange over the Docker socket: its status and body. */
const engine = (method, path, body = null, type = "application/json") => new Promise((resolve, reject) => {
  const sent = request({ socketPath: process.env.LAYERFS_DOCKER, method, path, headers: body ? { "content-type": type } : {} }, (response) => {
    const chunks = [];
    response.on("data", (chunk) => chunks.push(chunk));
    response.on("end", () => resolve({ status: response.statusCode, body: Buffer.concat(chunks).toString() }));
  });
  sent.on("error", reject);
  sent.end(body);
});

/** A one-file tar (ustar) holding `name`. */
function tar(name, text) {
  const data = Buffer.from(text), header = Buffer.alloc(512);
  const field = (offset, length, value) => header.write(value, offset, length, "ascii");
  field(0, 100, name);
  field(100, 8, "0000644\0");
  field(108, 8, "0000000\0");
  field(116, 8, "0000000\0");
  field(124, 12, `${data.length.toString(8).padStart(11, "0")}\0`);
  field(136, 12, "00000000000\0");
  field(148, 8, "        ");
  field(156, 1, "0");
  field(257, 8, "ustar\u000000");
  const sum = header.reduce((total, byte) => total + byte, 0);
  field(148, 8, `${sum.toString(8).padStart(6, "0")}\0 `);
  const padding = Buffer.alloc((512 - (data.length % 512)) % 512);
  return Buffer.concat([header, data, padding, Buffer.alloc(1024)]);
}

const { LayerFS } = await import("@layerfs/agent");
const A = process.env.LAYERFS_IMAGE;
const tag = `layerfs-image-placement:${Date.now().toString(36)}`;
const built = await engine("POST", `/v1.54/build?version=1&q=1&t=${encodeURIComponent(tag)}`,
  tar("Dockerfile", `FROM ${A}\nLABEL layerfs.test=image-placement\n`), "application/x-tar");
assert.equal(built.status, 200, built.body);
const inspected = await engine("GET", `/v1.54/images/${encodeURIComponent(tag)}/json`);
const B = JSON.parse(inspected.body).Id;
assert.match(B, /^sha256:[0-9a-f]{64}$/);
assert.notEqual(B, A);

const project = `ip-${Date.now().toString(36)}`;
let layerfs;
const container = (id) => readFileSync(join(process.env.LAYERFS_HOME, project, "sandboxes", id, "container"), "utf8").trim();
const source = mkdtempSync(join(tmpdir(), "layerfs-ip-"));
writeFileSync(join(source, "README.md"), "images\n");
try {
  layerfs = await LayerFS.init(project, source);
  await layerfs.branch.fork("lane");
  assert.deepEqual(await layerfs.sandbox.create(), { sandbox: "1", image: A }, "no image named: the deployment's");
  assert.deepEqual(await layerfs.sandbox.create({ image: B }), { sandbox: "2", image: B });
  const a = await layerfs.workspace.mount("1", "lane");
  const b = await layerfs.workspace.mount("2", "lane");
  assert.deepEqual([a.sandbox, a.image, b.sandbox, b.image], ["1", A, "2", B], "a mount says its Sandbox's image");
  assert.deepEqual((await layerfs.project.list_sandboxes()).items.map(({ sandbox, image }) => [sandbox, image]), [["1", A], ["2", B]]);
  const stopped = container("2");
  const engineImage = JSON.parse((await engine("GET", `/v1.54/containers/${stopped}/json`)).body).Image;
  assert.equal(engineImage, B, "Sandbox 2's container runs image B");
  assert.equal((await layerfs.workspace.exec(b, "cat README.md")).stdout, "images\n");
  await Promise.all([a, b].map((ws) => layerfs.workspace.unmount(ws, true)));
  // Sandbox 2 stopped (as its idle close leaves it): the listing skips it,
  // and the next sandbox.create reclaims it (records and container) and
  // starts there again, on the deployment's image
  assert.equal((await engine("POST", `/v1.54/containers/${stopped}/stop?t=10`)).status, 204);
  assert.deepEqual((await layerfs.project.list_sandboxes()).items.map(({ sandbox, image }) => [sandbox, image]), [["1", A]]);
  assert.deepEqual(await layerfs.sandbox.create(), { sandbox: "2", image: A }, "sandbox.create reclaims the stopped Sandbox 2 and reuses its number");
  assert.equal((await engine("GET", `/v1.54/containers/${stopped}/json`)).status, 404, "the stopped container is deleted");
  for (const id of ["1", "2"]) await layerfs.sandbox.close(id);
  console.log("sandbox image: ok");
} finally {
  await layerfs.close();
  rmSync(source, { recursive: true, force: true });
  const removed = await engine("DELETE", `/v1.54/images/${encodeURIComponent(tag)}`);
  if (removed.status !== 200) console.error(`image ${tag} not removed (${removed.status}): ${removed.body}`);
}

// Sandboxes of one project on different images (#373): a mount names its
// image (`{ image }`, else the deployment's), placement only uses a Sandbox
// of that image, and the listing and the mount say which image each runs.
// The second image is the deployment's with one label (its own digest),
// built here through the Engine and deleted at the end. Under the E2E
// launcher (its Store volume, home and deployment):
//   node sdk/js/test/verify/image_placement_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: image_placement_real.mjs inner (under tools/worktree.py run)");

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

const { LayerFS } = await import("../../src/index.js");
const { LayerFSOperator } = await import("../../operator/index.js");
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
const layerfs = new LayerFS(project);
const operator = new LayerFSOperator(layerfs);
const container = (id) => readFileSync(join(process.env.LAYERFS_HOME, project, "sandboxes", id, "container"), "utf8").trim();
const source = mkdtempSync(join(tmpdir(), "layerfs-ip-"));
writeFileSync(join(source, "README.md"), "images\n");
try {
  await layerfs.project.init(source);
  await layerfs.branch.fork("lane");
  const a = await layerfs.workspace.mount("lane");
  assert.deepEqual([a.sandbox, a.image], ["1", A], "no image named: the deployment's");
  const b = await layerfs.workspace.mount("lane", { image: B });
  assert.deepEqual([b.sandbox, b.image], ["2", B], "Sandbox 1 has room but runs another image");
  const c = await layerfs.workspace.mount("lane");
  assert.deepEqual([c.sandbox, c.image], ["1", A]);
  const d = await layerfs.workspace.mount("lane", { image: B });
  assert.deepEqual([d.sandbox, d.image], ["2", B]);
  assert.deepEqual((await operator.sandbox.list()).map(({ sandbox, image }) => [sandbox, image]), [["1", A], ["2", B]]);
  const engineImage = JSON.parse((await engine("GET", `/v1.54/containers/${container("2")}/json`)).body).Image;
  assert.equal(engineImage, B, "Sandbox 2's container runs image B");
  assert.equal((await layerfs.workspace.exec(d, "cat README.md")).stdout, "images\n");
  await Promise.all([a, b, c, d].map((ws) => layerfs.workspace.unmount(ws, true)));
  // Sandbox 2 stopped (as its idle close leaves it): the listing reclaims it, then image B can go
  assert.equal((await engine("POST", `/v1.54/containers/${container("2")}/stop?t=10`)).status, 204);
  assert.deepEqual((await operator.sandbox.list()).map(({ sandbox, image }) => [sandbox, image]), [["1", A]]);
  console.log("image placement: ok");
} finally {
  await layerfs.close();
  rmSync(source, { recursive: true, force: true });
  const removed = await engine("DELETE", `/v1.54/images/${encodeURIComponent(tag)}`);
  if (removed.status !== 200) console.error(`image ${tag} not removed (${removed.status}): ${removed.body}`);
}

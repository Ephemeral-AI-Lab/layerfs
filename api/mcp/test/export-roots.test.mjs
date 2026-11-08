// The export allowlist: project_export writes only under a directory named at
// launch with --export-root. With none it is refused; a path outside, a `..`
// path, a sibling that shares a prefix and a symlink that leads out are all
// refused before the library is called, and nothing is created. Bad roots are
// usage errors (exit 2).
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, symlinkSync } from "node:fs";
import { join } from "node:path";
import { call, connect, SERVER, value, workspace } from "./helpers.mjs";
import { exportRefusal, resolveRoots } from "../src/exportroots.js";

const refusal = (answer) => {
  assert.equal(answer.result.isError, true, "expected a refusal");
  return JSON.parse(answer.result.content[0].text);
};

async function prepare(server, box) {
  const run = async (name, args) => value((await call(server.client, name, args)).result);
  await run("store_create", { name: "shop" });
  await run("project_init", { project: "shop/api", folder: box.source });
}

test("project_export writes only under an --export-root, and is refused with none", async () => {
  const box = workspace("export-roots", { "a.txt": "hello\n" });
  const allowed = join(box.root, "allowed");
  const evil = join(box.root, "allowed-evil"); // shares the root's name as a prefix
  const outside = join(box.root, "outside");
  mkdirSync(allowed);
  mkdirSync(evil);
  mkdirSync(outside);
  symlinkSync(outside, join(allowed, "link"));

  // no root at all: the allowlist is empty, so the call is refused and says how to allow it
  const closed = await connect({ home: box.home, profile: "admin" });
  try {
    await prepare(closed, box);
    const none = refusal(await call(closed.client, "project_export", { project: "shop/api", destination: join(allowed, "x") }));
    assert.equal(none.code, "InvalidInput");
    assert.match(none.message, /no export root is configured/);
    assert.match(none.message, /--export-root/);
    assert.equal(existsSync(join(allowed, "x")), false, "a refused export must create nothing");
    assert.match(closed.log(), /export roots none/);
  } finally {
    await closed.close();
  }

  const server = await connect({ home: box.home, profile: "admin", extra: ["--export-root", allowed] });
  try {
    const attempt = (destination) => call(server.client, "project_export", { project: "shop/api", destination });

    // inside the root: exported
    const inside = join(allowed, "ok");
    assert.equal(value((await attempt(inside)).result).branch, "main");
    assert.equal(readFileSync(join(inside, "a.txt"), "utf8"), "hello\n");

    // every way out is refused, with the roots named, and creates nothing
    const ways = {
      "a plain outside path": join(box.root, "elsewhere"),
      "the root's parent": box.root,
      "a `..` path": join(allowed, "..", "outside", "dotdot"),
      "a sibling sharing the root's name as a prefix": join(evil, "x"),
      "a symlink inside the root that leads out": join(allowed, "link", "via-link"),
      "a relative path": "relative/path",
      "a destination whose parent is missing": join(allowed, "missing", "x"),
    };
    for (const [what, destination] of Object.entries(ways)) {
      const body = refusal(await attempt(destination));
      assert.equal(body.code, "InvalidInput", what);
      assert.match(body.message, /outside the export roots/, what);
      assert.ok(body.message.includes(allowed), `${what}: the message names the allowed root`);
    }
    for (const path of [join(box.root, "elsewhere"), join(evil, "x"), join(outside, "via-link"), join(outside, "dotdot")]) {
      assert.equal(existsSync(path), false, `${path} must not exist`);
    }

    // the destination itself a symlink to a directory outside the root: it passes the allowlist by name,
    // and the library refuses a symlink destination without following it, so the outside stays empty
    symlinkSync(outside, join(allowed, "pointer"));
    const pointed = await attempt(join(allowed, "pointer"));
    assert.equal(pointed.result.isError, true, "a symlink destination must be refused by the library");
    assert.deepEqual(readdirSync(outside), [], "nothing may be written through the link");
  } finally {
    await server.close();
    box.remove();
  }
});

test("roots are checked at launch and by the unit", () => {
  const box = workspace("export-roots-usage");
  try {
    const launch = (...roots) =>
      spawnSync(process.execPath, [SERVER, "--home", box.home, ...roots.flatMap((root) => ["--export-root", root])], {
        encoding: "utf8",
        input: "",
        timeout: 20000,
      });
    for (const [what, root, message] of [
      ["a relative root", "relative", /must be an absolute path/],
      ["a missing root", join(box.root, "missing"), /does not exist/],
      ["the filesystem root", "/", /must not be the filesystem root/],
    ]) {
      const result = launch(root);
      assert.equal(result.status, 2, `${what}: exit code`);
      assert.match(result.stderr, message, what);
      assert.equal(result.stdout, "", `${what}: nothing on stdout`);
    }

    // the unit: prefix siblings and `..` are not inside, the root itself is
    const [root] = resolveRoots([box.root]);
    assert.equal(exportRefusal(join(box.root, "ok"), [root]), null);
    assert.equal(exportRefusal(box.root, [root]), null);
    assert.notEqual(exportRefusal(`${box.root}-x/ok`, [root]), null);
    assert.notEqual(exportRefusal(join(box.root, "..", "ok"), [root]), null);
    assert.notEqual(exportRefusal(undefined, [root]), null);
    assert.match(exportRefusal(join(box.root, "ok"), []), /no export root is configured/);
  } finally {
    box.remove();
  }
});

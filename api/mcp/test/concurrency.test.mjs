// M5: ten concurrent tool calls complete correctly, and closing the client's
// stdin makes the server exit 0 within 5 s with no process left (ps check).
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { call, connect, value, workspace } from "./helpers.mjs";

const files = Object.fromEntries(Array.from({ length: 10 }, (_, index) => [`f${index}.txt`, `file ${index}\n`]));

test("ten concurrent calls, then stdin close exits 0 within 5 s with no orphan", async () => {
  const box = workspace("concurrency", files);
  const server = await connect({ home: box.home, profile: "admin" });
  try {
    const run = async (name, args) => value((await call(server.client, name, args)).result);
    await run("store_create", { name: "shop" });
    await run("project_init", { project: "shop/api", folder: box.source });

    const answers = await Promise.all(
      Array.from({ length: 10 }, (_, index) =>
        run("file_read", { project: "shop/api", branch: "main", path: `/f${index}.txt` }),
      ),
    );
    answers.forEach((answer, index) => assert.equal(answer.text, `file ${index}\n`));

    const child = server.transport._process;
    assert.ok(child, "the transport has no child process");
    assert.equal(child.exitCode, null);
    // end the client's stdin directly (not client.close(), which falls back to
    // SIGTERM after 2 s), and time the server's own exit
    const started = Date.now();
    child.stdin.end();
    const exited = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("the server did not exit within 5 s")), 5000);
      child.once("exit", (code, signal) => {
        clearTimeout(timer);
        resolve({ code, signal });
      });
    });
    const elapsed = Date.now() - started;
    assert.deepEqual(exited, { code: 0, signal: null });
    assert.ok(elapsed < 5000, `exit took ${elapsed} ms`);
    const ps = spawnSync("ps", ["-p", String(child.pid), "-o", "pid="], { encoding: "utf8" });
    assert.equal(ps.stdout.trim(), "", `the server is still there: ${ps.stdout}`);
    assert.throws(() => process.kill(child.pid, 0));
  } finally {
    if (server.transport._process) await server.close().catch(() => {});
    box.remove();
  }
});

test("SIGTERM closes idle Stores and exits 0", async () => {
  const box = workspace("sigterm", { "a.txt": "a\n" });
  const server = await connect({ home: box.home, profile: "admin" });
  try {
    const created = await call(server.client, "store_create", { name: "shop" });
    assert.equal(value(created.result).name, "shop");
    const child = server.transport._process;
    child.kill("SIGTERM");
    const exited = await new Promise((resolve) => child.once("exit", (code, signal) => resolve({ code, signal })));
    assert.deepEqual(exited, { code: 0, signal: null });
    assert.throws(() => process.kill(child.pid, 0));
  } finally {
    if (server.transport._process) await server.close().catch(() => {});
    box.remove();
  }
});

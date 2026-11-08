// M2: real SDK Client + StdioClientTransport against `node src/server.js` on a
// Disposable temp home: initialize, tools/list, a real Store/project/Branch/file
// workflow, project.export with progress, refusals, scope and profiles.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { table } from "layerfs";
import { ToolSchema } from "@modelcontextprotocol/sdk/types.js";
import { tools as toolsOfProfile } from "../src/profiles.js";
import { call, connect, value, workspace } from "./helpers.mjs";

const calls = (profile) => toolsOfProfile(table, profile).length;
const names = (profile) => toolsOfProfile(table, profile).map((spec) => `${spec.family}_${spec.call}`);

test("a real workflow, refusals, progress, scope and the two profiles", async () => {
  const box = workspace("workflow", { "a.txt": "hello\n", "sub/b.txt": "bee\n" });
  let server;
  try {
    server = await connect({ home: box.home, profile: "admin", extra: ["--export-root", box.root] });
    assert.equal(server.client.getServerVersion()?.name, "layerfs-mcp");

    const listed = await server.client.listTools();
    assert.equal(listed.tools.length, calls("admin"));
    assert.equal(listed.tools.length, table.length);
    assert.equal(new Set(listed.tools.map((tool) => tool.name)).size, table.length);
    for (const tool of listed.tools) ToolSchema.parse(tool);

    const run = async (name, args, options) => value((await call(server.client, name, args, options)).result);

    const store = await run("store_create", { name: "shop" });
    assert.equal(store.name, "shop");
    assert.equal(store.kind, "sqlite");

    const project = await run("project_init", { project: "shop/api", folder: box.source });
    assert.equal(project.project, "shop/api");

    const fork = await run("branch_fork", { project: "shop/api", name: "feat", from_branch: "main" });
    assert.equal(fork.kind, "Forked");
    // the namespace branch a folder scope composes: <scope branch>.<call branch>
    const namespaced = await run("branch_fork", { project: "shop/api", name: "feat.main", from_branch: "main" });
    assert.equal(namespaced.kind, "Forked");

    const write = await run("file_write", {
      project: "shop/api",
      branch: "feat",
      path: "/new.txt",
      content: "from mcp\n",
      message: "one file",
    });
    assert.equal(write.kind, "Committed");
    assert.equal(write.commit.message, "one file");

    const read = await run("file_read", { project: "shop/api", branch: "feat", path: "/new.txt" });
    assert.equal(read.text, "from mcp\n");

    const list = await run("file_list", { project: "shop/api", branch: "feat", path: "/" });
    assert.deepEqual(list.items.map((item) => item.name).sort(), ["a.txt", "new.txt", "sub"]);

    const commits = await run("commit_list", { project: "shop/api", branch: "feat" });
    assert.equal(commits.items.length, 1);
    assert.equal(commits.items[0].message, "one file");

    const diff = await run("branch_diff", { project: "shop/api", from_branch: "main", to_branch: "feat" });
    assert.deepEqual(diff.items.map((item) => [item.path, item.kind]), [["/new.txt", "Added"]]);

    const merge = await run("branch_merge", { project: "shop/api", name: "merged", a: "main", b: "feat" });
    assert.equal(merge.kind, "Merged");
    assert.equal(merge.branch, "merged");

    const exported = await call(
      server.client,
      "project_export",
      { project: "shop/api", destination: box.out, branch: "merged" },
      { progress: true },
    );
    const answer = value(exported.result);
    assert.equal(answer.branch, "merged");
    assert.ok(existsSync(join(box.out, "new.txt")));
    assert.equal(readFileSync(join(box.out, "new.txt"), "utf8"), "from mcp\n");
    assert.ok(exported.progress.length >= 1, "the client received no progress before the result");
    exported.progress.forEach((entry, index) => assert.equal(entry.progress, index + 1, "progress must increase"));
    assert.match(exported.progress.at(-1).message, /finalize/);

    const refused = (await call(server.client, "branch_get", { project: "shop/api", branch: "nope" })).result;
    assert.equal(refused.isError, true);
    const body = JSON.parse(refused.content[0].text);
    assert.equal(body.code, "NoSuchBranch");
    assert.equal(refused.content.length, 1, "a known refusal has no uncertainty note");

    await assert.rejects(
      server.client.callTool({ name: "not_a_tool", arguments: {} }),
      (error) => error.code === -32602 && /unknown tool/.test(error.message),
    );
    await assert.rejects(
      server.client.callTool({ name: "store_create", arguments: { name: 5 } }),
      (error) => error.code === -32602 && /name.*must be text/.test(error.message),
    );
    await assert.rejects(
      server.client.callTool({ name: "store_create", arguments: { name: "x", extra: true } }),
      (error) => error.code === -32602 && /unknown argument "extra"/.test(error.message),
    );
    await assert.rejects(
      server.client.callTool({ name: "store_create", arguments: {} }),
      (error) => error.code === -32602 && /required argument "name"/.test(error.message),
    );
    // the SDK's request schema refuses a non-object `arguments` before the handler
    await assert.rejects(
      server.client.callTool({ name: "project_list", arguments: [] }),
      (error) => error.code === -32603,
    );
    await server.close();
    server = null;

    // the work profile: the same home, no administrative tools
    const work = await connect({ home: box.home, profile: "work" });
    const workList = await work.client.listTools();
    assert.equal(workList.tools.length, calls("work"));
    assert.equal(workList.tools.length, 34);
    const workNames = new Set(workList.tools.map((tool) => tool.name));
    for (const name of names("work")) assert.ok(workNames.has(name), `${name} is missing from work`);
    for (const name of names("admin").filter((name) => !names("work").includes(name))) {
      assert.ok(!workNames.has(name), `${name} is an admin tool in the work profile`);
    }
    for (const prefix of ["store_", "machine_", "backup_", "project_init", "project_remove", "sandbox_create", "sandbox_close"]) {
      assert.ok(![...workNames].some((name) => name.startsWith(prefix)), `${prefix} in the work profile`);
    }
    const scopedRead = await call(work.client, "file_read", { project: "shop/api", branch: "feat", path: "/sub/b.txt" });
    assert.equal(value(scopedRead.result).text, "bee\n");
    await work.close();

    // a folder scope: the library refuses everything outside it
    const scoped = await connect({
      home: box.home,
      profile: "admin",
      scope: { project: "shop/api", folder: "/sub", branch: "feat" },
      // a root, so the library's own scope refusal is what answers the export
      extra: ["--export-root", box.root],
    });
    const storeRefusal = (await call(scoped.client, "store_list", {})).result;
    assert.equal(storeRefusal.isError, true);
    assert.match(JSON.parse(storeRefusal.content[0].text).message, /out of scope/);

    const exportRefusal = (await call(scoped.client, "project_export", {
      project: "shop/api",
      destination: join(box.root, "scoped-out"),
    })).result;
    assert.equal(exportRefusal.isError, true);
    assert.match(JSON.parse(exportRefusal.content[0].text).message, /out of scope/);

    const inside = await call(scoped.client, "file_read", { project: "shop/api", branch: "main", path: "/sub/b.txt" });
    assert.equal(value(inside.result).text, "bee\n", "a scope that refuses everything would still pass the refusals");

    const pathRefusal = (await call(scoped.client, "file_read", { project: "shop/api", branch: "feat", path: "/a.txt" })).result;
    assert.equal(pathRefusal.isError, true);
    assert.match(JSON.parse(pathRefusal.content[0].text).message, /out of scope: path/);
    await scoped.close();
  } finally {
    if (server) await server.close().catch(() => {});
    box.remove();
  }
});

import assert from "node:assert/strict";
import { test } from "node:test";
import { call, initialize, open, payload, startServer } from "./helpers.mjs";

const TOOLS = [
  "layerfs_init", "layerfs_open", "layerfs_close", "layerfs_results", "layerfs_events",
  "layerfs_project_list_branches", "layerfs_project_list_sandboxes",
  "layerfs_branch_list_commits", "layerfs_branch_graph", "layerfs_branch_changed_paths",
  "layerfs_branch_diff_commits", "layerfs_branch_diff_branches", "layerfs_branch_fork",
  "layerfs_branch_merge", "layerfs_branch_publish",
  "layerfs_fs_read", "layerfs_fs_list", "layerfs_fs_stat", "layerfs_fs_grep", "layerfs_fs_glob",
  "layerfs_fs_diff_files", "layerfs_fs_blame", "layerfs_fs_write", "layerfs_fs_edit",
  "layerfs_fs_apply", "layerfs_fs_mkdir", "layerfs_fs_rm", "layerfs_fs_cp", "layerfs_fs_mv",
  "layerfs_sandbox_create", "layerfs_sandbox_list_workspaces", "layerfs_sandbox_close",
  "layerfs_workspace_mount", "layerfs_workspace_exec", "layerfs_workspace_commit", "layerfs_workspace_unmount",
  "layerfs_backend_get_history", "layerfs_backend_get_storage", "layerfs_backend_get_comparable",
  "layerfs_backend_list_overwritten", "layerfs_backend_list_commit_storage", "layerfs_backend_fs_get_storage",
  "layerfs_backend_fs_list_revisions", "layerfs_backend_workspace_status", "layerfs_backend_workspace_list_changes",
  "layerfs_backend_workspace_get_storage", "layerfs_backend_process_start", "layerfs_backend_process_list",
  "layerfs_backend_process_cancel", "layerfs_backend_sandbox_get_log",
].sort();

test("stdio handshake lists exactly the typed SDK tools with strict schemas", async (t) => {
  const server = startServer(t);
  await initialize(server);
  const listed = await server.request("tools/list");
  assert.deepEqual(listed.tools.map((tool) => tool.name).sort(), TOOLS);
  assert.equal(listed.tools.length, 50);
  assert.ok(!listed.tools.some((tool) => /push|pull/.test(tool.name)));
  for (const tool of listed.tools) {
    assert.equal(tool.inputSchema.type, "object", tool.name);
    assert.equal(tool.inputSchema.additionalProperties, false, tool.name);
  }
  const read = listed.tools.find((tool) => tool.name === "layerfs_fs_read");
  assert.deepEqual(Object.keys(read.inputSchema.properties).sort(), ["binary", "endLine", "offset", "path", "ref", "startLine"]);
  assert.equal(read.inputSchema.properties.binary.default, false);
  assert.deepEqual(read.inputSchema.properties.offset.type, ["string", "null"]);
  const revisions = listed.tools.find((tool) => tool.name === "layerfs_backend_fs_list_revisions");
  assert.deepEqual(revisions.inputSchema.required.sort(), ["path", "ref"]);
  const cancel = listed.tools.find((tool) => tool.name === "layerfs_backend_process_cancel");
  assert.deepEqual(cancel.inputSchema.required, ["process"]);
  const history = listed.tools.find((tool) => tool.name === "layerfs_backend_get_history");
  assert.deepEqual(Object.keys(history.inputSchema.properties), ["since"]);
  assert.deepEqual(history.inputSchema.required ?? [], []);
  const log = listed.tools.find((tool) => tool.name === "layerfs_backend_sandbox_get_log");
  assert.deepEqual(Object.keys(log.inputSchema.properties).sort(), ["lines", "sandbox"]);
  assert.equal(log.inputSchema.properties.lines.type, "integer");
  assert.equal(log.inputSchema.properties.lines.minimum, 1);
  assert.equal(log.inputSchema.properties.lines.maximum, 2000);
  assert.equal(log.inputSchema.properties.lines.default, 200);
  const merge = listed.tools.find((tool) => tool.name === "layerfs_branch_merge");
  assert.equal(merge.inputSchema.properties.units.items.anyOf.length, 2);
  assert.equal(merge.inputSchema.properties.units.items.anyOf[1].additionalProperties, false);
  const createTool = listed.tools.find((tool) => tool.name === "layerfs_sandbox_create");
  assert.deepEqual(Object.keys(createTool.inputSchema.properties), ["image"]);
  const mkdirTool = listed.tools.find((tool) => tool.name === "layerfs_fs_mkdir");
  assert.deepEqual(Object.keys(mkdirTool.inputSchema.properties).sort(), ["branch", "message", "mode", "path"]);
  const apply = listed.tools.find((tool) => tool.name === "layerfs_fs_apply");
  const variants = apply.inputSchema.properties.changes.items.oneOf;
  assert.deepEqual(variants.map((item) => item.properties.op.const).sort(), ["chmod", "cp", "edit", "mkdir", "mv", "rm", "write"]);
  for (const item of variants) assert.equal(item.additionalProperties, false, item.properties.op.const);
  assert.deepEqual(variants.find((item) => item.properties.op.const === "chmod").required.sort(), ["mode", "op", "path"]);
  assert.deepEqual(variants.find((item) => item.properties.op.const === "rm").required.sort(), ["op", "path"]);
  assert.deepEqual(variants.find((item) => item.properties.op.const === "cp").required.sort(), ["from", "op", "to"]);

  const invalid = await call(server, "layerfs_open", { name: "site", unknown: true });
  assert.equal(invalid.isError, true);
  assert.match(invalid.content[0].text, /^Input validation error: Invalid arguments for tool layerfs_open/);
  assert.deepEqual(server.calls(), []);
  await server.stop();
  assert.ok(server.messages.every((message) => message.jsonrpc === "2.0"), "stdout contains protocol frames only");
});

test("fs.apply accepts exactly the SDK's seven change variants and rejects others", async (t) => {
  const server = startServer(t);
  await open(server);
  const changes = [
    { op: "write", path: "/a.txt", content: "a", mode: 420 },
    { op: "write", path: "/b.bin", base64: "AP8BCg==" },
    { op: "edit", path: "/a.txt", oldText: "a", newText: "b" },
    { op: "mkdir", path: "/dir", mode: 493 },
    { op: "chmod", path: "/a.txt", mode: 384 },
    { op: "rm", path: "/b.bin" },
    { op: "cp", from: "/a.txt", to: "/c.txt" },
    { op: "mv", from: "/c.txt", to: "/d.txt" },
  ];
  const applied = await call(server, "layerfs_fs_apply", { branch: "main", changes });
  assert.equal(applied.isError, undefined, JSON.stringify(applied));
  const sent = server.calls().filter(([op]) => op === "fs.apply").at(-1)[1];
  assert.deepEqual(sent.changes.map((change) => change.op), changes.map((change) => change.op));
  assert.deepEqual(sent.changes[1].bytes, { base64: "AP8BCg==" });
  assert.equal(sent.changes[1].base64, undefined);
  assert.equal(sent.changes[2].oldText, "a");
  assert.equal(sent.changes[4].mode, 384);
  assert.deepEqual([sent.changes[6].from, sent.changes[6].to], ["/a.txt", "/c.txt"]);

  const before = server.calls().length;
  for (const invalid of [
    { branch: "main", changes: [{ op: "delete", path: "/a.txt" }] },
    { branch: "main", changes: [{ op: "write", path: "/a.txt" }] },
    { branch: "main", changes: [{ op: "write", path: "/a.txt", content: "a", base64: "YQ==" }] },
    { branch: "main", changes: [{ op: "write", path: "/a.txt", content: "a", extra: 1 }] },
    { branch: "main", changes: [{ op: "chmod", path: "/a.txt" }] },
    { branch: "main", changes: [{ op: "chmod", path: "/a.txt", mode: -1 }] },
    { branch: "main", changes: [{ op: "cp", path: "/a.txt" }] },
    { branch: "main", changes: [{ op: "rm", path: "/a.txt", mode: 420 }] },
  ]) {
    const rejected = await call(server, "layerfs_fs_apply", invalid);
    assert.equal(rejected.isError, true, JSON.stringify(invalid));
    assert.match(rejected.content[0].text, /^Input validation error: Invalid arguments for tool layerfs_fs_apply/);
  }
  assert.equal(server.calls().length, before, "schema-invalid apply never reaches the SDK");
  const passed = await call(server, "layerfs_fs_apply", { branch: "main", changes: [{ op: "chmod", path: "/a.txt", mode: 4096 }] });
  assert.equal(passed.isError, undefined, JSON.stringify(passed));
  const forwarded = server.calls().filter(([op]) => op === "fs.apply").at(-1)[1];
  assert.equal(forwarded.changes[0].mode, 4096, "the SDK, not the MCP schema, decides whether a mode is storable");
  await server.stop();
});

test("backend tools pass the SDK's optional arguments and keep strict unknown keys", async (t) => {
  const server = startServer(t);
  await open(server);
  const last = (op) => server.calls().filter(([called]) => called === op).at(-1)[1];
  await call(server, "layerfs_backend_get_history", { since: "h1" });
  assert.equal(last("backend.store.project.get_history").since, "h1");
  await call(server, "layerfs_backend_get_history", {});
  assert.equal("since" in last("backend.store.project.get_history"), false);
  await call(server, "layerfs_backend_sandbox_get_log", { sandbox: "1" });
  assert.equal(last("backend.daemon.sandbox.get_log").lines, 200);
  await call(server, "layerfs_backend_sandbox_get_log", { sandbox: "1", lines: 20 });
  assert.equal(last("backend.daemon.sandbox.get_log").lines, 20);
  for (const invalid of [
    { sandbox: "1", lines: 0 },
    { sandbox: "1", lines: 2001 },
    { sandbox: "1", lines: 2.5 },
    { sandbox: "1", since: "now" },
  ]) {
    const rejected = await call(server, "layerfs_backend_sandbox_get_log", invalid);
    assert.equal(rejected.isError, true, JSON.stringify(invalid));
    assert.match(rejected.content[0].text, /^Input validation error: Invalid arguments for tool layerfs_backend_sandbox_get_log/);
  }
  const unknown = await call(server, "layerfs_backend_get_history", { since: "h1", extra: true });
  assert.equal(unknown.isError, true);
  await server.stop();
});

test("tools delegate to the SDK, preserve results/refusals, validate Branch names, and close idempotently", async (t) => {
  const server = startServer(t);
  await open(server);
  const branches = payload(await call(server, "layerfs_project_list_branches", { pageSize: 2 }));
  assert.deepEqual(branches.items.map((branch) => branch.name), ["main", "cart"]);
  assert.equal(branches.hasMore, false);

  const written = payload(await call(server, "layerfs_fs_write", { branch: "main", path: "/binary.bin", base64: "AP8BCg==" }));
  assert.equal(written.kind, "Committed");
  const binary = payload(await call(server, "layerfs_fs_read", {
    ref: `12${"a".repeat(64)}`, path: "/binary.bin", binary: true,
  }));
  assert.deepEqual(binary.bytes, { $layerfs: "bytes", base64: "AP8BCg==" });

  const missing = await call(server, "layerfs_fs_read", { ref: "main", path: "/missing" });
  assert.equal(missing.isError, true);
  assert.deepEqual(payload(missing), { code: "NoSuchPath", message: "NoSuchPath: /missing does not exist", detail: null });

  const beforeInvalid = server.calls().length;
  for (const branch of ["Feature", "bad/name", "-bad", "bad-", "a".repeat(64)]) {
    const invalid = await call(server, "layerfs_fs_write", { branch, path: "/x", content: "x" });
    assert.equal(invalid.isError, true);
    assert.match(invalid.content[0].text, /^Input validation error: Invalid arguments for tool layerfs_fs_write/);
  }
  for (const args of [
    { branch: "main", path: "/x" },
    { branch: "main", path: "/x", content: "x", base64: "eA==" },
    { branch: "main", path: "/x", content: "x", extra: true },
  ]) {
    const invalid = await call(server, "layerfs_fs_write", args);
    assert.equal(invalid.isError, true);
    assert.match(invalid.content[0].text, /^Input validation error: Invalid arguments for tool layerfs_fs_write/);
  }
  assert.equal(server.calls().length, beforeInvalid, "schema-invalid calls never reach the SDK");

  const results = payload(await call(server, "layerfs_results"));
  assert.ok(results.items.some((record) => record.method === "fs.write" && record.ok));
  assert.ok(results.items.some((record) => record.method === "fs.read" && record.code === "NoSuchPath"));
  assert.deepEqual(payload(await call(server, "layerfs_close")), { closed: true });
  assert.deepEqual(payload(await call(server, "layerfs_close")), { closed: true });
  assert.equal(payload(await call(server, "layerfs_results")).items.length, results.items.length);
  const closed = await call(server, "layerfs_project_list_branches");
  assert.equal(closed.isError, true);
  assert.equal(payload(closed).code, "Failed");
  await server.stop();
});

test("a second open is refused without constructing another SDK client", async (t) => {
  const server = startServer(t);
  await open(server);
  const before = server.calls().filter(([op]) => op === "store.open").length;
  const second = await call(server, "layerfs_open", { name: "site" });
  assert.equal(second.isError, true);
  assert.equal(payload(second).code, "Failed");
  assert.match(payload(second).message, /already open/);
  assert.equal(server.calls().filter(([op]) => op === "store.open").length, before);
  await server.stop();
});

test("every operation tool delegates to its matching public SDK operation", async (t) => {
  const server = startServer(t);
  await open(server);
  const mounted = payload(await call(server, "layerfs_workspace_mount", { sandbox: "1", branch: "main" }));
  const ws = { sandbox: mounted.sandbox, path: mounted.path };
  const workspace = { workspace: ws };
  const ref = "main";
  const cases = [
    ["layerfs_project_list_branches", {}, "project.list_branches"],
    ["layerfs_project_list_sandboxes", {}, "project.list_sandboxes"],
    ["layerfs_branch_list_commits", { branch: "main" }, "branch.list_commits"],
    ["layerfs_branch_graph", { branch: "main" }, "branch.graph"],
    ["layerfs_branch_changed_paths", { commit: "c0ffee" }, "branch.changed_paths"],
    ["layerfs_branch_diff_commits", { from: "c0ffee", to: "decaf" }, "branch.diff_commits"],
    ["layerfs_branch_diff_branches", { from: "main", to: "cart" }, "branch.diff_branches"],
    ["layerfs_branch_fork", { name: "feature" }, "branch.fork"],
    ["layerfs_branch_merge", { branch: "main", with: "cart", name: "merged", units: ["src/**", { name: "core", patterns: ["core/**"] }] }, "branch.merge"],
    ["layerfs_branch_publish", { branch: "main" }, "branch.publish"],
    ["layerfs_fs_read", { ref, path: "/file.txt" }, "fs.read"],
    ["layerfs_fs_list", { ref }, "fs.list"],
    ["layerfs_fs_stat", { ref, path: "/file.txt" }, "fs.stat"],
    ["layerfs_fs_grep", { ref, pattern: "text" }, "fs.grep"],
    ["layerfs_fs_glob", { ref, pattern: "**/*.txt" }, "fs.glob"],
    ["layerfs_fs_diff_files", { leftRef: ref, leftPath: "/a", rightRef: ref, rightPath: "/b" }, "fs.diff_files"],
    ["layerfs_fs_blame", { ref, path: "/file.txt" }, "fs.blame"],
    ["layerfs_fs_write", { branch: "main", path: "/file.txt", content: "new" }, "fs.write"],
    ["layerfs_fs_edit", { branch: "main", path: "/file.txt", oldText: "old", newText: "new" }, "fs.edit"],
    ["layerfs_fs_apply", { branch: "main", changes: [{ op: "write", path: "/file.txt", content: "new" }] }, "fs.apply"],
    ["layerfs_fs_mkdir", { branch: "main", path: "/dir" }, "fs.mkdir"],
    ["layerfs_fs_rm", { branch: "main", path: "/file.txt" }, "fs.rm"],
    ["layerfs_fs_cp", { branch: "main", from: "/a", to: "/b" }, "fs.cp"],
    ["layerfs_fs_mv", { branch: "main", from: "/b", to: "/c" }, "fs.mv"],
    ["layerfs_sandbox_create", {}, "sandbox.create"],
    ["layerfs_sandbox_list_workspaces", { sandbox: "1" }, "sandbox.list_workspaces"],
    ["layerfs_workspace_exec", { ...workspace, command: "exit 0" }, "workspace.exec"],
    ["layerfs_workspace_commit", workspace, "workspace.commit"],
    ["layerfs_backend_get_history", {}, "backend.store.project.get_history"],
    ["layerfs_backend_get_storage", {}, "backend.store.project.get_storage"],
    ["layerfs_backend_get_comparable", { ref: "a".repeat(64) }, "backend.store.project.get_history"],
    ["layerfs_backend_list_overwritten", { layer: "a".repeat(64) }, "backend.store.project.list_overwritten"],
    ["layerfs_backend_list_commit_storage", { branch: "main" }, "backend.store.branch.list_commit_storage"],
    ["layerfs_backend_fs_get_storage", { ref, path: "/file.txt" }, "backend.store.fs.get_storage"],
    ["layerfs_backend_fs_list_revisions", { ref, path: "/file.txt" }, "backend.store.fs.list_revisions"],
    ["layerfs_backend_workspace_status", workspace, "backend.daemon.workspace.status"],
    ["layerfs_backend_workspace_list_changes", workspace, "backend.daemon.workspace.list_changes"],
    ["layerfs_backend_workspace_get_storage", workspace, "backend.daemon.workspace.get_storage"],
    ["layerfs_backend_process_start", { ...workspace, command: "sleep 1" }, "backend.daemon.process.start"],
    ["layerfs_backend_process_list", {}, "backend.daemon.process.list"],
    ["layerfs_backend_process_cancel", { process: "proc_1_ab" }, "backend.daemon.process.cancel"],
    ["layerfs_backend_sandbox_get_log", { sandbox: "1" }, "backend.daemon.sandbox.get_log"],
    ["layerfs_workspace_unmount", workspace, "workspace.unmount"],
    ["layerfs_sandbox_close", { sandbox: "1" }, "sandbox.close"],
  ];
  for (const [name, args, op] of cases) {
    const before = server.calls().length;
    const result = await call(server, name, args);
    assert.ok(result.content?.[0]?.text, `${name} returned no content`);
    assert.ok(server.calls().slice(before).some(([called]) => called === op), `${name} did not call ${op}: ${JSON.stringify(server.calls().slice(before))}`);
  }
  await server.stop();
});

test("Workspace handles come from mount/list and EOF unmounts only server-created Workspaces", async (t) => {
  const server = startServer(t);
  await open(server);
  const foreign = await call(server, "layerfs_workspace_exec", {
    workspace: { sandbox: "1", path: "/workspaces/foreign" }, command: "exit 0",
  });
  assert.equal(foreign.isError, true);
  assert.equal(payload(foreign).code, "InvalidInput");
  const mounted = payload(await call(server, "layerfs_workspace_mount", { sandbox: "1", branch: "main" }));
  const executed = payload(await call(server, "layerfs_workspace_exec", {
    workspace: { sandbox: mounted.sandbox, path: mounted.path }, command: "exit 0",
  }));
  assert.equal(executed.exitCode, 0);
  const listed = payload(await call(server, "layerfs_sandbox_list_workspaces", { sandbox: "1" }));
  assert.equal(listed.items.length, 0);
  await server.stop();
  const calls = server.calls();
  assert.ok(calls.some(([op, args]) => op === "workspace.unmount" && args.path === mounted.path && args.discard === false));
  assert.equal(calls.filter(([op]) => op === "store.close").length, 1);
});

test("events returns its snapshot, emits notifications, and cancellation unsubscribes", async (t) => {
  const server = startServer(t);
  await open(server);
  const snapshotNotification = server.notification("notifications/layerfs_snapshot");
  const eventNotification = server.notification("notifications/layerfs_event");
  server.send({ jsonrpc: "2.0", id: 100, method: "tools/call", params: { name: "layerfs_events", arguments: {} } });
  const snapshot = await snapshotNotification;
  assert.equal(snapshot.params.cursor, 0);
  assert.deepEqual(snapshot.params.snapshot.branches.map((branch) => branch.name), ["main", "cart"]);
  await call(server, "layerfs_fs_write", { branch: "main", path: "/event.txt", content: "event" });
  const notification = await eventNotification;
  assert.equal(notification.params.event.type, "fs.committed");
  server.notify("notifications/cancelled", { requestId: 100 });
  for (let waited = 0; waited < 5_000 && !server.calls().some(([op]) => op === "events.unsubscribe"); waited += 20) {
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  assert.ok(server.calls().some(([op]) => op === "events.unsubscribe"));
  await server.stop();
});

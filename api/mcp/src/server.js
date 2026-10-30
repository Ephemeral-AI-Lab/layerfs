#!/usr/bin/env node
// LayerFS over MCP stdio. Every operation delegates to the public Node SDK;
// this process owns only the client lifecycle and MCP/JSON adaptation.
import { LayerFS, LayerFSError } from "@layerfs/agent";
import { LayerFSBackend } from "@layerfs/backend";
import { McpServer } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import { z } from "zod";

const MAX = Number.MAX_SAFE_INTEGER;
const integer = z.number().int().min(0).max(MAX);
const positive = integer.min(1);
const text = z.string();
const branch = text.regex(/^[a-z0-9](?:[a-z0-9_.-]{0,61}[a-z0-9])?$/, "invalid Branch name")
  .refine((name) => Buffer.byteLength(name) <= 63, "Branch name exceeds 63 bytes");
const paging = { pageSize: positive.max(1000).default(50), pagePos: integer.default(0) };
const workspace = z.strictObject({ sandbox: text, path: text });
const deploy = z.strictObject({
  docker: text.optional(), image: text.optional(), volume: text.optional(), daemon: text.optional(),
  containerPrefix: text.optional(), daemonHost: text.optional(),
});
const policyFields = {
  smallFileThreshold: integer.optional(), wholeFileDepth: integer.optional(),
  chunkDepth: integer.optional(), metadataDepth: integer.optional(),
};
const options = z.strictObject({
  home: text.optional(), trace: text.optional(),
  storePolicy: z.union([text, z.strictObject(policyFields)]).optional(),
  storePath: text.optional(), deploy: deploy.optional(),
});
const diff = { cursor: text.nullish(), hunks: z.boolean().optional() };
const object = (shape) => z.strictObject(shape);
const pageArgs = (input) => [input.pageSize, input.pagePos];
const diffArgs = (input) => [input.pageSize, input.pagePos, { cursor: input.cursor, hunks: input.hunks }];
const workspaceKey = (ws) => `${ws.sandbox}\0${ws.path}`;

function encode(value) {
  if (Buffer.isBuffer(value) || value instanceof Uint8Array) {
    return { $layerfs: "bytes", base64: Buffer.from(value).toString("base64") };
  }
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, encode(item)]));
  }
  return value;
}

function success(value) {
  const encoded = encode(value);
  return { content: [{ type: "text", text: JSON.stringify(encoded) }], structuredContent: encoded };
}

function failure(error) {
  const refusal = error instanceof LayerFSError
    ? { code: error.code, message: error.message, detail: encode(error.detail ?? null) }
    : { code: "Failed", message: error instanceof Error ? error.message : String(error), detail: null };
  return { content: [{ type: "text", text: JSON.stringify(refusal) }], isError: true };
}

class Session {
  client = null;
  backend = null;
  lastClient = null;
  allowed = new Map();
  owned = new Set();
  opening = false;

  require() {
    if (!this.client) throw new Error("no LayerFS client is open; call layerfs_open or layerfs_init first");
    return this.client;
  }

  operator() {
    this.require();
    return this.backend;
  }

  async establish(factory) {
    if (this.client || this.opening) throw new Error("a LayerFS client is already open; call layerfs_close first");
    this.opening = true;
    try {
      const client = await factory();
      this.client = client;
      this.backend = LayerFSBackend.of(client);
      this.lastClient = client;
      this.allowed = new Map();
      this.owned = new Set();
      return client;
    } finally {
      this.opening = false;
    }
  }

  results(after) {
    if (!this.lastClient) throw new Error("no LayerFS client has been opened");
    return this.lastClient.results(after);
  }

  workspace(ws) {
    const key = workspaceKey(ws);
    const known = this.allowed.get(key);
    if (!known) {
      throw new LayerFSError("InvalidInput", "Workspace must come from layerfs_workspace_mount or layerfs_sandbox_list_workspaces", null);
    }
    return known;
  }

  mounted(ws) {
    const known = { sandbox: ws.sandbox, path: ws.path };
    const key = workspaceKey(known);
    this.allowed.set(key, known);
    this.owned.add(key);
  }

  listed(items) {
    for (const ws of items) {
      const known = { sandbox: ws.sandbox, path: ws.path };
      this.allowed.set(workspaceKey(known), known);
    }
  }

  unmounted(ws) {
    const key = workspaceKey(ws);
    this.allowed.delete(key);
    this.owned.delete(key);
  }

  async close() {
    const client = this.client;
    if (!client) return;
    let firstError;
    for (const key of [...this.owned]) {
      const ws = this.allowed.get(key);
      if (!ws) continue;
      try {
        await client.workspace.unmount(ws, false, false);
        this.unmounted(ws);
      } catch (error) {
        firstError ??= error;
      }
    }
    this.client = null;
    this.backend = null;
    try {
      await client.close();
    } catch (error) {
      firstError ??= error;
    }
    if (firstError) throw firstError;
  }
}

const server = new McpServer({ name: "layerfs", version: "1.1.0" });
const session = new Session();

function tool(name, description, schema, operation) {
  server.registerTool(name, { description, inputSchema: schema }, async (input, context) => {
    try {
      return await operation(input, context);
    } catch (error) {
      return failure(error);
    }
  });
}

tool("layerfs_init", "Initialize a LayerFS project and open its one server client", object({
  name: branch, path: text, timeoutMs: positive.optional(), options: options.optional(),
}), async ({ name, path, timeoutMs, options: given }) => {
  const client = await session.establish(() => LayerFS.init(name, path, timeoutMs, given));
  return success({ created: client.created, client: { projectName: client.projectName } });
});

tool("layerfs_open", "Open an existing LayerFS project as the one server client", object({
  name: branch, options: options.optional(),
}), async ({ name, options: given }) => {
  const client = await session.establish(() => LayerFS.open(name, given));
  return success({ client: { projectName: client.projectName } });
});

tool("layerfs_close", "Unmount server-created Workspaces and close the client; idempotent", object({}), async () => {
  await session.close();
  return success({ closed: true });
});

tool("layerfs_results", "Read the client's bounded per-call result ring", object({
  after: integer.default(0),
}), async ({ after }) => success(session.results(after)));

tool("layerfs_events", "Subscribe to same-client events; emits notifications/layerfs_event", object({}), async (_input, context) => {
  const subscription = await session.require().events.subscribe();
  const { signal, notify } = context.mcpReq;
  const stop = () => subscription.close().catch((error) => console.error(`layerfs event unsubscribe: ${error.message}`));
  signal.addEventListener("abort", stop, { once: true });
  try {
    await notify({
      method: "notifications/layerfs_snapshot",
      params: encode({ cursor: subscription.cursor, snapshot: subscription.snapshot }),
    });
    for await (const event of subscription) {
      if (signal.aborted) break;
      await notify({ method: "notifications/layerfs_event", params: { event: encode(event) } });
    }
  } finally {
    signal.removeEventListener("abort", stop);
    if (signal.aborted) await stop();
  }
  return success({ closed: true });
});

tool("layerfs_project_list_branches", "List project Branches", object(paging), async (input) =>
  success(await session.require().project.list_branches(...pageArgs(input))));
tool("layerfs_project_list_sandboxes", "List project Sandboxes", object(paging), async (input) =>
  success(await session.require().project.list_sandboxes(...pageArgs(input))));
tool("layerfs_branch_list_commits", "List Commits on a Branch", object({ branch, ...paging }), async (input) =>
  success(await session.require().branch.list_commits(input.branch, ...pageArgs(input))));
tool("layerfs_branch_graph", "Read Branch history graph", object({ branch, ...paging }), async (input) =>
  success(await session.require().branch.graph(input.branch, ...pageArgs(input))));
tool("layerfs_branch_changed_paths", "List paths changed by a Commit", object({ commit: text, ...paging, cursor: text.nullish() }), async (input) =>
  success(await session.require().branch.changed_paths(input.commit, input.pageSize, input.pagePos, { cursor: input.cursor })));
tool("layerfs_branch_diff_commits", "Diff two Commits", object({ from: text, to: text, ...paging, ...diff }), async (input) =>
  success(await session.require().branch.diff_commits(input.from, input.to, ...diffArgs(input))));
tool("layerfs_branch_diff_branches", "Diff two Branches", object({ from: branch, to: branch, ...paging, ...diff }), async (input) =>
  success(await session.require().branch.diff_branches(input.from, input.to, ...diffArgs(input))));
tool("layerfs_branch_fork", "Fork a Branch from a reference", object({ name: branch, from: text.default("main") }), async ({ name, from }) =>
  success(await session.require().branch.fork(name, from)));
tool("layerfs_branch_merge", "Merge two Branches into a named Branch", object({
  branch, with: branch, name: branch, message: text.optional(),
  units: z.array(z.union([text, object({ name: text, patterns: z.array(text) })])).optional(),
}), async (input) => success(await session.require().branch.merge(input.branch, input.with, input.name, input.message, input.units)));
tool("layerfs_branch_publish", "Publish a Branch Commit as a Layer", object({ branch, commit: text.optional() }), async (input) =>
  success(await session.require().branch.publish(input.branch, input.commit)));

tool("layerfs_fs_read", "Read a file at a Branch, Commit, or Layer reference", object({
  ref: text, path: text, startLine: positive.default(1), endLine: positive.optional(),
  binary: z.boolean().default(false), offset: text.nullish(),
}), async (input) => success(await session.require().fs.read(input.ref, input.path, input.startLine, input.endLine, {
  binary: input.binary, offset: input.offset,
})));
tool("layerfs_fs_list", "List a directory at a reference", object({ ref: text, path: text.default("/"), ...paging }), async (input) =>
  success(await session.require().fs.list(input.ref, input.path, ...pageArgs(input))));
tool("layerfs_fs_stat", "Stat a path at a reference", object({ ref: text, path: text }), async (input) =>
  success(await session.require().fs.stat(input.ref, input.path)));
tool("layerfs_fs_grep", "Search text at a reference", object({ ref: text, pattern: text, path: text.default("/"), ...paging }), async (input) =>
  success(await session.require().fs.grep(input.ref, input.pattern, input.path, ...pageArgs(input))));
tool("layerfs_fs_glob", "Match paths at a reference", object({ ref: text, pattern: text, path: text.default("/"), ...paging }), async (input) =>
  success(await session.require().fs.glob(input.ref, input.pattern, input.path, ...pageArgs(input))));
tool("layerfs_fs_diff_files", "Diff two files at references", object({ leftRef: text, leftPath: text, rightRef: text, rightPath: text }), async (input) =>
  success(await session.require().fs.diff_files(input.leftRef, input.leftPath, input.rightRef, input.rightPath)));
tool("layerfs_fs_blame", "Blame file lines at a reference", object({ ref: text, path: text, fromLine: positive.default(1) }), async (input) =>
  success(await session.require().fs.blame(input.ref, input.path, input.fromLine)));
tool("layerfs_fs_write", "Write UTF-8 text or explicit base64 bytes and Commit", object({
  branch, path: text, content: text.optional(), base64: text.optional(), message: text.optional(), mode: integer.optional(),
}).superRefine((input, context) => {
  if ((input.content === undefined) === (input.base64 === undefined)) {
    context.addIssue({ code: "custom", message: "exactly one of content or base64 is required" });
  }
}), async (input) => {
  const content = input.content === undefined ? Buffer.from(input.base64, "base64") : input.content;
  return success(await session.require().fs.write(input.branch, input.path, content, { message: input.message, mode: input.mode }));
});
tool("layerfs_fs_edit", "Replace file text and Commit", object({ branch, path: text, oldText: text, newText: text, message: text.optional() }), async (input) =>
  success(await session.require().fs.edit(input.branch, input.path, input.oldText, input.newText, input.message)));
const writeChange = object({
  op: z.literal("write"), path: text, content: text.optional(), base64: text.optional(), mode: integer.optional(),
});
const change = z.discriminatedUnion("op", [
  writeChange,
  object({ op: z.literal("edit"), path: text, oldText: text, newText: text }),
  object({ op: z.literal("mkdir"), path: text, mode: integer.optional() }),
  object({ op: z.literal("chmod"), path: text, mode: integer }),
  object({ op: z.literal("rm"), path: text }),
  object({ op: z.literal("cp"), from: text, to: text }),
  object({ op: z.literal("mv"), from: text, to: text }),
]);
const changes = z.array(change).superRefine((items, context) => {
  items.forEach((item, index) => {
    if (item.op !== "write") return;
    if ((item.content === undefined) === (item.base64 === undefined)) {
      context.addIssue({ code: "custom", path: [index], message: "exactly one of content or base64 is required" });
    }
  });
});
const applyChange = (item) => {
  if (item.op !== "write" || item.base64 === undefined) return item;
  const { base64, ...rest } = item;
  return { ...rest, bytes: Buffer.from(base64, "base64") };
};
tool("layerfs_fs_apply", "Apply ordered SDK changes and Commit", object({
  branch, changes, message: text.optional(),
}), async (input) => success(await session.require().fs.apply(input.branch, input.changes.map(applyChange), input.message)));
tool("layerfs_fs_mkdir", "Create a directory recursively and Commit", object({ branch, path: text, message: text.optional(), mode: integer.optional() }), async (input) =>
  success(await session.require().fs.mkdir(input.branch, input.path, { message: input.message, mode: input.mode })));
tool("layerfs_fs_rm", "Remove a path and Commit", object({ branch, path: text, message: text.optional() }), async (input) =>
  success(await session.require().fs.rm(input.branch, input.path, input.message)));
tool("layerfs_fs_cp", "Copy a path and Commit", object({ branch, from: text, to: text, message: text.optional() }), async (input) =>
  success(await session.require().fs.cp(input.branch, input.from, input.to, input.message)));
tool("layerfs_fs_mv", "Move a path and Commit", object({ branch, from: text, to: text, message: text.optional() }), async (input) =>
  success(await session.require().fs.mv(input.branch, input.from, input.to, input.message)));

tool("layerfs_sandbox_create", "Create a Sandbox in the opened project", object({ image: text.optional() }), async ({ image }) =>
  success(await session.require().sandbox.create({ image })));
tool("layerfs_sandbox_list_workspaces", "List mounted Workspaces in a Sandbox", object({ sandbox: text, ...paging }), async (input) => {
  const result = await session.require().sandbox.list_workspaces(input.sandbox, ...pageArgs(input));
  session.listed(result.items);
  return success(result);
});
tool("layerfs_sandbox_close", "Close a Sandbox", object({ sandbox: text }), async ({ sandbox }) =>
  success(await session.require().sandbox.close(sandbox)));
tool("layerfs_workspace_mount", "Mount a new Workspace in a running Sandbox", object({ sandbox: text, branch }), async ({ sandbox, branch: name }) => {
  const result = await session.require().workspace.mount(sandbox, name);
  session.mounted(result);
  return success(result);
});
tool("layerfs_workspace_exec", "Execute Bash only inside a selected Sandbox Workspace", object({ workspace, command: text, message: text.optional() }), async (input) =>
  success(await session.require().workspace.exec(session.workspace(input.workspace), input.command, input.message)));
tool("layerfs_workspace_commit", "Commit a selected Workspace", object({
  workspace, message: text.optional(), force: z.boolean().optional(),
  options: object({ author: text.optional(), labels: z.array(text).optional() }).optional(),
}), async (input) => success(await session.require().workspace.commit(
  session.workspace(input.workspace), input.message, input.force, input.options,
)));
tool("layerfs_workspace_unmount", "Unmount a selected Workspace", object({
  workspace, discard: z.boolean().optional(), force: z.boolean().optional(),
}), async (input) => {
  const ws = session.workspace(input.workspace);
  const result = await session.require().workspace.unmount(ws, input.discard, input.force);
  session.unmounted(ws);
  return success(result);
});

tool("layerfs_backend_get_history", "Get project history through the backend SDK", object({ since: text.optional() }), async ({ since }) =>
  success(await session.operator().store.project.get_history(since)));
tool("layerfs_backend_get_storage", "Get fresh project Store statistics", object({ options: object({ commits: z.boolean().optional() }).optional() }), async ({ options: given }) =>
  success(await session.operator().store.project.get_storage(given)));
tool("layerfs_backend_get_comparable", "Resolve a saved Commit or Layer to a comparable Commit", object({ ref: text }), async ({ ref }) =>
  success(await session.operator().store.project.get_comparable(ref)));
tool("layerfs_backend_list_overwritten", "List overwritten Layers", object({ layer: text, ...paging }), async (input) =>
  success(await session.operator().store.project.list_overwritten(input.layer, ...pageArgs(input))));
tool("layerfs_backend_list_commit_storage", "List per-Commit storage statistics", object({ branch, ...paging }), async (input) =>
  success(await session.operator().store.branch.list_commit_storage(input.branch, ...pageArgs(input))));
tool("layerfs_backend_fs_get_storage", "Get file storage at a reference", object({ ref: text, path: text }), async (input) =>
  success(await session.operator().store.fs.get_storage(input.ref, input.path)));
tool("layerfs_backend_fs_list_revisions", "List file revisions at a reference", object({ ref: text, path: text, ...paging }), async (input) =>
  success(await session.operator().store.fs.list_revisions(input.ref, input.path, ...pageArgs(input))));
tool("layerfs_backend_workspace_status", "Get live Workspace status", object({ workspace }), async (input) =>
  success(await session.operator().daemon.workspace.status(session.workspace(input.workspace))));
tool("layerfs_backend_workspace_list_changes", "List live Workspace changes", object({ workspace, ...paging }), async (input) =>
  success(await session.operator().daemon.workspace.list_changes(session.workspace(input.workspace), ...pageArgs(input))));
tool("layerfs_backend_workspace_get_storage", "Get live Workspace storage", object({ workspace }), async (input) =>
  success(await session.operator().daemon.workspace.get_storage(session.workspace(input.workspace))));
tool("layerfs_backend_process_start", "Start a background process in a Workspace", object({ workspace, command: text }), async (input) =>
  success(await session.operator().daemon.process.start(session.workspace(input.workspace), input.command)));
tool("layerfs_backend_process_list", "List background processes", object(paging), async (input) =>
  success(await session.operator().daemon.process.list(...pageArgs(input))));
tool("layerfs_backend_process_cancel", "Cancel a background process by id", object({ process: text }), async ({ process: id }) =>
  success(await session.operator().daemon.process.cancel(id)));
tool("layerfs_backend_sandbox_get_log", "Read a Sandbox daemon log", object({ sandbox: text, lines: integer.min(1).max(2000).default(200) }), async ({ sandbox, lines }) =>
  success(await session.operator().daemon.sandbox.get_log(sandbox, { lines })));

const handle = serveStdio(() => server, { onerror: (error) => console.error(`layerfs MCP: ${error.message}`) });
let shutdown;
async function cleanup(signal) {
  if (!shutdown) {
    shutdown = (async () => {
      try {
        await session.close();
      } catch (error) {
        console.error(`layerfs cleanup: ${error.message}`);
        process.exitCode = 1;
      }
      await handle.close();
      if (signal === "SIGINT" && process.exitCode === undefined) process.exitCode = 130;
    })();
  }
  await shutdown;
}
process.stdin.once("end", () => void cleanup());
process.once("SIGINT", () => void cleanup("SIGINT"));

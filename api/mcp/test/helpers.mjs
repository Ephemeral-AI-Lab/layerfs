// Shared setup for the MCP tests: a Disposable temp home on TMPDIR (ext4, not
// /tmp tmpfs), a real server spawned over stdio, and an SDK Client.
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

export const ROOT = fileURLToPath(new URL("..", import.meta.url));
export const SERVER = join(ROOT, "src/server.js");
export const REPO = fileURLToPath(new URL("../../..", import.meta.url));
export const CLI = process.env.LAYERFS_CLI || join(REPO, "target/debug/layerfs");
export const ADDON = join(REPO, "api/node/layerfs.node");

/** A fresh directory under TMPDIR; without one, under the worktree's ignored
 * `target/mcp-tests` (ext4), because /tmp is tmpfs and cannot carry SQLite
 * locks between kernels. */
export function temp(prefix) {
  const base = process.env.TMPDIR || join(REPO, "target", "mcp-tests");
  mkdirSync(base, { recursive: true, mode: 0o700 });
  return mkdtempSync(join(base, `layerfs-mcp-${prefix}-`));
}

/** `{ home, source, out }` under one temp root; `source` holds `files`. */
export function workspace(prefix, files = {}) {
  const root = temp(prefix);
  const home = join(root, "home");
  const source = join(root, "source");
  const out = join(root, "out");
  mkdirSync(home, { mode: 0o700 });
  mkdirSync(source);
  for (const [path, content] of Object.entries(files)) {
    const full = join(source, path);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, content);
  }
  return { root, home, source, out, remove: () => rmSync(root, { recursive: true, force: true }) };
}

/** A connected SDK client over a spawned `node src/server.js`. */
export async function connect({ home, profile, scope, extra = [] }) {
  const args = [SERVER, "--home", home];
  if (profile) args.push("--profile", profile);
  if (scope) {
    args.push("--scope-project", scope.project, "--scope-folder", scope.folder);
    if (scope.branch) args.push("--scope-branch", scope.branch);
  }
  args.push(...extra);
  const transport = new StdioClientTransport({
    command: process.execPath,
    args,
    env: { ...process.env, LAYERFS_CLI: CLI },
    stderr: "pipe",
  });
  let log = "";
  transport.stderr.on("data", (chunk) => {
    log += chunk.toString();
  });
  const client = new Client({ name: "layerfs-mcp-test", version: "1.0.0" });
  await client.connect(transport);
  return {
    client,
    transport,
    log: () => log,
    close: async () => {
      await client.close();
    },
  };
}

/** One tool call; `progress` holds the notifications the client received. */
export async function call(client, name, args, { progress = false } = {}) {
  const seen = [];
  const result = await client.callTool(
    { name, arguments: args },
    undefined,
    progress ? { onprogress: (entry) => seen.push(entry) } : undefined,
  );
  return { result, progress: seen };
}

/** The JSON value of a successful result. */
export function value(result) {
  if (result.isError) throw new Error(`expected a result, got isError: ${JSON.stringify(result.content)}`);
  return JSON.parse(result.content[0].text);
}

/** The first text block of a result, parsed. */
export function text(result) {
  return result.content.map((block) => block.text).join("\n");
}

/** A `ps` check: true while the pid is a live process. */
export function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

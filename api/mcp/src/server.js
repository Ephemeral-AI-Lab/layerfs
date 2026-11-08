#!/usr/bin/env node
// layerfs-mcp: the LayerFS call table over the Model Context Protocol, stdio,
// one tool per call. A thin adapter: it builds tools from the Node package's
// `table`, passes arguments and a fixed `scope` to the package, and maps the
// answer or the refusal back. It adds no retry, replay, Commit or unmount.
//
// stdout carries the protocol only; every log line goes to stderr. Closing
// stdin or SIGTERM closes this process's idle Stores and exits 0.

import { readFileSync, writeSync } from "node:fs";
import process from "node:process";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  CancelledNotificationSchema,
  ErrorCode,
  ListToolsRequestSchema,
  McpError,
} from "@modelcontextprotocol/sdk/types.js";
import layerfs, { LayerFSError } from "layerfs";
import { errorResult } from "./errors.js";
import { exportRefusal, resolveRoots } from "./exportroots.js";
import { PROFILES, check as checkProfiles, tools as toolsOfProfile } from "./profiles.js";
import { progressReporter } from "./progress.js";
import { toolsOf } from "./tools.js";
import { InvalidArguments, encodeResult, readArguments } from "./values.js";

const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
const VERSION = pkg.version;
const GRACE_MS = 2000;

const USAGE = `usage: layerfs-mcp [--profile work|admin] [--home DIR]
                  [--scope-project <store>/<project> --scope-folder DIR [--scope-branch NAME]]
                  [--export-root DIR]...
                  [--help] [--version]

  --profile work   the calls an agent works with (default)
  --profile admin  every call, including machines, Stores, backups and
                   project create/remove
  --home DIR       the LayerFS home (default: LAYERFS_HOME, else ~/.layerfs)
  --scope-project  the folder scope's project, with --scope-folder
  --scope-folder   the folder scope; the library checks every scoped call
  --scope-branch   the scope's Branch namespace; Branch and file calls need it
  --export-root    a directory project_export may write under; repeat for more.
                   With none, project_export is refused.

while serving, stdout carries the MCP protocol only; logs go to stderr.
`;

function log(line) {
  try {
    process.stderr.write(`layerfs-mcp: ${line}\n`);
  } catch {
    // a closed stderr must not end the server
  }
}

function usageError(message) {
  process.stderr.write(`layerfs-mcp: ${message}\n${USAGE}`);
  process.exit(2);
}

/** `{ profile, home, scope }` from the command line, or a usage exit. */
function flagsOf(argv) {
  const flags = { profile: "work", home: undefined, scopeProject: undefined, scopeFolder: undefined, scopeBranch: undefined, exportRoots: [] };
  for (let i = 0; i < argv.length; i += 1) {
    let word = argv[i];
    let inline;
    const equals = word.indexOf("=");
    if (word.startsWith("--") && equals > 0) {
      inline = word.slice(equals + 1);
      word = word.slice(0, equals);
    }
    const take = () => {
      if (inline !== undefined) return inline;
      i += 1;
      if (i >= argv.length) usageError(`${word} needs a value`);
      return argv[i];
    };
    switch (word) {
      case "--profile":
        flags.profile = take();
        break;
      case "--home":
        flags.home = take();
        if (flags.home === "") usageError("--home needs a directory");
        break;
      case "--scope-project":
        flags.scopeProject = take();
        break;
      case "--scope-folder":
        flags.scopeFolder = take();
        break;
      case "--scope-branch":
        flags.scopeBranch = take();
        break;
      case "--export-root":
        flags.exportRoots.push(take());
        break;
      case "--help":
      case "-h":
        writeSync(1, USAGE);
        process.exit(0);
        break;
      case "--version":
        writeSync(1, `${VERSION}\n`);
        process.exit(0);
        break;
      default:
        usageError(`unknown argument "${word}"`);
    }
  }
  if (!PROFILES.includes(flags.profile)) usageError(`--profile must be one of ${PROFILES.join(", ")}`);
  const project = flags.scopeProject !== undefined;
  const folder = flags.scopeFolder !== undefined;
  if (project !== folder) usageError("--scope-project and --scope-folder must be given together");
  if (flags.scopeBranch !== undefined && !project) usageError("--scope-branch needs --scope-project and --scope-folder");
  if (flags.scopeBranch !== undefined && flags.scopeBranch === "") usageError("--scope-branch needs a name");
  if (project && flags.scopeFolder === "") usageError("--scope-folder needs a directory");
  let exportRoots;
  try {
    exportRoots = resolveRoots(flags.exportRoots);
  } catch (error) {
    usageError(error.message);
  }
  const scope = project
    ? { project: flags.scopeProject, folder: flags.scopeFolder, ...(flags.scopeBranch === undefined ? {} : { branch: flags.scopeBranch }) }
    : undefined;
  return { profile: flags.profile, home: flags.home, scope, exportRoots };
}

const flags = flagsOf(process.argv.slice(2));
const profileProblems = checkProfiles(layerfs.table);
if (profileProblems.length > 0) {
  for (const problem of profileProblems) log(`profile problem: ${problem}`);
  process.exit(1);
}
const specs = toolsOfProfile(layerfs.table, flags.profile);
const tools = toolsOf(specs);
const byName = new Map(tools.map((tool) => [tool.name, tool]));
const client = layerfs.client({ home: flags.home, scope: flags.scope });

const inflight = new Set();
let stopping = false;

/** One layerfs call with its positionals and its options object. */
function callTool(tool, { positionals, options }) {
  const family = client[tool.spec.family];
  const call = family?.[tool.spec.call];
  if (typeof call !== "function") throw new Error(`the layerfs package has no ${tool.spec.family}.${tool.spec.call}`);
  return call(...positionals, options);
}

const server = new Server({ name: "layerfs-mcp", version: VERSION }, { capabilities: { tools: {} } });
server.onerror = (error) => log(`server error: ${error?.message ?? error}`);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: tools.map(({ name, description, inputSchema }) => ({ name, description, inputSchema })),
}));

server.setRequestHandler(CallToolRequestSchema, async (request, extra) => {
  if (stopping) throw new McpError(ErrorCode.InvalidRequest, "the server is stopping; no new call is accepted");
  const { name, arguments: given } = request.params;
  const tool = byName.get(name);
  if (!tool) throw new McpError(ErrorCode.InvalidParams, `unknown tool "${name}"`);
  let read;
  try {
    read = readArguments(tool.spec, given);
  } catch (error) {
    if (error instanceof InvalidArguments) throw new McpError(ErrorCode.InvalidParams, error.message);
    throw error;
  }
  if (tool.spec.family === "project" && tool.spec.call === "export") {
    // the operator's allowlist, before anything is attempted; a missing argument position fails closed
    const at = tool.spec.args.findIndex((arg) => arg.name === "destination");
    const reason = exportRefusal(read.positionals[at], flags.exportRoots);
    if (reason !== null) {
      log(`${name} refused InvalidInput (export roots)`);
      return errorResult({ code: "InvalidInput", message: reason, detail: null });
    }
  }
  const reporter = progressReporter(extra.sendNotification, extra._meta?.progressToken);
  if (reporter && tool.spec.options.some((option) => option.name === "on_progress")) {
    read.options.on_progress = (event) => reporter.on_progress(event);
  }
  const started = Date.now();
  let running;
  try {
    running = callTool(tool, read);
  } catch (error) {
    log(`${name} internal error before the call: ${error?.message ?? error}`);
    throw new McpError(ErrorCode.InternalError, `${name}: ${error?.message ?? error}`);
  }
  inflight.add(running);
  try {
    const answer = await running;
    if (reporter) await reporter.finish();
    log(`${name} ok ${Date.now() - started}ms`);
    return { content: [{ type: "text", text: JSON.stringify(encodeResult(answer)) }] };
  } catch (error) {
    if (reporter) await reporter.finish();
    if (error instanceof LayerFSError || error?.name === "LayerFSError") {
      log(`${name} refused ${error.code} ${Date.now() - started}ms`);
      return errorResult(error);
    }
    log(`${name} internal error ${Date.now() - started}ms: ${error?.message ?? error}`);
    throw new McpError(ErrorCode.InternalError, `${name}: ${error?.message ?? error}`);
  } finally {
    inflight.delete(running);
  }
});

server.setNotificationHandler(CancelledNotificationSchema, (notification) => {
  const id = notification.params?.requestId;
  log(`cancel of request ${id ?? "(unnamed)"}: the native call cannot be cancelled and runs to its end; nothing is retried`);
});

const transport = new StdioServerTransport();
transport.onerror = (error) => log(`transport error: ${error?.message ?? error}`);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** No implicit Commit or unmount: only idle Stores are closed, then exit 0. */
async function stop(reason) {
  if (stopping) return;
  stopping = true;
  log(`stopping (${reason})`);
  const deadline = Date.now() + GRACE_MS;
  while (inflight.size > 0 && Date.now() < deadline) await sleep(25);
  if (inflight.size > 0) {
    log(`${inflight.size} call(s) still running: a native call cannot be cancelled; this process ends and cuts the call off; its outcome is unknown; read state before acting again; nothing was retried`);
  }
  try {
    const closed = layerfs.close();
    if (closed > 0) log(`closed ${closed} idle Store(s)`);
  } catch (error) {
    log(`close failed: ${error?.message ?? error}`);
  }
  try {
    await server.close();
  } catch (error) {
    log(`transport close failed: ${error?.message ?? error}`);
  }
  log("exit 0");
  process.exit(0);
}

process.stdin.on("end", () => stop("stdin closed"));
process.stdin.on("close", () => stop("stdin closed"));
process.on("SIGTERM", () => stop("SIGTERM"));
process.on("SIGINT", () => stop("SIGINT"));
process.stdout.on("error", (error) => log(`stdout error: ${error?.message ?? error}`));

await server.connect(transport);
log(
  `layerfs-mcp ${VERSION}: profile ${flags.profile}, ${tools.length} tools, home ${flags.home ?? process.env.LAYERFS_HOME ?? "default"}` +
    (flags.scope ? `, scope ${flags.scope.project} at ${flags.scope.folder}${flags.scope.branch ? ` (branch ${flags.scope.branch})` : ""}` : "") +
    `, export roots ${flags.exportRoots.length === 0 ? "none (project_export is refused)" : flags.exportRoots.join(", ")}`,
);

// runtime.project.start_init(name, folder, { image, timeout }): imports a folder as a
// new project on its own Docker volume, as a background job of this process
// (jobs.js). Copied from the dsh Host's init.js (#428; its demo-scenario and
// simulator branches are dropped). The steps: count the source, make the
// volume, then one Init (LayerFS.init) where the Store lives: in this
// process on Linux, in a container of its own on macOS, in the worker on
// Windows. Init seals the Store and installs it into the volume, starting no
// Sandbox. `timeout` (milliseconds) is LayerFS.init's, passed through
// unchanged: it bounds the Init itself, and when omitted LayerFS.init's
// default (INIT_TIMEOUT_MS, 300 s) applies. One attempt. A failed Init removes what it made (the volume, and
// the home entry unless another project holds the name); an uncertain one
// (`Unknown`, or any failure on Windows) keeps everything and is recorded in
// `<stateDir>/init-failures.jsonl`, never cleaned up on a guess.
import { appendFileSync, existsSync, statSync } from "node:fs";
import { lstat, opendir } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { setTimeout as pause } from "node:timers/promises";
import { LayerFS, LayerFSError } from "@layerfs/agent";
import { invokeBackend } from "@layerfs/agent/internal";
import { deploymentEnv, withEnv } from "../clients.js";
import { callHome, missing } from "../settings.js";
import { NAME, record, saveRecord } from "./list.js";

const WATCH_MS = 250;
const IMAGE = /^sha256:[0-9a-f]{64}$/;
const refuse = (code, message) => new LayerFSError(code, message, null);

/** Counts what Init will import: every entry under `source` (links not followed) and its regular files' bytes. */
export async function scanSource(source, report = () => {}) {
  const counts = { entries: 1, files: 0, bytes: 0 }, dirs = [source];
  while (dirs.length) {
    const dir = dirs.pop();
    for await (const entry of await opendir(dir)) {
      counts.entries++;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) dirs.push(path);
      else if (entry.isFile()) { counts.files++; counts.bytes += (await lstat(path)).size; }
    }
    report({ ...counts });
  }
  return counts;
}

/** Checks a request and names its volume, changing nothing. */
export async function prepare(runtime, name, folder, options = {}, pending = new Set()) {
  const { settings, platform } = runtime;
  const gaps = missing(settings);
  if (gaps.length) throw refuse("InvalidInput", `LayerFS is not configured: set ${gaps.join(" and ")}`);
  if (typeof name !== "string" || !NAME.test(name)) throw refuse("InvalidInput", "a project name is 1-63 lowercase letters, digits, _ . - and starts and ends alphanumeric");
  if (options.image !== undefined && !IMAGE.test(options.image)) throw refuse("InvalidInput", "image is a runtime image digest sha256:<64 hex>");
  if (options.timeout !== undefined && !(Number.isSafeInteger(options.timeout) && options.timeout > 0)) throw refuse("InvalidInput", "timeout must be a positive whole number of milliseconds");
  if (typeof folder !== "string" || !isAbsolute(folder) || !existsSync(folder) || !statSync(folder).isDirectory()) throw refuse("InvalidInput", `${folder} is not an absolute directory`);
  if (pending.has(name) || record(settings, name) || (await platform.homeNames(settings)).includes(name)) throw refuse("Exists", `a LayerFS project named ${name} exists`);
  const created = Date.now();
  return { name, source: folder, volume: `${settings.containerPrefix}${name}-store-${created.toString(36)}`, created, ...(options.image ? { image: options.image } : {}), ...(options.timeout !== undefined ? { timeout: options.timeout } : {}) };
}

/** Runs `making` (LayerFS.init), reading Init's counts every WATCH_MS in this process until it ends. */
async function watching(runtime, name, making, report) {
  let running = true;
  const watch = (async () => {
    while (running) {
      const counts = await invokeBackend("runtime.project.get_init", { project: name, home: callHome(runtime.settings) }, {}).catch(() => (running = false, null));
      if (counts && running) report({ imported: counts });
      if (running) await pause(WATCH_MS);
    }
  })();
  try {
    return await making;
  } finally {
    running = false;
    await watch;
  }
}

/** One prepared Init; `report` sees each step. Answers the project's record. */
export async function runInit(runtime, prepared, report = () => {}) {
  const { settings, platform } = runtime;
  const { timeout, ...project } = prepared; // the record keeps no timeout
  report({ step: "scan" });
  const totals = await scanSource(project.source, (counts) => report({ scanned: counts }));
  report({ step: "volume", totals });
  await platform.init(settings);
  // its owner, for runtime.volume.list: Docker sets a volume's labels only when it creates it
  await platform.docker(settings, ["volume", "create", "--label", "layerfs.role=store", "--label", `layerfs.home=${settings.home}`, "--label", `layerfs.project=${project.name}`, project.volume], { timeout: 30_000 });
  report({ step: "import" });
  let client;
  try {
    // LayerFS.init reads the environment before its first await
    const making = withEnv(deploymentEnv(settings, platform, project), () => LayerFS.init(project.name, platform.path(settings, project.source), timeout));
    // macOS: Init runs in its own container, whose counts this process cannot read
    client = await (platform.name === "macos" ? making : watching(runtime, project.name, making, report));
  } catch (error) {
    if (error?.code === "Unknown" || platform.name === "windows") {
      appendFileSync(join(settings.stateDir, "init-failures.jsonl"), JSON.stringify({ project, platform: platform.name, home: callHome(settings), code: error?.code ?? null, message: error?.message ?? String(error), at: new Date().toISOString() }) + "\n");
      throw error;
    }
    if (error?.code !== "Exists") await platform.removeHome(settings, project.name).catch(() => {});
    await platform.docker(settings, ["volume", "rm", project.volume]).catch(() => {});
    throw error;
  }
  await client.close();
  saveRecord(settings, project);
  return project;
}

// Sandbox management for the LayerFS page (#373): what runs, and the few
// actions on it.
//   one project   its Sandboxes from one inspect.snapshot (which starts none),
//                 each mounted Workspace with its Branch, uncommitted changes,
//                 background processes, and who mounted it (the agents' traces)
//   all projects  Docker, the Store container, free disk, and each project's
//                 Sandbox containers from one `docker ps`, each daemon pinged
//                 (a TCP connect) and its Workspaces from the agents' traces:
//                 no daemon is asked for its state (quickSandboxes)
// A read starts nothing, and waits for a project's Sandboxes only
// trees.sandboxMs: after that the tab shows the quick view (`pending`) while
// the daemons' answer is read on. An
// action is one SDK or Docker call, never repeated;
// one that would cut off an agent's call running in that Workspace is refused.
import { execFile } from "node:child_process";
import { readFileSync, statfsSync } from "node:fs";
import { connect } from "node:net";
import { promisify } from "node:util";
import { buildOf, ensureBox, storeFile, VOLUMES } from "./box.js";
import { isSim, within } from "./layerfs-client.js";
import { frame, readFrames } from "./port/frames.mjs";

const run = promisify(execFile);
/** Workspaces one Sandbox's daemon admits (the SDK's daemon setup, `namespaces`). */
export const CAPACITY = 16;
/** A Workspace with uncommitted changes and no call for this long is flagged. */
export const LEFT_MS = 10 * 60_000;
const DOCKER_MS = 5_000, PING_MS = 1_000, RESTART_MS = 20_000;
const RANK = { down: 0, degraded: 1, ok: 2 };

const docker = (spec, ...args) => run("docker", ["-H", `unix://${spec.docker}`, ...args], { timeout: DOCKER_MS }).then((r) => r.stdout.trim());
const key = (sandbox, path) => `${sandbox}\n${path}`;
const worst = (levels) => levels.reduce((a, b) => (RANK[b] < RANK[a] ? b : a), "ok");
const refuse = (status, code, message) => Object.assign(new Error(message), { status, code });

/**
 * One project's Sandboxes, as its Sandboxes tab draws them: each daemon's
 * Workspaces and processes. When the daemons take longer than
 * trees.sandboxMs, the quick view (`pending: true`) is drawn and the read runs on.
 */
export async function projectRuntime(trees, collector, project, now = Date.now()) {
  {
    const [read, listed] = await Promise.all([within(trees.snapshot(project), trees.sandboxMs), within(trees.processes(project), trees.sandboxMs)]);
    if (read.late || listed.late) {
      const sandboxes = isSim(project) ? [] : await quickSandboxes(collector, project, await containersOf(trees.spec), now);
      return { project: project.name, simulated: isSim(project), sandboxes, gone: [], pending: true, level: worst(sandboxes.map((s) => s.level)) };
    }
    const snap = read.value, processes = listed.value.items ?? [];
    const busy = collector.busyOf(project.name), mounts = collector.mountsOf(project.name);
    const sandboxes = (snap.sandboxes ?? []).map((sb) => {
      const workspaces = sb.workspaces.filter((w) => w.mounted && w.directory).map((w) => {
        const by = mounts[key(sb.id, w.directory)], inUse = busy.has(key(sb.id, w.directory));
        const uncommitted = w.dirty ? Math.max(w.dirty.inodes, w.dirty.entries) : 0;
        const idleMs = by && !inUse ? Math.max(0, now - by.lastAt) : null;
        return {
          path: w.directory, branch: w.branch ?? null, activity: w.activity ?? null, uncommitted,
          stale: w.headMovedSinceMount === true, pendingMerge: !!w.pendingMerge,
          agent: by?.agent ?? null, mountedAt: by?.at ?? null, idleMs, busy: inUse,
          processes: processes.filter((p) => p.sandbox === sb.id && p.path === w.directory).map(({ id, command, status }) => ({ id, command, status })),
          // left with uncommitted changes: worth a look, never acted on by itself
          level: uncommitted && idleMs !== null && idleMs >= LEFT_MS ? "degraded" : "ok",
        };
      });
      const full = workspaces.length >= CAPACITY;
      return { id: sb.id, name: sb.name ?? null, image: sb.image ?? null, capacity: CAPACITY, workspaces, level: worst([full ? "degraded" : "ok", ...workspaces.map((w) => w.level)]) };
    });
    const gone = (snap.gone ?? []).map(({ id, name, image }) => ({ id, name: name ?? null, image: image ?? null }));
    return { project: project.name, simulated: isSim(project), sandboxes, gone, level: worst([...sandboxes.map((s) => s.level), gone.length ? "degraded" : "ok"]) };
  }
}

/**
 * A project's running Sandbox containers (`containers`, from containersOf),
 * each daemon pinged (it accepts a connection within PING_MS), with the
 * Workspaces the agents' traces mounted there. No daemon is asked anything,
 * so a busy or stuck one cannot hold the read; uncommitted changes and
 * processes are not known here.
 */
export async function quickSandboxes(collector, project, containers, now = Date.now()) {
  const mounts = collector.mountsOf(project.name), busy = collector.busyOf(project.name);
  return Promise.all(containers.filter((c) => c.project === project.name && c.running).map(async (c) => {
    const answers = c.port ? await reachable(c.port) : false;
    const workspaces = Object.entries(mounts).filter(([k]) => k.split("\n")[0] === c.sandbox).map(([k, by]) => {
      const inUse = busy.has(k);
      return {
        path: k.split("\n")[1], branch: null, activity: null, uncommitted: 0, stale: false, pendingMerge: false,
        agent: by.agent ?? null, mountedAt: by.at, idleMs: inUse ? null : Math.max(0, now - by.lastAt), busy: inUse, processes: [], level: "ok",
      };
    });
    return { id: c.sandbox, name: c.name, image: c.image || null, capacity: CAPACITY, workspaces, answers, level: answers ? "ok" : "down" };
  }));
}

/** Every project's Sandboxes and the shared parts, as the All projects page draws them. */
export async function overview(spec, bindings, trees, collector) {
  const engine = await engineState(spec);
  const containers = engine.level === "ok" ? await containersOf(spec) : [];
  const store = spec.box ? await storeState(spec, engine) : null;
  const disk = { host: freeBytes(spec.home), vm: store?.level === "ok" ? await vmFree(spec) : null };
  const projects = bindings.projects(), known = new Set(projects.map((p) => p.name));
  const cards = await Promise.all(projects.map(async (p) => {
    const own = containers.filter((c) => c.project === p.name);
    const card = { name: p.name, simulated: isSim(p), containers: own.length, sandboxes: [], workspaces: 0, level: "ok", note: null };
    // a project with nothing running is read from Docker alone: no client, no Store opened
    if (!own.some((c) => c.running) && !card.simulated) return card;
    // the simulator's Sandboxes are its own state, read at once; a real project's from Docker and pings
    let sandboxes;
    try {
      sandboxes = card.simulated ? (await projectRuntime(trees, collector, p)).sandboxes : await quickSandboxes(collector, p, own);
    } catch (error) {
      return { ...card, level: "down", note: error.message };
    }
    card.sandboxes = sandboxes.map((s) => ({ id: s.id, name: s.name, image: s.image, used: s.workspaces.length, capacity: s.capacity, level: s.level }));
    card.workspaces = sandboxes.reduce((n, s) => n + s.workspaces.length, 0);
    card.level = worst(sandboxes.map((s) => (s.level === "ok" && s.workspaces.length >= CAPACITY ? "degraded" : s.level)));
    const silent = sandboxes.filter((s) => s.answers === false);
    card.note = silent.length ? `${silent.map((s) => s.name).join(", ")} does not accept a connection` : null;
    return card;
  }));
  cards.sort((a, b) => RANK[a.level] - RANK[b.level] || b.workspaces - a.workspaces || a.name.localeCompare(b.name));
  // a Sandbox container of no project of this home, or one left stopped
  const leftovers = containers.filter((c) => !known.has(c.project) || !c.running).map(({ name, project, running, status }) => ({ name, project, running, status }));
  // older Sandboxes the startup cleanup kept because they may hold work (migrate.js): yours to commit or discard, then remove
  for (const k of spec.migration?.kept ?? [])
    if (!leftovers.some((l) => l.name === k.container)) leftovers.push({ name: k.sandbox, project: "", running: true, status: `older Sandbox (container ${k.container.slice(0, 12)}), kept: ${k.reason}` });
  if (engine.level === "ok") await nameImages(spec, cards.flatMap((c) => c.sandboxes));
  const sandboxes = cards.reduce((n, c) => n + c.sandboxes.length, 0), workspaces = cards.reduce((n, c) => n + c.workspaces, 0);
  return {
    level: worst([engine.level, store?.level ?? "ok", leftovers.length ? "degraded" : "ok"]),
    // agents' calls running now in any project: a Store restart cuts them off
    engine, store, calls: collector.runningCalls(), disk, projects: cards, leftovers, totals: { projects: cards.length, sandboxes, workspaces },
  };
}

/** One action on a project's Workspace or process: unmount, discard (unmount dropping its changes) or stop. */
export async function projectAction(trees, collector, project, body) {
  const { action } = body;
  if (action === "unmount" || action === "discard") {
    const { sandbox, path } = body;
    if (typeof sandbox !== "string" || typeof path !== "string") throw refuse(400, "invalid-workspace", "a Workspace is { sandbox, path }");
    if (collector.busyOf(project.name).has(key(sandbox, path))) throw refuse(409, "busy", `an agent's call is running in ${path}; wait for it to end`);
    if (action === "discard" && body.confirm !== path) throw refuse(400, "not-confirmed", `confirm with the Workspace's path, ${path}`);
    return trees.use(project, ({ layerfs }) => layerfs.workspace.unmount({ sandbox, path }, action === "discard"));
  }
  if (action === "stop") {
    if (typeof body.process !== "string") throw refuse(400, "invalid-process", "a process is its id");
    return trees.use(project, ({ operator }) => operator.process.cancel(body.process));
  }
  throw refuse(400, "invalid-action", "the action is unmount, discard or stop");
}

/** Starts the Store container when it is stopped or missing; a running one is left alone. */
export async function startStore(spec) {
  if (!spec.box) throw refuse(409, "no-store", "this Host runs no Store container (Linux runs Store calls in-process)");
  const state = await storeState(spec, await engineState(spec));
  if (state.state === "running") throw refuse(409, "running", state.answering ? "layerfs-store is running and answering" : "layerfs-store runs but does not answer; restart it instead");
  if (state.state === "unknown") throw refuse(409, "no-docker", "Docker does not answer; start Docker first");
  await ensureBox(spec);
  return storeState(spec, await engineState(spec));
}

/**
 * Restarts a running Store container: one `docker restart`, then the Host
 * reads its new port and waits for its worker (ensureBox). Every project's
 * Store calls fail while it restarts, so `confirm` must be "restart".
 */
export async function restartStore(spec, body) {
  if (!spec.box) throw refuse(409, "no-store", "this Host runs no Store container (Linux runs Store calls in-process)");
  if (body?.confirm !== "restart") throw refuse(409, "not-confirmed", "a restart cuts off every project's Store calls; confirm it");
  const state = await storeState(spec, await engineState(spec));
  if (state.state === "unknown") throw refuse(409, "no-docker", "Docker does not answer; start Docker first");
  if (state.state !== "running") throw refuse(409, state.state, `layerfs-store is ${state.state}; start it instead`);
  // up to 5 s for the worker to stop, then its start: longer than other Docker calls
  await run("docker", ["-H", `unix://${spec.docker}`, "restart", "-t", "5", spec.box], { timeout: RESTART_MS });
  await ensureBox(spec);
  return storeState(spec, await engineState(spec));
}

/** Removes the stopped leftover containers named in `names`; one `docker rm` each, a running one refused. */
export async function cleanLeftovers(spec, bindings, names) {
  if (!Array.isArray(names) || !names.length || names.some((n) => typeof n !== "string")) throw refuse(400, "invalid-names", "names lists the leftover containers to remove");
  const known = new Set(bindings.projects().map((p) => p.name));
  const left = new Map((await containersOf(spec)).filter((c) => !known.has(c.project) || !c.running).map((c) => [c.name, c]));
  const results = [];
  for (const name of names) {
    const c = left.get(name);
    if (!c) { results.push({ name, ok: false, message: "not a leftover of this home" }); continue; }
    if (c.running) { results.push({ name, ok: false, message: "still running; stop it first" }); continue; }
    try { await docker(spec, "rm", name); results.push({ name, ok: true }); } catch (error) { results.push({ name, ok: false, message: error.stderr?.trim() || error.message }); }
  }
  return { results };
}

async function engineState(spec) {
  if (process.platform === "darwin" && !spec.box && !String(spec.docker).endsWith(".sock")) return { level: "ok", version: "darwin-native" };
  try { return { level: "ok", version: await docker(spec, "version", "--format", "{{.Server.Version}}") }; } catch (error) { return { level: "down", error: error.stderr?.trim() || error.message }; }
}

/** Gives each Sandbox (`{ image }`, an exact digest; one project's may differ) its `imageName`: the
 * image's first tag, else null. One `docker image inspect` for all; an image Docker no longer has stays unnamed. */
export async function nameImages(spec, sandboxes) {
  if (process.platform === "darwin" && !spec.box && !String(spec.docker).endsWith(".sock")) {
    for (const s of sandboxes) s.imageName = null;
    return sandboxes;
  }
  const digests = [...new Set(sandboxes.map((s) => s.image).filter(Boolean))];
  const names = new Map();
  if (digests.length) {
    let out;
    // a missing image fails the command, yet the found ones are on stdout
    try { out = await docker(spec, "image", "inspect", "-f", "{{.Id}}\t{{join .RepoTags \",\"}}", ...digests); } catch (error) { out = String(error.stdout ?? ""); }
    for (const row of out.split("\n").filter(Boolean)) {
      const [id, tags] = row.split("\t");
      names.set(id, tags?.split(",").filter(Boolean)[0] ?? null);
    }
  }
  for (const s of sandboxes) s.imageName = names.get(s.image) ?? null;
  return sandboxes;
}

/** This home's Sandbox containers (named `<prefix><project>-<n>`), running or not. */
async function containersOf(spec) {
  if (process.platform === "darwin" && !spec.box && !String(spec.docker).endsWith(".sock")) return [];
  const out = await docker(spec, "ps", "-a", "--no-trunc", "--filter", "label=layerfs.role=sandbox", "--format", "{{.Names}}\t{{.State}}\t{{.Status}}\t{{.Label \"layerfs.project\"}}\t{{.Label \"layerfs.sandbox\"}}\t{{.Image}}\t{{.Ports}}");
  return out.split("\n").filter(Boolean).map((row) => {
    const [name, state, status, project, sandbox, image, ports] = row.split("\t");
    // the daemon's port, published on the host's loopback
    const port = Number(/127\.0\.0\.1:(\d+)->/.exec(ports ?? "")?.[1]) || null;
    return { name, running: state === "running", status, project, sandbox, image, port };
  }).filter((c) => c.name.startsWith(spec.prefix));
}

async function storeState(spec, engine) {
  if (engine.level !== "ok") return { level: "down", state: "unknown", name: spec.box };
  let row;
  try { row = await docker(spec, "inspect", "-f", '{{.State.Running}}\t{{index .Config.Labels "layerfs.build"}}\t{{.State.StartedAt}}', spec.box); } catch { return { level: "down", state: "missing", name: spec.box }; }
  const [running, build, startedAt] = row.split("\t"), expected = buildOf(spec.bundle);
  if (running !== "true") return { level: "down", state: "stopped", name: spec.box, build };
  const answering = await ping(spec.home);
  return { level: !answering ? "down" : build !== expected ? "degraded" : "ok", state: "running", name: spec.box, build, expected, startedAt, answering };
}

/** Whether the Store container's worker answers a ping within PING_MS. */
function ping(home) {
  let file;
  try { file = JSON.parse(readFileSync(storeFile(home), "utf8")); } catch { return Promise.resolve(false); }
  return new Promise((resolve) => {
    const socket = connect({ host: "127.0.0.1", port: file.port }, () => socket.write(Buffer.concat([frame({ secret: file.secret }), frame({ id: 1, fn: "ping" })])));
    const done = (value) => { clearTimeout(timer); socket.destroy(); resolve(value); };
    const timer = setTimeout(() => done(false), PING_MS);
    readFrames(socket, (value) => { if (value?.id === 1) done(value.answer?.value === "ready"); }, () => done(false));
    socket.on("error", () => done(false));
    socket.on("close", () => done(false));
  });
}

/** Whether something accepts a TCP connection on the host's loopback `port` within PING_MS. */
export function reachable(port, ms = PING_MS) {
  return new Promise((resolve) => {
    const socket = connect({ host: "127.0.0.1", port });
    const done = (value) => { clearTimeout(timer); socket.destroy(); resolve(value); };
    const timer = setTimeout(() => done(false), ms);
    socket.on("connect", () => done(true));
    socket.on("error", () => done(false));
  });
}

function freeBytes(path) {
  try { const s = statfsSync(path); return s.bavail * s.bsize; } catch { return null; }
}

/** Free bytes of Docker's volume disk, as the Store container sees it. */
async function vmFree(spec) {
  try {
    const out = await docker(spec, "exec", spec.box, "df", "-kP", VOLUMES);
    const kb = Number(out.split("\n").pop().trim().split(/\s+/)[3]);
    return Number.isFinite(kb) ? kb * 1024 : null;
  } catch { return null; }
}

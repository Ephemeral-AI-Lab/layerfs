// The /layerfs route on the Host's web server. Every request first passes
// Connection's Host/Origin checks and browser authentication, like /api.
//   GET  /layerfs/status                      configuration, SDK location and what Init offers (empty: new empty projects);
//                                             migration.kept: older Sandboxes left running at start (migrate.js)
//   GET  /layerfs/workspace?folder=<abs>      every project (the folder's imports first) and the folder's default
//   POST /layerfs/workspace {folder, project}  make a project the folder's default for new sessions
//   POST /layerfs/init {folder, source?, name?, backend?, image?, empty?}   start Init LayerFS (backend "sim": the simulator;
//                                          image: the project's own runtime image; empty: a new empty project with a random
//                                          name); answers its job (see jobs.js)
//   GET  /layerfs/init/<job>                  the Init's step and counts
//   GET  /layerfs/session/<id>                the root session's project, agents and trace lines
//   POST /layerfs/session/<id>/project {project}  choose any project (null: none) until locked; answers as GET
//   GET  /layerfs/session/<id>/state          the project as LayerFS reports it
//   GET  /layerfs/session/<id>/{list,read,blame,changes,diff}?…  one Files read of saved ids (reads.js):
//                                          list?ref&dir&position, read?ref&path&line, blame?ref&path&line,
//                                          changes?from&to&position, diff?from&to&path
//   GET  /layerfs/projects                    every project, and each workspace folder's default
//   GET  /layerfs/project/<name>/state        the project as LayerFS reports it, with who made each node
//   GET  /layerfs/project/<name>/history      the same, History alone from the Store (no Sandbox asked)
//   GET  /layerfs/project/<name>/{list,read,blame,changes,diff}?…  as the session's Files reads
//   GET  /layerfs/project/<name>/storage      Store totals, what Init recorded, recent Commits' costs (storage.js)
//   GET  /layerfs/runtime                     Docker, the Store container, disk and every project's Sandboxes (runtime.js)
//   POST /layerfs/runtime/store               start the Store container when it is stopped or missing
//   POST /layerfs/runtime/store/restart {confirm: "restart"}  restart a running Store container
//   POST /layerfs/runtime/leftovers {names}   remove stopped leftover Sandbox containers
//   GET  /layerfs/project/<name>/runtime      the project's Sandboxes, Workspaces and processes
//   POST /layerfs/project/<name>/runtime {action, sandbox?, path?, process?, confirm?}  unmount, discard or stop
//   POST /layerfs/project/<name>/remove {confirm, stopSandboxes?}  remove it (confirm: its name;
//                                          stopSandboxes: stop its running Sandboxes first); see remove.js
import { missingDeployment } from "./config.js";
import { prepareInit, runInit } from "./init.js";
import { InitJobs } from "./jobs.js";
import { READS } from "./reads.js";
import { removeProject } from "./remove.js";
import { cleanLeftovers, nameImages, overview, projectAction, projectRuntime, restartStore, startStore } from "./runtime.js";

const PREFIX = "/layerfs";
const MAX_BODY = 64 * 1024;

const send = (res, status, body) => {
  res.statusCode = status;
  res.setHeader("content-type", "application/json; charset=utf-8");
  res.setHeader("cache-control", "no-store");
  res.end(JSON.stringify(body));
};

async function readJson(req) {
  let size = 0, text = "";
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY) throw Object.assign(new Error("request body too large"), { status: 413 });
    text += chunk;
  }
  try { return text ? JSON.parse(text) : {}; } catch { throw Object.assign(new Error("request body is not JSON"), { status: 400 }); }
}

/**
 * Registers the route; its disposer runs with the plugin fiber.
 * @param {object} ctx plugin context with webServer and connection
 */
export function registerRoutes(ctx, { spec, bindings, collector, trees, jobs = new InitJobs((prepared, report) => runInit(spec, bindings, prepared, report)) }) {
  const sessionView = (id) => {
    const session = ctx.sessions.get(id);
    const agent = session ? collector.agentOf(session) : { id, root: bindings.agent(id)?.root ?? id, cwd: undefined };
    // a Session that is not loaded has no header here; its choice keeps the folder
    const choice = bindings.session(agent.root), folder = agent.cwd ?? choice?.folder ?? null;
    const project = bindings.projectFor(agent.root, folder);
    return { session: id, root: agent.root, folder, choice, project, choices: folder ? bindings.workspace(folder) : { projects: [], lastUsed: null } };
  };
  const handler = async (req, res) => {
    const rejection = ctx.connection.requestRejection(req);
    if (rejection !== undefined) { res.statusCode = rejection; res.end(); return; }
    const url = new URL(String(req.url), "http://localhost"), parts = url.pathname.slice(PREFIX.length).split("/").filter(Boolean);
    try {
      if (req.method === "GET" && parts[0] === "status" && parts.length === 1)
        return send(res, 200, { configured: missingDeployment(spec).length === 0, missing: missingDeployment(spec), sdk: spec.sdk, operator: spec.operator, box: spec.box, bundle: spec.bundle, sandbox: spec.native ? "native" : "docker", localDir: spec.localDir ?? null, stores: spec.stores ?? null, traceDir: spec.traceDir, home: spec.home, docker: spec.docker, image: spec.image, daemon: spec.daemon, empty: true, migration: spec.migration ?? null, ...(spec.windows ? { windows: spec.windows } : {}) });
      if (req.method === "GET" && parts[0] === "workspace" && parts.length === 1) {
        const folder = url.searchParams.get("folder");
        if (!folder) return send(res, 400, { code: "invalid-folder", message: "folder is required" });
        return send(res, 200, bindings.workspace(folder));
      }
      if (req.method === "POST" && parts[0] === "workspace" && parts.length === 1) {
        const body = await readJson(req);
        if (typeof body.folder !== "string" || !body.folder) return send(res, 400, { code: "invalid-folder", message: "folder is required" });
        if (typeof body.project !== "string") return send(res, 400, { code: "invalid-project", message: "project is a project name" });
        bindings.setDefault(body.folder, body.project);
        return send(res, 200, bindings.workspace(body.folder));
      }
      if (req.method === "POST" && parts[0] === "init" && parts.length === 1) {
        const body = await readJson(req);
        const prepared = prepareInit(spec, bindings, { folder: body.folder, source: body.source || body.folder, name: body.name || undefined, backend: body.backend || undefined, image: body.image || undefined, empty: body.empty === true }, jobs.pending());
        return send(res, 202, { job: jobs.begin(prepared) });
      }
      if (req.method === "GET" && parts[0] === "init" && parts.length === 2) {
        const job = jobs.get(decodeURIComponent(parts[1]));
        return job ? send(res, 200, { job }) : send(res, 404, { code: "not-found", message: "no such Init job" });
      }
      if (req.method === "POST" && parts[0] === "runtime" && parts[1] === "store" && parts[2] === "restart" && parts.length === 3)
        return send(res, 200, await restartStore(spec, await readJson(req)));
      if (parts[0] === "runtime" && parts.length <= 2) {
        if (req.method === "GET" && parts.length === 1) return send(res, 200, await overview(spec, bindings, trees, collector));
        if (req.method === "POST" && parts[1] === "store") return send(res, 200, await startStore(spec));
        if (req.method === "POST" && parts[1] === "leftovers") return send(res, 200, await cleanLeftovers(spec, bindings, (await readJson(req)).names));
      }
      if (req.method === "GET" && parts[0] === "projects" && parts.length === 1) return send(res, 200, { projects: bindings.projects(), defaults: bindings.defaults() });
      if (parts[0] === "project" && parts.length === 3) {
        const project = bindings.byName(decodeURIComponent(parts[1]));
        if (!project) return send(res, 404, { code: "not-found", message: `no LayerFS project named ${decodeURIComponent(parts[1])}` });
        if (req.method === "GET" && parts[2] === "state") return send(res, 200, { ...(await trees.state(project)), ...collector.nodesOf(project.name) });
        if (req.method === "GET" && parts[2] === "history") return send(res, 200, { ...(await trees.history(project)), ...collector.nodesOf(project.name) });
        if (req.method === "GET" && Object.hasOwn(READS, parts[2])) return send(res, 200, await trees.files(project, parts[2], url.searchParams));
        if (req.method === "GET" && parts[2] === "storage") return send(res, 200, await trees.storage(project));
        if (req.method === "GET" && parts[2] === "runtime") {
          const runtime = await projectRuntime(trees, collector, project);
          await nameImages(spec, [...runtime.sandboxes, ...runtime.gone]);
          return send(res, 200, runtime);
        }
        if (req.method === "POST" && parts[2] === "runtime") return send(res, 200, await projectAction(trees, collector, project, await readJson(req)));
        if (req.method === "POST" && parts[2] === "remove") {
          const body = await readJson(req);
          if (body.confirm !== project.name) return send(res, 400, { code: "not-confirmed", message: `confirm with the project's name, ${project.name}` });
          return send(res, 200, await removeProject(spec, bindings, trees, project.name, undefined, { stopSandboxes: body.stopSandboxes === true }));
        }
      }
      if (parts[0] === "session" && parts.length >= 2) {
        const view = sessionView(decodeURIComponent(parts[1]));
        const full = (v) => ({ ...v, ...collector.snapshot(v.root) });
        if (req.method === "GET" && parts.length === 2) return send(res, 200, full(view));
        if (req.method === "POST" && parts[2] === "project" && parts.length === 3) {
          const body = await readJson(req);
          if (!view.folder) return send(res, 409, { code: "no-folder", message: "this session has no workspace folder" });
          const taken = bindings.choose(view.root, view.folder, body.project ?? null);
          return taken ? send(res, 200, full(sessionView(view.root))) : send(res, 409, { code: "locked", message: "the session's project is fixed once an agent has used it" });
        }
        if (req.method === "GET" && parts[2] === "state" && parts.length === 3) {
          if (!view.project) return send(res, 404, { code: "no-project", message: "this session has no LayerFS project" });
          return send(res, 200, await trees.state(view.project));
        }
        if (req.method === "GET" && Object.hasOwn(READS, parts[2] ?? "") && parts.length === 3) {
          if (!view.project) return send(res, 404, { code: "no-project", message: "this session has no LayerFS project" });
          return send(res, 200, await trees.files(view.project, parts[2], url.searchParams));
        }
      }
      return send(res, 404, { code: "not-found", message: `no route ${req.method} ${url.pathname}` });
    } catch (error) {
      return send(res, error.status || (error.code ? 409 : 500), { code: error.code || "failed", message: error.message });
    }
  };
  ctx.effect(() => ctx.webServer.register({ kind: "prefix", path: PREFIX, handler }), `layerfs-host: ${PREFIX}`);
}

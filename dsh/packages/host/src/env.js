// The bash environment of a session bound to a LayerFS project: the SDK
// entry, the project name, its deployment and the calling agent's trace
// file. dsh rebuilds the DSH_* namespace for every bash call, so these
// values always describe the agent that runs the command.
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { deploymentEnv, isSim } from "./layerfs-client.js";
import { appendProject } from "./records.js";

const CLI = fileURLToPath(new URL("./lfs.js", import.meta.url));
// The LayerFS SDK skill of the checkout this plugin runs from, when it has one.
const SKILL = fileURLToPath(new URL("../../../../.agents/skills/layerfs-js-sdk/SKILL.md", import.meta.url));
const skill = existsSync(SKILL) ? SKILL : null;

const VARIABLES = {
  DSH_LAYERFS_PROJECT: "LayerFS project of this conversation; pass it to new LayerFS(...)",
  DSH_LAYERFS_SDK: "file URL of the LayerFS JavaScript SDK; import it from Node scripts",
  DSH_LAYERFS_OPERATOR: "file URL of the SDK's operator (background processes in a Workspace)",
  DSH_LAYERFS_SKILL: "path of the LayerFS SDK skill (SKILL.md, its references beside it); read it before the first LayerFS call",
  DSH_LAYERFS_CLI: "path of lfs.js: status, log, ls, cat, grep, diff, fork, write and run, one command each",
  DSH_LAYERFS_HOME: "LayerFS home the SDK reads its project and Sandbox records from",
  DSH_LAYERFS_DOCKER: "Docker Engine socket the SDK starts Sandboxes through",
  DSH_LAYERFS_IMAGE: "Runtime image digest a new LayerFS Sandbox runs; the SDK reads it",
  DSH_LAYERFS_CONTAINER_PREFIX: "Prefix of the project's Sandbox container names (<prefix><project>-<n>)",
  DSH_LAYERFS_VOLUME: "Docker volume holding this project's Store",
  DSH_LAYERFS_DAEMON: "Host path of the LayerFS daemon Sandboxes run",
  DSH_LAYERFS_STORE_PATH: "Store file the SDK opens in this process for Store calls",
  DSH_LAYERFS_DAEMON_HOST: "Host the SDK dials for a Sandbox daemon's published port",
  DSH_LAYERFS_STORE_CONTAINER: "macOS: the Store container (Linux) the SDK's calls run in",
  DSH_LAYERFS_BUNDLE: "macOS: the LayerFS build whose SDK DSH_LAYERFS_SDK runs",
  DSH_LAYERFS_WSL_ENDPOINT: "Windows: private endpoint file of the WSL LayerFS worker",
  DSH_LAYERFS_SANDBOX: "native: Sandboxes are local daemons on this machine (no Docker); Workspaces are NFS mounts under DSH_LAYERFS_LOCAL_DIR and commands run as you",
  DSH_LAYERFS_LOCAL_DIR: "native: the folder local Sandboxes and their Workspace mounts live in",
  DSH_LAYERFS_NFS_MOUNT: "native: the NFS mount command the Sandbox daemon uses, when the default is overridden",
  DSH_LAYERFS_TRACE: "File the SDK appends one line per call to",
  DSH_LAYERFS_TRACE_STEP: "Label of this bash call on the SDK's trace lines",
  DSH_LAYERFS_TRACE_START: "1: the SDK also writes a line when each call starts",
};

/**
 * The runtime-context line naming the project of `session`'s conversation (a
 * subagent's is its root's), or "" when the conversation has none, so agents
 * know it before their first bash call. Reading it binds nothing: the first
 * bash call that uses the project still locks the choice.
 * @param {import("./binding.js").BindingStore} bindings
 * @param {import("./collector.js").Collector} collector
 * @param {object} [spec] resolved Host settings (the native route adds one sentence)
 */
export function projectContext(bindings, collector, session, spec) {
  if (!session) return "";
  const agent = collector.agentOf(session);
  const project = bindings.projectFor(agent.root, agent.cwd);
  if (!project) return "";
  const native = spec?.native && !isSim(project)
    ? " Its Sandboxes run natively on this machine: a Workspace is an NFS mount, and the path workspace.mount returns may also be your bash commands' working directory (commit through the SDK)."
    : "";
  return `LayerFS project of this conversation: ${project.name} (${isSim(project) ? "simulated" : "live"}). Every bash call has it as DSH_LAYERFS_PROJECT; work on it through the LayerFS SDK.${native}`;
}

/**
 * @param {object} ctx plugin context (ctx.sessions)
 * @param {object} spec resolved Host settings
 * @param {import("./binding.js").BindingStore} bindings
 * @param {import("./collector.js").Collector} collector
 */
export function contributor(ctx, spec, bindings, collector) {
  return {
    name: "layerfs",
    variables: Object.fromEntries(Object.entries(VARIABLES).map(([key, description]) => [key, { description }])),
    resolve(execution) {
      const session = execution.agent && execution.agent.session;
      if (!session) return {};
      const agent = collector.agentOf(session);
      const project = bindings.projectFor(agent.root, agent.cwd);
      if (!project) return {};
      const before = bindings.session(agent.root);
      bindings.lock(agent.root, project.name, agent.cwd);
      if (!before || !before.locked) {
        const root = ctx.sessions.get(agent.root);
        if (root) appendProject(root, { project: project.name, folder: agent.cwd, volume: project.volume });
      }
      const deploy = deploymentEnv(spec, project);
      return {
        DSH_LAYERFS_PROJECT: project.name,
        DSH_LAYERFS_SDK: isSim(project) ? spec.simSdk : spec.sdk,
        DSH_LAYERFS_OPERATOR: isSim(project) ? spec.simOperator : spec.operator,
        DSH_LAYERFS_CLI: CLI,
        ...(skill ? { DSH_LAYERFS_SKILL: skill } : {}),
        ...Object.fromEntries(Object.entries(deploy).filter(([, v]) => v).map(([k, v]) => [`DSH_LAYERFS_${k}`, v])),
        DSH_LAYERFS_TRACE: collector.track(agent, project),
        DSH_LAYERFS_TRACE_STEP: String(execution.rootCallId ?? execution.callId),
        DSH_LAYERFS_TRACE_START: "1",
      };
    },
  };
}

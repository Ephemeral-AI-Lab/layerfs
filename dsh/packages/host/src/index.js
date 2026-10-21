// @dsh-layerfs/host: the Host half of LayerFS × DSH, a Cordis function
// plugin that uses only public dsh services.
//   ctx.shellEnv     a contributor gives bash calls in sessions bound to a
//                    project the SDK's DSH_LAYERFS_* settings and a trace file
//   ctx.sessions     each traced call becomes a plugin:layerfs/call record in
//                    the calling agent's own Session (subagents included)
//   session/event    a subagent's descriptor names it
//   ctx.webServer    the /layerfs route, behind ctx.connection's checks
//                    (only where a web server runs: Web and Desktop)
//   ctx.systemPrompt each agent's runtime context names its conversation's
//                    LayerFS project (where a system prompt is assembled)
import { BindingStore } from "./binding.js";
import { ensureBox, onUnreachable } from "@layerfs/port";
import { migrate } from "./migrate.js";
import { Collector } from "./collector.js";
import z from "@deepseek-ai/schemastery";
import { resolveConfig } from "./config.js";
import { contributor, projectContext } from "./env.js";
import { registerRoutes } from "./route.js";
import { TreeReader } from "./trees.js";

export const name = "layerfs-host";
export const inject = ["shellEnv", "sessions"];
/** Host settings; see config.js for each field. */
export const Config = z.object({
  image: z.string().description("LayerFS runtime image digest (sha256:…) that Sandboxes run"),
  daemon: z.string().description("Host path of the built Linux layerfs-daemon"),
  bundle: z.string().description("Folder of a sealed Linux LayerFS build; on macOS, Live SDK calls run it in the Store container"),
  containerPrefix: z.string().description("Prefix of this home's container names (default layerfs-): Sandboxes <prefix><project>-<n>, the Store container <prefix>store"),
  docker: z.string().description("Local Docker Engine Unix socket"),
  home: z.string().description("LayerFS home for discovery records (default: <temp>/dsh-layerfs/home)"),
  stateDir: z.string().description("Project bindings and agent names (default: <profile>/dsh-layerfs)"),
  traceDir: z.string().description("Per-agent SDK trace files (default: <temp>/dsh-layerfs/traces)"),
  pollMs: z.natural().min(50).max(10_000).default(400).description("How often trace files are read, in milliseconds"),
});

/**
 * @param {object} ctx plugin context
 * @param {import("./config.js").Config} config
 */
export function apply(ctx, config) {
  const spec = resolveConfig(config, ctx.get("profileContext"));
  if (spec.box) {
    // the Host's own clients use the ported SDK too (@layerfs/port/sdk)
    Object.assign(process.env, { DSH_LAYERFS_STORE_CONTAINER: spec.box, DSH_LAYERFS_BUNDLE: spec.bundle });
    ensureBox(spec).catch((error) => ctx.logger?.warn?.(`layerfs: the Store container did not start: ${error.message}`));
  }
  // a call of the Host's or an agent's that finds the Store container stopped, missing or
  // unreachable starts it again, one start at a time; the failed call keeps its own answer
  const heal = spec.box ? () => ensureBox(spec).catch((error) => ctx.logger?.warn?.(`layerfs: the Store container did not start again: ${error.message}`)) : null;
  if (heal) ctx.effect(() => onUnreachable(heal), "layerfs-host: Store container self-heal");
  // older Sandboxes left running (they may hold work) are shown in GET /layerfs/status
  spec.migration = { kept: [] };
  migrate(spec, undefined, (text) => ctx.logger?.warn?.(text))
    .then((result) => { spec.migration = result; })
    .catch((error) => ctx.logger?.warn?.(`layerfs: moving to the #373 layout: ${error.message}`));
  const bindings = new BindingStore(spec.stateDir);
  const collector = new Collector(ctx, spec, bindings);
  collector.onUnreachable = heal;
  ctx.effect(() => ctx.shellEnv.register(contributor(ctx, spec, bindings, collector)), "layerfs-host: bash environment");
  ctx.effect(() => collector.start(), "layerfs-host: trace collection");
  ctx.on("session/event", (session, event) => collector.observe(session, event));
  const trees = new TreeReader(spec);
  ctx.effect(() => () => trees.dispose(), "layerfs-host: tree clients");
  ctx.inject(["webServer", "connection"], (child) => registerRoutes(child, { spec, bindings, collector, trees }));
  ctx.inject(["systemPrompt"], (scope) => {
    scope.systemPrompt.context({
      name: "layerfs:project",
      // right after the working directory, the other fact about where the agent works
      order: scope.systemPrompt.getContextOrder("WORKING_DIRECTORY") + 1,
      interpolate: false,
      text: (context) => (context.agent ? projectContext(bindings, collector, context.agent.session) : ""),
    });
  });
}

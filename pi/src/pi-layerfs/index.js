// LayerFS × Pi: with --layerfs <project>, a Pi session works on one LayerFS
// Branch of the project instead of the local folder. Pi's read, write, edit,
// ls, find, grep and bash, and the user's !command, are replaced by ones that
// read and commit on the Branch; the `layerfs` tool shows and changes its
// history. Without the flag the extension changes nothing.
// Pi names a package installed from a path by its entry's folder, hence pi-layerfs/.
import { createBashToolDefinition } from "@earendil-works/pi-coding-agent";
import { bashOperations } from "./bash.js";
import { fileTools } from "./files.js";
import { historyTool } from "./history.js";
import { branchPaths } from "./paths.js";
import { connect } from "./sdk.js";
import { ensureBranch, reporting, sessionBranch } from "./session.js";

const STATUS_KEY = "layerfs";
/** Pi's own tools this extension replaces with ones on the Branch. */
const REPLACED = new Set(["read", "write", "edit", "ls", "find", "grep", "bash"]);
/**
 * Tools whose calls write the Branch or see it as of their start (a bash
 * Workspace shows the Branch at its mount). Pi runs one message's tool calls
 * in parallel unless one of them is sequential, so these keep the model's order.
 */
const ORDERED = new Set(["write", "edit", "bash", "layerfs"]);

/** What the model is told about the session's Branch, as one section of Pi's system prompt. */
const guide = (project, branch, root) => `LayerFS project ${project}, Branch ${branch}. The project's files live in this LayerFS Branch, not on the local disk: the working directory ${root} stands for the Branch root "/" and stays empty locally. Name files by paths relative to it (src/app.js); an absolute path must lie inside it.
- read, ls, find and grep see the Branch's latest Commit. Every write or edit is one Commit.
- grep matches literal text, case-sensitively; for a regular expression run grep -rnE in bash.
- Each bash call runs in a new Workspace of the Branch and commits what it changed as one Commit, a failing command too. Only committed files carry over between calls: environment variables, the shell's directory and running processes do not. Do not leave a background process running: it keeps the Workspace busy, and the call fails Busy after committing.
- main is read-only: it is the published version. The layerfs tool lists Branches and Commits, merges, and publishes this Branch to main.
- In the project's first step, write .lww at the Branch root containing "%include .gitignore", so generated files (dependencies, build output) keep the newer version instead of conflicting when Commits merge. Never list source files in it.`;

/** @param {import("@earendil-works/pi-coding-agent").ExtensionAPI} pi */
export default function layerfs(pi) {
  pi.registerFlag("layerfs", { description: "Work on this LayerFS project instead of the local folder", type: "string" });
  pi.registerFlag("layerfs-branch", { description: "LayerFS Branch of this session (default: derived from the session id)", type: "string" });

  let lf = null;

  pi.on("session_start", async (_event, ctx) => {
    const project = pi.getFlag("layerfs");
    if (!project) return;
    // registered first: the user's !command waits for the setup and, when it
    // failed, is refused with its error, so it never runs on the local folder
    let setup;
    pi.on("user_bash", async () => ({ operations: await setup }));
    setup = setUp(project, ctx);
    await setup;
  });

  /** Makes the session work on its Branch; resolves to the bash operations. */
  async function setUp(project, ctx) {
    const branch = pi.getFlag("layerfs-branch") || sessionBranch(ctx.sessionManager.getSessionId());
    const paths = branchPaths(ctx.cwd);
    // Pi's own tools go first, so a session that asked for LayerFS never works
    // on the local folder, even when the SDK below does not load
    pi.setActiveTools(pi.getActiveTools().filter((name) => !REPLACED.has(name)));
    // the client takes its deployment from the SDK's own LAYERFS_* variables
    const client = await connect(project);
    lf = client.lf;
    const operations = bashOperations(lf, branch, paths);
    // a Workspace command gets no environment from Pi, so the PI_* variables are not offered
    const bash = createBashToolDefinition(paths.root, { operations, exposeSessionEnvironment: false });
    // registered before the Store container and the Branch are made, so their failures fail the calls
    const tools = [...fileTools(lf, branch, paths), historyTool(lf, branch), bash];
    for (const tool of tools) pi.registerTool(reporting(ORDERED.has(tool.name) ? { ...tool, executionMode: "sequential" } : tool));
    // Pi activates a tool on registration only under a new name, so the replaced ones are turned back on here
    pi.setActiveTools([...new Set([...pi.getActiveTools(), ...tools.map((tool) => tool.name)])]);
    pi.on("before_agent_start", (event) => {
      event.systemPromptOptions.sections.layerfs = guide(project, branch, paths.root);
    });
    if (ctx.hasUI) ctx.ui.setStatus(STATUS_KEY, `${project}@${branch}`);
    await client.start();
    await ensureBranch(lf, branch);
    return operations;
  }

  pi.on("session_shutdown", async () => {
    await lf?.close();
    lf = null;
  });
}

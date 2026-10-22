// LayerFS for every agent magpie connects: instead of each agent copying a
// project tree to work on it, agents share one durable, deduplicated history
// and work through stable workspace paths. Forking a branch for a parallel
// agent copies no file bytes; a checkpoint stores only what changed.
// Arguments are JSON Schema, so the package needs no dependency.
//
// Backends: the real LayerFS through its JavaScript SDK when configured
// (LAYERFS_SDK_PATH + LAYERFS_STORE_PATH), otherwise a local content-
// addressed store with the same model. See layerfs.mjs and the README.
import { checkBranch, checkInnerPath, checkProject, createBackend } from "./layerfs.mjs"

const str = (v, name) => {
  if (typeof v !== "string" || !v) throw new Error(`${name} is a non-empty string`)
  return v
}
const opt = (v, name, dflt) => (v === undefined || v === "" ? dflt : str(v, name))

export const LayerfsTools = async (ctx = {}) => {
  let backendPromise
  const backend = () => (backendPromise ??= createBackend())
  const call = async (fn) => fn(await backend())

  return {
    tool: {
      provision: {
        description:
          "Gives the agent a LayerFS workspace for a project branch and answers the filesystem path to work in. " +
          "The first provision of a project imports the source directory (default: the agent's working directory) as its first checkpoint; " +
          "later provisions of the same branch answer the same path without copying the tree again. " +
          "Work in the returned path, then save progress with the checkpoint tool.",
        args: {
          type: "object",
          properties: {
            project: { type: "string", description: "LayerFS project name, e.g. website" },
            branch: { type: "string", description: "branch to work on (default main)" },
            source: { type: "string", description: "directory to import when the project is first created (default: the agent's working directory)" },
          },
          required: ["project"],
        },
        async execute(args) {
          const project = checkProject(str(args.project, "project"))
          const branch = checkBranch(opt(args.branch, "branch", "main"))
          const source = args.source === undefined ? ctx.directory : str(args.source, "source")
          return call((b) => b.provision(project, branch, source))
        },
      },
      checkpoint: {
        description:
          "Saves the workspace's current state as a deduplicated checkpoint on its branch: only blobs the store has not seen are written, " +
          "so keeping the full history of a large tree costs little more than the changes themselves. " +
          "Answers the commit id and the added / modified / deleted counts with the new-bytes and saved-bytes accounting.",
        args: {
          type: "object",
          properties: {
            project: { type: "string", description: "LayerFS project name" },
            branch: { type: "string", description: "branch the workspace belongs to (default main)" },
            message: { type: "string", description: "what this checkpoint did" },
          },
          required: ["project"],
        },
        async execute(args) {
          const project = checkProject(str(args.project, "project"))
          const branch = checkBranch(opt(args.branch, "branch", "main"))
          return call((b) => b.checkpoint(project, branch, opt(args.message, "message", "")))
        },
      },
      fork: {
        description:
          "Forks a branch of a project for a parallel agent. A fork is a pointer at an existing checkpoint: it copies no file bytes and is ready immediately, " +
          "where copying the tree for another agent costs the whole tree's size and time. Provision the new branch to get its workspace path.",
        args: {
          type: "object",
          properties: {
            project: { type: "string", description: "LayerFS project name" },
            branch: { type: "string", description: "the new branch's name" },
            from: { type: "string", description: "branch to fork from (default main)" },
          },
          required: ["project", "branch"],
        },
        async execute(args) {
          const project = checkProject(str(args.project, "project"))
          const branch = checkBranch(str(args.branch, "branch"))
          const from = checkBranch(opt(args.from, "from", "main"))
          return call((b) => b.fork(project, branch, from))
        },
      },
      history: {
        description:
          "Lists a branch's checkpoints, newest first: commit id, message, time, file count and the logical size of the tree at that point, " +
          "plus how many new bytes that checkpoint actually stored. The history is shared and durable: any agent can read what every other agent committed.",
        args: {
          type: "object",
          properties: {
            project: { type: "string", description: "LayerFS project name" },
            branch: { type: "string", description: "branch to read (default main)" },
            limit: { type: "number", description: "how many checkpoints to return (default 20)" },
          },
          required: ["project"],
        },
        async execute(args) {
          const project = checkProject(str(args.project, "project"))
          const branch = checkBranch(opt(args.branch, "branch", "main"))
          const limit = args.limit === undefined ? 20 : Math.max(1, Math.min(200, Number(args.limit) || 20))
          return call((b) => b.history(project, branch, limit))
        },
      },
      resolve: {
        description:
          "Resolves a logical LayerFS location to a real filesystem path without copying anything: the workspace path for a project branch, " +
          "or one file inside it, with the checkpoint that path currently answers for and, for a file, its content hash and size. " +
          "Use it to find where a branch lives before reading or handing the path to another tool.",
        args: {
          type: "object",
          properties: {
            project: { type: "string", description: "LayerFS project name" },
            branch: { type: "string", description: "branch to resolve (default main)" },
            path: { type: "string", description: "a file path inside the workspace, relative to its root" },
          },
          required: ["project"],
        },
        async execute(args) {
          const project = checkProject(str(args.project, "project"))
          const branch = checkBranch(opt(args.branch, "branch", "main"))
          const inner = args.path === undefined || args.path === "" ? "" : checkInnerPath(str(args.path, "path"))
          return call((b) => b.resolve(project, branch, inner))
        },
      },
      status: {
        description:
          "Reports the LayerFS backend in use (native or sim), the projects and branches it holds, and the storage accounting: " +
          "the logical bytes of every checkpoint ever taken against the bytes the deduplicated store actually occupies, and the ratio between them.",
        args: { type: "object", properties: {} },
        async execute() {
          return call((b) => b.status())
        },
      },
    },
  }
}

// layerfs workspace mount: workspace.mount(sandbox, branch); prints the handle as ws, <sandbox>:<path>.

export default {
  args: ["sandbox", "branch"],
  options: {},
  summary: "a new Workspace of a Branch in a running Sandbox; prints its handle as ws, <sandbox>:<path>",
  run: async (lf, a) => {
    const ws = await lf.workspace.mount(a.sandbox, a.branch);
    return { ...ws, ws: `${ws.sandbox}:${ws.path}` };
  },
};

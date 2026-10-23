// layerfs workspace unmount: workspace.unmount(ws, discard?).

export default {
  args: ["ws"],
  options: { discard: "bool" },
  summary: "end the Workspace; --discard drops uncommitted changes",
  run: (lf, a, o) => lf.workspace.unmount(a.ws, o.discard),
};

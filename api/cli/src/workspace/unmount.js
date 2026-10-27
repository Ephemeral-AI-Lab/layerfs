// layerfs workspace unmount: workspace.unmount(ws, discard?, force?).

export default {
  args: ["ws"],
  options: { discard: "bool", force: "bool" },
  summary: "end the Workspace; --discard drops uncommitted changes, --force aborts its connection",
  run: (lf, a, o) => lf.workspace.unmount(a.ws, o.discard, o.force),
};

// layerfs workspace unmount: workspace.unmount(ws, discard?, force?, kill?).

export default {
  args: ["ws"],
  options: { discard: "bool", force: "bool", kill: "text" },
  summary: "end the Workspace; --discard drops uncommitted changes, --force aborts its connection, --kill workspace cancels only its holders first",
  run: (lf, a, o) => lf.workspace.unmount(a.ws, { discard: o.discard, force: o.force, kill: o.kill }),
};

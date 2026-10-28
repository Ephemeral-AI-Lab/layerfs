// layerfs branch fork: branch.fork(name, fromRef?).

export default {
  args: ["name", "from-ref?"],
  options: {},
  summary: "a new Branch, from main by default",
  run: (lf, a) => lf.branch.fork(a.name, a.fromRef),
};

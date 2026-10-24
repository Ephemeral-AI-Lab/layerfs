// layerfs fs blame: fs.blame(ref, path, fromLine?).

export default {
  args: ["ref", "path"],
  options: { "from-line": "int" },
  summary: "the Commit that last changed each line",
  run: (lf, a, o) => lf.fs.blame(a.ref, a.path, o.fromLine),
};

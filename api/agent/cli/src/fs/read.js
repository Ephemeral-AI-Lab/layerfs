// layerfs fs read: fs.read(ref, path, startLine?, endLine?).

export default {
  args: ["ref", "path"],
  options: { "start-line": "int", "end-line": "int" },
  summary: "a file's text, a window of lines at a time",
  run: (lf, a, o) => lf.fs.read(a.ref, a.path, o.startLine, o.endLine),
};

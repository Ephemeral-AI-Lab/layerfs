// layerfs fs read: fs.read(ref, path, startLine?, endLine?, { binary? }?).

export default {
  args: ["ref", "path"],
  options: { "start-line": "int", "end-line": "int", binary: "bool" },
  summary: "a file's text or bytes, a window of lines at a time",
  run: (lf, a, o) => lf.fs.read(a.ref, a.path, o.startLine, o.endLine, { binary: o.binary }),
};

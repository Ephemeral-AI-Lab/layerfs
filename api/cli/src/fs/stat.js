// layerfs fs stat: fs.stat(ref, path).

export default {
  args: ["ref", "path"],
  options: {},
  summary: "one entry's type, size, mode, mtime and nlink",
  run: (lf, a) => lf.fs.stat(a.ref, a.path),
};

// layerfs fs read_bytes: fs.read_bytes(ref, path, offset?, length?).

export default {
  args: ["ref", "path"],
  options: { offset: "int", length: "int" },
  summary: "a file's raw bytes, a bounded window at a time",
  run: (lf, a, o) => lf.fs.read_bytes(a.ref, a.path, o.offset, o.length),
};

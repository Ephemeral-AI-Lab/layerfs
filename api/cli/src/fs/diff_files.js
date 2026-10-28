// layerfs fs diff_files: fs.diff_files(leftRef, leftPath, rightRef, rightPath).

export default {
  args: ["left-ref", "left-path", "right-ref", "right-path"],
  options: {},
  summary: "line changes between two files",
  run: (lf, a) => lf.fs.diff_files(a.leftRef, a.leftPath, a.rightRef, a.rightPath),
};

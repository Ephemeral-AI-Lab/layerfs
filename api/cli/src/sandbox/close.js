// layerfs sandbox close: sandbox.close(sandbox).

export default {
  args: ["sandbox"],
  options: {},
  summary: "stop a Sandbox and delete its container",
  run: (lf, a) => lf.sandbox.close(a.sandbox),
};

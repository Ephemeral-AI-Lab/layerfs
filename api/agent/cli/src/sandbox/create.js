// layerfs sandbox create: sandbox.create({ image? }).

export default {
  args: [],
  options: { image: "text" },
  summary: "start a Sandbox, the lowest free number (the only call that starts one)",
  run: (lf, a, o) => lf.sandbox.create({ image: o.image }),
};

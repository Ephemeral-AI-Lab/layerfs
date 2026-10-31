// layerfs sandbox create: sandbox.create({ image?, platform? }).

export default {
  args: [],
  options: { image: "text", platform: "text" },
  summary: "start a Sandbox, the lowest free number, on a platform the home allows (the only call that starts one)",
  run: (lf, a, o) => lf.sandbox.create({ image: o.image, platform: o.platform }),
};

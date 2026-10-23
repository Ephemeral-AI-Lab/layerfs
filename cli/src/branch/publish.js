// layerfs branch publish: branch.publish(branch, commit).

export default {
  args: ["branch", "commit"],
  options: {},
  summary: "publish one of a Branch's Commits as main's next Layer",
  run: (lf, a) => lf.branch.publish(a.branch, a.commit),
};

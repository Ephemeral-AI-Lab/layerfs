// layerfs branch publish: branch.publish(branch, commit?); without a commit, the Branch head.

export default {
  args: ["branch", "commit?"],
  options: {},
  summary: "publish a Branch's head, or one of its Commits, as main's next Layer",
  run: (lf, a) => lf.branch.publish(a.branch, a.commit),
};

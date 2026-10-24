// backend.store.project.get_comparable: the Commit with the same files as a
// saved Commit or Layer id, so a Layer can be compared through
// branch.diff_commits, which takes Commits: { commit, root, exact }.
// A Commit is itself. A Layer is the listed Commit with its root, else the
// Commit it was published from; `exact` is false when that Commit is not in
// get_history's answer, and the caller must then check the diff answer's root
// equals `root` (a publish that merged has other files). Refused
// NotComparable when no Commit has the same files (Init's Layer, or a publish
// that merged). Saved ids never change, so what is learned is kept per client,
// up to 100,000 ids.
import { LayerFSError } from "@layerfs/agent";
import { get_history } from "./get_history.js";

const SAVED = /^[0-9a-f]{64,68}$/;
const LIMIT = 100_000;
const learned = new WeakMap();
const unlike = (ref) => new LayerFSError("NotComparable", "LayerFS compares Commits, and this Layer has no Commit with the same files (Init's Layer, or a publish that merged)", { ref });

export async function get_comparable(shop, ref) {
  if (typeof ref !== "string" || !SAVED.test(ref)) throw new LayerFSError("InvalidInput", "ref must be a saved Commit or Layer id", null);
  let known = learned.get(shop);
  if (!known) learned.set(shop, (known = new Map()));
  if (!known.has(ref)) {
    const history = await get_history(shop);
    if (known.size > LIMIT) known.clear();
    const byRoot = new Map(), rootOf = new Map();
    for (const c of history.commits ?? []) {
      known.set(c.id, { commit: c.id, root: c.root, exact: true });
      byRoot.set(c.root, c.id);
      rootOf.set(c.id, c.root);
    }
    for (const l of history.layers ?? []) {
      const same = byRoot.get(l.root);
      if (same) known.set(l.id, { commit: same, root: l.root, exact: true });
      else if (l.sourceCommit && !rootOf.has(l.sourceCommit)) known.set(l.id, { commit: l.sourceCommit, root: l.root, exact: false });
      else known.set(l.id, null);
    }
  }
  const found = known.get(ref);
  if (!found) throw unlike(ref);
  return { ...found };
}

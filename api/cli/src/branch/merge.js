// layerfs branch merge: branch.merge(a, b, name, message?, units?).
import { MESSAGE } from "../common.js";

export default {
  args: ["a", "b", "name"],
  options: { ...MESSAGE, units: "json" },
  summary: "merge two Branches into a new one; --units is a JSON list of patterns or {name, patterns} groups",
  run: (lf, a, o) => lf.branch.merge(a.a, a.b, a.name, o.message, o.units),
};

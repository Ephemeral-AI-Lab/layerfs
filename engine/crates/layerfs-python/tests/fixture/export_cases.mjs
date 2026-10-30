// Export the shared P6 calls as language-neutral Python invocation records.
// The JavaScript functions execute against a recording proxy; no SDK or Store runs.
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { CASES as BASE, plain } from "../../../../../api/agent/test/fixture/cases.mjs";
import { CASES as AREA } from "../../../../../api/agent/test/fixture/cases-area-p.mjs";

const require = createRequire(import.meta.url);
const fakePath = fileURLToPath(new URL("../../../../../api/agent/test/fixture/fake-native.cjs", import.meta.url));

function recorder(path) {
  const call = async (...args) => {
    const record = { path: path.join("."), args: plain(args) };
    return path.join(".") === "LayerFS.init" ? { created: record } : record;
  };
  return new Proxy(call, {
    get(_target, property) {
      if (property === "then") return undefined;
      return recorder([...path, property]);
    },
  });
}

const cases = [];
for (const source of [...BASE, ...AREA]) {
  const native = require(fakePath);
  native.branchList = null;
  native.branchReplyBudget = null;
  native.workspaceList = [];
  source.prepare?.(native);
  const invoked = await source.sdk(recorder(["lf"]), recorder(["LayerFS"]));
  const call = invoked.created ?? invoked;
  call.path = call.path.replace(/^lf\./, "");
  cases.push({
    name: source.name,
    call,
    op: source.op ?? null,
    sent: source.sent ?? null,
    result: source.result ?? null,
    refused: source.refused ?? null,
    refuse: source.refuse ?? [],
    prepare: {
      branchList: native.branchList,
      branchReplyBudget: native.branchReplyBudget,
      workspaceList: native.workspaceList,
    },
  });
}
process.stdout.write(JSON.stringify(cases));

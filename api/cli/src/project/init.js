// layerfs init: LayerFS.init(name, folder, timeout?), the one command that
// opens no project first. A relative source folder is taken from the
// current directory. Its file sits in project/ as the SDK's init does.
import { resolve } from "node:path";

export default {
  args: ["name", "source-dir"],
  options: { timeout: "int" },
  summary: "create a project from a folder (LayerFS.init); timeout in ms, default 300000",
  run: async (LayerFS, a, o) => {
    const layerfs = await LayerFS.init(a.name, resolve(a.sourceDir), o.timeout);
    try {
      return layerfs.created;
    } finally {
      await layerfs.close();
    }
  },
};

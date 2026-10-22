// The SDK's operator of a ported Live project (DSH_LAYERFS_OPERATOR); see sdk.js.
import { setting } from "./env.js";
import { operatorEntry } from "./sdk.js";

export const { LayerFSOperator } = await import(operatorEntry(setting("BUNDLE")));

// The SDK's operator of a ported Live project (DSH_LAYERFS_OPERATOR); see sdk.js.
import { operatorEntry } from "./sdk.js";

export const { LayerFSOperator } = await import(operatorEntry(process.env.LAYERFS_BUNDLE || process.env.DSH_LAYERFS_BUNDLE));

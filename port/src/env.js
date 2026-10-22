// A ported process's deployment settings, read as the SDK reads them:
// LAYERFS_<NAME>, else its DSH_LAYERFS_<NAME> twin.
export const setting = (name) => process.env[`LAYERFS_${name}`] || process.env[`DSH_LAYERFS_${name}`];

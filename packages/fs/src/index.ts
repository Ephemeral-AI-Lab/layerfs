import type { BranchCapableFilesystem } from "./branches/types.js";

export const EPHEMERAL_AI_FS_VERSION = "0.1.0-rc.0";
export { EphemeralFS } from "./filesystem/ephemeral-fs.js";
export { EphemeralRuntime } from "./filesystem/ephemeral-runtime.js";
export type { OpenEphemeralRuntimeOptions } from "./filesystem/ephemeral-runtime.js";
declare module "./filesystem/ephemeral-fs.js" {
  interface EphemeralFS extends BranchCapableFilesystem {}
}
export { FilesystemError } from "./filesystem/errors.js";
export type { FilesystemErrorCode } from "./filesystem/errors.js";
export type * from "./filesystem/types.js";
export type {
  BranchConfiguration,
  FilesystemLimits,
  RuntimeLimits,
  StorageLimits,
} from "./resources/limits.js";
export { BranchError } from "./branches/types.js";
export type * from "./branches/types.js";

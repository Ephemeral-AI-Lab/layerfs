import type { FilesystemObject } from "../../examples/durable-object-workspace/src/index.js";

declare const __EFS_M6_MIGRATION_VERSION__: string;
declare const __EFS_M6_MIGRATION_START__: string;
declare const __EFS_M6_MIGRATION_END__: string;
declare const __EFS_M6_PUBLICATION_VARIANT__: string;
declare const __EFS_M6_PUBLICATION_START__: string;
declare const __EFS_M6_PUBLICATION_END__: string;
declare const __EFS_M6_MAINTENANCE_VARIANT__: string;
declare const __EFS_M6_MAINTENANCE_KIND__: string;
declare const __EFS_M6_MAINTENANCE_START__: string;
declare const __EFS_M6_MAINTENANCE_END__: string;
declare const __EFS_M6_FILESYSTEM_FAULT_OPERATION__: string;
declare const __EFS_M6_RESOURCE_CONTROL__: string;

declare module "cloudflare:workers" {
  interface ProvidedEnv {
    readonly FILESYSTEM: DurableObjectNamespace<FilesystemObject>;
  }
}

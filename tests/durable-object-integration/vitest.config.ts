import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { defineConfig } from "vitest/config";

const directory = path.dirname(fileURLToPath(import.meta.url));
const acceptedPreviewBundle = process.env.EFS_M6_PREVIEW_BUNDLE;
if (acceptedPreviewBundle === undefined)
  throw new Error(
    "EFS_M6_PREVIEW_BUNDLE must identify the exact Wrangler dry-run bundle",
  );

export default defineConfig({
  define: {
    __EFS_M6_MIGRATION_VERSION__: JSON.stringify(
      process.env.EFS_M6_MIGRATION_VERSION ?? "0",
    ),
    __EFS_M6_MIGRATION_START__: JSON.stringify(
      process.env.EFS_M6_MIGRATION_START ?? "0",
    ),
    __EFS_M6_MIGRATION_END__: JSON.stringify(process.env.EFS_M6_MIGRATION_END ?? "0"),
    __EFS_M6_PUBLICATION_VARIANT__: JSON.stringify(
      process.env.EFS_M6_PUBLICATION_VARIANT ?? "",
    ),
    __EFS_M6_PUBLICATION_START__: JSON.stringify(
      process.env.EFS_M6_PUBLICATION_START ?? "0",
    ),
    __EFS_M6_PUBLICATION_END__: JSON.stringify(
      process.env.EFS_M6_PUBLICATION_END ?? "0",
    ),
    __EFS_M6_MAINTENANCE_VARIANT__: JSON.stringify(
      process.env.EFS_M6_MAINTENANCE_VARIANT ?? "",
    ),
    __EFS_M6_MAINTENANCE_KIND__: JSON.stringify(
      process.env.EFS_M6_MAINTENANCE_KIND ?? "",
    ),
    __EFS_M6_MAINTENANCE_START__: JSON.stringify(
      process.env.EFS_M6_MAINTENANCE_START ?? "0",
    ),
    __EFS_M6_MAINTENANCE_END__: JSON.stringify(
      process.env.EFS_M6_MAINTENANCE_END ?? "0",
    ),
    __EFS_M6_FILESYSTEM_FAULT_OPERATION__: JSON.stringify(
      process.env.EFS_M6_FILESYSTEM_FAULT_OPERATION ?? "",
    ),
    __EFS_M6_RESOURCE_CONTROL__: JSON.stringify(
      process.env.EFS_M6_RESOURCE_CONTROL ?? "0",
    ),
  },
  plugins: [
    cloudflareTest({
      main: path.resolve(acceptedPreviewBundle),
      wrangler: {
        configPath: path.resolve(
          directory,
          "../../examples/durable-object-workspace/wrangler.jsonc",
        ),
      },
    }),
  ],
  test: {
    include: ["tests/durable-object-integration/cloudflare-*.test.ts"],
    disableConsoleIntercept: true,
    testTimeout: 60_000,
    hookTimeout: 60_000,
  },
});

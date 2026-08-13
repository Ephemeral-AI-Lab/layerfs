import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/durable-object-integration/node-*.test.ts"],
    disableConsoleIntercept: true,
    testTimeout: 60_000,
    hookTimeout: 60_000,
  },
});

import { McpServer } from '@modelcontextprotocol/server';
import { serveStdio } from '@modelcontextprotocol/server/stdio';
import { z } from 'zod';

import { createOperations } from './commands.js';

const server = new McpServer({ name: 'layerfs-area-p-build', version: '1.0.0' });
const operations = createOperations({ resources: process.env.LAYERFS_BUILD_RESOURCES });

function register(name, description, inputSchema, operation) {
  server.registerTool(name, { description, inputSchema }, async (input) => {
    try {
      const result = await operation(input);
      return { content: [{ type: 'text', text: JSON.stringify(result) }] };
    } catch (error) {
      return { content: [{ type: 'text', text: error.message }], isError: true };
    }
  });
}

register('layerfs_status', 'Inspect registered resources, checkout status and revision', {}, () => operations.status());
register('layerfs_prepare', 'Fetch dependencies into this checkout’s pinned Linux caches', {}, () => operations.prepare());
register('layerfs_build', 'Build and seal the pinned Linux SDK bundle', {
  profile: z.enum(['debug', 'release']).optional(),
  jobs: z.number().int().min(1).max(32).optional(),
}, (input) => operations.build(input));
register('layerfs_cargo_test', 'Build then run one named Cargo integration test on Linux', {
  package: z.string(),
  testTarget: z.string(),
  timeout: z.number().positive().max(100).optional(),
}, (input) => operations.cargoTest(input));
register('layerfs_checkpoint', 'Write a non-destructive binary diff and revision manifest', {}, () => operations.checkpoint());

await serveStdio(server);

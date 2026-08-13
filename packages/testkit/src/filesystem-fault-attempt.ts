import { EphemeralFS } from "@ephemeralai/fs";
import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";
import {
  createStatementFaultController,
  PORTABLE_FAULT_OPERATION_POSITIONS,
  PORTABLE_FAULT_SEED,
  type StatementFaultController,
} from "./fault.js";

const FAULT_POINT = "after-sql-statement";
const FAULT_MESSAGE = "portable injected fault after SQL statement";

export type PortableFilesystemFaultOperation =
  keyof typeof PORTABLE_FAULT_OPERATION_POSITIONS;

export const PORTABLE_FILESYSTEM_FAULT_OPERATIONS = Object.freeze(
  Object.keys(PORTABLE_FAULT_OPERATION_POSITIONS) as PortableFilesystemFaultOperation[],
);

/**
 * Exact topology for isolated per-operation fixtures. The three range mutations read
 * the small current manifest through four additional statements that the cumulative
 * mixed-state matrix satisfies from its already-authenticated cache.
 */
export const PORTABLE_FILESYSTEM_RESTART_FAULT_OPERATION_POSITIONS = Object.freeze({
  ...PORTABLE_FAULT_OPERATION_POSITIONS,
  writeRange: 78,
  replaceRange: 78,
  truncate: 78,
} as const);
export const PORTABLE_FILESYSTEM_RESTART_FAULT_POSITIONS = 1_218;

const payload = Uint8Array.from(
  { length: 64 * 1024 },
  (_, index) => (index * 31 + 7) & 0xff,
);

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable filesystem fault attempt: ${message}`);
}

async function expectMissing(filesystem: EphemeralFS, path: string): Promise<void> {
  try {
    await filesystem.stat(path);
  } catch (error) {
    if (
      error !== null &&
      typeof error === "object" &&
      "code" in error &&
      error.code === "ENOENT"
    )
      return;
    throw error;
  }
  throw new Error(`portable filesystem fault attempt: ${path} unexpectedly exists`);
}

async function expectText(
  filesystem: EphemeralFS,
  path: string,
  expected: string,
): Promise<void> {
  const actual = await filesystem.readFile(path, { encoding: "utf8" });
  invariant(actual === expected, `${path} contained ${JSON.stringify(actual)}`);
}

async function verifyMetadata(filesystem: EphemeralFS): Promise<void> {
  let cursor: string | undefined;
  for (let batch = 0; batch < 100_000; batch += 1) {
    const result = await filesystem.maintenance.verify({
      scopes: ["metadata"],
      ...(cursor === undefined ? {} : { cursor }),
      maxEntities: 256,
    });
    cursor = result.nextCursor ?? undefined;
    if (result.complete) return;
  }
  throw new Error("portable filesystem fault attempt verification did not complete");
}

async function setup(
  filesystem: EphemeralFS,
  operation: PortableFilesystemFaultOperation,
): Promise<void> {
  try {
    await filesystem.stat("/fault");
    return;
  } catch (error) {
    if (
      error === null ||
      typeof error !== "object" ||
      !("code" in error) ||
      error.code !== "ENOENT"
    )
      throw error;
  }
  await filesystem.mkdir("/fault", { recursive: true });
  if (operation === "writeRange") await filesystem.writeFile("/fault/ranges", "abcdef");
  else if (operation === "replaceRange")
    await filesystem.writeFile("/fault/replace", "abcdef");
  else if (operation === "truncate")
    await filesystem.writeFile("/fault/truncate", "abcdef");
  else if (operation === "chmod")
    await filesystem.writeFile("/fault/mode", "mode", { mode: 0o640 });
  else if (operation === "link")
    await filesystem.writeFile("/fault/link-source", "linked");
  else if (operation === "rename")
    await filesystem.writeFile("/fault/rename-source", "renamed");
  else if (operation === "unlink")
    await filesystem.writeFile("/fault/unlink", "removed");
  else if (operation === "rm-recursive") {
    await filesystem.mkdir("/fault/remove/tree", { recursive: true });
    await filesystem.writeFile("/fault/remove/tree/value", "removed-recursively");
  }
}

async function mutate(
  filesystem: EphemeralFS,
  operation: PortableFilesystemFaultOperation,
): Promise<void> {
  switch (operation) {
    case "writeFile-create":
      await filesystem.writeFile("/fault/value", payload, { exclusive: true });
      return;
    case "writeFile-stream":
      await filesystem.writeFile(
        "/fault/streamed",
        new ReadableStream<Uint8Array>({
          start(controller) {
            controller.enqueue(payload.slice(0, 17_000));
            controller.enqueue(payload.slice(17_000, 51_000));
            controller.enqueue(payload.slice(51_000));
            controller.close();
          },
        }),
        { exclusive: true, maxBytes: payload.byteLength },
      );
      return;
    case "writeRange":
      await filesystem.writeRange("/fault/ranges", 2, new TextEncoder().encode("XY"));
      return;
    case "replaceRange":
      await filesystem.replaceRange(
        "/fault/replace",
        1,
        3,
        new TextEncoder().encode("ZZ"),
      );
      return;
    case "truncate":
      await filesystem.truncate("/fault/truncate", 3);
      return;
    case "mkdir":
      await filesystem.mkdir("/fault/mkdir/a/b", { recursive: true });
      return;
    case "chmod":
      await filesystem.chmod("/fault/mode", 0o600);
      return;
    case "link":
      await filesystem.link("/fault/link-source", "/fault/link-alias");
      return;
    case "symlink":
      await filesystem.symlink("link-source", "/fault/link-symbolic");
      return;
    case "rename":
      await filesystem.rename("/fault/rename-source", "/fault/rename-destination");
      return;
    case "unlink":
      await filesystem.unlink("/fault/unlink");
      return;
    case "rm-recursive":
      await filesystem.rm("/fault/remove", { recursive: true });
  }
}

async function verify(
  filesystem: EphemeralFS,
  operation: PortableFilesystemFaultOperation,
  committed: boolean,
): Promise<void> {
  switch (operation) {
    case "writeFile-create": {
      if (!committed) await expectMissing(filesystem, "/fault/value");
      else {
        const actual = await filesystem.readFile("/fault/value");
        invariant(
          actual.byteLength === payload.byteLength &&
            actual.every((byte, index) => byte === payload[index]),
          "committed writeFile bytes differ",
        );
      }
      break;
    }
    case "writeFile-stream": {
      if (!committed) await expectMissing(filesystem, "/fault/streamed");
      else {
        const actual = await filesystem.readFile("/fault/streamed");
        invariant(
          actual.byteLength === payload.byteLength &&
            actual.every((byte, index) => byte === payload[index]),
          "committed streamed bytes differ",
        );
      }
      break;
    }
    case "writeRange":
      await expectText(filesystem, "/fault/ranges", committed ? "abXYef" : "abcdef");
      break;
    case "replaceRange":
      await expectText(filesystem, "/fault/replace", committed ? "aZZef" : "abcdef");
      break;
    case "truncate":
      await expectText(filesystem, "/fault/truncate", committed ? "abc" : "abcdef");
      break;
    case "mkdir":
      if (!committed) await expectMissing(filesystem, "/fault/mkdir");
      else
        invariant(
          (await filesystem.stat("/fault/mkdir/a/b")).isDirectory(),
          "committed directory is missing",
        );
      break;
    case "chmod":
      invariant(
        (await filesystem.stat("/fault/mode")).mode === (committed ? 0o600 : 0o640),
        "mode is not the complete old or new value",
      );
      break;
    case "link": {
      const source = await filesystem.stat("/fault/link-source");
      if (!committed) {
        await expectMissing(filesystem, "/fault/link-alias");
        invariant(source.nlink === 1, "rolled-back link changed nlink");
      } else {
        const alias = await filesystem.stat("/fault/link-alias");
        invariant(
          source.id === alias.id && source.nlink === 2,
          "committed hard link identity differs",
        );
      }
      break;
    }
    case "symlink":
      if (!committed) await expectMissing(filesystem, "/fault/link-symbolic");
      else
        invariant(
          (await filesystem.readlink("/fault/link-symbolic")) === "link-source",
          "committed symbolic-link target differs",
        );
      break;
    case "rename":
      if (!committed) {
        await expectText(filesystem, "/fault/rename-source", "renamed");
        await expectMissing(filesystem, "/fault/rename-destination");
      } else {
        await expectMissing(filesystem, "/fault/rename-source");
        await expectText(filesystem, "/fault/rename-destination", "renamed");
      }
      break;
    case "unlink":
      if (!committed) await expectText(filesystem, "/fault/unlink", "removed");
      else await expectMissing(filesystem, "/fault/unlink");
      break;
    case "rm-recursive":
      if (!committed)
        await expectText(filesystem, "/fault/remove/tree/value", "removed-recursively");
      else await expectMissing(filesystem, "/fault/remove");
      break;
  }
  await verifyMetadata(filesystem);
}

export interface PortableFilesystemFaultAttemptResult {
  readonly operation: PortableFilesystemFaultOperation;
  readonly occurrence: number;
  readonly injected: boolean;
  readonly observedStatements: number;
  readonly seed: typeof PORTABLE_FAULT_SEED;
}

/**
 * Execute one selected mutation occurrence without orderly close. The caller owns the
 * physical driver/runtime restart before invoking `verifyFilesystemFaultAttempt`.
 */
export async function prepareFilesystemFaultAttempt(
  adapter: FilesystemSQLiteDriver,
  operation: PortableFilesystemFaultOperation,
  occurrence: number,
  faults: StatementFaultController = createStatementFaultController(),
): Promise<PortableFilesystemFaultAttemptResult> {
  invariant(
    Number.isSafeInteger(occurrence) && occurrence > 0,
    "occurrence must be a positive safe integer",
  );
  const filesystem = await EphemeralFS.open({
    database: faults.wrap(adapter),
    ownsDatabase: false,
    storage: { maxGcBatchSize: 8, maxQueryBatchSize: 16 },
  });
  await setup(filesystem, operation);
  faults.arm(FAULT_POINT, occurrence);
  let injected = false;
  let observedStatements: number;
  try {
    await mutate(filesystem, operation);
  } catch (error) {
    if (!String(error).includes(FAULT_MESSAGE)) throw error;
    injected = true;
  } finally {
    observedStatements = faults.statementCount();
    faults.clear();
  }
  return Object.freeze({
    operation,
    occurrence,
    injected,
    observedStatements,
    seed: PORTABLE_FAULT_SEED,
  });
}

/** Verify complete old/new state after the caller has physically restarted storage. */
export async function verifyFilesystemFaultAttempt(
  adapter: FilesystemSQLiteDriver,
  operation: PortableFilesystemFaultOperation,
  committed: boolean,
): Promise<void> {
  const filesystem = await EphemeralFS.open({
    database: adapter,
    ownsDatabase: false,
    storage: { maxGcBatchSize: 8, maxQueryBatchSize: 16 },
  });
  try {
    await verify(filesystem, operation, committed);
  } finally {
    await filesystem.close();
  }
}

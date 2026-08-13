import { EphemeralFS } from "@ephemeralai/fs";
import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  SqliteBindings,
  SqliteRow,
} from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory, ConformanceFaultController } from "./index.js";

const FAULT_POINT = "after-sql-statement";
const FAULT_MESSAGE = "portable injected fault after SQL statement";
export const PORTABLE_FAULT_SEED = 0xfa017;
export const PORTABLE_FAULT_OPERATION_POSITIONS = Object.freeze({
  "writeFile-create": 214,
  "writeFile-stream": 214,
  writeRange: 74,
  replaceRange: 74,
  truncate: 74,
  mkdir: 175,
  chmod: 29,
  link: 70,
  symlink: 59,
  rename: 60,
  unlink: 49,
  "rm-recursive": 114,
} as const);
export const PORTABLE_FAULT_POSITIONS = 1_206;

export interface StatementFaultController extends ConformanceFaultController {
  wrap(driver: FilesystemSQLiteDriver): FilesystemSQLiteDriver;
  statementCount(): number;
}

/** Adapter-neutral statement fault injection used by both required SQLite drivers. */
export function createStatementFaultController(): StatementFaultController {
  let target: number | undefined;
  let position = 0;
  const afterStatement = (): void => {
    position += 1;
    if (target === position) throw new Error(`${FAULT_MESSAGE} ${position}`);
  };
  return Object.freeze({
    arm(point: string, occurrence = 1): void {
      if (point !== FAULT_POINT || !Number.isSafeInteger(occurrence) || occurrence <= 0)
        throw new RangeError("invalid portable SQL fault point");
      target = occurrence;
      position = 0;
    },
    clear(): void {
      target = undefined;
      position = 0;
    },
    statementCount(): number {
      return position;
    },
    wrap(driver: FilesystemSQLiteDriver): FilesystemSQLiteDriver {
      return Object.freeze({
        kind: driver.kind,
        readOnly: driver.readOnly,
        capabilities: driver.capabilities,
        ...(driver.hashBytes === undefined
          ? {}
          : { hashBytes: driver.hashBytes.bind(driver) }),
        ...(driver.hashBytesAsync === undefined
          ? {}
          : { hashBytesAsync: driver.hashBytesAsync.bind(driver) }),
        transaction<T>(
          mode: "read" | "write" | "exclusive",
          callback: (tx: FilesystemSQLiteTransaction) => T,
        ): T {
          return driver.transaction(mode, (tx) =>
            callback(
              Object.freeze({
                scope: tx.scope,
                run(sql: string, bindings: SqliteBindings = []) {
                  const result = tx.run(sql, bindings);
                  afterStatement();
                  return result;
                },
                all<Row extends SqliteRow = SqliteRow>(
                  sql: string,
                  bindings: SqliteBindings,
                  budget: { readonly maxRows: number; readonly maxBytes: number },
                ): readonly Row[] {
                  const rows = tx.all<Row>(sql, bindings, budget);
                  afterStatement();
                  return rows;
                },
              }),
            ),
          );
        },
        physicalStorage: () => driver.physicalStorage?.() ?? Object.freeze({}),
        ...(driver.checkpoint === undefined
          ? {}
          : { checkpoint: driver.checkpoint.bind(driver) }),
        close: () => driver.close(),
      });
    },
  });
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
  throw new Error("portable fault matrix exposed a partially committed file");
}

async function expectText(
  filesystem: EphemeralFS,
  path: string,
  expected: string,
): Promise<void> {
  const actual = await filesystem.readFile(path, { encoding: "utf8" });
  if (actual !== expected)
    throw new Error(
      `portable fault matrix expected ${JSON.stringify(expected)} at ${path}, received ${JSON.stringify(actual)}`,
    );
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
  throw new Error("portable fault-matrix verification did not complete");
}

export interface PortableFaultMatrixResult {
  readonly schema: "efs-portable-fault-result-v1";
  readonly adapter: string;
  readonly seed: typeof PORTABLE_FAULT_SEED;
  readonly fixtureDigest: string;
  readonly faultPoint: typeof FAULT_POINT;
  readonly positions: number;
  readonly payloadBytes: number;
  readonly operationPositions: Readonly<Record<string, number>>;
}

/**
 * Fail after every SQL statement in every public filesystem mutation family.
 * Every injected position must reopen to the complete old state; the first
 * position beyond each operation must reopen to the complete new state.
 */
export async function runFilesystemFaultMatrix(
  factory: ConformanceAdapterFactory,
): Promise<PortableFaultMatrixResult> {
  const fixture = await factory.create({
    label: "portable-fault",
    seed: PORTABLE_FAULT_SEED,
  });
  if (
    !fixture.capabilities.includes("fault-injection") ||
    fixture.faults === undefined
  ) {
    await fixture.dispose();
    throw new Error("portable fault matrix requires the fault-injection capability");
  }
  const payload = Uint8Array.from(
    { length: 64 * 1024 },
    (_, index) => (index * 31 + 7) & 0xff,
  );
  const digest = fixture.adapter.hashBytes
    ? fixture.adapter.hashBytes(payload)
    : await fixture.adapter.hashBytesAsync?.(payload);
  if (!(digest instanceof Uint8Array) || digest.byteLength !== 32) {
    await fixture.dispose();
    throw new Error("portable fault matrix requires SHA-256 fixture identity");
  }
  const fixtureDigest = [...digest]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  let adapter = fixture.adapter;
  let filesystem: EphemeralFS | undefined;
  let faultPositions = 0;
  const operationPositions: Record<string, number> = {};
  try {
    filesystem = await EphemeralFS.open({
      database: adapter,
      ownsDatabase: false,
      storage: { maxGcBatchSize: 8, maxQueryBatchSize: 16 },
    });
    await filesystem.mkdir("/fault", { recursive: true });
    const reopen = async (): Promise<void> => {
      await filesystem!.close();
      filesystem = undefined;
      adapter.close();
      adapter = await fixture.reopen({ physical: true });
      filesystem = await EphemeralFS.open({
        database: adapter,
        ownsDatabase: false,
        storage: { maxGcBatchSize: 8, maxQueryBatchSize: 16 },
      });
    };
    const runOperation = async (
      label: string,
      operation: (selected: EphemeralFS) => Promise<void>,
      assertOld: (selected: EphemeralFS) => Promise<void>,
      assertNew: (selected: EphemeralFS) => Promise<void>,
    ): Promise<void> => {
      for (let occurrence = 1; occurrence <= 512; occurrence += 1) {
        fixture.faults!.arm(FAULT_POINT, occurrence);
        let injected = false;
        try {
          await operation(filesystem!);
        } catch (error) {
          if (!String(error).includes(FAULT_MESSAGE)) throw error;
          injected = true;
          faultPositions += 1;
          operationPositions[label] = (operationPositions[label] ?? 0) + 1;
        } finally {
          fixture.faults!.clear();
        }
        await reopen();
        if (!injected) {
          await assertNew(filesystem!);
          await verifyMetadata(filesystem!);
          return;
        }
        await assertOld(filesystem!);
        await verifyMetadata(filesystem!);
      }
      throw new Error(
        `portable fault matrix ${label} exceeded its finite position cap`,
      );
    };

    await runOperation(
      "writeFile-create",
      (selected) => selected.writeFile("/fault/value", payload, { exclusive: true }),
      (selected) => expectMissing(selected, "/fault/value"),
      async (selected) => {
        const actual = await selected.readFile("/fault/value");
        if (
          actual.byteLength !== payload.byteLength ||
          actual.some((byte, index) => byte !== payload[index])
        )
          throw new Error("portable fault matrix committed incorrect bytes");
      },
    );

    await runOperation(
      "writeFile-stream",
      (selected) =>
        selected.writeFile(
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
        ),
      (selected) => expectMissing(selected, "/fault/streamed"),
      async (selected) => {
        const actual = await selected.readFile("/fault/streamed");
        if (!actual.every((byte, index) => byte === payload[index]))
          throw new Error("portable streamed fault matrix committed incorrect bytes");
      },
    );

    await filesystem.writeFile("/fault/ranges", "abcdef");
    await runOperation(
      "writeRange",
      (selected) =>
        selected.writeRange("/fault/ranges", 2, new TextEncoder().encode("XY")),
      (selected) => expectText(selected, "/fault/ranges", "abcdef"),
      (selected) => expectText(selected, "/fault/ranges", "abXYef"),
    );
    await filesystem.writeFile("/fault/replace", "abcdef");
    await runOperation(
      "replaceRange",
      (selected) =>
        selected.replaceRange("/fault/replace", 1, 3, new TextEncoder().encode("ZZ")),
      (selected) => expectText(selected, "/fault/replace", "abcdef"),
      (selected) => expectText(selected, "/fault/replace", "aZZef"),
    );
    await filesystem.writeFile("/fault/truncate", "abcdef");
    await runOperation(
      "truncate",
      (selected) => selected.truncate("/fault/truncate", 3),
      (selected) => expectText(selected, "/fault/truncate", "abcdef"),
      (selected) => expectText(selected, "/fault/truncate", "abc"),
    );
    await runOperation(
      "mkdir",
      (selected) => selected.mkdir("/fault/mkdir/a/b", { recursive: true }),
      (selected) => expectMissing(selected, "/fault/mkdir"),
      async (selected) => {
        if (!(await selected.stat("/fault/mkdir/a/b")).isDirectory())
          throw new Error("portable mkdir fault matrix did not commit a directory");
      },
    );
    await filesystem.writeFile("/fault/mode", "mode", { mode: 0o640 });
    await runOperation(
      "chmod",
      (selected) => selected.chmod("/fault/mode", 0o600),
      async (selected) => {
        if ((await selected.stat("/fault/mode")).mode !== 0o640)
          throw new Error("portable chmod fault matrix changed the old mode");
      },
      async (selected) => {
        if ((await selected.stat("/fault/mode")).mode !== 0o600)
          throw new Error("portable chmod fault matrix did not commit the new mode");
      },
    );
    await filesystem.writeFile("/fault/link-source", "linked");
    const linkSourceId = (await filesystem.stat("/fault/link-source")).id;
    await runOperation(
      "link",
      (selected) => selected.link("/fault/link-source", "/fault/link-alias"),
      async (selected) => {
        await expectMissing(selected, "/fault/link-alias");
        if ((await selected.stat("/fault/link-source")).nlink !== 1)
          throw new Error("portable link fault matrix changed the old link count");
      },
      async (selected) => {
        const source = await selected.stat("/fault/link-source");
        const alias = await selected.stat("/fault/link-alias");
        if (
          source.id !== linkSourceId ||
          alias.id !== linkSourceId ||
          source.nlink !== 2
        )
          throw new Error("portable link fault matrix committed incorrect identity");
      },
    );
    await runOperation(
      "symlink",
      (selected) => selected.symlink("link-source", "/fault/link-symbolic"),
      (selected) => expectMissing(selected, "/fault/link-symbolic"),
      async (selected) => {
        if ((await selected.readlink("/fault/link-symbolic")) !== "link-source")
          throw new Error("portable symlink fault matrix committed the wrong target");
      },
    );
    await filesystem.writeFile("/fault/rename-source", "renamed");
    await runOperation(
      "rename",
      (selected) =>
        selected.rename("/fault/rename-source", "/fault/rename-destination"),
      async (selected) => {
        await expectText(selected, "/fault/rename-source", "renamed");
        await expectMissing(selected, "/fault/rename-destination");
      },
      async (selected) => {
        await expectMissing(selected, "/fault/rename-source");
        await expectText(selected, "/fault/rename-destination", "renamed");
      },
    );
    await filesystem.writeFile("/fault/unlink", "removed");
    await runOperation(
      "unlink",
      (selected) => selected.unlink("/fault/unlink"),
      (selected) => expectText(selected, "/fault/unlink", "removed"),
      (selected) => expectMissing(selected, "/fault/unlink"),
    );
    await filesystem.mkdir("/fault/remove/tree", { recursive: true });
    await filesystem.writeFile("/fault/remove/tree/value", "removed-recursively");
    await runOperation(
      "rm-recursive",
      (selected) => selected.rm("/fault/remove", { recursive: true }),
      (selected) =>
        expectText(selected, "/fault/remove/tree/value", "removed-recursively"),
      (selected) => expectMissing(selected, "/fault/remove"),
    );

    if (faultPositions !== PORTABLE_FAULT_POSITIONS)
      throw new Error(
        `portable fault topology changed (${faultPositions} != ${PORTABLE_FAULT_POSITIONS})`,
      );
    if (
      JSON.stringify(operationPositions) !==
      JSON.stringify(PORTABLE_FAULT_OPERATION_POSITIONS)
    )
      throw new Error("portable per-operation fault topology changed");
    return Object.freeze({
      schema: "efs-portable-fault-result-v1",
      adapter: factory.name,
      seed: PORTABLE_FAULT_SEED,
      fixtureDigest,
      faultPoint: FAULT_POINT,
      positions: faultPositions,
      payloadBytes: payload.byteLength,
      operationPositions: Object.freeze({ ...operationPositions }),
    });
  } finally {
    fixture.faults.clear();
    try {
      await filesystem?.close();
    } catch {}
    try {
      adapter.close();
    } catch {}
    await fixture.dispose();
  }
}

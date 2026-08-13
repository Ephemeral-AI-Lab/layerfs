import { EphemeralFS } from "@ephemeralai/fs";
import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  SqliteBindings,
  SqliteRow,
} from "@ephemeralai/fs/sqlite-driver";

export type PortablePublicationFaultVariant = "direct" | "prepared";
export const PORTABLE_PUBLICATION_FAULT_POSITIONS = Object.freeze({
  direct: 95,
  prepared: 91,
} as const);

export interface PortablePublicationFaultAttempt {
  readonly schema: "efs-portable-publication-fault-attempt-v1";
  readonly variant: PortablePublicationFaultVariant;
  readonly occurrence: number;
  readonly maxTransactionStatements: number;
  readonly injected: boolean;
}

const DIRECT_BRANCH = "portable-publication-direct";
const PREPARED_BRANCH = "portable-publication-prepared";

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable publication fault: ${message}`);
}

function selectedBranch(variant: PortablePublicationFaultVariant): string {
  return variant === "direct" ? DIRECT_BRANCH : PREPARED_BRANCH;
}

function selectedOperation(variant: PortablePublicationFaultVariant): string {
  return `${selectedBranch(variant)}-operation`;
}

function faultingDriver(
  adapter: FilesystemSQLiteDriver,
  occurrence: number,
  state: { armed: boolean; maxPosition: number },
): FilesystemSQLiteDriver {
  return Object.freeze({
    kind: adapter.kind,
    readOnly: adapter.readOnly,
    capabilities: adapter.capabilities,
    ...(adapter.hashBytes === undefined
      ? {}
      : { hashBytes: adapter.hashBytes.bind(adapter) }),
    ...(adapter.hashBytesAsync === undefined
      ? {}
      : { hashBytesAsync: adapter.hashBytesAsync.bind(adapter) }),
    transaction<T>(
      mode: "read" | "write" | "exclusive",
      callback: (tx: FilesystemSQLiteTransaction) => T,
    ): T {
      return adapter.transaction(mode, (tx) => {
        if (!state.armed) return callback(tx);
        let position = 0;
        const afterStatement = (): void => {
          position += 1;
          state.maxPosition = Math.max(state.maxPosition, position);
          if (position === occurrence) {
            state.armed = false;
            throw new Error(`portable publication fault ${occurrence}`);
          }
        };
        return callback(
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
              const result = tx.all<Row>(sql, bindings, budget);
              afterStatement();
              return result;
            },
          }),
        );
      });
    },
    physicalStorage: () => adapter.physicalStorage?.() ?? Object.freeze({}),
    ...(adapter.checkpoint === undefined
      ? {}
      : { checkpoint: adapter.checkpoint.bind(adapter) }),
    close: () => adapter.close(),
  });
}

async function prepare(
  adapter: FilesystemSQLiteDriver,
  variant: PortablePublicationFaultVariant,
): Promise<void> {
  const filesystem = await EphemeralFS.open({ database: adapter, ownsDatabase: false });
  try {
    if (variant === "direct") {
      await filesystem.writeFile("/publication-main", "base");
      const branch = await filesystem.branches.create(DIRECT_BRANCH);
      await branch.writeFile("/publication-branch", "branch-value");
      await branch.close();
    } else {
      await filesystem.writeFile(
        "/publication-candidate",
        new Uint8Array(20_000).fill(1),
      );
      const branch = await filesystem.branches.create(PREPARED_BRANCH);
      await branch.writeRange("/publication-candidate", 0, Uint8Array.of(2));
      await branch.close();
    }
  } finally {
    await filesystem.close();
  }
}

async function assertOldState(
  filesystem: EphemeralFS,
  adapter: FilesystemSQLiteDriver,
  variant: PortablePublicationFaultVariant,
): Promise<void> {
  const branch = await filesystem.branches.open(selectedBranch(variant));
  try {
    invariant((await branch.info()).state === "active", "fault made branch terminal");
    if (variant === "direct") {
      invariant(
        (await filesystem.readFile("/publication-main", { encoding: "utf8" })) ===
          "base",
        "fault changed main bytes",
      );
      invariant(
        (await branch.readFile("/publication-branch", { encoding: "utf8" })) ===
          "branch-value",
        "fault changed branch bytes",
      );
    } else {
      invariant(
        (
          await filesystem.readRange("/publication-candidate", {
            offset: 0,
            length: 4,
          })
        ).every((byte) => byte === 1),
        "fault changed prepared main bytes",
      );
      const selected = await branch.readRange("/publication-candidate", {
        offset: 0,
        length: 4,
      });
      invariant(
        selected[0] === 2 && selected.slice(1).every((byte) => byte === 1),
        "fault changed prepared branch bytes",
      );
    }
  } finally {
    await branch.close();
  }
  const durable = adapter.transaction(
    "read",
    (tx) =>
      tx.all<{
        readonly state: number;
        readonly results: number;
        readonly reservations: number;
      }>(
        "SELECT (SELECT state FROM efs_branches WHERE id=?) state,(SELECT count(*) FROM efs_operation_results WHERE operation_id=?) results,(SELECT count(*) FROM efs_operation_ids WHERE id=?) reservations",
        [
          selectedBranch(variant),
          selectedOperation(variant),
          selectedOperation(variant),
        ],
        { maxRows: 1, maxBytes: 256 },
      )[0],
  );
  invariant(durable?.state === 0, "fault durably changed branch state");
  invariant(durable.results === 0, "fault exposed a partial publication result");
  invariant(
    durable.reservations === 0 || durable.reservations === 1,
    "fault created duplicate operation reservations",
  );
}

/** Run one fresh publication attempt with a fault at one final-transaction position. */
export async function runPortablePublicationFaultAttempt(
  adapter: FilesystemSQLiteDriver,
  variant: PortablePublicationFaultVariant,
  occurrence: number,
): Promise<PortablePublicationFaultAttempt> {
  invariant(
    Number.isSafeInteger(occurrence) && occurrence > 0,
    "invalid fault occurrence",
  );
  await prepare(adapter, variant);
  const control = { armed: false, maxPosition: 0 };
  const selected = faultingDriver(adapter, occurrence, control);
  const filesystem = await EphemeralFS.open({
    database: selected,
    ownsDatabase: false,
  });
  const branch = await filesystem.branches.open(selectedBranch(variant));
  control.armed = true;
  let injected = false;
  try {
    const result = await branch.publish({ operationId: selectedOperation(variant) });
    invariant(result.outcome === "merged", "unfaulted publication did not merge");
  } catch (error) {
    if (!String(error).includes(`portable publication fault ${occurrence}`))
      throw error;
    injected = true;
  } finally {
    control.armed = false;
  }
  if (injected) await assertOldState(filesystem, adapter, variant);
  else {
    invariant(
      occurrence > control.maxPosition,
      "publication succeeded before the selected fault position",
    );
  }
  await branch.close();
  await filesystem.close();
  return Object.freeze({
    schema: "efs-portable-publication-fault-attempt-v1",
    variant,
    occurrence,
    maxTransactionStatements: control.maxPosition,
    injected,
  });
}

/** Verify old state after the caller has physically recreated the driver/runtime. */
export async function verifyPortablePublicationFaultRecovery(
  adapter: FilesystemSQLiteDriver,
  variant: PortablePublicationFaultVariant,
): Promise<void> {
  const filesystem = await EphemeralFS.open({ database: adapter, ownsDatabase: false });
  try {
    await assertOldState(filesystem, adapter, variant);
  } finally {
    await filesystem.close();
  }
}

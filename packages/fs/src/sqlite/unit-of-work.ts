import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
  QueryBudget,
  SqliteBindings,
  SqliteRow,
  SqliteRunResult,
  TransactionMode,
} from "./driver.js";
import { intrinsicByteLength } from "../cas/bytes.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";

export interface TransactionLimits {
  readonly maxRows: number;
  readonly maxBytes: number;
  readonly maxStatements?: number;
  readonly maxElapsedMs?: number;
  readonly maxResultRows?: number;
  readonly maxResultBytes?: number;
}

function bindingBytes(bindings: SqliteBindings): number {
  return bindings.reduce<number>(
    (sum, value) =>
      checkedAdd(
        sum,
        value instanceof Uint8Array
          ? intrinsicByteLength(value)
          : typeof value === "string"
            ? checkedMultiply(value.length, 2, "SQLite string binding")
            : 8,
        "SQLite bindings",
      ),
    0,
  );
}
function valueBytes(value: SqliteRow[string]): number {
  return value instanceof Uint8Array
    ? intrinsicByteLength(value)
    : typeof value === "string"
      ? checkedMultiply(value.length, 2, "SQLite string result")
      : 8;
}
function resultBytes(rows: readonly SqliteRow[]): number {
  return rows.reduce(
    (sum, row) =>
      checkedAdd(
        sum,
        checkedAdd(
          32,
          Object.entries(row).reduce(
            (rowSum, [name, value]) =>
              checkedAdd(
                rowSum,
                checkedAdd(
                  checkedMultiply(name.length, 2, "SQLite result column name"),
                  valueBytes(value),
                ),
                "SQLite result row",
              ),
            0,
          ),
        ),
        "SQLite results",
      ),
    0,
  );
}

export function runUnitOfWork<T>(
  driver: FilesystemSQLiteDriver,
  mode: TransactionMode,
  limits: TransactionLimits,
  callback: (tx: FilesystemSQLiteTransaction) => T,
): T {
  if (
    !Number.isSafeInteger(limits.maxRows) ||
    limits.maxRows <= 0 ||
    !Number.isSafeInteger(limits.maxBytes) ||
    limits.maxBytes <= 0
  )
    throw new RangeError("invalid transaction limits");
  return driver.transaction(mode, (tx) => {
    const maxStatements = limits.maxStatements ?? Math.max(16, limits.maxRows * 4);
    const maxElapsedMs = limits.maxElapsedMs ?? 5_000;
    const maxResultRows = limits.maxResultRows ?? limits.maxRows;
    const maxResultBytes = limits.maxResultBytes ?? limits.maxBytes;
    for (const [name, value] of [
      ["maxStatements", maxStatements],
      ["maxElapsedMs", maxElapsedMs],
      ["maxResultRows", maxResultRows],
      ["maxResultBytes", maxResultBytes],
    ] as const)
      if (!Number.isSafeInteger(value) || value <= 0)
        throw new RangeError(`invalid transaction ${name}`);
    const started = performance.now();
    let changedRows = 0;
    let boundBytes = 0;
    let statements = 0;
    let returnedRows = 0;
    let returnedBytes = 0;
    const statement = (): void => {
      statements += 1;
      if (statements > maxStatements)
        throw new RangeError("transaction statement limit exceeded");
      if (performance.now() - started > maxElapsedMs)
        throw new RangeError("transaction elapsed-time limit exceeded");
    };
    const account = (bindings: SqliteBindings): void => {
      boundBytes = checkedAdd(boundBytes, bindingBytes(bindings), "SQLite bindings");
      if (boundBytes > limits.maxBytes)
        throw new RangeError("final transaction byte limit exceeded");
    };
    const bounded: FilesystemSQLiteTransaction = Object.freeze({
      scope: tx.scope,
      run(sql: string, bindings: SqliteBindings = []): SqliteRunResult {
        statement();
        account(bindings);
        const result = tx.run(sql, bindings);
        changedRows = checkedAdd(changedRows, result.changes, "changed SQLite rows");
        if (changedRows > limits.maxRows)
          throw new RangeError("final transaction row limit exceeded");
        return result;
      },
      all<Row extends SqliteRow = SqliteRow>(
        sql: string,
        bindings: SqliteBindings,
        budget: QueryBudget,
      ): readonly Row[] {
        statement();
        account(bindings);
        if (
          !Number.isSafeInteger(budget.maxRows) ||
          budget.maxRows <= 0 ||
          !Number.isSafeInteger(budget.maxBytes) ||
          budget.maxBytes <= 0
        )
          throw new RangeError("invalid query budget");
        const remainingRows = maxResultRows - returnedRows;
        const remainingBytes = maxResultBytes - returnedBytes;
        if (remainingRows <= 0)
          throw new RangeError("transaction result row limit exhausted");
        if (remainingBytes <= 0)
          throw new RangeError("transaction result byte limit exhausted");
        const rows = tx.all<Row>(sql, bindings, {
          maxRows: Math.min(budget.maxRows, remainingRows),
          maxBytes: Math.min(budget.maxBytes, remainingBytes),
        });
        returnedRows = checkedAdd(returnedRows, rows.length, "returned SQLite rows");
        returnedBytes = checkedAdd(
          returnedBytes,
          resultBytes(rows),
          "returned SQLite bytes",
        );
        if (returnedRows > maxResultRows)
          throw new RangeError("transaction result row limit exceeded");
        if (returnedBytes > maxResultBytes)
          throw new RangeError("transaction result byte limit exceeded");
        return rows;
      },
    });
    const result = callback(bounded);
    if (performance.now() - started > maxElapsedMs)
      throw new RangeError("transaction elapsed-time limit exceeded");
    return result;
  });
}

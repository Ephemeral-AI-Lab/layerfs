import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, QueryBudget, SqliteBindings, SqliteRow, SqliteRunResult, TransactionMode } from "../sqlite-driver.js";

export interface TransactionLimits { readonly maxRows: number; readonly maxBytes: number }

function bindingBytes(bindings: SqliteBindings): number {
  return bindings.reduce<number>((sum, value) => sum + (value instanceof Uint8Array ? value.byteLength : typeof value === "string" ? value.length * 2 : 8), 0);
}

export function runUnitOfWork<T>(driver: FilesystemSQLiteDriver, mode: TransactionMode, limits: TransactionLimits, callback: (tx: FilesystemSQLiteTransaction) => T): T {
  if (!Number.isSafeInteger(limits.maxRows) || limits.maxRows <= 0 || !Number.isSafeInteger(limits.maxBytes) || limits.maxBytes <= 0) throw new RangeError("invalid transaction limits");
  return driver.transaction(mode, (tx) => {
    let changedRows = 0; let boundBytes = 0;
    const account = (bindings: SqliteBindings): void => {
      boundBytes += bindingBytes(bindings);
      if (boundBytes > limits.maxBytes) throw new RangeError("final transaction byte limit exceeded");
    };
    const bounded: FilesystemSQLiteTransaction = Object.freeze({
      scope: tx.scope,
      run(sql: string, bindings: SqliteBindings = []): SqliteRunResult {
        account(bindings); const result = tx.run(sql, bindings); changedRows += result.changes;
        if (changedRows > limits.maxRows) throw new RangeError("final transaction row limit exceeded"); return result;
      },
      all<Row extends SqliteRow = SqliteRow>(sql: string, bindings: SqliteBindings, budget: QueryBudget): readonly Row[] { account(bindings); return tx.all<Row>(sql, bindings, budget); },
    });
    return callback(bounded);
  });
}

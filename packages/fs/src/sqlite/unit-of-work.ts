import type { FilesystemSQLiteDriver, FilesystemSQLiteTransaction, QueryBudget, SqliteBindings, SqliteRow, SqliteRunResult, TransactionMode } from "../sqlite-driver.js";

export interface TransactionLimits { readonly maxRows: number; readonly maxBytes: number; readonly maxStatements?: number; readonly maxElapsedMs?: number; readonly maxResultRows?: number; readonly maxResultBytes?: number }

function bindingBytes(bindings: SqliteBindings): number {
  return bindings.reduce<number>((sum, value) => sum + (value instanceof Uint8Array ? value.byteLength : typeof value === "string" ? value.length * 2 : 8), 0);
}
function valueBytes(value: SqliteRow[string]): number { return value instanceof Uint8Array ? value.byteLength : typeof value === "string" ? value.length * 2 : 8; }
function resultBytes(rows: readonly SqliteRow[]): number { return rows.reduce((sum, row) => sum + 32 + Object.entries(row).reduce((rowSum, [name, value]) => rowSum + name.length * 2 + valueBytes(value), 0), 0); }

export function runUnitOfWork<T>(driver: FilesystemSQLiteDriver, mode: TransactionMode, limits: TransactionLimits, callback: (tx: FilesystemSQLiteTransaction) => T): T {
  if (!Number.isSafeInteger(limits.maxRows) || limits.maxRows <= 0 || !Number.isSafeInteger(limits.maxBytes) || limits.maxBytes <= 0) throw new RangeError("invalid transaction limits");
  return driver.transaction(mode, (tx) => {
    const maxStatements = limits.maxStatements ?? Math.max(16, limits.maxRows * 4); const maxElapsedMs = limits.maxElapsedMs ?? 5_000; const maxResultRows = limits.maxResultRows ?? limits.maxRows; const maxResultBytes = limits.maxResultBytes ?? limits.maxBytes;
    for (const [name, value] of [["maxStatements", maxStatements], ["maxElapsedMs", maxElapsedMs], ["maxResultRows", maxResultRows], ["maxResultBytes", maxResultBytes]] as const) if (!Number.isSafeInteger(value) || value <= 0) throw new RangeError(`invalid transaction ${name}`);
    const started = performance.now(); let changedRows = 0; let boundBytes = 0; let statements = 0; let returnedRows = 0; let returnedBytes = 0;
    const statement = (): void => { statements += 1; if (statements > maxStatements) throw new RangeError("transaction statement limit exceeded"); if (performance.now() - started > maxElapsedMs) throw new RangeError("transaction elapsed-time limit exceeded"); };
    const account = (bindings: SqliteBindings): void => {
      boundBytes += bindingBytes(bindings);
      if (boundBytes > limits.maxBytes) throw new RangeError("final transaction byte limit exceeded");
    };
    const bounded: FilesystemSQLiteTransaction = Object.freeze({
      scope: tx.scope,
      run(sql: string, bindings: SqliteBindings = []): SqliteRunResult {
        statement(); account(bindings); const result = tx.run(sql, bindings); changedRows += result.changes;
        if (changedRows > limits.maxRows) throw new RangeError("final transaction row limit exceeded"); return result;
      },
      all<Row extends SqliteRow = SqliteRow>(sql: string, bindings: SqliteBindings, budget: QueryBudget): readonly Row[] { statement(); account(bindings); const rows = tx.all<Row>(sql, bindings, budget); returnedRows += rows.length; returnedBytes += resultBytes(rows); if (returnedRows > maxResultRows) throw new RangeError("transaction result row limit exceeded"); if (returnedBytes > maxResultBytes) throw new RangeError("transaction result byte limit exceeded"); return rows; },
    });
    const result = callback(bounded); if (performance.now() - started > maxElapsedMs) throw new RangeError("transaction elapsed-time limit exceeded"); return result;
  });
}

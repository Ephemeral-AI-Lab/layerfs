/**
 * Wrap a SQLite driver and fail only after a write transaction has committed.
 * A target durable-statement ordinal maps to the transaction that made that
 * statement durable. Tests still visit every observed ordinal, including
 * ordinals that share a transaction, without pretending that a partially
 * committed SQLite transaction can exist.
 */
export function maintenanceFaultInjector(driver, { captureTrace = true } = {}) {
  const state = {
    armed: false,
    failAfterStatement: undefined,
    failAfterBatch: undefined,
    durableStatements: 0,
    executedStatements: 0,
    transactions: 0,
    committedBatches: 0,
    maxBatchStatements: 0,
    trace: [],
  };

  const fault = (kind, ordinal) => {
    state.armed = false;
    // An abrupt process stop cannot run the manager's ordinary error cleanup.
    // AbortError follows that same no-abandon path while remaining observable.
    const error = new DOMException(
      `maintenance ${kind} fault after ${ordinal}`,
      "AbortError",
    );
    Object.defineProperties(error, {
      faultKind: { value: kind, enumerable: true },
      faultOrdinal: { value: ordinal, enumerable: true },
    });
    return error;
  };

  const wrapped = Object.freeze({
    kind: driver.kind,
    readOnly: driver.readOnly,
    capabilities: driver.capabilities,
    hashBytes: driver.hashBytes?.bind(driver),
    hashBytesAsync: driver.hashBytesAsync?.bind(driver),
    transaction(mode, callback) {
      const observing = state.armed;
      const statements = [];
      let executed = 0;
      const result = driver.transaction(mode, (tx) =>
        callback({
          scope: tx.scope,
          run(sql, bindings = []) {
            if (observing) executed += 1;
            const value = tx.run(sql, bindings);
            statements.push(sql);
            return value;
          },
          all(sql, bindings, budget) {
            if (observing) executed += 1;
            return tx.all(sql, bindings, budget);
          },
        }),
      );
      if (!observing) return result;
      state.executedStatements += executed;
      state.transactions += 1;
      if (mode === "read") return result;

      // Read-only write transactions do not create a durable maintenance
      // boundary. Every actual mutation in this codebase goes through run().
      if (statements.length === 0) return result;
      state.committedBatches += 1;
      const batch = state.committedBatches;
      state.maxBatchStatements = Math.max(state.maxBatchStatements, statements.length);
      let matchedStatement;
      for (const [transactionStatement, sql] of statements.entries()) {
        state.durableStatements += 1;
        const statement = state.durableStatements;
        if (captureTrace)
          state.trace.push(
            Object.freeze({
              batch,
              statement,
              transactionStatement: transactionStatement + 1,
              sql,
            }),
          );
        if (state.failAfterStatement === statement) matchedStatement = statement;
      }
      if (matchedStatement !== undefined) throw fault("statement", matchedStatement);
      if (state.failAfterBatch === batch) throw fault("batch", batch);
      return result;
    },
    physicalStorage: () => driver.physicalStorage?.(),
    checkpoint: (mode) => driver.checkpoint?.(mode),
    close: () => driver.close(),
  });

  return Object.freeze({
    driver: wrapped,
    arm({ afterStatement, afterBatch } = {}) {
      if (
        (afterStatement === undefined) === (afterBatch === undefined) ||
        (afterStatement !== undefined &&
          (!Number.isSafeInteger(afterStatement) || afterStatement <= 0)) ||
        (afterBatch !== undefined &&
          (!Number.isSafeInteger(afterBatch) || afterBatch <= 0))
      )
        throw new RangeError("select exactly one positive maintenance fault ordinal");
      state.armed = true;
      state.failAfterStatement = afterStatement;
      state.failAfterBatch = afterBatch;
    },
    disarm() {
      state.armed = false;
    },
    metrics() {
      return Object.freeze({
        durableStatements: state.durableStatements,
        executedStatements: state.executedStatements,
        transactions: state.transactions,
        committedBatches: state.committedBatches,
        maxBatchStatements: state.maxBatchStatements,
        trace: Object.freeze([...state.trace]),
      });
    },
  });
}

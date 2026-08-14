import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  EFS_APPLICATION_ID,
  EFS_SCHEMA_VERSION,
  EFS_UNBOUND_REPLICA_MARKER_ID,
  initializeOrValidateSchema,
  initializeOrValidateUnboundReplicaSchema,
} from "../../packages/fs/dist/sqlite/schema.js";

async function removeTree(target) {
  await rm(target, { recursive: true, force: true });
}

function inspectUnbound(driver) {
  return driver.transaction("read", (tx) => ({
    applicationId: tx.all(
      "SELECT application_id value FROM pragma_application_id",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0].value,
    userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    metaRows: tx.all("SELECT count(*) value FROM efs_meta", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    revisionRows: tx.all("SELECT count(*) value FROM efs_revisions", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    inodeRows: tx.all("SELECT count(*) value FROM efs_inodes", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    markerRows: tx.all(
      "SELECT count(*) value FROM efs_replication_sessions WHERE id=?",
      [EFS_UNBOUND_REPLICA_MARKER_ID],
      { maxRows: 1, maxBytes: 128 },
    )[0].value,
  }));
}

function statementFaultDriver(base, failAt, count) {
  return {
    kind: "sqlite",
    readOnly: base.readOnly,
    capabilities: base.capabilities,
    close: () => base.close(),
    transaction(mode, callback) {
      return base.transaction(mode, (tx) => {
        if (mode !== "exclusive") return callback(tx);
        const run = (...args) => {
          count.value += 1;
          if (count.value === failAt)
            throw new Error(`unbound initialization fault ${failAt}`);
          return tx.run(...args);
        };
        return callback({ scope: tx.scope, run, all: tx.all });
      });
    },
  };
}

function withDurableTableIdentity(driver) {
  return Object.freeze({
    kind: driver.kind,
    readOnly: driver.readOnly,
    capabilities: Object.freeze({
      ...driver.capabilities,
      schemaIdentityMode: "durable-table",
    }),
    hashBytes: driver.hashBytes,
    hashBytesAsync: driver.hashBytesAsync,
    transaction: driver.transaction.bind(driver),
    physicalStorage: driver.physicalStorage?.bind(driver),
    checkpoint: driver.checkpoint?.bind(driver),
    close: driver.close.bind(driver),
  });
}

function inspectEmpty(driver) {
  return driver.transaction("read", (tx) => ({
    applicationId: tx.all(
      "SELECT application_id value FROM pragma_application_id",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0].value,
    userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    objectCount: tx.all(
      "SELECT count(*) value FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0].value,
  }));
}

test("unbound replica initialization persists only schema identity and its marker", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-unbound-"));
  const filename = path.join(directory, "replica.db");
  try {
    const driver = await openNodeSqlite({ filename });
    const created = initializeOrValidateUnboundReplicaSchema(driver);
    assert.deepEqual(created, {
      provisioningState: "unbound-replica",
      applicationId: EFS_APPLICATION_ID,
      storageUserVersion: EFS_SCHEMA_VERSION,
    });
    assert.deepEqual(inspectUnbound(driver), {
      applicationId: EFS_APPLICATION_ID,
      userVersion: EFS_SCHEMA_VERSION,
      metaRows: 0,
      revisionRows: 0,
      inodeRows: 0,
      markerRows: 1,
    });
    assert.throws(() => initializeOrValidateSchema(driver), /ESCHEMA/);
    assert.equal(inspectUnbound(driver).markerRows, 1);
    driver.close();

    const reopened = await openNodeSqlite({ filename });
    assert.deepEqual(initializeOrValidateUnboundReplicaSchema(reopened), created);
    assert.deepEqual(inspectUnbound(reopened), {
      applicationId: EFS_APPLICATION_ID,
      userVersion: EFS_SCHEMA_VERSION,
      metaRows: 0,
      revisionRows: 0,
      inodeRows: 0,
      markerRows: 1,
    });
    reopened.close();
  } finally {
    await removeTree(directory);
  }
});

test("unbound replica initialization rejects unrelated nonempty and bound databases", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-unbound-reject-"));
  const unrelatedFilename = path.join(directory, "unrelated.db");
  const boundFilename = path.join(directory, "bound.db");
  try {
    const unrelated = new DatabaseSync(unrelatedFilename);
    unrelated.exec("CREATE TABLE foreign_state(value TEXT)");
    unrelated.close();
    const unrelatedDriver = await openNodeSqlite({ filename: unrelatedFilename });
    assert.throws(
      () => initializeOrValidateUnboundReplicaSchema(unrelatedDriver),
      /ESCHEMA/,
    );
    assert.equal(
      unrelatedDriver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT count(*) value FROM sqlite_schema WHERE name='foreign_state'",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0].value,
      ),
      1,
    );
    unrelatedDriver.close();

    const boundDriver = await openNodeSqlite({ filename: boundFilename });
    const bound = initializeOrValidateSchema(boundDriver);
    assert.throws(
      () => initializeOrValidateUnboundReplicaSchema(boundDriver),
      /ProvisioningRejected/,
    );
    assert.deepEqual(initializeOrValidateSchema(boundDriver), bound);
    boundDriver.close();
  } finally {
    await removeTree(directory);
  }
});

test("unbound replica uses the runtime-owned durable identity representation", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-unbound-durable-id-"));
  const filename = path.join(directory, "replica.db");
  try {
    let raw = await openNodeSqlite({ filename });
    initializeOrValidateUnboundReplicaSchema(withDurableTableIdentity(raw));
    assert.deepEqual(
      raw.transaction("read", (tx) => ({
        nativeApplicationId: tx.all(
          "SELECT application_id value FROM pragma_application_id",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].value,
        nativeUserVersion: tx.all(
          "SELECT user_version value FROM pragma_user_version",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].value,
        durable: tx.all(
          "SELECT application_id,user_version FROM efs_schema_identity WHERE singleton=1",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
      })),
      {
        nativeApplicationId: 0,
        nativeUserVersion: 0,
        durable: {
          application_id: EFS_APPLICATION_ID,
          user_version: EFS_SCHEMA_VERSION,
        },
      },
    );
    raw.close();
    raw = await openNodeSqlite({ filename, create: false });
    initializeOrValidateUnboundReplicaSchema(withDurableTableIdentity(raw));
    assert.throws(
      () => initializeOrValidateSchema(withDurableTableIdentity(raw)),
      /ESCHEMA/,
    );
    raw.close();
  } finally {
    await removeTree(directory);
  }
});

test("every unbound initialization statement fault rolls back to a physically empty database", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-unbound-fault-"));
  try {
    const probeFilename = path.join(directory, "probe.db");
    let probe = await openNodeSqlite({ filename: probeFilename });
    const probeCount = { value: 0 };
    initializeOrValidateUnboundReplicaSchema(
      statementFaultDriver(probe, Number.MAX_SAFE_INTEGER, probeCount),
    );
    probe.close();
    assert.ok(probeCount.value > 0);

    for (let failAt = 1; failAt <= probeCount.value; failAt += 1) {
      const filename = path.join(directory, `fault-${failAt}.db`);
      let driver = await openNodeSqlite({ filename });
      const count = { value: 0 };
      assert.throws(
        () =>
          initializeOrValidateUnboundReplicaSchema(
            statementFaultDriver(driver, failAt, count),
          ),
        new RegExp(`unbound initialization fault ${failAt}`),
      );
      assert.equal(count.value, failAt);
      driver.close();
      driver = await openNodeSqlite({ filename, create: false });
      assert.deepEqual(inspectEmpty(driver), {
        applicationId: 0,
        userVersion: 0,
        objectCount: 0,
      });
      initializeOrValidateUnboundReplicaSchema(driver);
      assert.equal(inspectUnbound(driver).markerRows, 1);
      driver.close();
    }
  } finally {
    await removeTree(directory);
  }
});

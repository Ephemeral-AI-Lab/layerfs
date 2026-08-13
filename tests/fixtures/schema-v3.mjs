import {
  EFS_APPLICATION_ID,
  EFS_DURABLE_IDENTITY_DDL,
  EFS_SCHEMA_V3_CREATE_STATEMENTS,
} from "../../packages/fs/dist/sqlite/schema.js";
import { seedReleasedSchemaData } from "./released-schema-data.mjs";

function initializeIdentity(driver, tx, version) {
  if (driver.capabilities.schemaIdentityMode === "durable-table") {
    tx.run(EFS_DURABLE_IDENTITY_DDL);
    tx.run(
      "INSERT INTO efs_schema_identity(singleton,application_id,user_version) VALUES(1,?,?)",
      [EFS_APPLICATION_ID, version],
    );
  } else {
    tx.run(`PRAGMA application_id=${EFS_APPLICATION_ID}`);
    tx.run(`PRAGMA user_version=${version}`);
  }
}

export function createV3Schema(driver) {
  driver.transaction("exclusive", (tx) => {
    initializeIdentity(driver, tx, 3);
    for (const statement of EFS_SCHEMA_V3_CREATE_STATEMENTS) tx.run(statement);
    const seeded = seedReleasedSchemaData(tx);
    tx.run("INSERT INTO efs_meta VALUES(1,3,'v3-fixture',1,'root',0,4,4096,1)");
    tx.run("INSERT INTO efs_usage VALUES(1,1,4,1,?,1,?,0,0,0,0,0,0,0,1,4096)", [
      seeded.rootBytes,
      seeded.nodeBytes,
    ]);
  });
}

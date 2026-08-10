import {
  EFS_APPLICATION_ID,
  EFS_SCHEMA_V3_CREATE_STATEMENTS,
} from "../../packages/fs/dist/sqlite/schema.js";

// Released schema v2 equals the frozen v3 base before reconciliation state was
// added. Keep this fixture declarative so a v2 database never runs migration
// code merely to construct the test input.
const V3_ONLY_TABLES = [
  "efs_staging_reconciliations",
  "efs_staging_reconciliation_queue",
];

export function createV2Schema(driver) {
  driver.transaction("exclusive", (tx) => {
    tx.run(`PRAGMA application_id=${EFS_APPLICATION_ID}`);
    for (const statement of EFS_SCHEMA_V3_CREATE_STATEMENTS) {
      if (
        V3_ONLY_TABLES.some((table) => statement.startsWith(`CREATE TABLE ${table} `))
      )
        continue;
      tx.run(statement);
    }
    tx.run("INSERT INTO efs_revisions VALUES(0,NULL,1,'bootstrap',1)");
    tx.run("INSERT INTO efs_meta VALUES(1,2,'v2-fixture',0,'root',0,1,4096,1)");
    tx.run("INSERT INTO efs_usage VALUES(1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,256)");
    tx.run("INSERT INTO efs_inodes VALUES('root',1,493,1,1,1,1,NULL,NULL,NULL,0)");
    tx.run("INSERT INTO efs_inode_revisions VALUES(0,'root',0,?)", [
      new TextEncoder().encode("{}"),
    ]);
    tx.run("PRAGMA user_version=2");
  });
}

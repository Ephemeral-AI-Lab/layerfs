import { env } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

const enabled = __EFS_M6_RESOURCE_CONTROL__ === "1";
const BASELINE_ROWS = 10_240;
const FULL_ROWS = 100_000;
const BATCH_ROWS = 256;
const PAYLOAD_BYTES = 256;

afterEach(async () => {
  await reset();
});

async function extendRawFixture(
  stub: DurableObjectStub,
  start: number,
  end: number,
): Promise<{ rows: number; databaseSize: number }> {
  return runInDurableObject(stub, async (_instance, state) => {
    state.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS raw_scale_rows(id INTEGER PRIMARY KEY,name TEXT NOT NULL,payload BLOB NOT NULL,hash BLOB NOT NULL)",
    );
    state.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS raw_scale_aux(id INTEGER PRIMARY KEY,kind INTEGER NOT NULL,hash BLOB NOT NULL,payload BLOB NOT NULL)",
    );
    state.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS raw_scale_marks(id INTEGER PRIMARY KEY,kind INTEGER NOT NULL,hash BLOB NOT NULL,processed INTEGER NOT NULL DEFAULT 0)",
    );
    state.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS raw_scale_state(singleton INTEGER PRIMARY KEY,counted INTEGER NOT NULL,cursor INTEGER NOT NULL)",
    );
    state.storage.sql.exec(
      "INSERT OR IGNORE INTO raw_scale_state(singleton,counted,cursor) VALUES(1,0,-1)",
    );
    state.storage.sql.exec("DELETE FROM raw_scale_marks");
    state.storage.sql.exec(
      "UPDATE raw_scale_state SET counted=0,cursor=-1 WHERE singleton=1",
    );
    for (let batchStart = start; batchStart < end; batchStart += BATCH_ROWS) {
      const batchEnd = Math.min(end, batchStart + BATCH_ROWS);
      state.storage.transactionSync(() => {
        for (let index = batchStart; index < batchEnd; index += 1) {
          const payload = new Uint8Array(PAYLOAD_BYTES);
          const hash = new Uint8Array(32);
          new DataView(payload.buffer).setUint32(0, index, true);
          new DataView(hash.buffer).setUint32(0, index, true);
          state.storage.sql.exec(
            "INSERT INTO raw_scale_rows(id,name,payload,hash) VALUES(?,?,?,?)",
            index,
            `raw-${index.toString().padStart(6, "0")}`,
            payload,
            hash,
          );
          for (let kind = 0; kind < 3; kind += 1) {
            const auxHash = hash.slice();
            auxHash[4] = kind;
            state.storage.sql.exec(
              "INSERT INTO raw_scale_aux(id,kind,hash,payload) VALUES(?,?,?,?)",
              index * 3 + kind,
              kind,
              auxHash,
              payload,
            );
          }
        }
      });
    }
    return {
      rows: Number(
        state.storage.sql.exec("SELECT count(*) AS value FROM raw_scale_rows").one()
          .value,
      ),
      databaseSize: state.storage.sql.databaseSize,
    };
  });
}

async function scanRawFixture(
  stub: DurableObjectStub,
): Promise<{ rows: number; databaseSize: number }> {
  return runInDurableObject(stub, async (_instance, state) => {
    let afterAux = -1;
    let marked = 0;
    for (;;) {
      const page = state.storage.sql
        .exec(
          "SELECT id,kind,hash FROM raw_scale_aux WHERE id>? ORDER BY id LIMIT 256",
          afterAux,
        )
        .toArray() as unknown as readonly {
        id: number;
        kind: number;
        hash: Uint8Array;
      }[];
      if (!page.length) break;
      state.storage.transactionSync(() => {
        for (const row of page)
          state.storage.sql.exec(
            "INSERT INTO raw_scale_marks(id,kind,hash,processed) VALUES(?,?,?,0)",
            row.id,
            row.kind,
            row.hash,
          );
      });
      marked += page.length;
      afterAux = page.at(-1)!.id;
    }
    expect(marked).toBe(
      Number(
        state.storage.sql.exec("SELECT count(*) value FROM raw_scale_aux").one().value,
      ),
    );

    let processed = 0;
    for (;;) {
      const page = state.storage.sql
        .exec(
          "SELECT id,kind,hash FROM raw_scale_marks WHERE processed=0 ORDER BY id LIMIT 256",
        )
        .toArray() as unknown as readonly {
        id: number;
        kind: number;
        hash: Uint8Array;
      }[];
      if (!page.length) break;
      state.storage.transactionSync(() => {
        for (const row of page) {
          const payload = state.storage.sql
            .exec("SELECT payload FROM raw_scale_aux WHERE id=?", row.id)
            .one().payload as Uint8Array;
          expect(payload.byteLength).toBe(PAYLOAD_BYTES);
          state.storage.sql.exec(
            "UPDATE raw_scale_marks SET processed=0 WHERE id=?",
            row.id,
          );
          state.storage.sql.exec(
            "UPDATE raw_scale_state SET counted=counted+1 WHERE singleton=1",
          );
          expect(
            Number(
              state.storage.sql
                .exec("SELECT processed FROM raw_scale_marks WHERE id=?", row.id)
                .one().processed,
            ),
          ).toBe(0);
          state.storage.sql.exec(
            "UPDATE raw_scale_marks SET processed=1 WHERE id=? AND kind=? AND hash=?",
            row.id,
            row.kind,
            row.hash,
          );
          state.storage.sql.exec(
            "UPDATE raw_scale_state SET cursor=? WHERE singleton=1",
            row.id,
          );
        }
      });
      processed += page.length;
    }
    expect(processed).toBe(marked);
    expect(
      Number(
        state.storage.sql
          .exec("SELECT counted FROM raw_scale_state WHERE singleton=1")
          .one().counted,
      ),
    ).toBe(marked);

    let rows = 0;
    for (let pass = 0; pass < 4; pass += 1) {
      let after = -1;
      let passRows = 0;
      for (;;) {
        const page = state.storage.sql
          .exec(
            "SELECT id,payload,hash FROM raw_scale_rows WHERE id>? ORDER BY id LIMIT 256",
            after,
          )
          .toArray() as unknown as readonly {
          id: number;
          payload: Uint8Array;
          hash: Uint8Array;
        }[];
        if (!page.length) break;
        expect(page.length).toBeLessThanOrEqual(256);
        for (const row of page) {
          expect(row.payload.byteLength).toBe(PAYLOAD_BYTES);
          expect(row.hash.byteLength).toBe(32);
        }
        passRows += page.length;
        after = page.at(-1)!.id;
      }
      if (pass === 0) rows = passRows;
      else expect(passRows).toBe(rows);
    }

    for (;;) {
      const page = state.storage.sql
        .exec("SELECT id FROM raw_scale_marks ORDER BY id LIMIT 256")
        .toArray() as unknown as readonly { id: number }[];
      if (!page.length) break;
      state.storage.transactionSync(() => {
        for (const row of page)
          state.storage.sql.exec("DELETE FROM raw_scale_marks WHERE id=?", row.id);
      });
    }
    expect(
      Number(
        state.storage.sql.exec("SELECT count(*) value FROM raw_scale_marks").one()
          .value,
      ),
    ).toBe(0);
    return { rows, databaseSize: state.storage.sql.databaseSize };
  });
}

(enabled ? test : test.skip)(
  "raw Durable Object SQLite reproduces the scale resident-memory effect without filesystem caches",
  { timeout: 600_000 },
  async () => {
    const stub = env.FILESYSTEM.getByName("raw-workerd-resource-control");
    const baselineBuild = await extendRawFixture(stub, 0, BASELINE_ROWS);
    expect(baselineBuild.rows).toBe(BASELINE_ROWS);
    await evictDurableObject(stub);
    console.log('m6-workerd-resource-window {"phase":"baseline","edge":"start"}');
    const baseline = await scanRawFixture(stub);
    expect(baseline.rows).toBe(BASELINE_ROWS);
    console.log('m6-workerd-resource-window {"phase":"baseline","edge":"end"}');
    console.log(
      `m6-workerd-control-phase ${JSON.stringify({
        phase: "baseline-measured",
        rows: baseline.rows,
        databaseSize: baseline.databaseSize,
      })}`,
    );
    await evictDurableObject(stub);
    const fullBuild = await extendRawFixture(stub, BASELINE_ROWS, FULL_ROWS);
    expect(fullBuild.rows).toBe(FULL_ROWS);
    await evictDurableObject(stub);
    console.log('m6-workerd-resource-window {"phase":"full","edge":"start"}');
    const full = await scanRawFixture(stub);
    expect(full.rows).toBe(FULL_ROWS);
    console.log('m6-workerd-resource-window {"phase":"full","edge":"end"}');
    console.log(
      `m6-workerd-control-phase ${JSON.stringify({
        phase: "full-measured",
        rows: full.rows,
        databaseSize: full.databaseSize,
      })}`,
    );
    await evictDurableObject(stub);
    const recovered = await runInDurableObject(stub, async (_instance, state) => ({
      rows: Number(
        state.storage.sql.exec("SELECT count(*) AS value FROM raw_scale_rows").one()
          .value,
      ),
      databaseSize: state.storage.sql.databaseSize,
    }));
    expect(recovered.rows).toBe(FULL_ROWS);
    console.log(
      `m6-workerd-control-evidence ${JSON.stringify({
        schema: "efs-m6-workerd-raw-control-v1",
        baselineRows: BASELINE_ROWS,
        fullRows: FULL_ROWS,
        payloadBytes: PAYLOAD_BYTES,
        baselineDatabaseBytes: baseline.databaseSize,
        fullDatabaseBytes: recovered.databaseSize,
        restart: "evictDurableObject",
        filesystemCachesInstantiated: false,
      })}`,
    );
  },
);

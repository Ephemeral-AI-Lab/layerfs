import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralRuntime } from "../../packages/fs/dist/index.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import {
  batchEnvelopeDigest,
  createCanonicalBatch,
  createCanonicalBatchAcknowledgement,
  encodeCanonicalBatchAcknowledgement,
  encodeCanonicalEnvelope,
  receiptChainDigest,
} from "../../packages/replication/dist/index.js";
import { ReplicationSessionRepository } from "../../packages/fs/dist/sqlite/replication-repository.js";
import { initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

const digest = (value) => sha256(new TextEncoder().encode(value));
const bytes = (value) => new TextEncoder().encode(value);
const cursor = (value) => new Uint8Array(16).fill(value);

function binding(overrides = {}) {
  return {
    operationId: "operation-01",
    sessionId: "00112233445566778899aabbccddeeff",
    resumeKey: bytes("opaque-resume-key-01"),
    ownerNonce: Uint8Array.from({ length: 16 }, (_, index) => index + 1),
    flow: "authority-main-to-replica",
    branchId: null,
    sourceFilesystemId: "filesystem-01",
    destinationFilesystemId: "filesystem-01",
    sourceRole: "main-authority",
    destinationRole: "replica",
    sourceAuthorizationDigest: digest("source-authorization"),
    destinationAuthorizationDigest: digest("destination-authorization"),
    sourceCapabilityDigest: digest("source-capabilities"),
    destinationCapabilityDigest: digest("destination-capabilities"),
    effectiveLimitsDigest: digest("effective-limits"),
    maxBatchEntries: 8,
    maxBatchBytes: 1024,
    maxRequestBytes: 3072,
    maxResponseBytes: 3072,
    maxBufferedBytes: 8192,
    maxInFlightBatches: 1,
    maxConcurrentSessions: 4,
    maxCursorBytes: 256,
    maxReplicationSessionRows: 100,
    maxReplicationMetadataBytes: 1024 * 1024,
    maxReceiptsPerSession: 8,
    maxReceiptBytesPerSession: 4096,
    maxStagingBytesPerSession: 1024,
    maxAcknowledgementBytes: 1024,
    maxTerminalResultBytes: 1024,
    maxCursorAgeMs: 1000,
    stagingLeaseMs: 1000,
    maxRetryAttempts: 3,
    maxRetryElapsedMs: 1000,
    minRetryDelayMs: 10,
    maxRetryDelayMs: 100,
    resultRetentionMs: 10_000,
    ...overrides,
  };
}

function alternateBinding(index, overrides = {}) {
  return binding({
    operationId: `operation-${String(index).padStart(2, "0")}`,
    sessionId: index.toString(16).padStart(32, "0"),
    resumeKey: bytes(`opaque-resume-key-${String(index).padStart(2, "0")}`),
    ownerNonce: new Uint8Array(16).fill(index),
    ...overrides,
  });
}

function openRequest(overrides = {}) {
  return {
    binding: binding(),
    phase: "handshake",
    cursor: cursor(0),
    cursorDigest: sha256(cursor(0)),
    now: 1000,
    expiresAtMs: 2000,
    ...overrides,
  };
}

function withRepository(driver, mode, callback) {
  return driver.transaction(mode, (tx) =>
    callback(new ReplicationSessionRepository(tx, sha256)),
  );
}

function canonicalAcceptance() {
  const batch = createCanonicalBatch({
    sessionId: binding().sessionId,
    plan: { flow: "authority-main-to-replica" },
    phase: "handshake",
    sequence: 0,
    priorCursorDigest: sha256(cursor(0)),
    records: [
      {
        kind: "missing-content",
        contentKind: "object",
        digest: digest("missing-object"),
      },
    ],
  });
  const chainDigest = receiptChainDigest(
    new Uint8Array(32),
    batch.sequence,
    batchEnvelopeDigest(batch),
  );
  const acknowledgement = encodeCanonicalBatchAcknowledgement(
    createCanonicalBatchAcknowledgement({
      batch,
      nextPhase: "plan-selection",
      cursor: cursor(1),
      chainDigest,
      acceptedEntries: batch.entryCount,
      acceptedBytes: batch.payloadByteCount,
      stagedBytes: 9,
    }),
  );
  return { batch, acknowledgement, chainDigest };
}

function canonicalTerminalResult() {
  const resultBytes = bytes("terminal-result-payload");
  return encodeCanonicalEnvelope({
    kind: "terminal-result",
    value: {
      operationId: "operation-01",
      branchId: null,
      generation: null,
      generationDigest: null,
      resultDigest: sha256(resultBytes),
      resultBytes,
    },
  });
}

async function removeTree(target, attempts = 20) {
  let lastError;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      await rm(target, { recursive: true, force: true });
      return;
    } catch (error) {
      lastError = error;
      if (attempt === attempts - 1) throw error;
      await new Promise((resolve) => setTimeout(resolve, 25 * (attempt + 1)));
    }
  }
  throw lastError;
}

test("durable sessions bind operation, identity, policy, plan, profile, and limits", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-session-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    const created = withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest()),
    );
    assert.equal(created.created, true);
    assert.equal(created.session.operationId, "operation-01");
    driver.close();

    driver = await openNodeSqlite({ filename, create: false });
    const resumed = withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest({ now: 1100 })),
    );
    assert.equal(resumed.created, false);
    assert.deepEqual(resumed.session, created.session);
    const changed = [
      { sourceAuthorizationDigest: digest("changed-auth") },
      { destinationCapabilityDigest: digest("changed-capability") },
      { effectiveLimitsDigest: digest("changed-limits") },
      { flow: "authority-branch-to-replica", branchId: "branch-1" },
      { sourceFilesystemId: "other-filesystem" },
      { resumeKey: bytes("other-resume-key") },
    ];
    for (const change of changed) {
      assert.throws(
        () =>
          withRepository(driver, "write", (repository) =>
            repository.createOrResume(
              openRequest({ binding: binding(change), now: 1200 }),
            ),
          ),
        /OperationMismatch/,
      );
    }
    assert.throws(
      () =>
        withRepository(driver, "write", (repository) =>
          repository.createOrResume(
            openRequest({
              binding: alternateBinding(2, {
                sourceRole: "replica",
                destinationRole: "main-authority",
              }),
              now: 1200,
            }),
          ),
        ),
      /UnauthorizedScope: replication roles do not authorize the selected flow/,
    );
    assert.equal(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT count(*) value FROM efs_replication_sessions WHERE state<>-1",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0].value,
      ),
      1,
    );
    driver.close();
  } finally {
    await removeTree(directory);
  }
});

test("active session admission is aggregate, serialized, and released by terminal state", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-active-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    const firstBinding = alternateBinding(1, { maxConcurrentSessions: 1 });
    const secondBinding = alternateBinding(2, { maxConcurrentSessions: 1 });
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest({ binding: firstBinding })),
    );
    assert.throws(
      () =>
        withRepository(driver, "write", (repository) =>
          repository.createOrResume(openRequest({ binding: secondBinding, now: 1100 })),
        ),
      /ResourceLimit: aggregate active replication session limit exceeded/,
    );
    withRepository(driver, "write", (repository) =>
      repository.storeTerminalResult({
        operationId: firstBinding.operationId,
        sessionId: firstBinding.sessionId,
        ownerNonce: firstBinding.ownerNonce,
        result: bytes("terminal"),
        now: 1200,
      }),
    );
    const second = withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest({ binding: secondBinding, now: 1300 })),
    );
    assert.equal(second.created, true);
  } finally {
    try {
      driver?.close();
    } catch {}
    await removeTree(directory);
  }
});

test("retry-aborted sessions release their durable row and retained receipts", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-abort-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest()),
    );
    withRepository(driver, "write", (repository) =>
      repository.abortSession({
        operationId: binding().operationId,
        sessionId: binding().sessionId,
        ownerNonce: binding().ownerNonce,
        now: 1100,
      }),
    );
    const counts = driver.transaction("read", (tx) => ({
      sessions: tx.all(
        "SELECT count(*) value FROM efs_replication_sessions WHERE state>=0",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].value,
      receipts: tx.all(
        "SELECT count(*) value FROM efs_replication_receipts",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].value,
      exports: tx.all(
        "SELECT count(*) value FROM efs_replication_exports",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].value,
    }));
    assert.deepEqual(counts, { sessions: 0, receipts: 0, exports: 0 });
  } finally {
    try { driver?.close(); } catch {}
    await removeTree(directory);
  }
});

test("terminal sessions remain charged to the retained session-row aggregate", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-rows-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    const firstBinding = alternateBinding(1, {
      maxConcurrentSessions: 2,
      maxReplicationSessionRows: 1,
    });
    const secondBinding = alternateBinding(2, {
      maxConcurrentSessions: 2,
      maxReplicationSessionRows: 1,
    });
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest({ binding: firstBinding })),
    );
    withRepository(driver, "write", (repository) =>
      repository.storeTerminalResult({
        operationId: firstBinding.operationId,
        sessionId: firstBinding.sessionId,
        ownerNonce: firstBinding.ownerNonce,
        result: bytes("terminal"),
        now: 1100,
      }),
    );
    assert.throws(
      () =>
        withRepository(driver, "write", (repository) =>
          repository.createOrResume(openRequest({ binding: secondBinding, now: 1200 })),
        ),
      /ResourceLimit: aggregate retained replication session row limit exceeded/,
    );
  } finally {
    try {
      driver?.close();
    } catch {}
    await removeTree(directory);
  }
});

test("aggregate replication metadata admission rejects session and receipt growth atomically", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-metadata-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    const firstBinding = alternateBinding(1, {
      maxReplicationMetadataBytes: 4096,
      maxTerminalResultBytes: 2048,
    });
    const secondBinding = alternateBinding(2, {
      maxReplicationMetadataBytes: 4096,
      maxTerminalResultBytes: 2048,
    });
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest({ binding: firstBinding })),
    );
    assert.throws(
      () =>
        withRepository(driver, "write", (repository) =>
          repository.createOrResume(openRequest({ binding: secondBinding, now: 1100 })),
        ),
      /ResourceLimit: aggregate replication metadata limit exceeded/,
    );
    assert.throws(
      () =>
        withRepository(driver, "write", (repository) =>
          repository.storeTerminalResult({
            operationId: firstBinding.operationId,
            sessionId: firstBinding.sessionId,
            ownerNonce: firstBinding.ownerNonce,
            result: new Uint8Array(2048),
            now: 1200,
          }),
        ),
      /ResourceLimit: aggregate replication metadata limit exceeded/,
    );
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT state,(SELECT count(*) FROM efs_replication_receipts) receipt_count FROM efs_replication_sessions WHERE id=?",
            [firstBinding.operationId],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      ),
      { state: 0, receipt_count: 0 },
    );
  } finally {
    try {
      driver?.close();
    } catch {}
    await removeTree(directory);
  }
});

test("batch receipt, cursor, counters, and exact acknowledgement commit atomically", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-batch-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest()),
    );
    const canonical = canonicalAcceptance();
    const batch = {
      operationId: "operation-01",
      sessionId: binding().sessionId,
      ownerNonce: binding().ownerNonce,
      sequence: 0,
      phase: "handshake",
      priorCursorDigest: sha256(cursor(0)),
      batchEnvelopeDigest: batchEnvelopeDigest(canonical.batch),
      payloadDigest: canonical.batch.payloadDigest,
      entryCount: canonical.batch.entryCount,
      payloadByteCount: canonical.batch.payloadByteCount,
      nextPhase: "plan-selection",
      nextCursor: cursor(1),
      nextCursorDigest: sha256(cursor(1)),
      acknowledgement: canonical.acknowledgement,
      stagedBytesDelta: 9,
      now: 1100,
    };
    assert.throws(
      () =>
        withRepository(driver, "write", (repository) =>
          repository.acceptBatch({
            ...batch,
            acknowledgement: Uint8Array.of(1),
          }),
        ),
      /ProtocolMismatch: acknowledgement\.magic is truncated/,
    );
    assert.equal(
      withRepository(driver, "read", (repository) =>
        repository.resume({
          operationId: "operation-01",
          sessionId: binding().sessionId,
          resumeKey: binding().resumeKey,
        }),
      ).nextSequence,
      0,
    );
    const accepted = withRepository(driver, "write", (repository) =>
      repository.acceptBatch(batch),
    );
    assert.equal(accepted.replayed, false);
    assert.deepEqual(accepted.acknowledgement, batch.acknowledgement);
    driver.close();
    driver = undefined;

    driver = await openNodeSqlite({ filename, create: false });
    const replayed = withRepository(driver, "write", (repository) =>
      repository.acceptBatch({ ...batch, now: 1200 }),
    );
    assert.equal(replayed.replayed, true);
    assert.deepEqual(replayed.acknowledgement, batch.acknowledgement);
    let mismatchIndex = 0;
    for (const mismatch of [
      { payloadDigest: digest("changed") },
      { entryCount: 2 },
      { payloadByteCount: 10 },
      { priorCursorDigest: sha256(cursor(2)) },
      { phase: "plan-selection" },
    ]) {
      mismatchIndex += 1;
      assert.throws(
        () =>
          withRepository(driver, "write", (repository) =>
            repository.acceptBatch({
              ...batch,
              ...mismatch,
              batchEnvelopeDigest: digest(`changed-envelope-${mismatchIndex}`),
              now: 1300,
            }),
          ),
        /BatchReplayMismatch/,
      );
    }
    const state = withRepository(driver, "read", (repository) =>
      repository.resume({
        operationId: "operation-01",
        sessionId: binding().sessionId,
        resumeKey: binding().resumeKey,
      }),
    );
    assert.equal(state.nextSequence, 1);
    assert.equal(state.phase, "plan-selection");
    assert.equal(state.acceptedEntries, 1);
    assert.equal(state.acceptedBytes, canonical.batch.payloadByteCount);
    assert.equal(state.stagedBytes, 9);
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT digest,encoded FROM efs_replication_receipts", [], {
            maxRows: 1,
            maxBytes: 2048,
          })[0],
      ),
      {
        digest: batch.batchEnvelopeDigest,
        encoded: canonical.acknowledgement,
      },
    );
    driver.close();
    driver = undefined;
  } finally {
    try {
      driver?.close();
    } catch {}
    await removeTree(directory);
  }
});

test("receipt compaction and maintenance are bounded and durable", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-maintenance-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest({ expiresAtMs: 2000 })),
    );
    const canonical = canonicalAcceptance();
    const batch = {
      operationId: "operation-01",
      sessionId: binding().sessionId,
      ownerNonce: binding().ownerNonce,
      sequence: 0,
      phase: "handshake",
      priorCursorDigest: sha256(cursor(0)),
      batchEnvelopeDigest: batchEnvelopeDigest(canonical.batch),
      payloadDigest: canonical.batch.payloadDigest,
      entryCount: canonical.batch.entryCount,
      payloadByteCount: canonical.batch.payloadByteCount,
      nextPhase: "plan-selection",
      nextCursor: cursor(1),
      nextCursorDigest: sha256(cursor(1)),
      acknowledgement: canonical.acknowledgement,
      stagedBytesDelta: 9,
      now: 1100,
    };
    withRepository(driver, "write", (repository) => repository.acceptBatch(batch));
    const compacted = withRepository(driver, "write", (repository) =>
      repository.compactReceipts({
        operationId: "operation-01",
        ownerNonce: binding().ownerNonce,
        throughSequence: 0,
        maxRows: 1,
      }),
    );
    assert.equal(compacted.compactedThrough, 0);
    assert.equal(compacted.deletedRows, 1);
    assert.throws(
      () => withRepository(driver, "write", (repository) => repository.acceptBatch(batch)),
      /BatchReplayMismatch.*compacted/,
    );
    const expired = withRepository(driver, "write", (repository) =>
      repository.maintenance({ now: 2000, maxRows: 8 }),
    );
    assert.equal(expired.expiredSessions, 1);
    assert.equal(
      driver.transaction(
        "read",
        (tx) => tx.all("SELECT count(*) value FROM efs_replication_sessions WHERE id=?", ["operation-01"], { maxRows: 1, maxBytes: 128 })[0].value,
      ),
      0,
    );
  } finally {
    try { driver?.close(); } catch {}
    await removeTree(directory);
  }
});

test("retry budget and terminal result survive restart without clock rollback extension", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-retry-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest()),
    );
    assert.equal(
      withRepository(driver, "write", (repository) =>
        repository.consumeAttempt({
          operationId: "operation-01",
          sessionId: binding().sessionId,
          ownerNonce: binding().ownerNonce,
          wallNowMs: 1100,
          monotonicElapsedMs: 100,
          delayMs: 10,
        }),
      ).exhausted,
      false,
    );
    driver.close();

    driver = await openNodeSqlite({ filename, create: false });
    const second = withRepository(driver, "write", (repository) =>
      repository.consumeAttempt({
        operationId: "operation-01",
        sessionId: binding().sessionId,
        ownerNonce: binding().ownerNonce,
        wallNowMs: 1050,
        monotonicElapsedMs: 400,
        delayMs: 100,
      }),
    );
    assert.equal(second.exhausted, false);
    assert.equal(second.attempts, 2);
    assert.equal(second.elapsedRetryMs, 500);
    assert.equal(second.lastWallClockMs, 1100);
    const exhausted = withRepository(driver, "write", (repository) =>
      repository.consumeAttempt({
        operationId: "operation-01",
        sessionId: binding().sessionId,
        ownerNonce: binding().ownerNonce,
        wallNowMs: 1150,
        monotonicElapsedMs: 501,
        delayMs: 50,
      }),
    );
    assert.equal(exhausted.exhausted, true);
    assert.equal(exhausted.attempts, 3);
    assert.equal(exhausted.elapsedRetryMs, 1001);

    const terminal = canonicalTerminalResult();
    withRepository(driver, "write", (repository) =>
      repository.storeTerminalResult({
        operationId: "operation-01",
        sessionId: binding().sessionId,
        ownerNonce: binding().ownerNonce,
        result: terminal,
        now: 1200,
      }),
    );
    driver.close();
    driver = await openNodeSqlite({ filename, create: false });
    assert.deepEqual(
      withRepository(driver, "read", (repository) =>
        repository.replayTerminalResult({
          operationId: "operation-01",
          sessionId: binding().sessionId,
          resumeKey: binding().resumeKey,
          now: 1300,
        }),
      ),
      terminal,
    );
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT digest,encoded FROM efs_replication_receipts WHERE session_id=? AND batch_index=-1",
            ["operation-01"],
            { maxRows: 1, maxBytes: 2048 },
          )[0],
      ),
      { digest: sha256(terminal), encoded: terminal },
    );
    driver.close();
  } finally {
    await removeTree(directory);
  }
});

test("one public runtime owns bound filesystem, branch VFS, and durable replication", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replication-runtime-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let database = await openNodeSqlite({ filename });
    let runtime = await EphemeralRuntime.open({ database });
    assert.equal(runtime.provisioningState, "bound");
    assert.ok(runtime.filesystem);
    assert.ok(runtime.openNodeVfs());
    const created = await runtime.replication.createOrResumeSession(openRequest());
    assert.equal(created.created, true);
    await runtime.close();
    await database.close();

    database = await openNodeSqlite({ filename, create: false });
    runtime = await EphemeralRuntime.open({ database });
    const resumed = await runtime.replication.createOrResumeSession(
      openRequest({ now: 1100 }),
    );
    assert.equal(resumed.created, false);
    assert.deepEqual(resumed.session, created.session);
    await assert.rejects(
      runtime.replication.acceptBatch({
        operationId: "operation-01",
        sessionId: binding().sessionId,
        ownerNonce: binding().ownerNonce,
        sequence: 0,
        phase: "state-transfer",
        priorCursorDigest: sha256(cursor(0)),
        batchEnvelopeDigest: digest("opaque-state-envelope"),
        payloadDigest: digest("opaque-state-fragment"),
        entryCount: 1,
        payloadByteCount: 21,
        nextPhase: "activation",
        nextCursor: cursor(1),
        nextCursorDigest: sha256(cursor(1)),
        acknowledgement: bytes("ack"),
        stagedBytesDelta: 0,
        now: 1200,
      }),
      /CursorMismatch|SchemaMismatch/,
    );
    await runtime.close();
    await database.close();
  } finally {
    await removeTree(directory);
  }
});

test("durable replica identity makes main read-only while private branches remain writable", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-replica-identity-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let database = await openNodeSqlite({ filename });
    let runtime = await EphemeralRuntime.open({
      database,
      replicationIdentity: { authorityId: "authority-01", role: "replica" },
    });
    const identity = runtime.identity;
    assert.equal(identity.authorityId, "authority-01");
    assert.equal(identity.role, "replica");
    assert.equal(typeof identity.filesystemId, "string");
    await assert.rejects(runtime.filesystem.writeFile("/main", "denied"), {
      code: "EROFS",
    });
    assert.throws(
      () => runtime.openNodeVfs().writeFileSync("/main", bytes("denied")),
      { code: "EROFS" },
    );

    const branch = await runtime.filesystem.branches.create("replica-work");
    await branch.writeFile("/private", "branch-data");
    const branchVfs = runtime.openNodeVfs({ branchId: "replica-work" });
    branchVfs.writeFileSync("/private-vfs", bytes("vfs-data"));
    assert.equal(await branch.readFile("/private-vfs", { encoding: "utf8" }), "vfs-data");
    await assert.rejects(branch.publish(), { code: "EROFS" });
    await assert.rejects(branch.discard(), { code: "EROFS" });
    await branch.close();
    await runtime.close();
    await database.close();

    database = await openNodeSqlite({ filename, create: false });
    runtime = await EphemeralRuntime.open({ database });
    assert.deepEqual(runtime.identity, identity);
    await assert.rejects(runtime.filesystem.mkdir("/main-again"), { code: "EROFS" });
    const reopenedBranch = await runtime.filesystem.branches.open("replica-work");
    assert.equal(
      await reopenedBranch.readFile("/private", { encoding: "utf8" }),
      "branch-data",
    );
    await reopenedBranch.close();
    await runtime.close();
    await database.close();

    database = await openNodeSqlite({ filename, create: false });
    await assert.rejects(
      EphemeralRuntime.open({
        database,
        replicationIdentity: { authorityId: "other-authority", role: "replica" },
      }),
      /AuthorityMismatch|already bound differently/,
    );
  } finally {
    await removeTree(directory);
  }
});

test("unbound runtime exposes only resumable provisioning replication", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-unbound-runtime-"));
  const filename = path.join(directory, "replica.db");
  try {
    let database = await openNodeSqlite({ filename });
    let runtime = await EphemeralRuntime.open({
      database,
      provisioningState: "unbound-replica",
    });
    assert.equal(runtime.provisioningState, "unbound-replica");
    assert.equal(runtime.filesystem, null);
    assert.throws(() => runtime.openNodeVfs(), /ProvisioningRejected/);
    const created = await runtime.replication.createOrResumeSession(openRequest());
    assert.equal(created.created, true);
    await runtime.close();
    await database.close();

    database = await openNodeSqlite({ filename, create: false });
    runtime = await EphemeralRuntime.open({
      database,
      provisioningState: "unbound-replica",
    });
    const resumed = await runtime.replication.createOrResumeSession(
      openRequest({ now: 1100 }),
    );
    assert.equal(resumed.created, false);
    assert.deepEqual(resumed.session, created.session);
    await runtime.close();
    await database.close();
  } finally {
    await removeTree(directory);
  }
});

test("lost outbound responses replay from a durable receipt and bind the request digest", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-outbound-receipt-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    initializeOrValidateSchema(driver);
    withRepository(driver, "write", (repository) =>
      repository.createOrResume(openRequest()),
    );
    const requestDigest = digest("missing-content-request");
    const responseBytes = bytes("canonical-missing-content-response");
    withRepository(driver, "write", (repository) =>
      repository.recordOutboundBatch({
        operationId: binding().operationId,
        sessionId: binding().sessionId,
        ownerNonce: binding().ownerNonce,
        sequence: 0,
        phase: "handshake",
        nextPhase: "plan-selection",
        nextCursor: cursor(1),
        nextCursorDigest: sha256(cursor(1)),
        requestDigest,
        responseBytes,
      }),
    );
    driver.close();
    driver = await openNodeSqlite({ filename, create: false });
    assert.deepEqual(
      withRepository(driver, "read", (repository) =>
        repository.replayOutboundBatch({
          operationId: binding().operationId,
          sessionId: binding().sessionId,
          ownerNonce: binding().ownerNonce,
          sequence: 0,
          requestDigest,
        }),
      ),
      responseBytes,
    );
    assert.throws(
      () =>
        withRepository(driver, "read", (repository) =>
          repository.replayOutboundBatch({
            operationId: binding().operationId,
            sessionId: binding().sessionId,
            ownerNonce: binding().ownerNonce,
            sequence: 0,
            requestDigest: digest("different-request"),
          }),
        ),
      /BatchReplayMismatch/,
    );
  } finally {
    try { driver?.close(); } catch {}
    await removeTree(directory);
  }
});

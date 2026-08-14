import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralRuntime } from "../../packages/fs/dist/integrations/runtime.js";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  createReplicationEndpoint,
  replicate,
  ReplicationError,
} from "../../packages/replication/dist/index.js";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function authorization(filesystemId, allowedPlans) {
  const limitPolicy = {
    ceilings: {
      maxBatchEntries: 256,
      maxBatchBytes: 3 * 1024 * 1024 - 64 * 1024,
      maxRequestBytes: 3 * 1024 * 1024,
      maxResponseBytes: 3 * 1024 * 1024,
      maxBufferedBytes: 10 * 1024 * 1024,
      maxInFlightBatches: 1,
      maxConcurrentSessions: 16,
      maxStagingBytesPerSession: 128 * 1024 * 1024,
      maxReplicationSessionRows: 10_000,
      maxReplicationMetadataBytes: 64 * 1024 * 1024,
      maxReceiptsPerSession: 100_000,
      maxReceiptBytesPerSession: 16 * 1024 * 1024,
      maxCursorBytes: 256,
      maxTerminalResultBytes: 1024 * 1024,
      maxCursorAgeMs: 24 * 60 * 60 * 1000,
      stagingLeaseMs: 15 * 60 * 1000,
      resultRetentionMs: 30 * 24 * 60 * 60 * 1000,
      maxRetryAttempts: 8,
      maxRetryElapsedMs: 5 * 60 * 1000,
      minRetryDelayMs: 100,
      maxRetryDelayMs: 10_000,
    },
    minRetryDelayMsFloor: 100,
  };
  return {
    principalId: "principal-a",
    hostScopeId: "workspace-a",
    expectedFilesystemId: filesystemId,
    expectedAuthorityId: "authority-a",
    policyVersion: "policy-1",
    hostProfile: "computer-efs-carrier-v1",
    limitPolicy,
    allowedPlans,
  };
}

class LoopbackTransport {
  constructor(endpoint) {
    this.endpoint = endpoint;
  }
  async exchange(request) {
    return this.endpoint.exchange(request);
  }
}

class DropResponseTransport {
  constructor(endpoint, dropAfter) {
    this.endpoint = endpoint;
    this.dropAfter = dropAfter;
    this.count = 0;
    this.dropped = false;
  }
  async exchange(request) {
    const response = await this.endpoint.exchange(request);
    this.count += 1;
    if (!this.dropped && this.count === this.dropAfter) {
      this.dropped = true;
      throw new ReplicationError("TransportFailure", "test dropped the response after durable acceptance");
    }
    return response;
  }
}

async function openAuthority(directory) {
  const database = await openNodeSqlite({ filename: path.join(directory, "authority.db") });
  const runtime = await EphemeralRuntime.open({
    database,
    replicationIdentity: { authorityId: "authority-a", role: "main-authority" },
  });
  return { database, runtime };
}

async function openReplica(directory) {
  const database = await openNodeSqlite({ filename: path.join(directory, "replica.db") });
  const runtime = await EphemeralRuntime.open({
    database,
    replicationIdentity: { authorityId: "authority-a", role: "replica" },
  });
  return { database, runtime };
}

test("authority main transfers to an authenticated replica through the wire", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-repl-transfer-"));
  try {
    const { database: authorityDb, runtime: authority } = await openAuthority(
      directory,
    );
    try {
      await authority.filesystem.writeFile("/hello.txt", "hello world");
      await authority.filesystem.mkdir("/dir");
      await authority.filesystem.writeFile("/dir/nested.bin", new Uint8Array(4096).fill(7));
      const filesystemId = authority.identity.filesystemId;
      const authorityBridge = authority.replication;
      const plan = { flow: "authority-main-to-replica" };

      let replicaDb = await openNodeSqlite({
        filename: path.join(directory, "replica.db"),
      });
      let unbound;
      try {
        unbound = await EphemeralRuntime.open({
          database: replicaDb,
          provisioningState: "unbound-replica",
        });
        const unboundEndpoint = createReplicationEndpoint({
          bridge: unbound.replication,
          authorization: authorization(filesystemId, [plan]),
        });
        const provision = await replicate({
          bridge: authorityBridge,
          transport: new LoopbackTransport(unboundEndpoint),
          authorization: authorization(filesystemId, [plan]),
          plan,
          operationId: "op-provision-main",
        });
        assert.equal(provision.status, "complete");
        assert.equal(provision.result.activation.kind, "main");
        assert.equal(provision.result.activation.revision, "0");
      } finally {
        await unbound?.close();
        await replicaDb.close();
      }

      replicaDb = await openNodeSqlite({
        filename: path.join(directory, "replica.db"),
        create: false,
      });

      const replica = await EphemeralRuntime.open({
        database: replicaDb,
        replicationIdentity: { authorityId: "authority-a", role: "replica" },
      });
      try {
        const replicaEndpoint = createReplicationEndpoint({
          bridge: replica.replication,
          authorization: authorization(filesystemId, [plan]),
        });
        const run = await replicate({
          bridge: authorityBridge,
          transport: new LoopbackTransport(replicaEndpoint),
          authorization: authorization(filesystemId, [plan]),
          plan,
          operationId: "op-main-1",
        });
        assert.equal(run.status, "complete");
        assert.equal(run.result.activation.kind, "main");
        assert.equal(run.result.activation.revision, "3");
        assert.ok(run.result.transferredBytes > 0);

        // The destination runtime remains live across activation. Its
        // branch-scoped Node VFS must observe the newly activated namespace
        // without requiring a second filesystem core or process restart.
        const liveNodeView = replica.openNodeVfs();
        assert.ok(liveNodeView.readdirSync("/").some((entry) => entry.name === "hello.txt"));
        assert.equal(
          new TextDecoder().decode(liveNodeView.readFileSync("/hello.txt")),
          "hello world",
        );

        const replicaFs = await EphemeralFS.open({ database: replicaDb });
        try {
          assert.equal(
            await replicaFs.readFile("/hello.txt", { encoding: "utf8" }),
            "hello world",
          );
          const bytes = await replicaFs.readFile("/dir/nested.bin");
          assert.equal(bytes.byteLength, 4096);
          assert.equal(bytes[0], 7);
          assert.equal(
            await replicaFs.stat("/hello.txt").then((s) => s.size),
            11,
          );
          assert.equal(
            await replicaFs.stat("/dir/nested.bin").then((s) => s.id),
            await authority.filesystem
              .stat("/dir/nested.bin")
              .then((s) => s.id),
          );
        } finally {
          await replicaFs.close();
        }

        const rerun = await replicate({
          bridge: authorityBridge,
          transport: new LoopbackTransport(replicaEndpoint),
          authorization: authorization(filesystemId, [plan]),
          plan,
          operationId: "op-main-2",
        });
        assert.equal(rerun.status, "complete");
        assert.equal(rerun.result.transferredBytes, 0);
        assert.ok(rerun.result.reusedBytes > 0);
      } finally {
        await replica.close();
        replicaDb.close();
      }
    } finally {
      await authority.close();
      authorityDb.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("main transfer resumes after a dropped response and restart without a second revision", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-repl-restart-"));
  let authorityDb;
  let authority;
  let replicaDb;
  let replica;
  try {
    ({ database: authorityDb, runtime: authority } = await openAuthority(directory));
    await authority.filesystem.writeFile("/restart.txt", "restart-safe");
    const filesystemId = authority.identity.filesystemId;
    const plan = { flow: "authority-main-to-replica" };
    const auth = authorization(filesystemId, [plan]);
    replicaDb = await openNodeSqlite({ filename: path.join(directory, "replica.db") });
    const unbound = await EphemeralRuntime.open({
      database: replicaDb,
      provisioningState: "unbound-replica",
    });
    const provision = await replicate({
      bridge: authority.replication,
      transport: new LoopbackTransport(
        createReplicationEndpoint({ bridge: unbound.replication, authorization: auth }),
      ),
      authorization: auth,
      plan,
      operationId: "restart-provision",
    });
    assert.equal(provision.status, "complete");
    await unbound.close();
    await replicaDb.close();
    replicaDb = await openNodeSqlite({ filename: path.join(directory, "replica.db"), create: false });
    replica = await EphemeralRuntime.open({
      database: replicaDb,
      replicationIdentity: { authorityId: "authority-a", role: "replica" },
    });

    const firstEndpoint = createReplicationEndpoint({ bridge: replica.replication, authorization: auth });
    const pending = await replicate({
      bridge: authority.replication,
      transport: new DropResponseTransport(firstEndpoint, 8),
      authorization: auth,
      plan,
      operationId: "restart-main",
    });
    assert.equal(pending.status, "pending");
    const resumeKey = pending.resumeKey;
    await replica.close();
    await replicaDb.close();
    replica = undefined;
    replicaDb = undefined;
    await authority.close();
    await authorityDb.close();
    authority = undefined;
    authorityDb = undefined;

    ({ database: authorityDb, runtime: authority } = await openAuthority(directory));
    replicaDb = await openNodeSqlite({ filename: path.join(directory, "replica.db"), create: false });
    replica = await EphemeralRuntime.open({
      database: replicaDb,
      replicationIdentity: { authorityId: "authority-a", role: "replica" },
    });
    const resumed = await replicate({
      bridge: authority.replication,
      transport: new LoopbackTransport(
        createReplicationEndpoint({ bridge: replica.replication, authorization: auth }),
      ),
      authorization: auth,
      plan,
      operationId: "restart-main",
      resumeKey,
    });
    assert.equal(resumed.status, "complete");
    assert.equal(resumed.result.activation.kind, "main");
    assert.equal(resumed.result.activation.revision, "1");

    const replay = await replicate({
      bridge: authority.replication,
      transport: new LoopbackTransport(
        createReplicationEndpoint({ bridge: replica.replication, authorization: auth }),
      ),
      authorization: auth,
      plan,
      operationId: "restart-main",
      resumeKey,
    });
    assert.equal(replay.status, "complete");
    assert.deepEqual(replay.result.activation, resumed.result.activation);
    await assert.rejects(
      replicate({
        bridge: authority.replication,
        transport: new LoopbackTransport(
          createReplicationEndpoint({ bridge: replica.replication, authorization: auth }),
        ),
        authorization: { ...auth, policyVersion: "policy-changed" },
        plan,
        operationId: "restart-main",
        resumeKey,
      }),
      (error) => error instanceof ReplicationError && error.code === "UnauthorizedScope",
    );
    assert.equal(
      replicaDb.transaction(
        "read",
        (tx) => tx.all("SELECT count(*) value FROM efs_revisions", [], { maxRows: 1, maxBytes: 128 })[0].value,
      ),
      2,
    );
  } finally {
    try { await replica?.close(); } catch {}
    try { await replicaDb?.close(); } catch {}
    try { await authority?.close(); } catch {}
    try { await authorityDb?.close(); } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("provisioning adopts the authority genesis into an unbound replica", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-repl-provision-"));
  try {
    const { database: authorityDb, runtime: authority } = await openAuthority(
      directory,
    );
    try {
      await authority.filesystem.writeFile("/genesis.txt", "genesis");
      const filesystemId = authority.identity.filesystemId;
      const authorityBridge = authority.replication;
      const authorityEndpoint = createReplicationEndpoint({
        bridge: authorityBridge,
        authorization: authorization(filesystemId, [
          { flow: "authority-main-to-replica" },
        ]),
      });

      let replicaDb = await openNodeSqlite({
        filename: path.join(directory, "replica.db"),
      });
      let unbound;
      try {
        unbound = await EphemeralRuntime.open({
          database: replicaDb,
          provisioningState: "unbound-replica",
        });
        const unboundEndpoint = createReplicationEndpoint({
          bridge: unbound.replication,
          authorization: authorization(filesystemId, [
            { flow: "authority-main-to-replica" },
          ]),
        });
        const plan = { flow: "authority-main-to-replica" };
        const run = await replicate({
          bridge: authorityBridge,
          transport: new LoopbackTransport(unboundEndpoint),
          authorization: authorization(filesystemId, [plan]),
          plan,
          operationId: "op-provision-1",
        });
        assert.equal(run.status, "complete");
        assert.equal(run.result.activation.kind, "main");
      } finally {
        await unbound?.close();
        await replicaDb.close();
      }

      replicaDb = await openNodeSqlite({
        filename: path.join(directory, "replica.db"),
        create: false,
      });

      const bound = await EphemeralRuntime.open({
        database: replicaDb,
        replicationIdentity: { authorityId: "authority-a", role: "replica" },
      });
      try {
        assert.equal(bound.filesystem !== null, true);
        assert.equal(bound.identity?.filesystemId, filesystemId);
        assert.equal(
          bound.identity?.filesystemId,
          authority.identity?.filesystemId,
        );
        const plan = { flow: "authority-main-to-replica" };
        const boundEndpoint = createReplicationEndpoint({
          bridge: bound.replication,
          authorization: authorization(filesystemId, [plan]),
        });
        const mainTransfer = await replicate({
          bridge: authorityBridge,
          transport: new LoopbackTransport(boundEndpoint),
          authorization: authorization(filesystemId, [plan]),
          plan,
          operationId: "op-main-after-provision",
        });
        assert.equal(mainTransfer.status, "complete");
        assert.equal(
          await bound.filesystem.readFile("/genesis.txt", { encoding: "utf8" }),
          "genesis",
        );
        assert.equal(
          bound.filesystem.capabilities.format.cowPageBytes,
          authority.filesystem.capabilities.format.cowPageBytes,
        );
      } finally {
        await bound.close();
        replicaDb.close();
      }
    } finally {
      await authority.close();
      authorityDb.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("authority branch transfer preserves the selected generation and private content", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-repl-branch-"));
  try {
    const { database: authorityDb, runtime: authority } = await openAuthority(directory);
    try {
      await authority.filesystem.writeFile("/base.txt", "base");
      const branch = await authority.filesystem.branches.create("branch-a");
      await branch.writeFile("/private.txt", "private");
      const branchInfo = await branch.info();
      await branch.close();
      const filesystemId = authority.identity.filesystemId;
      const mainPlan = { flow: "authority-main-to-replica" };
      const replicaPath = path.join(directory, "replica.db");
      let replicaDb = await openNodeSqlite({ filename: replicaPath });
      let unbound;
      try {
        unbound = await EphemeralRuntime.open({
          database: replicaDb,
          provisioningState: "unbound-replica",
        });
        const provision = await replicate({
          bridge: authority.replication,
          transport: new LoopbackTransport(
            createReplicationEndpoint({
              bridge: unbound.replication,
              authorization: authorization(filesystemId, [mainPlan]),
            }),
          ),
          authorization: authorization(filesystemId, [mainPlan]),
          plan: mainPlan,
          operationId: "branch-provision",
        });
        assert.equal(provision.status, "complete");
      } finally {
        await unbound?.close();
        await replicaDb.close();
      }
      replicaDb = await openNodeSqlite({ filename: replicaPath, create: false });
      const replica = await EphemeralRuntime.open({
        database: replicaDb,
        replicationIdentity: { authorityId: "authority-a", role: "replica" },
      });
      try {
        const mainEndpoint = createReplicationEndpoint({
          bridge: replica.replication,
          authorization: authorization(filesystemId, [mainPlan]),
        });
        const main = await replicate({
          bridge: authority.replication,
          transport: new LoopbackTransport(mainEndpoint),
          authorization: authorization(filesystemId, [mainPlan]),
          plan: mainPlan,
          operationId: "branch-main",
        });
        assert.equal(main.status, "complete");

        const branchPlan = {
          flow: "authority-branch-to-replica",
          branchId: "branch-a",
        };
        const branchRun = await replicate({
          bridge: authority.replication,
          transport: new LoopbackTransport(
            createReplicationEndpoint({
              bridge: replica.replication,
              authorization: authorization(filesystemId, [branchPlan]),
            }),
          ),
          authorization: authorization(filesystemId, [branchPlan]),
          plan: branchPlan,
          operationId: "branch-transfer",
        });
        assert.equal(branchRun.status, "complete");
        assert.equal(branchRun.result.activation.branchId, "branch-a");
        assert.equal(branchRun.result.activation.baseRevision, String(branchInfo.baseRevision));
        assert.equal(branchRun.result.activation.generation, branchInfo.generation);
        assert.equal(branchRun.result.activation.generationDigest.length, 64);
        const received = await replica.filesystem.branches.open("branch-a");
        try {
          assert.equal(
            await received.readFile("/private.txt", { encoding: "utf8" }),
            "private",
          );
          await assert.rejects(replica.filesystem.stat("/private.txt"), { code: "ENOENT" });
        } finally {
          await received.close();
        }

        const authorityBranch = await authority.filesystem.branches.open("branch-a");
        let advancedBranchInfo;
        try {
          await authorityBranch.writeFile("/private-2.txt", "advanced");
          advancedBranchInfo = await authorityBranch.info();
        } finally {
          await authorityBranch.close();
        }
        assert.ok(advancedBranchInfo.generation > branchInfo.generation);
        const advanced = await replicate({
          bridge: authority.replication,
          transport: new LoopbackTransport(
            createReplicationEndpoint({
              bridge: replica.replication,
              authorization: authorization(filesystemId, [branchPlan]),
            }),
          ),
          authorization: authorization(filesystemId, [branchPlan]),
          plan: branchPlan,
          operationId: "branch-transfer-advanced",
        });
        assert.equal(advanced.status, "complete");
        assert.equal(advanced.result.activation.generation, advancedBranchInfo.generation);
        assert.equal(
          advanced.result.activation.generationDigest,
          advancedBranchInfo.generationDigest,
        );
        const advancedReceived = await replica.filesystem.branches.open("branch-a");
        try {
          assert.equal(
            await advancedReceived.readFile("/private-2.txt", { encoding: "utf8" }),
            "advanced",
          );
        } finally {
          await advancedReceived.close();
        }
      } finally {
        await replica.close();
        await replicaDb.close();
      }
    } finally {
      await authority.close();
      await authorityDb.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("replica branch returns, publishes with a generation guard, and returns the terminal result", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-repl-return-"));
  let authorityDb;
  let authority;
  let replicaDb;
  let replica;
  try {
    ({ database: authorityDb, runtime: authority } = await openAuthority(directory));
    await authority.filesystem.writeFile("/base.txt", "base");
    const filesystemId = authority.identity.filesystemId;
    const mainPlan = { flow: "authority-main-to-replica" };
    const auth = authorization(filesystemId, [mainPlan]);
    const replicaPath = path.join(directory, "replica.db");
    replicaDb = await openNodeSqlite({ filename: replicaPath });
    const unbound = await EphemeralRuntime.open({
      database: replicaDb,
      provisioningState: "unbound-replica",
    });
    try {
      const provision = await replicate({
        bridge: authority.replication,
        transport: new LoopbackTransport(
          createReplicationEndpoint({ bridge: unbound.replication, authorization: auth }),
        ),
        authorization: auth,
        plan: mainPlan,
        operationId: "return-provision",
      });
      assert.equal(provision.status, "complete");
    } finally {
      await unbound.close();
      await replicaDb.close();
    }
    replicaDb = await openNodeSqlite({ filename: replicaPath, create: false });
    replica = await EphemeralRuntime.open({
      database: replicaDb,
      replicationIdentity: { authorityId: "authority-a", role: "replica" },
    });
    const main = await replicate({
      bridge: authority.replication,
      transport: new LoopbackTransport(
        createReplicationEndpoint({ bridge: replica.replication, authorization: auth }),
      ),
      authorization: auth,
      plan: mainPlan,
      operationId: "return-main",
    });
    assert.equal(main.status, "complete");

    const branch = await replica.filesystem.branches.create("returned");
    await branch.writeFile("/private.txt", "returned-value");
    const beforeReturn = await branch.info();
    await branch.close();
    const returnPlan = { flow: "replica-branch-to-authority", branchId: "returned" };
    const returnAuth = authorization(filesystemId, [returnPlan]);
    const returned = await replicate({
      bridge: replica.replication,
      transport: new LoopbackTransport(
        createReplicationEndpoint({ bridge: authority.replication, authorization: returnAuth }),
      ),
      authorization: returnAuth,
      plan: returnPlan,
      operationId: "return-branch",
    });
    assert.equal(returned.status, "complete");
    assert.equal(returned.result.activation.state, "active");
    assert.equal(returned.result.activation.generation, beforeReturn.generation);
    const authorityBranch = await authority.filesystem.branches.open("returned");
    const publicationRequest = await authorityBranch.info();
    const published = await authorityBranch.publish({
      operationId: "return-publication",
      expectedGeneration: publicationRequest.generation,
      expectedGenerationDigest: publicationRequest.generationDigest,
    });
    assert.equal(published.outcome, "merged");
    assert.equal(
      await authority.filesystem.readFile("/private.txt", { encoding: "utf8" }),
      "returned-value",
    );
    await authorityBranch.close();

    const catchup = await replicate({
      bridge: authority.replication,
      transport: new LoopbackTransport(
        createReplicationEndpoint({ bridge: replica.replication, authorization: auth }),
      ),
      authorization: auth,
      plan: mainPlan,
      operationId: "return-main-catchup",
    });
    assert.equal(catchup.status, "complete");

    const terminalPlan = { flow: "authority-branch-to-replica", branchId: "returned" };
    const terminalAuth = authorization(filesystemId, [terminalPlan]);
    const terminal = await replicate({
      bridge: authority.replication,
      transport: new LoopbackTransport(
        createReplicationEndpoint({ bridge: replica.replication, authorization: terminalAuth }),
      ),
      authorization: terminalAuth,
      plan: terminalPlan,
      operationId: "return-terminal",
    });
    assert.equal(terminal.status, "complete");
    assert.equal(terminal.result.activation.state, "merged");
    assert.equal(terminal.result.activation.authorityResult?.kind, "publication");
    assert.equal(
      terminal.result.activation.authorityResult?.operationId,
      "return-publication",
    );
    await assert.rejects(
      Promise.resolve().then(() => replica.openNodeVfs({ branchId: "returned" })),
      (error) => error?.code === "EROFS",
    );
  } finally {
    try { await replica?.close(); } catch {}
    try { await replicaDb?.close(); } catch {}
    try { await authority?.close(); } catch {}
    try { await authorityDb?.close(); } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

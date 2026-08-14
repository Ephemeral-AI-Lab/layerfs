import type { EphemeralFS as PublicEphemeralFS } from "./ephemeral-fs.js";
import type {
  OpenFilesystemOptions,
  ReplicationFilesystemBridge,
  ReplicationFilesystemIdentity,
  ReplicationRole,
} from "./types.js";
import { EphemeralFS as OperationsFilesystem } from "../operations/filesystem.js";
import type { NodeVfsFilesystemBridge } from "../operations/node-vfs-bridge.js";
import { createReplicationOperationsBridge } from "../operations/replication-bridge.js";
import { buildUnboundReplicationCapabilities } from "../operations/replication-capabilities.js";
import {
  AdmissionController,
  constrainStorageLimits,
  DEFAULT_RUNTIME_LIMITS,
  DEFAULT_STORAGE_LIMITS,
  RuntimeConcurrency,
  resolveLimits,
} from "../resources/limits.js";
import { createSqliteOperationsStorage } from "../sqlite/operations-storage.js";
import { initializeOrValidateUnboundReplicaSchema } from "../sqlite/schema.js";

export interface OpenEphemeralRuntimeOptions extends OpenFilesystemOptions {
  readonly provisioningState?: "bound" | "unbound-replica";
  readonly replicationIdentity?: {
    readonly authorityId: string;
    readonly role: ReplicationRole;
  };
}

/** One ownership root for the portable FS, replication, and branch Node VFS. */
export class EphemeralRuntime {
  readonly provisioningState: "bound" | "unbound-replica";
  readonly identity: ReplicationFilesystemIdentity | null;
  readonly filesystem: PublicEphemeralFS | null;
  readonly replication: ReplicationFilesystemBridge;
  readonly #operations: OperationsFilesystem | null;
  readonly #markReplicationClosed: () => void;
  #closed = false;

  private constructor(options: {
    readonly provisioningState: "bound" | "unbound-replica";
    readonly identity: ReplicationFilesystemIdentity | null;
    readonly operations: OperationsFilesystem | null;
    readonly storage: ReturnType<typeof createSqliteOperationsStorage>;
    readonly replication: ReplicationFilesystemBridge;
    readonly markReplicationClosed?: () => void;
  }) {
    this.provisioningState = options.provisioningState;
    this.identity = options.identity;
    this.#operations = options.operations;
    this.filesystem = options.operations as unknown as PublicEphemeralFS | null;
    this.replication = options.replication;
    this.#markReplicationClosed = options.markReplicationClosed ?? (() => undefined);
  }

  static async open(options: OpenEphemeralRuntimeOptions): Promise<EphemeralRuntime> {
    const {
      provisioningState = "bound",
      replicationIdentity,
      ...filesystemOptions
    } = options;
    const storage = createSqliteOperationsStorage(options.database);
    if (provisioningState === "unbound-replica") {
      try {
        if (replicationIdentity !== undefined)
          throw new Error(
            "ProvisioningRejected: an unbound replica cannot have a bound identity",
          );
        initializeOrValidateUnboundReplicaSchema(options.database);
        const runtimeLimits = resolveLimits(DEFAULT_RUNTIME_LIMITS, options.runtime);
        const storageLimits = constrainStorageLimits({}, options.database.capabilities);
        const admission = new AdmissionController(
          runtimeLimits.maxManagedResidentBytes,
        );
        const concurrency = new RuntimeConcurrency(runtimeLimits);
        let closed = false;
        const replication = createReplicationOperationsBridge({
          capabilities: buildUnboundReplicationCapabilities(storageLimits),
          storage,
          storageLimits,
          admission,
          concurrency,
          assertOpen: () => {
            if (closed) throw new Error("Closed: replication runtime is closed");
          },
        });
        return new EphemeralRuntime({
          provisioningState,
          identity: null,
          operations: null,
          storage,
          replication,
          markReplicationClosed: () => {
            closed = true;
          },
        });
      } catch (error) {
        await storage.close();
        throw error;
      }
    }
    try {
      const operations = await OperationsFilesystem.open(
        { ...filesystemOptions, ownsDatabase: false },
        storage,
      );
      const identity = operations.configureReplicationIdentity(replicationIdentity);
      return new EphemeralRuntime({
        provisioningState,
        identity,
        operations,
        storage,
        replication: operations.createReplicationBridge(),
      });
    } catch (error) {
      await storage.close();
      throw error;
    }
  }

  openNodeVfs(options: { readonly branchId?: string } = {}): NodeVfsFilesystemBridge {
    if (this.#closed) throw new Error("Closed: filesystem runtime is closed");
    if (!this.#operations)
      throw new Error("ProvisioningRejected: unbound replica exposes no Node VFS view");
    return this.#operations.createNodeVfsBridge(options.branchId);
  }

  async close(): Promise<void> {
    if (this.#closed) return;
    this.#closed = true;
    this.#markReplicationClosed();
    await this.#operations?.close();
  }
}

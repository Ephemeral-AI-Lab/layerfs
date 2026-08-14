import type {
  CreateReplicationSessionRequest,
  ReplicationBatchAcceptanceRequest,
  ReplicationBridgeCapabilities,
  ReplicationFilesystemBridge,
  ReplicationSessionStore,
} from "../filesystem/types.js";
import { AdmissionController, RuntimeConcurrency } from "../resources/limits.js";
import { copyBytes } from "../cas/bytes.js";
import type { OperationsStorage, StorageTransactionPorts } from "./storage-ports.js";
import type { ContentCache } from "../cache/content-cache.js";

function byteLength(value: unknown): number {
  if (value instanceof Uint8Array) return value.byteLength;
  if (typeof value === "string") return new TextEncoder().encode(value).byteLength;
  if (!value || typeof value !== "object") return 8;
  if (Array.isArray(value))
    return value.reduce((sum, item) => sum + byteLength(item), 32);
  return Object.entries(value).reduce(
    (sum, [name, item]) => sum + name.length * 2 + byteLength(item),
    64,
  );
}

class Bridge implements ReplicationFilesystemBridge {
  readonly capabilities: ReplicationBridgeCapabilities;
  readonly #storage: OperationsStorage;
  readonly #storageLimits: import("../resources/limits.js").StorageLimits;
  readonly #admission: AdmissionController;
  readonly #concurrency: RuntimeConcurrency;
  readonly #cache: ContentCache | undefined;
  readonly #assertOpen: () => void;
  readonly #branchDigest:
    | ((tx: StorageTransactionPorts, branchId: string, generation: number) => string)
    | null;
  readonly #sessionOperations = new Map<string, string>();
  readonly #sessionNonces = new Map<string, Uint8Array>();

  constructor(options: {
    readonly capabilities: ReplicationBridgeCapabilities;
    readonly storage: OperationsStorage;
    readonly storageLimits: import("../resources/limits.js").StorageLimits;
    readonly admission: AdmissionController;
    readonly concurrency: RuntimeConcurrency;
    readonly cache?: ContentCache;
    readonly assertOpen: () => void;
    readonly branchDigest?:
      (tx: StorageTransactionPorts, branchId: string, generation: number) => string;
  }) {
    this.capabilities = options.capabilities;
    this.#storage = options.storage;
    this.#storageLimits = options.storageLimits;
    this.#admission = options.admission;
    this.#concurrency = options.concurrency;
    this.#cache = options.cache;
    this.#assertOpen = options.assertOpen;
    this.#branchDigest = options.branchDigest ?? null;
  }

  #sessionIdOf(sessionId: string): string {
    const operationId = this.#sessionOperations.get(sessionId);
    if (!operationId)
      throw new Error("CursorMismatch: session is not bound to a durable operation");
    return operationId;
  }

  #register(sessionId: string, operationId: string, ownerNonce: Uint8Array): void {
    this.#sessionOperations.set(sessionId, operationId);
    this.#sessionNonces.set(sessionId, copyBytes(ownerNonce));
  }

  async #execute<T>(
    mode: "read" | "write",
    input: unknown,
    callback: (
      store: ReplicationSessionStore,
      transfer: import("./storage-ports.js").ReplicationTransferStore,
    ) => T,
    minimumBytes = 0,
  ): Promise<T> {
    this.#assertOpen();
    const releaseOperation = this.#concurrency.tryAcquireOperation();
    if (!releaseOperation)
      throw new Error("Busy: replication operation concurrency is exhausted");
    const chargedBytes = Math.max(4096, minimumBytes, byteLength(input) + 4096);
    let releaseBytes: (() => void) | undefined;
    try {
      try {
        releaseBytes = this.#admission.reserve(chargedBytes);
      } catch {
        throw new Error("ResourceLimit: replication managed-memory admission failed");
      }
      return this.#storage.transaction(
        mode,
        {
          maxRows: 8192,
          maxBytes: chargedBytes,
          maxStatements: this.#storageLimits.maxFinalTransactionRows * 4,
          maxResultRows: 8192,
          maxResultBytes: chargedBytes,
        },
        (ports) =>
          callback(
            ports.replication(),
            ports.replicationTransfer(
              this.#storageLimits,
              this.#cache,
              this.#branchDigest
                ? (branchId, generation) =>
                    this.#branchDigest!(ports, branchId, generation)
                : undefined,
            ),
          ),
      );
    } finally {
      releaseBytes?.();
      releaseOperation();
    }
  }

  createOrResumeSession(request: CreateReplicationSessionRequest) {
    this.#register(
      request.binding.sessionId,
      request.binding.operationId,
      request.binding.ownerNonce,
    );
    return this.#execute("write", request, (store) => store.createOrResume(request));
  }

  resumeSession(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly resumeKey: Uint8Array;
  }) {
    return this.#execute("read", request, (store) => store.resume(request));
  }

  findSession(request: {
    readonly operationId: string;
    readonly resumeKey: Uint8Array;
  }) {
    return this.#execute("read", request, (store) => store.findSession(request));
  }

  async loadSession(request: { readonly operationId: string }) {
    const loaded = await this.#execute("read", request, (store) =>
      store.loadSession(request),
    );
    this.#register(
      loaded.binding.sessionId,
      loaded.binding.operationId,
      loaded.binding.ownerNonce,
    );
    return loaded;
  }

  acceptBatch(request: ReplicationBatchAcceptanceRequest & {
    readonly records?: readonly import("./storage-ports.js").ReplicationTransferRecord[];
  }) {
    return this.#execute(
      "write",
      request,
      (store, transfer) => {
        const outcome = store.acceptBatch(request);
        if (!outcome.replayed && request.records && request.records.length > 0) {
          const apply = transfer.applyImportRecords({
            sessionId: this.#sessionIdOf(request.sessionId),
            records: request.records,
            now: request.now,
          });
          return { ...outcome, apply };
        }
        return outcome;
      },
      Math.max(64 * 1024, request.records?.length ?? 0) * 64 + 4096,
    );
  }

  compactReceipts(request: {
    readonly operationId: string;
    readonly ownerNonce: Uint8Array;
    readonly throughSequence: number;
    readonly maxRows: number;
  }) {
    return this.#execute("write", request, (store) => store.compactReceipts(request));
  }

  maintenance(request: { readonly now: number; readonly maxRows: number }) {
    return this.#execute("write", request, (store, transfer) => {
      const transferResult = transfer.maintenance({ now: request.now, limit: request.maxRows });
      const sessionResult = store.maintenance(request);
      return {
        expiredSessions: sessionResult.expiredSessions,
        expiredLeases: transferResult.expiredLeases,
        cleanupPasses: transferResult.cleanupPasses,
      };
    });
  }

  abortSession(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
  }) {
    return this.#execute("write", request, (store, transfer) => {
      transfer.abortImportIfPresent({
        sessionId: this.#sessionIdOf(request.sessionId),
        ownerNonce: request.ownerNonce,
        now: request.now,
      });
      store.abortSession(request);
    });
  }

  consumeAttempt(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly wallNowMs: number;
    readonly monotonicElapsedMs: number;
    readonly delayMs: number;
  }) {
    return this.#execute("write", request, (store) => store.consumeAttempt(request));
  }

  recordOutboundBatch(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly sequence: number;
    readonly phase: import("../filesystem/types.js").ReplicationPhase;
    readonly nextPhase: import("../filesystem/types.js").ReplicationPhase;
    readonly nextCursor: Uint8Array;
    readonly nextCursorDigest: Uint8Array;
  }) {
    return this.#execute("write", request, (store) =>
      store.recordOutboundBatch(request),
    );
  }

  storeTerminalResult(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly result: Uint8Array;
    readonly now: number;
  }) {
    return this.#execute("write", request, (store) =>
      store.storeTerminalResult(request),
    );
  }

  replayTerminalResult(request: {
    readonly operationId: string;
    readonly sessionId: string;
    readonly resumeKey: Uint8Array;
    readonly now: number;
  }) {
    return this.#execute(
      "read",
      request,
      (store) => store.replayTerminalResult(request),
      1024 * 1024 + 4096,
    );
  }

  captureExport(request: {
    readonly sessionId: string;
    readonly flow: import("../filesystem/types.js").ReplicationFlow;
    readonly branchId: string | null;
    readonly destinationHead: number;
    readonly now: number;
  }) {
    return this.#execute(
      "write",
      request,
      (_store, transfer) =>
        transfer.captureExport({
          ...request,
          sessionId: this.#sessionIdOf(request.sessionId),
          expiresAt: request.now + 24 * 60 * 60 * 1000,
        }),
      64 * 1024,
    );
  }

  captureGenesis(request: { readonly sessionId: string; readonly now: number }) {
    return this.#execute(
      "write",
      request,
      (_store, transfer) =>
        transfer.captureGenesis({
          ...request,
          sessionId: this.#sessionIdOf(request.sessionId),
          expiresAt: request.now + 24 * 60 * 60 * 1000,
        }),
      64 * 1024,
    );
  }

  readExportBatch(request: {
    readonly sessionId: string;
    readonly flow: import("../filesystem/types.js").ReplicationFlow;
    readonly branchId: string | null;
    readonly maxEntries: number;
    readonly maxBytes: number;
    readonly now: number;
  }) {
    return this.#execute(
      "write",
      request,
      (_store, transfer) => transfer.readExportBatch({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
      request.maxBytes + 4096,
    );
  }

  readExportPayloads(request: {
    readonly sessionId: string;
    readonly requested: readonly {
      readonly contentKind: "object" | "manifest-root" | "manifest-node";
      readonly digest: Uint8Array;
    }[];
    readonly maxEntries: number;
    readonly maxBytes: number;
    readonly now: number;
  }) {
    return this.#execute(
      "read",
      request,
      (_store, transfer) => transfer.readExportPayloads({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
      request.maxBytes + 4096,
    );
  }

  readExportStateBatch(request: {
    readonly sessionId: string;
    readonly flow: import("../filesystem/types.js").ReplicationFlow;
    readonly branchId: string | null;
    readonly maxEntries: number;
    readonly maxBytes: number;
    readonly now: number;
    readonly checkpoint: boolean;
    readonly allowTerminal: boolean;
  }) {
    return this.#execute(
      "write",
      request,
      (_store, transfer) => transfer.readExportStateBatch({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
      request.maxBytes + 4096,
    );
  }

  exportSummary(request: {
    readonly sessionId: string;
    readonly flow: import("../filesystem/types.js").ReplicationFlow;
  }) {
    return this.#execute("read", request, (_store, transfer) =>
      transfer.exportSummary({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
    );
  }

  beginImport(request: {
    readonly sessionId: string;
    readonly kind: 0 | 1 | 2;
    readonly leaseId: string;
    readonly ownerNonce: Uint8Array;
    readonly branchId: string | null;
    readonly baseRevision: number | null;
    readonly generation: number | null;
    readonly expectedGenerationDigest: Uint8Array | null;
    readonly now: number;
    readonly expiresAt: number;
    readonly maxStagingBytesPerSession: number;
    readonly resultRetentionMs: number;
  }) {
    return this.#execute("write", request, (_store, transfer) =>
      transfer.beginImport({
        ...request,
        sessionId: this.#sessionIdOf(request.sessionId),
        ingestReservationBytes: 0,
        metadataReservationBytes: 4096,
        resultRetentionMs: request.resultRetentionMs,
      }),
    );
  }

  readMissingContent(request: {
    readonly sessionId: string;
    readonly maxEntries: number;
    readonly maxBytes: number;
  }) {
    return this.#execute("read", request, (_store, transfer) =>
      transfer.readMissingContent({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
      request.maxBytes + 4096,
    );
  }

  finalizeImport(request: {
    readonly sessionId: string;
    readonly kind: 0 | 1 | 2;
    readonly expectedRevision: number;
    readonly expectedRootMutationGeneration: number;
    readonly expectedNextAllocationSequence: number;
    readonly expectedRootInode: string;
    readonly expectedRevisionCount: number;
    readonly expectedStateRows: number;
    readonly expectedClosureRoots: number;
    readonly expectedClosureNodes: number;
    readonly expectedClosureObjects: number;
    readonly expectedClosureObjectBytes: number;
    readonly branchId: string | null;
    readonly baseRevision: string | null;
    readonly generation: number | null;
    readonly generationDigest: Uint8Array | null;
    readonly checkpoint: boolean;
    readonly terminalState: 0 | 1 | 2;
    readonly terminalResultOperationId: string | null;
    readonly terminalResultBytes: Uint8Array | null;
    readonly genesisMeta: import("./storage-ports.js").ReplicationExportMeta | null;
    readonly genesisRows: readonly {
      readonly inodeId: string;
      readonly tombstone: boolean;
      readonly encoded: Uint8Array | null;
    }[];
    readonly now: number;
  }) {
    return this.#execute(
      "write",
      request,
      (_store, transfer) => transfer.finalizeImport({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
      Math.max(64 * 1024, request.expectedClosureObjectBytes) + 4096,
    );
  }

  renewImportLease(request: {
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
    readonly expiresAt: number;
  }) {
    return this.#execute("write", request, (_store, transfer) =>
      transfer.renewLease({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
    );
  }

  abortImport(request: {
    readonly sessionId: string;
    readonly ownerNonce: Uint8Array;
    readonly now: number;
  }) {
    return this.#execute("write", request, (_store, transfer) =>
      transfer.abortImport({ ...request, sessionId: this.#sessionIdOf(request.sessionId) }),
    );
  }
}

export function createReplicationOperationsBridge(options: {
  readonly capabilities: ReplicationBridgeCapabilities;
  readonly storage: OperationsStorage;
  readonly storageLimits: import("../resources/limits.js").StorageLimits;
  readonly admission: AdmissionController;
  readonly concurrency: RuntimeConcurrency;
  readonly cache?: ContentCache;
  readonly assertOpen: () => void;
  readonly branchDigest?:
    (tx: StorageTransactionPorts, branchId: string, generation: number) => string;
}): ReplicationFilesystemBridge {
  return new Bridge(options);
}

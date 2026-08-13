/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs-node-vfs; subpath: .; entry: packages/node-vfs/dist/index.d.ts */

/* export: CowPageBytes; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export type CowPageBytes = 4096 | 8192 | 16384;

/* export: FlushOptions; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface FlushOptions {
    readonly dataOnly?: boolean;
}

/* export: NodeFileSession; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface NodeFileSession {
    readonly id: string;
    readonly path: string;
    readonly writable: boolean;
    readIntoSync(destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    readRangeSync(position: number, length: number): Uint8Array;
    writeSync(content: Uint8Array, position: number): number;
    truncateSync(size: number): void;
    statSync(): FileStat;
    stagePrefixSync(): void;
    commitVisibleSync(options?: FlushOptions): void;
    flushSync(options?: FlushOptions): void;
    closeSync(): void;
    abortSync(): void;
}

/* export: NodeVfsCapabilities; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface NodeVfsCapabilities {
    readonly cowPageBytes: CowPageBytes;
    readonly runtime: Readonly<RuntimeLimits>;
    readonly preferredReadBytes: number;
    readonly supportsDirectRangeIo: true;
    readonly supportsWriteSessions: true;
    readonly supportsDataSync: boolean;
}

/* export: NodeVfsHandle; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface NodeVfsHandle {
    readonly filesystem: EphemeralFS;
    readonly provider: NodeVfsProvider;
    close(): Promise<void>;
}

/* export: NodeVfsMetrics; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface NodeVfsMetrics {
    snapshot(): NodeVfsMetricsSnapshot;
}

/* export: NodeVfsMetricsSnapshot; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface NodeVfsMetricsSnapshot {
    readonly openSessions: number;
    readonly peakOpenSessions: number;
    readonly dirtySessions: number;
    readonly residentWriteBytes: number;
    readonly peakResidentWriteBytes: number;
    readonly residentControlBytes: number;
    readonly peakManagedResidentBytes: number;
    readonly stagedLogicalBytes: number;
    readonly admittedWriteBytes: number;
    readonly flushedWriteBytes: number;
    readonly flushCount: number;
    readonly forcedFlushCount: number;
    readonly failedFlushCount: number;
    readonly rejectedWriteCount: number;
    readonly directReadBytes: number;
    readonly coreBatchCount: number;
    readonly cowEditCount: number;
    readonly cowEditSourceBytes: number;
    readonly callbackSizeDistribution: Readonly<{
        upTo4KiB: number;
        upTo64KiB: number;
        upTo1MiB: number;
        over1MiB: number;
    }>;
    readonly contiguousRunBytes: number;
    readonly peakContiguousRunBytes: number;
    readonly flushReasonCounts: Readonly<{
        explicitCommit: number;
        flush: number;
        close: number;
        providerSync: number;
    }>;
}

/* export: NodeVfsObservation; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export type NodeVfsObservation = {
    readonly kind: "session-open";
    readonly sessionId: string;
} | {
    readonly kind: "session-close";
    readonly sessionId: string;
} | {
    readonly kind: "forced-flush";
    readonly bytes: number;
} | {
    readonly kind: "flush-failed";
    readonly code: string;
} | {
    readonly kind: "memory-rejected";
    readonly bytes: number;
};

/* export: NodeVfsObserver; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export type NodeVfsObserver = (event: NodeVfsObservation) => void;

/* export: NodeVfsProvider; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface NodeVfsProvider {
    readonly capabilities: NodeVfsCapabilities;
    readonly metrics: NodeVfsMetrics;
    existsSync(path: string): boolean;
    statSync(path: string): FileStat;
    lstatSync(path: string): FileStat;
    readdirSync(path: string): string[];
    readlinkSync(path: string): string;
    readRangeSync(path: string, position: number, length: number): Uint8Array;
    openFileSync(path: string, options?: OpenFileOptions): NodeFileSession;
    mkdirSync(path: string, options?: {
        recursive?: boolean;
        mode?: number;
    }): void;
    chmodSync(path: string, mode: number): void;
    linkSync(existingPath: string, newPath: string): void;
    symlinkSync(target: string, path: string): void;
    renameSync(oldPath: string, newPath: string): void;
    unlinkSync(path: string): void;
    rmdirSync(path: string): void;
    syncSync(): void;
    closeSync(): void;
}

/* export: OpenFileOptions; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface OpenFileOptions {
    readonly writable?: boolean;
    readonly create?: boolean;
    readonly exclusive?: boolean;
    readonly truncate?: boolean;
    readonly mode?: number;
}

/* export: openNodeVfs; kinds: value */
/* source: packages/node-vfs/dist/index.d.ts */
export declare function openNodeVfs(options: OpenNodeVfsOptions): Promise<NodeVfsHandle>;

/* export: OpenNodeVfsOptions; kinds: type */
/* source: packages/node-vfs/dist/index.d.ts */
export interface OpenNodeVfsOptions {
    readonly database: NodeSQLiteDriver;
    readonly branchId?: string;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly observer?: NodeVfsObserver;
    readonly ownsDatabase?: boolean;
}

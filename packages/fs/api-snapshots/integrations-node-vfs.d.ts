/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: ./integrations/node-vfs; entry: packages/fs/dist/integrations/node-vfs.d.ts */

/* export: createNodeVfsBridge; kinds: value */
/* source: packages/fs/dist/integrations/node-vfs.d.ts */
/** Compose the public bridge with the private SQLite storage implementation. */
export declare function createNodeVfsBridge(options: CreateNodeVfsBridgeOptions): NodeVfsFilesystemBridge;

/* export: CreateNodeVfsBridgeOptions; kinds: type */
/* source: packages/fs/dist/integrations/node-vfs.d.ts */
/** Public composition-root options for the synchronous Node VFS bridge. */
export interface CreateNodeVfsBridgeOptions {
    readonly database: FilesystemSQLiteDriver;
    readonly filesystem?: Partial<FilesystemLimits>;
    readonly storage?: Partial<StorageLimits>;
    readonly runtime?: Partial<RuntimeLimits>;
    readonly format?: StorageFormatOptions;
    readonly clock?: () => number;
}

/* export: NodeVfsFilesystemBridge; kinds: type */
/* source: packages/fs/dist/operations/node-vfs-bridge.d.ts */
export interface NodeVfsFilesystemBridge {
    readonly filesystemLimits: Readonly<FilesystemLimits>;
    readonly storageLimits: Readonly<StorageLimits>;
    readonly runtimeLimits: Readonly<RuntimeLimits>;
    readonly cowPageBytes: 4096 | 8192 | 16384;
    canonicalPathSync(path: string, syscall?: string): string;
    resolvePathSync(path: string, followFinal?: boolean): NodeVfsResolvedPath;
    openPinnedReadSync(path: string): NodeVfsPinnedReadBridge;
    acquireSlabSync(source: Uint8Array, sourceOffset: number, length: number): NodeVfsManagedSlab | undefined;
    reserveControlSync(bytes: number): (() => void) | undefined;
    managedMemorySync(): NodeVfsManagedMemorySnapshot;
    existsSync(path: string): boolean;
    statSync(path: string, followFinal?: boolean): FileStat;
    readdirSync(path: string): DirectoryEntry[];
    readlinkSync(path: string): string;
    readIntoSync(path: string, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    readRangeSync(path: string, position: number, length: number): Uint8Array;
    readFileSync(path: string): Uint8Array;
    prepareContentSync(bytes: Uint8Array): NodeVfsPreparedContent;
    prepareContentSourceSync(source: SynchronousContentSource): NodeVfsPreparedContent;
    prepareOverwriteSync(path: string, offset: number, source: SynchronousContentSource): NodeVfsPreparedContent | undefined;
    prepareOverwritesSync(path: string, edits: readonly NodeVfsOverwriteEdit[]): NodeVfsPreparedContent | undefined;
    abortPreparedSync(prepared: NodeVfsPreparedContent): void;
    readPreparedIntoSync(prepared: NodeVfsPreparedContent, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    commitPreparedSync(path: string, prepared: NodeVfsPreparedContent, options?: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
        inodeId?: string;
        aliases?: readonly string[];
    }): NodeVfsCommitResult;
    writeFileSync(path: string, bytes: Uint8Array, options?: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
    }): void;
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
}

/* export: NodeVfsManagedSlab; kinds: type */
/* source: packages/fs/dist/operations/node-vfs-bridge.d.ts */
export interface NodeVfsManagedSlab {
    readonly bytes: Uint8Array;
    release(): void;
}

/* export: NodeVfsPinnedReadBridge; kinds: type */
/* source: packages/fs/dist/operations/node-vfs-bridge.d.ts */
export interface NodeVfsPinnedReadBridge {
    readonly canonicalPath: string;
    readonly inodeId: string;
    readonly stat: FileStat;
    readonly size: number;
    readIntoSync(destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    closeSync(): void;
}

/* export: NodeVfsPreparedContent; kinds: type */
/* source: packages/fs/dist/operations/node-vfs-bridge.d.ts */
/** Opaque durable content owned by the core bridge. */
export interface NodeVfsPreparedContent {
    readonly size: number;
    /** Bounded source bytes read while applying page-local edits. */
    readonly editSourceBytes?: number;
}

/* export: openNodeVfsBridge; kinds: value */
/* source: packages/fs/dist/integrations/node-vfs.d.ts */
/**
 * Open the portable filesystem and its synchronous bridge as one core instance.
 * This is the production Node VFS composition root: both views share limits,
 * caches, concurrency, and the aggregate admission controller.
 */
export declare function openNodeVfsBridge(options: OpenFilesystemOptions): Promise<OpenNodeVfsBridgeResult>;

/* export: OpenNodeVfsBridgeResult; kinds: type */
/* source: packages/fs/dist/integrations/node-vfs.d.ts */
export interface OpenNodeVfsBridgeResult {
    readonly filesystem: PublicEphemeralFS;
    readonly bridge: NodeVfsFilesystemBridge;
}

/* export: SynchronousContentSource; kinds: type */
/* source: packages/fs/dist/operations/streaming-prepare.d.ts */
/**
 * Synchronous, bounded content source used by the Node VFS bridge. The source
 * owns neither the destination nor any durable state and must fill exactly the
 * requested range before returning.
 */
export interface SynchronousContentSource {
    readonly size: number;
    readInto(destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
}

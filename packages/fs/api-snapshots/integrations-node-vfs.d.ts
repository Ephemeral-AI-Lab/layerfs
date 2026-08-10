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
    existsSync(path: string): boolean;
    statSync(path: string, followFinal?: boolean): FileStat;
    readdirSync(path: string): DirectoryEntry[];
    readlinkSync(path: string): string;
    readIntoSync(path: string, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    readRangeSync(path: string, position: number, length: number): Uint8Array;
    readFileSync(path: string): Uint8Array;
    prepareContentSync(bytes: Uint8Array): SyncPreparedContent;
    readPreparedIntoSync(prepared: SyncPreparedContent, destination: Uint8Array, destinationOffset: number, position: number, length: number): number;
    commitPreparedSync(path: string, prepared: SyncPreparedContent, options?: {
        create?: boolean;
        exclusive?: boolean;
        mode?: number;
    }): void;
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

/* export: SyncPreparedContent; kinds: type */
/* source: packages/fs/dist/operations/node-vfs-bridge.d.ts */
export interface SyncPreparedContent {
    readonly manifestHash: Uint8Array;
    readonly size: number;
    readonly certificate: ClosureCertificate;
}

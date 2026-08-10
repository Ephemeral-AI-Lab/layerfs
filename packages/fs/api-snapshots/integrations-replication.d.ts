/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: ./integrations/replication; entry: packages/fs/dist/integrations/replication.d.ts */

/* export: ReplicationFilesystemBridge; kinds: type */
/* source: packages/fs/dist/integrations/replication.d.ts */
export interface ReplicationFilesystemBridge {
    readonly capabilities: Readonly<Record<string, unknown>>;
    captureExport(plan: ReplicationPlan): Promise<unknown>;
    readExportBatch(request: unknown): Promise<unknown>;
    applyImportBatch(batch: unknown): Promise<unknown>;
    finalizeImport(request: unknown): Promise<void>;
    abortSession(sessionId: string): Promise<void>;
}

/* export: ReplicationPlan; kinds: type */
/* source: packages/fs/dist/integrations/replication.d.ts */
export interface ReplicationPlan {
    readonly pullMain?: boolean;
    readonly pushBranchId?: string;
    readonly pullBranchId?: string;
}

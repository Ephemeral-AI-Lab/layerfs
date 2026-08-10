export interface ReplicationPlan {
  readonly pullMain?: boolean;
  readonly pushBranchId?: string;
  readonly pullBranchId?: string;
}
export interface ReplicationFilesystemBridge {
  readonly capabilities: Readonly<Record<string, unknown>>;
  captureExport(plan: ReplicationPlan): Promise<unknown>;
  readExportBatch(request: unknown): Promise<unknown>;
  applyImportBatch(batch: unknown): Promise<unknown>;
  finalizeImport(request: unknown): Promise<void>;
  abortSession(sessionId: string): Promise<void>;
}

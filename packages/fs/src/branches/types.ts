import type { EphemeralFilesystem } from "../filesystem/types.js";

export type RevisionId = string;
export type BranchState = "active" | "merged" | "discarded";
export interface BranchInfo {
  readonly id: string;
  readonly baseRevision: RevisionId;
  readonly state: BranchState;
  readonly generation: number;
  readonly createdAt: number;
  readonly terminalAt: number | null;
  readonly mergedRevision: RevisionId | null;
}
export interface CreateBranchOptions {
  readonly id?: string;
  readonly baseRevision?: RevisionId;
}
export interface PublishOptions {
  readonly operationId?: string;
}
export type ConflictReason =
  | "entry-changed"
  | "node-changed"
  | "source-changed"
  | "destination-changed"
  | "subtree-changed"
  | "ancestor-changed";
export interface PublishConflict {
  readonly path: string;
  readonly reason: ConflictReason;
  readonly expectedRevision: RevisionId | null;
  readonly actualRevision: RevisionId | null;
}
export interface MergedPublishResult {
  readonly outcome: "merged";
  readonly branchId: string;
  readonly operationId: string | null;
  readonly baseRevision: RevisionId;
  readonly parentRevision: RevisionId;
  readonly revision: RevisionId;
  readonly changedPaths: string[];
  readonly conflicts: [];
}
export interface ConflictPublishResult {
  readonly outcome: "conflict";
  readonly branchId: string;
  readonly operationId: string | null;
  readonly baseRevision: RevisionId;
  readonly headRevision: RevisionId;
  readonly revision: null;
  readonly changedPaths: [];
  readonly conflicts: PublishConflict[];
}
export type PublishResult = MergedPublishResult | ConflictPublishResult;
export interface EphemeralBranch extends Omit<EphemeralFilesystem, "close"> {
  readonly id: string;
  info(): Promise<BranchInfo>;
  publish(options?: PublishOptions): Promise<PublishResult>;
  discard(): Promise<BranchInfo>;
  close(): Promise<void>;
}
export interface Branches {
  create(id: string): Promise<EphemeralBranch>;
  create(options?: CreateBranchOptions): Promise<EphemeralBranch>;
  open(id: string): Promise<EphemeralBranch>;
  get(id: string): Promise<BranchInfo>;
  replay(operationId: string, branchId?: string): Promise<PublishResult>;
}
export type BranchErrorCode =
  | "InvalidBranchId"
  | "InvalidOperationId"
  | "BranchNotFound"
  | "BranchNotActive"
  | "RevisionNotFound"
  | "BranchChanged"
  | "OperationBranchMismatch"
  | "OperationNotFound"
  | "OperationResultExpired"
  | "LimitExceeded";
export class BranchError extends Error {
  readonly name = "BranchError" as const;
  readonly code: BranchErrorCode;
  readonly branchId?: string;
  readonly operationId?: string;
  readonly limit?: string;
  constructor(
    code: BranchErrorCode,
    message: string,
    details: { branchId?: string; operationId?: string; limit?: string } = {},
  ) {
    super(message);
    this.code = code;
    if (details.branchId !== undefined) this.branchId = details.branchId;
    if (details.operationId !== undefined) this.operationId = details.operationId;
    if (details.limit !== undefined) this.limit = details.limit;
  }
}

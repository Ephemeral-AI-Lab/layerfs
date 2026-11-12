/**
 * The Storage routes' bodies (LayerFS Connect's `host-api.ts`; the dsh-layerfs
 * host answers the same shapes).
 */
import type { BranchInfo, CommitInfo } from '../files/types.ts'

export interface SdkOperation { id: string, name: string, phase: string, started: number | null, uncertain: boolean }
export interface SdkRefusal { code: string, message: string, detail?: any }

export type StoreFamily = 'sqlite' | 'postgresql'
export type BackupState = 'current' | 'store_changed' | 'bucket_changed' | 'both_changed' | 'unknown' | 'unlinked' | 'missing'

export interface StoreBackupLink {
  bucket: string
  file: string
  last: { op: string, at: number | null, bytes: number | null } | null
}

export interface PgServer { target: string | null, reachable: boolean | null, version: string | null, latencyMs: number | null, error: string | null }

/** One Store: store.list with its store.status. */
export interface StoreRow {
  name: string
  kind: StoreFamily
  machine: string | null
  projects: string[]
  created: number | null
  size: { bytes: number, file: number, wal: number } | null
  state: string
  op?: SdkOperation | null
  refusal?: SdkRefusal | null
  holders: number
  down: string | null
  links: StoreBackupLink[]
  job: string | null
  postgresql?: PgServer | null
  /** A PostgreSQL Store's machines (store.attach); null from a CLI before attachments. */
  attachments?: PgAttachment[] | null
  /** Refused attaches from this host to machines it is not attached to. */
  attachRefusals?: (SdkRefusal & { machine: string, at: number })[]
}

/** A machine a PostgreSQL Store is attached to: its connection and its last check. */
export interface PgAttachment {
  machine: string
  /** The machine dials its own URL or password, not the Store's. */
  own: boolean
  /** Its own URL (no password), or null (the Store's). */
  url: string | null
  target: string | null
  /** The machine's own password reference, or null (the Store's). */
  password: string | null
  /** The reference names a file the host wrote from a typed password. */
  saved: boolean
  checked: { at: number | null, version: string | null, latencyMs: number | null } | null
  lastError: (SdkRefusal & { at: number }) | null
}

export interface BackupFile { file: string, store: string | null, state: BackupState, bytes: number | null, modified: number | null, reason: string | null }

export interface BucketRow {
  name: string
  provider: string | null
  url?: string | null
  region?: string | null
  keys?: 'saved' | 'reference' | null
  key?: string | null
  listed: number | null
  files: BackupFile[] | null
  error: { code: string, message: string } | null
  stores: string[]
}

export type JobKind = 'push' | 'pull' | 'move' | 'backup' | 'restore'

/** A push, pull, move, backup or restore the host started. */
export interface StorageJob {
  id: string
  kind: JobKind
  store: string
  target: string
  from?: string | null
  half?: 'pull' | 'push' | null
  file: string | null
  started: number
  ended: number | null
  phase: string | null
  done: number | null
  total: number | null
  error: (SdkRefusal & { phase: string | null, note?: 'pulled' }) | null
  outcome?: 'succeeded' | 'failed' | 'interrupted' | 'unknown' | null
  phases?: { phase: string, half: string | null, at: number }[]
  summary?: string | null
}

export interface MachineSupports { sqlite: boolean, postgresql_sandbox: boolean, reason: string | null }

/** GET /api/storage. */
export interface StorageView {
  machines: { name: string, platform: string, state: string | null, supports: MachineSupports | null }[]
  stores: StoreRow[]
  buckets: BucketRow[]
  jobs: StorageJob[]
  /** The pinned CLI's optional calls. */
  features?: { attach: boolean }
  at: number
}

export interface DoctorFinding { severity: 'error' | 'warning', entity: { kind: string, name: string } | null, message: string, fix: string | null }
/** GET /api/doctor. */
export interface DoctorReport { ok: boolean, findings: DoctorFinding[], at: number }

/** GET /api/projects: one project. */
export interface ProjectSummary {
  name: string
  key: string
  store: string
  kind: StoreFamily | null
  machine: string | null
  created: number | null
  storeState?: string | null
  storeDown?: string | null
  refusal?: SdkRefusal | null
}

/** POST /api/projects and GET /api/project-inits/<id>. */
export interface ProjectInitJob {
  id: string
  name: string
  source: string | null
  started: number
  ended: number | null
  step: string
  progress: { phase?: string | null, done?: number | null, total?: number | null, files?: number, bytes?: number } | null
  project: ProjectSummary | null
  error: (SdkRefusal & { transport?: string }) | null
}

export interface WorkspaceInfo { ws: string, name?: string | null, branch: string, path: string }

/** GET /api/projects/<key>/sandboxes: one Sandbox. */
export interface SandboxInfo {
  id: string
  name: string
  state: string
  op?: SdkOperation | null
  project?: string | null
  refusal?: SdkRefusal | null
  image: string | null
  machine: string | null
  runtime?: string | null
  platform?: string
  created?: number | null
  workspaces: WorkspaceInfo[]
  unread?: boolean
}

/** GET /api/projects/<key>. */
export interface ProjectState {
  project: ProjectSummary
  branches: BranchInfo[]
  layers: { id: string, message: string, at: number | null, from: string | null }[]
  sandboxes: SandboxInfo[]
  last: CommitInfo | null
  commits: number | null
  store: { path: string | null, bytes: number | null }
  storage: { partial?: boolean, logical: number, canonical: number, stored: number, metadata: number, file: number, free: number } | null
}

/** GET /api/machines (what Push and Move need of it). */
export type MachinesView =
  | {
    available: true
    machines: { name: string, platform: string, state: string, supports?: MachineSupports }[]
    sandboxes: { name: string, machine: string | null, project: string | null, state?: string }[]
    status: Record<string, { freeDisk: number | null }>
  }
  | { available: false, reason: string }

/** `sessions.list`: one conversation bound to a project. */
export interface SessionRow {
  sessionId: string
  title: string | null
  branch: string | null
  state: string | null
  checkpoints: number | null
  updatedAt: number | null
}

// ---------- a project's own storage ----------

/** GET /api/projects/<key>/storage: this project's saves summed from their Commit counters. */
export interface ProjectStorageView {
  project: string
  /** Null until the first read of the history finishes. */
  totals: {
    saves: number
    merges: number
    branches: number
    publications: number
    commitLogical: number
    publicationLogical: number
    canonical: number
    stored: number
    /** Part of `stored`, not extra bytes. */
    metadata: number
  } | null
  /** Cumulative packed bytes at even steps through the saves, oldest first. */
  growth: number[]
  /** The newest saves, oldest first. */
  recent: { id: string, time: number | null, via: string | null, logical: number, stored: number, branch: string, message: string }[]
  updatedAt: number | null
  /** The refresh running now. */
  building: { branches: number, branchesTotal: number, saves: number } | null
  /** Why the last refresh failed. */
  error: { code: string, message: string, detail: unknown } | null
  store: { name: string, projects: number | null, historyLogical?: number | null, canonical?: number | null, stored?: number | null, metadata?: number | null, file?: number | null, refusal?: unknown }
  main: { logical: number | null, files: number | null, directories: number | null, complete: boolean } | null
}

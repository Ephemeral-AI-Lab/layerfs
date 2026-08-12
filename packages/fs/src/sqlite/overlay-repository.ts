import type { CowPage, CowPageBytes } from "../cow/pages.js";
import type { StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";
import { CHARGED_ROW_BYTES, UsageRepository } from "./usage-repository.js";
import { validateDurableIdentifier } from "./identifiers.js";
import { intrinsicByteLength } from "../cas/bytes.js";
import { checkedAdd, checkedMultiply } from "../resources/safe-integers.js";

interface BranchRow extends SqliteRow {
  generation: number;
  state: number;
}
interface PageRow extends SqliteRow {
  page_index: number;
  generation: number;
  bytes: Uint8Array;
}
interface PriorPageRow extends SqliteRow {
  generation: number;
  size: number;
  pinned: number;
}
interface CountRow extends SqliteRow {
  count: number;
  bytes?: number;
  sequence?: number;
  segments?: number;
}
const PATCH_RESULT_ROW_BYTES = 172;
const PATCH_SEGMENT_RESULT_OVERHEAD_BYTES = 100;
const PATCH_SEGMENT_BINDING_OVERHEAD_BYTES = 272;
const PATCH_WRITE_FIXED_OVERHEAD_BYTES = 16 * 1024;
const PAGE_HEAD_RESULT_ROW_BYTES = 128;
interface PatchRow extends SqliteRow {
  sequence: number;
  generation: number;
  offset: number;
  delete_length: number;
  insert_length: number;
}
interface SegmentRow extends SqliteRow {
  sequence: number;
  segment_index: number;
  bytes: Uint8Array;
}

export interface PersistedPatch {
  readonly sequence: number;
  readonly generation: number;
  readonly offset: number;
  readonly deleteLength: number;
  readonly insertLength: number;
  readonly segments: readonly Uint8Array[];
}

export class OverlayRepository {
  readonly #tx: FilesystemSQLiteTransaction;
  readonly #limits: StorageLimits;
  readonly #pageBytes: CowPageBytes;
  constructor(
    tx: FilesystemSQLiteTransaction,
    limits: StorageLimits,
    pageBytes: CowPageBytes,
  ) {
    this.#tx = tx;
    this.#limits = limits;
    this.#pageBytes = pageBytes;
  }

  writePages(
    branchId: string,
    inodeId: string,
    fileSize: number,
    pages: readonly CowPage[],
    now: number,
  ): number {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    if (!pages.length) return this.#active(branchId).generation;
    this.#integer(fileSize, "fileSize");
    this.#integer(now, "now");
    const maximumPages = Math.min(
      this.#limits.maxQueryBatchSize,
      Math.floor((this.#limits.maxFinalTransactionRows - 3) / 4),
    );
    if (pages.length > maximumPages)
      throw new RangeError("COW page batch exceeds the final transaction row envelope");
    const branch = this.#active(branchId);
    const generation = branch.generation + 1;
    const seen = new Set<number>();
    let addedBytes = 0;
    let removedBytes = 0;
    let removedCount = 0;
    let addedHeads = 0;
    for (const page of pages) {
      this.#integer(page.index, "page.index");
      if (seen.has(page.index)) throw new Error("duplicate page in one overlay write");
      seen.add(page.index);
      const pageOffset = page.index * this.#pageBytes;
      if (!Number.isSafeInteger(pageOffset) || pageOffset >= fileSize)
        throw new RangeError("COW page lies outside the file");
      const expected = Math.min(this.#pageBytes, fileSize - pageOffset);
      const pageLength = intrinsicByteLength(page.bytes);
      if (pageLength !== expected)
        throw new RangeError(
          "COW page payload does not match its exact logical length",
        );
      addedBytes = checkedAdd(addedBytes, pageLength);
    }
    const transactionEnvelope = checkedAdd(
      addedBytes,
      checkedMultiply(pages.length, 4096, "COW page transaction overhead"),
      "COW page transaction envelope",
    );
    if (transactionEnvelope > this.#limits.maxFinalTransactionBytes)
      throw new RangeError(
        "COW page batch exceeds the final transaction byte envelope",
      );
    const priors: (PriorPageRow | undefined)[] = [];
    for (const page of pages) {
      const prior = this.#tx.all<PriorPageRow>(
        "SELECT h.generation,length(v.bytes) size,EXISTS(SELECT 1 FROM efs_lease_cow_pages p JOIN efs_leases l ON l.id=p.lease_id WHERE p.branch_id=h.branch_id AND p.inode_id=h.inode_id AND p.page_index=h.page_index AND p.generation=h.generation AND l.state IN (1,2)) OR EXISTS(SELECT 1 FROM efs_leases l WHERE l.kind=0 AND l.branch_id=h.branch_id AND l.generation>=h.generation AND l.state IN (1,2)) pinned FROM efs_cow_page_heads h JOIN efs_cow_page_versions v ON v.branch_id=h.branch_id AND v.inode_id=h.inode_id AND v.page_index=h.page_index AND v.generation=h.generation LEFT JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=? AND h.inode_id=? AND h.page_index=? AND h.generation>coalesce(CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER),-1)",
        [branchId, inodeId, page.index],
        { maxRows: 1, maxBytes: 256 },
      )[0];
      priors.push(prior);
      if (!prior) addedHeads += 1;
      else if (!prior.pinned) {
        removedBytes = checkedAdd(removedBytes, prior.size);
        removedCount += 1;
      }
    }
    this.#admitOverlay(
      pages.length - removedCount,
      addedBytes - removedBytes,
      0,
      0,
      pages.length - removedCount + addedHeads,
    );
    for (let index = 0; index < pages.length; index += 1) {
      const page = pages[index]!;
      const prior = priors[index];
      this.#tx.run(
        "INSERT INTO efs_cow_page_versions(branch_id,inode_id,page_index,generation,bytes,created_at_ms) VALUES(?,?,?,?,?,?)",
        [branchId, inodeId, page.index, generation, page.bytes, now],
      );
      this.#tx.run(
        "INSERT INTO efs_cow_page_heads(branch_id,inode_id,page_index,generation) VALUES(?,?,?,?) ON CONFLICT(branch_id,inode_id,page_index) DO UPDATE SET generation=excluded.generation",
        [branchId, inodeId, page.index, generation],
      );
      if (prior && !prior.pinned)
        this.#tx.run(
          "DELETE FROM efs_cow_page_versions WHERE branch_id=? AND inode_id=? AND page_index=? AND generation=?",
          [branchId, inodeId, page.index, prior.generation],
        );
    }
    const updated = this.#tx.run(
      "UPDATE efs_branches SET generation=? WHERE id=? AND state=0 AND generation=?",
      [generation, branchId, branch.generation],
    );
    if (updated.changes !== 1)
      throw new Error("ECORRUPT: branch generation changed during overlay write");
    return generation;
  }

  headPages(
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
  ): readonly CowPage[] {
    this.#integer(firstPage, "firstPage");
    this.#integer(lastPage, "lastPage");
    if (lastPage < firstPage) return [];
    const count = lastPage - firstPage + 1;
    if (count > this.#limits.maxQueryBatchSize)
      throw new RangeError("page query exceeds configured batch size");
    return this.#tx
      .all<PageRow>(
        "SELECT h.page_index,h.generation,v.bytes FROM efs_cow_page_heads h JOIN efs_cow_page_versions v ON v.branch_id=h.branch_id AND v.inode_id=h.inode_id AND v.page_index=h.page_index AND v.generation=h.generation LEFT JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=? AND h.inode_id=? AND h.page_index BETWEEN ? AND ? AND h.generation>coalesce(CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER),-1) ORDER BY h.page_index",
        [branchId, inodeId, firstPage, lastPage],
        { maxRows: count, maxBytes: count * (this.#pageBytes + 256) },
      )
      .map((row) => Object.freeze({ index: row.page_index, bytes: row.bytes }));
  }

  leasedPages(
    leaseId: string,
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
    baseGeneration = -1,
    ownerNonce?: Uint8Array,
  ): readonly CowPage[] {
    validateDurableIdentifier(leaseId, "lease identifier");
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    this.#assertReadLease(leaseId, ownerNonce);
    if (lastPage < firstPage) return [];
    const count = lastPage - firstPage + 1;
    if (count > this.#limits.maxQueryBatchSize)
      throw new RangeError("leased page query exceeds configured batch size");
    if (!Number.isSafeInteger(baseGeneration) || baseGeneration < -1)
      throw new RangeError("invalid leased page base generation");
    const selected = this.#tx.all<PageRow>(
      "SELECT p.page_index,p.generation,v.bytes FROM efs_lease_cow_pages p JOIN efs_cow_page_versions v ON v.branch_id=p.branch_id AND v.inode_id=p.inode_id AND v.page_index=p.page_index AND v.generation=p.generation WHERE p.lease_id=? AND p.branch_id=? AND p.inode_id=? AND p.page_index BETWEEN ? AND ? ORDER BY p.page_index",
      [leaseId, branchId, inodeId, firstPage, lastPage],
      { maxRows: count, maxBytes: count * (this.#pageBytes + 256) },
    );
    if (selected.length)
      return selected.map((row) =>
        Object.freeze({ index: row.page_index, bytes: row.bytes }),
      );
    if (baseGeneration < 0) return [];
    return this.#tx
      .all<PageRow>(
        "SELECT v.page_index,v.generation,v.bytes FROM efs_cow_page_versions v JOIN efs_leases l ON l.id=? AND l.kind=0 AND l.branch_id=v.branch_id AND l.state IN (1,2) WHERE v.branch_id=? AND v.inode_id=? AND v.page_index BETWEEN ? AND ? AND v.generation>? AND v.generation<=l.generation AND NOT EXISTS(SELECT 1 FROM efs_cow_page_versions later WHERE later.branch_id=v.branch_id AND later.inode_id=v.inode_id AND later.page_index=v.page_index AND later.generation>v.generation AND later.generation<=l.generation) ORDER BY v.page_index",
        [leaseId, branchId, inodeId, firstPage, lastPage, baseGeneration],
        { maxRows: count, maxBytes: count * (this.#pageBytes + 256) },
      )
      .map((row) => Object.freeze({ index: row.page_index, bytes: row.bytes }));
  }

  leaseMembershipFits(
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
    baseGeneration: number,
    includePages: boolean,
    includePatches: boolean,
  ): boolean {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    if (!Number.isSafeInteger(baseGeneration) || baseGeneration < 0)
      throw new RangeError("invalid lease membership base generation");
    if (lastPage < firstPage && includePages) return !includePatches;
    const pages = includePages
      ? (this.#tx.all<{ count: number } & SqliteRow>(
          "SELECT count(*) count FROM efs_cow_page_heads h LEFT JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=? AND h.inode_id=? AND h.page_index BETWEEN ? AND ? AND h.generation>coalesce(CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER),-1)",
          [branchId, inodeId, firstPage, lastPage],
          { maxRows: 1, maxBytes: 128 },
        )[0]?.count ?? 0)
      : 0;
    const patches = includePatches
      ? (this.#tx.all<{ count: number } & SqliteRow>(
          "SELECT count(*) count FROM efs_patches WHERE branch_id=? AND inode_id=? AND generation>?",
          [branchId, inodeId, baseGeneration],
          { maxRows: 1, maxBytes: 128 },
        )[0]?.count ?? 0)
      : 0;
    const maxRows = Math.floor(
      Math.max(0, this.#limits.maxFinalTransactionRows - 32) / 4,
    );
    const maxBytes = Math.floor(
      Math.max(0, this.#limits.maxFinalTransactionBytes - 32 * 1024) / 512,
    );
    return pages + patches <= Math.min(maxRows, maxBytes);
  }

  pinHeads(
    leaseId: string,
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
    ownerNonce: Uint8Array,
  ): number {
    validateDurableIdentifier(leaseId, "lease identifier");
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    this.#assertReadLease(leaseId, ownerNonce);
    if (lastPage < firstPage) return 0;
    const requested =
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_cow_page_heads h LEFT JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=? AND h.inode_id=? AND h.page_index BETWEEN ? AND ? AND h.generation>coalesce(CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER),-1)",
        [branchId, inodeId, firstPage, lastPage],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.count ?? 0;
    this.#assertLeaseMembershipBudget(leaseId, requested);
    let cursor = firstPage - 1;
    let pinned = 0;
    while (cursor < lastPage) {
      const batchLast = Math.min(lastPage, cursor + this.#limits.maxQueryBatchSize);
      const batchRows = batchLast - cursor;
      const rows = this.#tx.all<PageRow>(
        "SELECT h.page_index,h.generation,X'' bytes FROM efs_cow_page_heads h LEFT JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=? AND h.inode_id=? AND h.page_index>? AND h.page_index<=? AND h.generation>coalesce(CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER),-1) ORDER BY h.page_index",
        [branchId, inodeId, cursor, batchLast],
        {
          maxRows: batchRows,
          maxBytes: batchRows * PAGE_HEAD_RESULT_ROW_BYTES,
        },
      );
      if (rows.length)
        new UsageRepository(this.#tx, this.#limits).apply(
          { charged_metadata_bytes: rows.length * CHARGED_ROW_BYTES },
          "leased COW page membership",
        );
      for (const row of rows)
        this.#tx.run(
          "INSERT INTO efs_lease_cow_pages(lease_id,branch_id,inode_id,page_index,generation) VALUES(?,?,?,?,?)",
          [leaseId, branchId, inodeId, row.page_index, row.generation],
        );
      pinned += rows.length;
      cursor = batchLast;
    }
    return pinned;
  }

  pinPatches(
    leaseId: string,
    branchId: string,
    inodeId: string,
    ownerNonce: Uint8Array,
    baseGeneration = -1,
  ): number {
    validateDurableIdentifier(leaseId, "lease identifier");
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    this.#assertReadLease(leaseId, ownerNonce);
    if (!Number.isSafeInteger(baseGeneration) || baseGeneration < -1)
      throw new RangeError("invalid patch base generation");
    const requested =
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT count(*) count FROM efs_patches WHERE branch_id=? AND inode_id=? AND generation>?",
        [branchId, inodeId, baseGeneration],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.count ?? 0;
    this.#assertLeaseMembershipBudget(leaseId, requested);
    const rows = this.#tx.all<{ sequence: number } & SqliteRow>(
      "SELECT sequence FROM efs_patches WHERE branch_id=? AND inode_id=? AND generation>? ORDER BY sequence",
      [branchId, inodeId, baseGeneration],
      {
        maxRows: this.#limits.maxPatchesPerFile,
        maxBytes: this.#limits.maxPatchesPerFile * 64,
      },
    );
    if (rows.length)
      new UsageRepository(this.#tx, this.#limits).apply(
        { charged_metadata_bytes: rows.length * CHARGED_ROW_BYTES },
        "leased structural patch membership",
      );
    for (const row of rows)
      this.#tx.run(
        "INSERT INTO efs_lease_patches(lease_id,branch_id,inode_id,sequence) VALUES(?,?,?,?)",
        [leaseId, branchId, inodeId, row.sequence],
      );
    return rows.length;
  }

  leasedPatches(
    leaseId: string,
    branchId: string,
    inodeId: string,
    ownerNonce?: Uint8Array,
    baseGeneration = -1,
  ): readonly PersistedPatch[] {
    validateDurableIdentifier(leaseId, "lease identifier");
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    this.#assertReadLease(leaseId, ownerNonce);
    const selected = this.#tx.all<{ sequence: number } & SqliteRow>(
      "SELECT sequence FROM efs_lease_patches WHERE lease_id=? AND branch_id=? AND inode_id=? ORDER BY sequence",
      [leaseId, branchId, inodeId],
      {
        maxRows: this.#limits.maxPatchesPerFile,
        maxBytes: this.#limits.maxPatchesPerFile * 64,
      },
    );
    if (!selected.length) {
      if (baseGeneration < 0) return [];
      const leaseGeneration = this.#tx.all<{ generation: number } & SqliteRow>(
        "SELECT generation FROM efs_leases WHERE id=? AND kind=0 AND branch_id=? AND state IN (1,2)",
        [leaseId, branchId],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.generation;
      if (!Number.isSafeInteger(leaseGeneration)) return [];
      const pinnedGeneration = Number(leaseGeneration);
      return this.patches(branchId, inodeId, baseGeneration, 0).filter(
        (patch) => patch.generation <= pinnedGeneration,
      );
    }
    const selectedSequences = new Set(selected.map((row) => row.sequence));
    const firstSequence = Math.min(...selected.map((row) => row.sequence));
    return this.patches(branchId, inodeId, -1, firstSequence).filter((patch) =>
      selectedSequences.has(patch.sequence),
    );
  }

  hasPages(branchId: string, inodeId: string): boolean {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    return (
      this.#tx.all(
        "SELECT 1 present FROM efs_cow_page_heads h LEFT JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=? AND h.inode_id=? AND h.generation>coalesce(CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER),-1) LIMIT 1",
        [branchId, inodeId],
        { maxRows: 1, maxBytes: 512 },
      ).length !== 0
    );
  }

  /** Patches applied after the given materialization generation are live overlay content. */
  hasPatchesAfter(branchId: string, inodeId: string, baseGeneration: number): boolean {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    if (!Number.isSafeInteger(baseGeneration) || baseGeneration < 0)
      throw new RangeError("patch base generation must be a nonnegative safe integer");
    return (
      this.#tx.all(
        "SELECT 1 present FROM efs_patches WHERE branch_id=? AND inode_id=? AND generation>? LIMIT 1",
        [branchId, inodeId, baseGeneration],
        { maxRows: 1, maxBytes: 512 },
      ).length !== 0
    );
  }

  clearPages(branchId: string, inodeId: string): void {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    // Full materialization advances overlayBaseGeneration.  Stale heads are
    // reclaimed by bounded maintenance; deleting an arbitrary-sized page set
    // here would make the mutating transaction unbounded.
  }

  clearPatches(branchId: string, inodeId: string): void {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    const before = this.#patchCounts(branchId, inodeId);
    this.#tx.run(
      "DELETE FROM efs_patches WHERE branch_id=? AND inode_id=? AND NOT EXISTS(SELECT 1 FROM efs_lease_patches p WHERE p.branch_id=efs_patches.branch_id AND p.inode_id=efs_patches.inode_id AND p.sequence=efs_patches.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases l WHERE l.kind=0 AND l.branch_id=efs_patches.branch_id AND l.generation>=efs_patches.generation AND l.state IN (1,2))",
      [branchId, inodeId],
    );
    const after = this.#patchCounts(branchId, inodeId);
    this.#applyOverlayDeltas(
      0,
      0,
      after.patches - before.patches,
      after.patch_bytes - before.patch_bytes,
      after.patches - before.patches + after.segments - before.segments,
    );
  }

  cleanupUnleased(limit: number): {
    readonly worked: boolean;
    readonly reclaimedPayloadBytes: number;
  } {
    if (!Number.isSafeInteger(limit) || limit <= 0)
      throw new RangeError("invalid unleased overlay cleanup limit");
    const itemLimit = Math.max(
      1,
      Math.min(
        limit,
        Math.floor(Math.max(1, this.#limits.maxFinalTransactionRows - 24) / 4),
      ),
    );
    const pages = this.#tx.all<
      {
        branch_id: string;
        inode_id: string;
        page_index: number;
        generation: number;
        bytes: number;
      } & SqliteRow
    >(
      "SELECT v.branch_id,v.inode_id,v.page_index,v.generation,length(v.bytes) bytes FROM efs_cow_page_versions v JOIN efs_branches b ON b.id=v.branch_id WHERE (NOT EXISTS(SELECT 1 FROM efs_cow_page_heads h WHERE h.branch_id=v.branch_id AND h.inode_id=v.inode_id AND h.page_index=v.page_index AND h.generation=v.generation) OR b.state<>0 OR EXISTS(SELECT 1 FROM efs_cow_page_heads h JOIN efs_branch_inode_overlays o ON o.branch_id=h.branch_id AND o.inode_id=h.inode_id WHERE h.branch_id=v.branch_id AND h.inode_id=v.inode_id AND h.page_index=v.page_index AND h.generation=v.generation AND h.generation<=CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER))) AND NOT EXISTS(SELECT 1 FROM efs_lease_cow_pages l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=v.branch_id AND l.inode_id=v.inode_id AND l.page_index=v.page_index AND l.generation=v.generation) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=v.branch_id AND e.generation>=v.generation AND e.state IN (1,2)) ORDER BY v.branch_id,v.inode_id,v.page_index,v.generation LIMIT ?",
      [itemLimit],
      { maxRows: itemLimit, maxBytes: Math.max(4096, itemLimit * 256) },
    );
    let pageBytes = 0;
    let deletedHeads = 0;
    for (const page of pages) {
      pageBytes += page.bytes;
      const head = this.#tx.run(
        "DELETE FROM efs_cow_page_heads WHERE branch_id=? AND inode_id=? AND page_index=? AND generation=? AND (NOT EXISTS(SELECT 1 FROM efs_branches b WHERE b.id=efs_cow_page_heads.branch_id AND b.state=0) OR EXISTS(SELECT 1 FROM efs_branch_inode_overlays o WHERE o.branch_id=efs_cow_page_heads.branch_id AND o.inode_id=efs_cow_page_heads.inode_id AND efs_cow_page_heads.generation<=CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER))) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=efs_cow_page_heads.branch_id AND e.generation>=efs_cow_page_heads.generation AND e.state IN (1,2))",
        [page.branch_id, page.inode_id, page.page_index, page.generation],
      );
      deletedHeads += head.changes;
      this.#tx.run(
        "DELETE FROM efs_cow_page_versions WHERE branch_id=? AND inode_id=? AND page_index=? AND generation=? AND NOT EXISTS(SELECT 1 FROM efs_cow_page_heads h WHERE h.branch_id=efs_cow_page_versions.branch_id AND h.inode_id=efs_cow_page_versions.inode_id AND h.page_index=efs_cow_page_versions.page_index AND h.generation=efs_cow_page_versions.generation) AND NOT EXISTS(SELECT 1 FROM efs_lease_cow_pages l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=efs_cow_page_versions.branch_id AND l.inode_id=efs_cow_page_versions.inode_id AND l.page_index=efs_cow_page_versions.page_index AND l.generation=efs_cow_page_versions.generation) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=efs_cow_page_versions.branch_id AND e.generation>=efs_cow_page_versions.generation AND e.state IN (1,2))",
        [page.branch_id, page.inode_id, page.page_index, page.generation],
      );
    }
    const candidatePatches = pages.length
      ? []
      : this.#tx.all<
          {
            branch_id: string;
            inode_id: string;
            sequence: number;
            insert_length: number;
            segments: number;
            segment_bytes: number;
          } & SqliteRow
        >(
          "SELECT p.branch_id,p.inode_id,p.sequence,p.insert_length,(SELECT count(*) FROM efs_patch_segments s WHERE s.branch_id=p.branch_id AND s.inode_id=p.inode_id AND s.sequence=p.sequence) segments,(SELECT coalesce(sum(length(s.bytes)),0) FROM efs_patch_segments s WHERE s.branch_id=p.branch_id AND s.inode_id=p.inode_id AND s.sequence=p.sequence) segment_bytes FROM efs_patches p JOIN efs_branches b ON b.id=p.branch_id WHERE (b.state<>0 OR EXISTS(SELECT 1 FROM efs_branch_inode_overlays o WHERE o.branch_id=p.branch_id AND o.inode_id=p.inode_id AND CAST(json_extract(CAST(o.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER)>=p.generation)) AND NOT EXISTS(SELECT 1 FROM efs_lease_patches l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=p.branch_id AND l.inode_id=p.inode_id AND l.sequence=p.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=p.branch_id AND e.generation>=p.generation AND e.state IN (1,2)) AND NOT EXISTS(SELECT 1 FROM efs_patches later WHERE later.branch_id=p.branch_id AND later.inode_id=p.inode_id AND later.sequence>p.sequence AND NOT ((b.state<>0 OR EXISTS(SELECT 1 FROM efs_branch_inode_overlays o2 WHERE o2.branch_id=later.branch_id AND o2.inode_id=later.inode_id AND CAST(json_extract(CAST(o2.encoded AS TEXT),'$.overlayBaseGeneration') AS INTEGER)>=later.generation)) AND NOT EXISTS(SELECT 1 FROM efs_lease_patches l2 JOIN efs_leases e2 ON e2.id=l2.lease_id AND e2.state IN (1,2) WHERE l2.branch_id=later.branch_id AND l2.inode_id=later.inode_id AND l2.sequence=later.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases e3 WHERE e3.kind=0 AND e3.branch_id=later.branch_id AND e3.generation>=later.generation AND e3.state IN (1,2)))) ORDER BY p.branch_id,p.inode_id,p.sequence DESC LIMIT ?",
          [itemLimit],
          { maxRows: itemLimit, maxBytes: Math.max(4096, itemLimit * 384) },
        );
    const patchBudget = Math.max(1, this.#limits.maxFinalTransactionRows - 2);
    const patches: Array<(typeof candidatePatches)[number]> = [];
    let patchWork = 0;
    for (const patch of candidatePatches) {
      const work = 1 + patch.segments;
      if (work > patchBudget && patches.length === 0) break;
      if (patchWork + work > patchBudget) break;
      patches.push(patch);
      patchWork += work;
    }
    if (!pages.length && !patches.length && candidatePatches.length) {
      const patch = candidatePatches[0]!;
      const segmentBudget = Math.max(1, this.#limits.maxFinalTransactionRows - 4);
      const segments = this.#tx.all<SegmentRow>(
        "SELECT sequence,segment_index,bytes FROM efs_patch_segments WHERE branch_id=? AND inode_id=? AND sequence=? AND NOT EXISTS(SELECT 1 FROM efs_lease_patches l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=efs_patch_segments.branch_id AND l.inode_id=efs_patch_segments.inode_id AND l.sequence=efs_patch_segments.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=efs_patch_segments.branch_id AND e.generation>=(SELECT generation FROM efs_patches p WHERE p.branch_id=efs_patch_segments.branch_id AND p.inode_id=efs_patch_segments.inode_id AND p.sequence=efs_patch_segments.sequence) AND e.state IN (1,2)) ORDER BY segment_index LIMIT ?",
        [patch.branch_id, patch.inode_id, patch.sequence, segmentBudget],
        {
          maxRows: segmentBudget,
          maxBytes: this.#limits.maxFinalTransactionBytes,
        },
      );
      if (segments.length) {
        let bytes = 0;
        for (const segment of segments) {
          bytes += segment.bytes.byteLength;
          this.#tx.run(
            "DELETE FROM efs_patch_segments WHERE branch_id=? AND inode_id=? AND sequence=? AND segment_index=? AND NOT EXISTS(SELECT 1 FROM efs_lease_patches l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=efs_patch_segments.branch_id AND l.inode_id=efs_patch_segments.inode_id AND l.sequence=efs_patch_segments.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=efs_patch_segments.branch_id AND e.generation>=(SELECT generation FROM efs_patches p WHERE p.branch_id=efs_patch_segments.branch_id AND p.inode_id=efs_patch_segments.inode_id AND p.sequence=efs_patch_segments.sequence) AND e.state IN (1,2))",
            [patch.branch_id, patch.inode_id, patch.sequence, segment.segment_index],
          );
        }
        new UsageRepository(this.#tx, this.#limits).apply(
          {
            patch_bytes: -bytes,
            charged_metadata_bytes: -segments.length * CHARGED_ROW_BYTES,
          },
          "unleased branch patch segment cleanup",
        );
        return Object.freeze({ worked: true, reclaimedPayloadBytes: bytes });
      }
    }
    let patchBytes = 0;
    let patchRows = 0;
    let segmentRows = 0;
    for (const patch of patches) {
      patchBytes += patch.segment_bytes;
      patchRows += 1;
      segmentRows += patch.segments;
      this.#tx.run(
        "DELETE FROM efs_patches WHERE branch_id=? AND inode_id=? AND sequence=? AND NOT EXISTS(SELECT 1 FROM efs_lease_patches l JOIN efs_leases e ON e.id=l.lease_id AND e.state IN (1,2) WHERE l.branch_id=efs_patches.branch_id AND l.inode_id=efs_patches.inode_id AND l.sequence=efs_patches.sequence) AND NOT EXISTS(SELECT 1 FROM efs_leases e WHERE e.kind=0 AND e.branch_id=efs_patches.branch_id AND e.generation>=efs_patches.generation AND e.state IN (1,2))",
        [patch.branch_id, patch.inode_id, patch.sequence],
      );
    }
    if (pages.length || patches.length)
      new UsageRepository(this.#tx, this.#limits).apply(
        {
          page_count: -pages.length,
          page_bytes: -pageBytes,
          patch_count: -patchRows,
          patch_bytes: -patchBytes,
          charged_metadata_bytes:
            -(pages.length + deletedHeads + patchRows + segmentRows) *
            CHARGED_ROW_BYTES,
        },
        "unleased branch overlay cleanup",
      );
    return Object.freeze({
      worked: pages.length + patches.length > 0,
      reclaimedPayloadBytes: pageBytes + patchBytes,
    });
  }

  #assertLeaseMembershipBudget(leaseId: string, additional: number): void {
    if (!Number.isSafeInteger(additional) || additional < 0)
      throw new RangeError("invalid lease membership count");
    const existing =
      this.#tx.all<{ count: number } & SqliteRow>(
        "SELECT (SELECT count(*) FROM efs_lease_cow_pages WHERE lease_id=?)+(SELECT count(*) FROM efs_lease_patches WHERE lease_id=?) count",
        [leaseId, leaseId],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.count ?? 0;
    const maxRows = Math.floor(
      Math.max(0, this.#limits.maxFinalTransactionRows - 32) / 4,
    );
    const maxBytes = Math.floor(
      Math.max(0, this.#limits.maxFinalTransactionBytes - 32 * 1024) / 512,
    );
    if (existing + additional > Math.min(maxRows, maxBytes))
      throw new RangeError(
        "branch stream lease exceeds the final transaction envelope",
      );
  }

  #assertReadLease(leaseId: string, ownerNonce?: Uint8Array): void {
    validateDurableIdentifier(leaseId, "lease identifier");
    if (!ownerNonce || intrinsicByteLength(ownerNonce) !== 16)
      throw new RangeError("read lease owner nonce must contain exactly 16 bytes");
    if (
      this.#tx.all(
        "SELECT 1 FROM efs_leases WHERE id=? AND kind=0 AND owner_nonce=? AND state IN (1,2)",
        [leaseId, ownerNonce],
        { maxRows: 1, maxBytes: 128 },
      ).length !== 1
    )
      throw new Error("read lease owner nonce mismatch");
  }

  #patchCounts(
    branchId: string,
    inodeId: string,
  ): { patches: number; patch_bytes: number; segments: number } {
    const row = this.#tx.all<
      {
        patches: number;
        patch_bytes: number;
        segments: number;
      } & SqliteRow
    >(
      "SELECT (SELECT count(*) FROM efs_patches WHERE branch_id=? AND inode_id=?) patches,(SELECT coalesce(sum(length(bytes)),0) FROM efs_patch_segments WHERE branch_id=? AND inode_id=?) patch_bytes,(SELECT count(*) FROM efs_patch_segments WHERE branch_id=? AND inode_id=?) segments",
      [branchId, inodeId, branchId, inodeId, branchId, inodeId],
      { maxRows: 1, maxBytes: 256 },
    )[0]!;
    for (const [name, value] of Object.entries(row))
      if (!Number.isSafeInteger(value))
        throw new Error(`ECORRUPT: invalid branch patch count ${name}`);
    return row;
  }

  #applyOverlayDeltas(
    pageCount: number,
    pageBytes: number,
    patchCount: number,
    patchBytes: number,
    metadataRows: number,
  ): void {
    if (
      pageCount === 0 &&
      pageBytes === 0 &&
      patchCount === 0 &&
      patchBytes === 0 &&
      metadataRows === 0
    )
      return;
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        page_count: pageCount,
        page_bytes: pageBytes,
        patch_count: patchCount,
        patch_bytes: patchBytes,
        charged_metadata_bytes: metadataRows * CHARGED_ROW_BYTES,
      },
      "branch overlay cleanup",
    );
  }

  appendPatch(
    branchId: string,
    inodeId: string,
    currentSize: number,
    offset: number,
    deleteLength: number,
    segments: readonly Uint8Array[],
  ): number {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    for (const [name, value] of [
      ["currentSize", currentSize],
      ["offset", offset],
      ["deleteLength", deleteLength],
    ] as const)
      this.#integer(value, name);
    if (offset > currentSize || deleteLength > currentSize - offset)
      throw new RangeError("structural patch is outside the current file");
    const maxSegments = Math.min(
      checkedMultiply(
        this.#limits.maxPatchesPerFile,
        32,
        "structural patch segment envelope",
      ),
      this.#limits.maxFinalTransactionRows - 4,
    );
    if (segments.length > maxSegments)
      throw new RangeError("structural patch segment limit requires materialization");
    let insertLength = 0;
    for (const segment of segments) {
      const segmentLength = intrinsicByteLength(segment);
      if (!segmentLength || segmentLength > 524_288)
        throw new RangeError("patch segment size is invalid");
      insertLength = checkedAdd(insertLength, segmentLength, "patch insertion length");
    }
    const aggregate = this.#tx.all<CountRow>(
      "SELECT count(*) count,coalesce(sum(insert_length),0) bytes,coalesce(max(sequence),-1) sequence,(SELECT count(*) FROM efs_patch_segments s WHERE s.branch_id=? AND s.inode_id=?) segments FROM efs_patches WHERE branch_id=? AND inode_id=?",
      [branchId, inodeId, branchId, inodeId],
      { maxRows: 1, maxBytes: 256 },
    )[0]!;
    const projectedPatchCount = checkedAdd(aggregate.count, 1);
    const projectedSegmentCount = checkedAdd(aggregate.segments ?? 0, segments.length);
    const projectedPayloadBytes = checkedAdd(aggregate.bytes ?? 0, insertLength);
    const projectedReadBytes = checkedAdd(
      projectedPayloadBytes,
      checkedAdd(
        checkedMultiply(
          projectedPatchCount,
          PATCH_RESULT_ROW_BYTES,
          "structural patch result headers",
        ),
        checkedMultiply(
          projectedSegmentCount,
          PATCH_SEGMENT_RESULT_OVERHEAD_BYTES,
          "structural patch result segments",
        ),
        "structural patch result overhead",
      ),
      "structural patch result envelope",
    );
    const projectedWriteBytes = checkedAdd(
      insertLength,
      checkedAdd(
        checkedMultiply(
          segments.length,
          PATCH_SEGMENT_BINDING_OVERHEAD_BYTES,
          "structural patch binding rows",
        ),
        PATCH_WRITE_FIXED_OVERHEAD_BYTES,
        "structural patch write overhead",
      ),
      "structural patch write envelope",
    );
    if (
      aggregate.count >= this.#limits.maxPatchesPerFile ||
      (aggregate.bytes ?? 0) + insertLength > this.#limits.maxPatchBytesPerFile ||
      (aggregate.segments ?? 0) + segments.length > maxSegments ||
      aggregate.count + 1 + (aggregate.segments ?? 0) + segments.length >
        this.#limits.maxFinalTransactionRows ||
      projectedReadBytes > this.#limits.maxFinalTransactionBytes ||
      projectedWriteBytes > this.#limits.maxFinalTransactionBytes
    )
      throw new RangeError("structural patch limit requires materialization");
    const branch = this.#active(branchId);
    const generation = branch.generation + 1;
    const sequence = (aggregate.sequence ?? -1) + 1;
    this.#admitOverlay(0, 0, 1, insertLength, 1 + segments.length);
    this.#tx.run(
      "INSERT INTO efs_patches(branch_id,inode_id,sequence,generation,offset,delete_length,insert_length) VALUES(?,?,?,?,?,?,?)",
      [branchId, inodeId, sequence, generation, offset, deleteLength, insertLength],
    );
    for (let index = 0; index < segments.length; index += 1)
      this.#tx.run(
        "INSERT INTO efs_patch_segments(branch_id,inode_id,sequence,segment_index,bytes) VALUES(?,?,?,?,?)",
        [branchId, inodeId, sequence, index, segments[index]!],
      );
    this.#tx.run(
      "UPDATE efs_branches SET generation=? WHERE id=? AND state=0 AND generation=?",
      [generation, branchId, branch.generation],
    );
    return generation;
  }

  patches(
    branchId: string,
    inodeId: string,
    minimumGeneration = -1,
    minimumSequence = 0,
  ): readonly PersistedPatch[] {
    validateDurableIdentifier(branchId, "branch identifier");
    validateDurableIdentifier(inodeId, "inode identifier");
    if (!Number.isSafeInteger(minimumGeneration) || minimumGeneration < -1)
      throw new RangeError("invalid patch generation lower bound");
    if (!Number.isSafeInteger(minimumSequence) || minimumSequence < 0)
      throw new RangeError("invalid patch sequence lower bound");
    const patches = this.#tx.all<PatchRow>(
      "SELECT sequence,generation,offset,delete_length,insert_length FROM efs_patches WHERE branch_id=? AND inode_id=? AND generation>? AND sequence>=? ORDER BY sequence",
      [branchId, inodeId, minimumGeneration, minimumSequence],
      {
        maxRows: this.#limits.maxPatchesPerFile,
        maxBytes: Math.min(
          this.#limits.maxFinalTransactionBytes,
          this.#limits.maxPatchesPerFile * PATCH_RESULT_ROW_BYTES,
        ),
      },
    );
    const segments = this.#tx.all<SegmentRow>(
      "SELECT s.sequence,s.segment_index,s.bytes FROM efs_patch_segments s JOIN efs_patches p ON p.branch_id=s.branch_id AND p.inode_id=s.inode_id AND p.sequence=s.sequence WHERE s.branch_id=? AND s.inode_id=? AND p.generation>? AND s.sequence>=? ORDER BY s.sequence,s.segment_index",
      [branchId, inodeId, minimumGeneration, minimumSequence],
      {
        maxRows: this.#limits.maxPatchesPerFile * 32,
        maxBytes: this.#limits.maxFinalTransactionBytes,
      },
    );
    let cursor = 0;
    const firstSequence = patches[0]?.sequence ?? minimumSequence;
    if (
      patches.length > 0 &&
      minimumGeneration === -1 &&
      patches[0]!.sequence !== minimumSequence
    )
      throw new Error("ECORRUPT: structural patch sequence has a gap");
    const result = patches.map((patch, patchIndex) => {
      if (patch.sequence !== firstSequence + patchIndex)
        throw new Error("ECORRUPT: structural patch sequence has a gap");
      const values: Uint8Array[] = [];
      let length = 0;
      while (
        cursor < segments.length &&
        segments[cursor]!.sequence === patch.sequence
      ) {
        const segment = segments[cursor]!;
        if (segment.segment_index !== values.length)
          throw new Error("ECORRUPT: patch segment sequence has a gap");
        values.push(segment.bytes);
        length = checkedAdd(
          length,
          intrinsicByteLength(segment.bytes),
          "persisted patch length",
        );
        cursor += 1;
      }
      if (length !== patch.insert_length)
        throw new Error("ECORRUPT: patch insertion length mismatch");
      return Object.freeze({
        sequence: patch.sequence,
        generation: patch.generation,
        offset: patch.offset,
        deleteLength: patch.delete_length,
        insertLength: patch.insert_length,
        segments: Object.freeze(values),
      });
    });
    if (cursor !== segments.length)
      throw new Error("ECORRUPT: structural patch segment has no patch header");
    return result;
  }

  #active(branchId: string): BranchRow {
    const row = this.#tx.all<BranchRow>(
      "SELECT generation,state FROM efs_branches WHERE id=?",
      [branchId],
      { maxRows: 1, maxBytes: 128 },
    )[0];
    if (!row || row.state !== 0) throw new Error("branch is absent or not active");
    return row;
  }
  #admitOverlay(
    pageCount: number,
    pageBytes: number,
    patchCount: number,
    patchBytes: number,
    metadataRows: number,
  ): void {
    new UsageRepository(this.#tx, this.#limits).apply(
      {
        page_count: pageCount,
        page_bytes: pageBytes,
        patch_count: patchCount,
        patch_bytes: patchBytes,
        charged_metadata_bytes: metadataRows * CHARGED_ROW_BYTES,
      },
      "branch overlay",
    );
  }
  #integer(value: number, name: string): void {
    if (!Number.isSafeInteger(value) || value < 0)
      throw new RangeError(`${name} must be a nonnegative safe integer`);
  }
}

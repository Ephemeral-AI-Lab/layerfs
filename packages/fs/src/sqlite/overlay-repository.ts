import type { CowPage, CowPageBytes } from "../cow/pages.js";
import type { StorageLimits } from "../resources/limits.js";
import type { FilesystemSQLiteTransaction, SqliteRow } from "./driver.js";

interface BranchRow extends SqliteRow {
  generation: number;
  state: number;
}
interface PageRow extends SqliteRow {
  page_index: number;
  generation: number;
  bytes: Uint8Array;
}
interface UsageRow extends SqliteRow {
  page_count: number;
  page_bytes: number;
  patch_count: number;
  patch_bytes: number;
  charged_metadata_bytes: number;
}
interface CountRow extends SqliteRow {
  count: number;
  bytes?: number;
  sequence?: number;
}
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
    if (!pages.length) return this.#active(branchId).generation;
    this.#integer(fileSize, "fileSize");
    this.#integer(now, "now");
    const branch = this.#active(branchId);
    const generation = branch.generation + 1;
    const seen = new Set<number>();
    let addedBytes = 0;
    let removedBytes = 0;
    let removedCount = 0;
    for (const page of pages) {
      this.#integer(page.index, "page.index");
      if (seen.has(page.index)) throw new Error("duplicate page in one overlay write");
      seen.add(page.index);
      const pageOffset = page.index * this.#pageBytes;
      if (!Number.isSafeInteger(pageOffset) || pageOffset >= fileSize)
        throw new RangeError("COW page lies outside the file");
      const expected = Math.min(this.#pageBytes, fileSize - pageOffset);
      if (page.bytes.byteLength !== expected)
        throw new RangeError(
          "COW page payload does not match its exact logical length",
        );
      const prior = this.#tx.all<PageRow>(
        "SELECT h.page_index,h.generation,v.bytes FROM efs_cow_page_heads h JOIN efs_cow_page_versions v ON v.branch_id=h.branch_id AND v.inode_id=h.inode_id AND v.page_index=h.page_index AND v.generation=h.generation WHERE h.branch_id=? AND h.inode_id=? AND h.page_index=?",
        [branchId, inodeId, page.index],
        { maxRows: 1, maxBytes: this.#pageBytes + 256 },
      )[0];
      this.#tx.run(
        "INSERT INTO efs_cow_page_versions(branch_id,inode_id,page_index,generation,bytes,created_at_ms) VALUES(?,?,?,?,?,?)",
        [branchId, inodeId, page.index, generation, page.bytes, now],
      );
      this.#tx.run(
        "INSERT INTO efs_cow_page_heads(branch_id,inode_id,page_index,generation) VALUES(?,?,?,?) ON CONFLICT(branch_id,inode_id,page_index) DO UPDATE SET generation=excluded.generation",
        [branchId, inodeId, page.index, generation],
      );
      addedBytes += page.bytes.byteLength;
      if (prior) {
        const pinned =
          this.#tx.all(
            "SELECT 1 present FROM efs_lease_cow_pages p JOIN efs_leases l ON l.id=p.lease_id WHERE p.branch_id=? AND p.inode_id=? AND p.page_index=? AND p.generation=? AND l.state=1 LIMIT 1",
            [branchId, inodeId, page.index, prior.generation],
            { maxRows: 1, maxBytes: 128 },
          ).length > 0;
        if (!pinned) {
          this.#tx.run(
            "DELETE FROM efs_cow_page_versions WHERE branch_id=? AND inode_id=? AND page_index=? AND generation=?",
            [branchId, inodeId, page.index, prior.generation],
          );
          removedBytes += prior.bytes.byteLength;
          removedCount += 1;
        }
      }
    }
    this.#admitOverlay(pages.length - removedCount, addedBytes - removedBytes, 0, 0);
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
        "SELECT h.page_index,h.generation,v.bytes FROM efs_cow_page_heads h JOIN efs_cow_page_versions v ON v.branch_id=h.branch_id AND v.inode_id=h.inode_id AND v.page_index=h.page_index AND v.generation=h.generation WHERE h.branch_id=? AND h.inode_id=? AND h.page_index BETWEEN ? AND ? ORDER BY h.page_index",
        [branchId, inodeId, firstPage, lastPage],
        { maxRows: count, maxBytes: count * (this.#pageBytes + 256) },
      )
      .map((row) => Object.freeze({ index: row.page_index, bytes: row.bytes }));
  }

  pinHeads(
    leaseId: string,
    branchId: string,
    inodeId: string,
    firstPage: number,
    lastPage: number,
  ): number {
    const rows = this.#tx.all<PageRow>(
      "SELECT page_index,generation,X'' bytes FROM efs_cow_page_heads WHERE branch_id=? AND inode_id=? AND page_index BETWEEN ? AND ? ORDER BY page_index",
      [branchId, inodeId, firstPage, lastPage],
      {
        maxRows: this.#limits.maxQueryBatchSize,
        maxBytes: this.#limits.maxQueryBatchSize * 96,
      },
    );
    for (const row of rows)
      this.#tx.run(
        "INSERT INTO efs_lease_cow_pages(lease_id,branch_id,inode_id,page_index,generation) VALUES(?,?,?,?,?)",
        [leaseId, branchId, inodeId, row.page_index, row.generation],
      );
    return rows.length;
  }

  appendPatch(
    branchId: string,
    inodeId: string,
    currentSize: number,
    offset: number,
    deleteLength: number,
    segments: readonly Uint8Array[],
  ): number {
    for (const [name, value] of [
      ["currentSize", currentSize],
      ["offset", offset],
      ["deleteLength", deleteLength],
    ] as const)
      this.#integer(value, name);
    if (offset > currentSize || deleteLength > currentSize - offset)
      throw new RangeError("structural patch is outside the current file");
    let insertLength = 0;
    for (const segment of segments) {
      if (!segment.byteLength || segment.byteLength > 524_288)
        throw new RangeError("patch segment size is invalid");
      insertLength += segment.byteLength;
      if (!Number.isSafeInteger(insertLength))
        throw new RangeError("patch insertion length overflow");
    }
    const aggregate = this.#tx.all<CountRow>(
      "SELECT count(*) count,coalesce(sum(insert_length),0) bytes,coalesce(max(sequence),-1) sequence FROM efs_patches WHERE branch_id=? AND inode_id=?",
      [branchId, inodeId],
      { maxRows: 1, maxBytes: 256 },
    )[0]!;
    if (
      aggregate.count >= this.#limits.maxPatchesPerFile ||
      (aggregate.bytes ?? 0) + insertLength > this.#limits.maxPatchBytesPerFile
    )
      throw new RangeError("structural patch limit requires materialization");
    const branch = this.#active(branchId);
    const generation = branch.generation + 1;
    const sequence = (aggregate.sequence ?? -1) + 1;
    this.#admitOverlay(0, 0, 1, insertLength);
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

  patches(branchId: string, inodeId: string): readonly PersistedPatch[] {
    const patches = this.#tx.all<PatchRow>(
      "SELECT sequence,generation,offset,delete_length,insert_length FROM efs_patches WHERE branch_id=? AND inode_id=? ORDER BY sequence",
      [branchId, inodeId],
      {
        maxRows: this.#limits.maxPatchesPerFile,
        maxBytes: this.#limits.maxQueryBatchSize * 128,
      },
    );
    const segments = this.#tx.all<SegmentRow>(
      "SELECT sequence,segment_index,bytes FROM efs_patch_segments WHERE branch_id=? AND inode_id=? ORDER BY sequence,segment_index",
      [branchId, inodeId],
      {
        maxRows: this.#limits.maxPatchesPerFile * 32,
        maxBytes: this.#limits.maxPatchBytesPerFile + 1024,
      },
    );
    let cursor = 0;
    return patches.map((patch) => {
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
        length += segment.bytes.byteLength;
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
  ): void {
    const usage = this.#tx.all<UsageRow>(
      "SELECT page_count,page_bytes,patch_count,patch_bytes,charged_metadata_bytes FROM efs_usage WHERE singleton=1",
      [],
      { maxRows: 1, maxBytes: 256 },
    )[0];
    if (!usage) throw new Error("ECORRUPT: missing usage singleton");
    const metadata = (pageCount + patchCount) * 96;
    if (
      usage.page_bytes + usage.patch_bytes + pageBytes + patchBytes >
        this.#limits.maxBranchOverlayBytes ||
      usage.charged_metadata_bytes + metadata > this.#limits.maxChargedMetadataBytes
    )
      throw new Error("ENOSPC: branch overlay quota exceeded");
    this.#tx.run(
      "UPDATE efs_usage SET page_count=page_count+?,page_bytes=page_bytes+?,patch_count=patch_count+?,patch_bytes=patch_bytes+?,charged_metadata_bytes=charged_metadata_bytes+? WHERE singleton=1",
      [pageCount, pageBytes, patchCount, patchBytes, metadata],
    );
  }
  #integer(value: number, name: string): void {
    if (!Number.isSafeInteger(value) || value < 0)
      throw new RangeError(`${name} must be a nonnegative safe integer`);
  }
}

import { sha256 } from "../cas/sha256.js";
import { copyBytes, intrinsicByteLength, intrinsicByteRange } from "../cas/bytes.js";
import {
  StreamingFastCdc,
  type FastCdcConfiguration,
  validateSupportedFastCdcConfiguration,
} from "../cdc/fastcdc.js";
import {
  buildManifestFromEntries,
  type BuiltManifestRoot,
  type ManifestBuildWorkspace,
} from "../manifests/builder.js";
import type { ManifestEntry } from "../manifests/codec.js";
import {
  checkedAdd,
  checkedInteger,
  checkedMultiply,
} from "../resources/safe-integers.js";
import { MAX_CONTENT_OBJECT_BYTES } from "../resources/limits.js";
import type { DiagnosticBuiltManifest } from "./full-rebuild.js";
import {
  DEFAULT_LOCAL_REBUILD_LIMITS,
  LocalRebuildLimitError,
  ownLocalContentInputs,
  rebuildManifestLocallyWithParametersOwned,
  snapshotLocalRebuildLimits,
  snapshotMatchingLocalParameters,
  validateLocalContentInputs,
  type LocalRebuildAttemptMetrics,
  type LocalContentEdit,
  type LocalRebuildLimits,
  type LocallyRebuiltManifest,
  type OwnedLocalContentInputs,
  type RandomAccessContentSource,
} from "./local-rebuild.js";

export interface StreamedObjectSink {
  putObject(hash: Uint8Array, bytes: Uint8Array): void;
}
export interface StreamedRebuildOptions {
  readonly readWindowBytes?: number;
  readonly manifestReadBatchRecords?: number;
  readonly maxManifestDepth?: number;
}
export interface StreamedRebuildMetrics {
  readonly sourceBytesRead: number;
  readonly bytesHashed: number;
  readonly attemptedLocalSourceBytesRead: number;
  readonly attemptedLocalBytesHashed: number;
  readonly attemptedLocalLargestSourceRead: number;
  readonly attemptedLocalChunkerInputBytesCopied: number;
  readonly attemptedLocalChunkerOutputBytesCopied: number;
  readonly attemptedLocalChunkerBoundaryBytesScanned: number;
  readonly attemptedLocalEditedInputBytesPrepared: number;
  readonly fallbackSourceBytesRead: number;
  readonly fallbackBytesHashed: number;
  readonly fallbackLargestSourceRead: number;
  readonly fallbackChunkerInputBytesCopied: number;
  readonly fallbackChunkerOutputBytesCopied: number;
  readonly fallbackChunkerBoundaryBytesScanned: number;
  readonly objectCount: number;
  readonly largestSourceRead: number;
  readonly peakRetainedRecords: number;
  readonly peakPendingEntries: number;
  readonly insertionCopyCount: 1;
  readonly insertionBytesCopied: number;
  readonly chunkerInputBytesCopied: number;
  readonly chunkerOutputBytesCopied: number;
  readonly chunkerBoundaryBytesScanned: number;
}
export interface StreamedRebuildResult {
  readonly mode: "streamed-fallback";
  readonly manifest: BuiltManifestRoot;
  readonly metrics: StreamedRebuildMetrics;
  readonly localLimitReason: string;
}
export interface LocalRebuildResult {
  readonly mode: "local";
  readonly manifest: LocallyRebuiltManifest;
}

export const MAX_STREAMED_REBUILD_PENDING_ENTRIES = 256;

interface StreamedRebuildControls {
  readonly parameters: Readonly<FastCdcConfiguration>;
  readonly options: Readonly<Required<StreamedRebuildOptions>>;
  readonly attemptedLocal: Readonly<LocalRebuildAttemptMetrics>;
}

function snapshotStreamedRebuildControls(
  parameters: FastCdcConfiguration,
  options: StreamedRebuildOptions,
  attemptedLocal: LocalRebuildAttemptMetrics,
): StreamedRebuildControls {
  const ownedParameters = Object.freeze({
    minimum: parameters.minimum,
    average: parameters.average,
    maximum: parameters.maximum,
  });
  validateSupportedFastCdcConfiguration(ownedParameters);
  const readWindowBytes = checkedInteger(
    options.readWindowBytes ?? ownedParameters.maximum,
    "readWindowBytes",
    MAX_CONTENT_OBJECT_BYTES,
  );
  const manifestReadBatchRecords = checkedInteger(
    options.manifestReadBatchRecords ?? 64,
    "manifestReadBatchRecords",
    4096,
  );
  const maxManifestDepth = checkedInteger(
    options.maxManifestDepth ?? 8,
    "maxManifestDepth",
    64,
  );
  if (readWindowBytes === 0 || manifestReadBatchRecords === 0 || maxManifestDepth === 0)
    throw new RangeError("streamed rebuild controls must be positive");
  const ownedAttempt = Object.freeze({
    sourceBytesRead: checkedInteger(
      attemptedLocal.sourceBytesRead,
      "attemptedLocal.sourceBytesRead",
    ),
    bytesHashed: checkedInteger(
      attemptedLocal.bytesHashed,
      "attemptedLocal.bytesHashed",
    ),
    largestSourceRead: checkedInteger(
      attemptedLocal.largestSourceRead,
      "attemptedLocal.largestSourceRead",
    ),
    chunkerInputBytesCopied: checkedInteger(
      attemptedLocal.chunkerInputBytesCopied ?? 0,
      "attemptedLocal.chunkerInputBytesCopied",
    ),
    chunkerOutputBytesCopied: checkedInteger(
      attemptedLocal.chunkerOutputBytesCopied ?? 0,
      "attemptedLocal.chunkerOutputBytesCopied",
    ),
    chunkerBoundaryBytesScanned: checkedInteger(
      attemptedLocal.chunkerBoundaryBytesScanned ?? 0,
      "attemptedLocal.chunkerBoundaryBytesScanned",
    ),
    editedInputBytesPrepared: checkedInteger(
      attemptedLocal.editedInputBytesPrepared ?? 0,
      "attemptedLocal.editedInputBytesPrepared",
    ),
  });
  if (ownedAttempt.largestSourceRead > ownedAttempt.sourceBytesRead)
    throw new RangeError(
      "attemptedLocal.largestSourceRead exceeds attemptedLocal.sourceBytesRead",
    );
  if (
    ownedAttempt.chunkerOutputBytesCopied > ownedAttempt.chunkerInputBytesCopied ||
    ownedAttempt.chunkerBoundaryBytesScanned > ownedAttempt.chunkerInputBytesCopied
  )
    throw new RangeError(
      "attempted-local chunker copy/scan metrics exceed chunker input",
    );
  if (
    ownedAttempt.bytesHashed > ownedAttempt.chunkerOutputBytesCopied ||
    ownedAttempt.chunkerInputBytesCopied > ownedAttempt.editedInputBytesPrepared ||
    ownedAttempt.sourceBytesRead > ownedAttempt.editedInputBytesPrepared ||
    ownedAttempt.largestSourceRead > ownedParameters.maximum
  )
    throw new RangeError("attempted-local phase metrics are internally inconsistent");
  if (
    ownedAttempt.editedInputBytesPrepared > ownedAttempt.chunkerInputBytesCopied &&
    ownedAttempt.editedInputBytesPrepared - ownedAttempt.chunkerInputBytesCopied >
      ownedParameters.maximum
  )
    throw new RangeError(
      "attempted-local prepared input exceeds processed input plus one window",
    );
  return Object.freeze({
    parameters: ownedParameters,
    options: Object.freeze({
      readWindowBytes,
      manifestReadBatchRecords,
      maxManifestDepth,
    }),
    attemptedLocal: ownedAttempt,
  });
}

export function rebuildEditedContentStreaming(
  source: RandomAccessContentSource,
  edit: LocalContentEdit,
  parameters: FastCdcConfiguration,
  workspace: ManifestBuildWorkspace,
  objects: StreamedObjectSink,
  reason = "explicit streamed rebuild",
  options: StreamedRebuildOptions = {},
  attemptedLocal: LocalRebuildAttemptMetrics = Object.freeze({
    sourceBytesRead: 0,
    bytesHashed: 0,
    largestSourceRead: 0,
    chunkerInputBytesCopied: 0,
    chunkerOutputBytesCopied: 0,
    chunkerBoundaryBytesScanned: 0,
    editedInputBytesPrepared: 0,
  }),
): StreamedRebuildResult {
  const controls = snapshotStreamedRebuildControls(parameters, options, attemptedLocal);
  const owned = ownLocalContentInputs(validateLocalContentInputs(source, edit));
  return rebuildEditedContentStreamingOwned(
    owned,
    controls,
    workspace,
    objects,
    reason,
  );
}

function rebuildEditedContentStreamingOwned(
  owned: OwnedLocalContentInputs,
  controls: StreamedRebuildControls,
  workspace: ManifestBuildWorkspace,
  objects: StreamedObjectSink,
  reason: string,
): StreamedRebuildResult {
  const { source, edit } = owned;
  const { parameters, options, attemptedLocal } = controls;
  const { readWindowBytes } = options;
  let sourceBytesRead = 0;
  let bytesHashed = 0;
  let objectCount = 0;
  let largestSourceRead = 0;
  let peakPendingEntries = 0;
  let activeChunker: StreamingFastCdc | undefined;
  const read = (offset: number, length: number): Uint8Array => {
    const bytes = source.read(offset, length);
    if (!(bytes instanceof Uint8Array) || intrinsicByteLength(bytes) !== length)
      throw new Error("random-access source returned a partial range");
    sourceBytesRead = checkedAdd(sourceBytesRead, length);
    largestSourceRead = Math.max(largestSourceRead, length);
    return intrinsicByteRange(bytes);
  };
  function* entries(): Generator<ManifestEntry> {
    const chunker = new StreamingFastCdc(parameters);
    activeChunker = chunker;
    const drainInputBytes = Math.min(
      readWindowBytes,
      checkedMultiply(
        parameters.minimum,
        MAX_STREAMED_REBUILD_PENDING_ENTRIES - 1,
        "streamed rebuild drain input bytes",
      ),
    );
    const prepareEntry = (chunk: Uint8Array): ManifestEntry => {
      const hash = sha256(chunk);
      const retainedHash = copyBytes(hash);
      bytesHashed = checkedAdd(bytesHashed, chunk.byteLength);
      objectCount = checkedAdd(objectCount, 1);
      objects.putObject(copyBytes(hash), copyBytes(chunk));
      return Object.freeze({ hash: retainedHash, length: chunk.byteLength });
    };
    const accept = function* (input: Uint8Array): Generator<ManifestEntry> {
      const inputBytes = intrinsicByteRange(input);
      for (let offset = 0; offset < inputBytes.byteLength; offset += drainInputBytes) {
        const pending: ManifestEntry[] = [];
        chunker.drain(
          intrinsicByteRange(
            inputBytes,
            offset,
            Math.min(inputBytes.byteLength, offset + drainInputBytes),
          ),
          (chunk) => pending.push(prepareEntry(chunk)),
        );
        if (pending.length > MAX_STREAMED_REBUILD_PENDING_ENTRIES)
          throw new Error("streamed rebuild pending-entry bound exceeded");
        peakPendingEntries = Math.max(peakPendingEntries, pending.length);
        yield* pending;
      }
    };
    for (let offset = 0; offset < edit.offset;) {
      const length = Math.min(readWindowBytes, edit.offset - offset);
      yield* accept(read(offset, length));
      offset += length;
    }
    for (let offset = 0; offset < edit.insertBytes.byteLength;) {
      const length = Math.min(readWindowBytes, edit.insertBytes.byteLength - offset);
      yield* accept(edit.insertBytes.subarray(offset, offset + length));
      offset += length;
    }
    for (let offset = edit.offset + edit.deleteLength; offset < source.size;) {
      const length = Math.min(readWindowBytes, source.size - offset);
      yield* accept(read(offset, length));
      offset += length;
    }
    const finalEntries: ManifestEntry[] = [];
    chunker.drain(
      new Uint8Array(),
      (chunk) => finalEntries.push(prepareEntry(chunk)),
      true,
    );
    peakPendingEntries = Math.max(peakPendingEntries, finalEntries.length);
    yield* finalEntries;
  }
  const manifest = buildManifestFromEntries(entries(), parameters, workspace, {
    readBatchRecords: options.manifestReadBatchRecords,
    maxDepth: options.maxManifestDepth,
  });
  if (!activeChunker)
    throw new Error("streamed rebuild did not initialize its content chunker");
  const fallbackChunkerMetrics = activeChunker.metrics;
  return Object.freeze({
    mode: "streamed-fallback",
    manifest,
    metrics: Object.freeze({
      sourceBytesRead: checkedAdd(attemptedLocal.sourceBytesRead, sourceBytesRead),
      bytesHashed: checkedAdd(attemptedLocal.bytesHashed, bytesHashed),
      attemptedLocalSourceBytesRead: attemptedLocal.sourceBytesRead,
      attemptedLocalBytesHashed: attemptedLocal.bytesHashed,
      attemptedLocalLargestSourceRead: attemptedLocal.largestSourceRead,
      attemptedLocalChunkerInputBytesCopied: attemptedLocal.chunkerInputBytesCopied,
      attemptedLocalChunkerOutputBytesCopied: attemptedLocal.chunkerOutputBytesCopied,
      attemptedLocalChunkerBoundaryBytesScanned:
        attemptedLocal.chunkerBoundaryBytesScanned,
      attemptedLocalEditedInputBytesPrepared: attemptedLocal.editedInputBytesPrepared,
      fallbackSourceBytesRead: sourceBytesRead,
      fallbackBytesHashed: bytesHashed,
      fallbackLargestSourceRead: largestSourceRead,
      fallbackChunkerInputBytesCopied: fallbackChunkerMetrics.inputBytesCopied,
      fallbackChunkerOutputBytesCopied: fallbackChunkerMetrics.outputBytesCopied,
      fallbackChunkerBoundaryBytesScanned: fallbackChunkerMetrics.boundaryBytesScanned,
      objectCount,
      largestSourceRead: Math.max(attemptedLocal.largestSourceRead, largestSourceRead),
      peakRetainedRecords: manifest.peakRetainedRecords,
      peakPendingEntries,
      insertionCopyCount: 1,
      insertionBytesCopied: edit.insertBytes.byteLength,
      chunkerInputBytesCopied: checkedAdd(
        attemptedLocal.chunkerInputBytesCopied,
        fallbackChunkerMetrics.inputBytesCopied,
      ),
      chunkerOutputBytesCopied: checkedAdd(
        attemptedLocal.chunkerOutputBytesCopied,
        fallbackChunkerMetrics.outputBytesCopied,
      ),
      chunkerBoundaryBytesScanned: checkedAdd(
        attemptedLocal.chunkerBoundaryBytesScanned,
        fallbackChunkerMetrics.boundaryBytesScanned,
      ),
    }),
    localLimitReason: reason,
  });
}

export function rebuildManifestLocallyOrStream(
  source: RandomAccessContentSource,
  old: DiagnosticBuiltManifest,
  edit: LocalContentEdit,
  parameters: FastCdcConfiguration,
  workspace: ManifestBuildWorkspace,
  objects: StreamedObjectSink,
  localLimits: LocalRebuildLimits = DEFAULT_LOCAL_REBUILD_LIMITS,
  options: StreamedRebuildOptions = {},
): LocalRebuildResult | StreamedRebuildResult {
  parameters = snapshotMatchingLocalParameters(old, parameters);
  localLimits = snapshotLocalRebuildLimits(localLimits);
  const baseControls = snapshotStreamedRebuildControls(
    parameters,
    options,
    Object.freeze({
      sourceBytesRead: 0,
      bytesHashed: 0,
      largestSourceRead: 0,
      chunkerInputBytesCopied: 0,
      chunkerOutputBytesCopied: 0,
      chunkerBoundaryBytesScanned: 0,
      editedInputBytesPrepared: 0,
    }),
  );
  const owned = ownLocalContentInputs(validateLocalContentInputs(source, edit));
  try {
    return Object.freeze({
      mode: "local",
      manifest: rebuildManifestLocallyWithParametersOwned(
        owned.source,
        old,
        owned.edit,
        parameters,
        localLimits,
      ),
    });
  } catch (error) {
    if (!(error instanceof LocalRebuildLimitError)) throw error;
    const fallbackControls = snapshotStreamedRebuildControls(
      baseControls.parameters,
      baseControls.options,
      error.attemptMetrics,
    );
    return rebuildEditedContentStreamingOwned(
      owned,
      fallbackControls,
      workspace,
      objects,
      error.message,
    );
  }
}

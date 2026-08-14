import assert from "node:assert/strict";
import { test } from "node:test";
import {
  COMPUTER_EFS_CARRIER_V1_LIMITS,
  EFS_REPLICATION_V1_WIRE,
  ReplicationError,
  authorizationDigestHex,
  authorizeReplicationFlow,
  batchEnvelopeDigestHex,
  batchPayloadDigestHex,
  bytesToLowerHex,
  capabilityDigestHex,
  createReplicationEndpoint,
  cursorBindingDigestHex,
  decodeCanonicalBatchAcknowledgement,
  decodeCanonicalEnvelope,
  encodeCanonicalEnvelope,
  encodeCanonicalBatchAcknowledgement,
  generateReplicationSessionId,
  limitPolicyFromLimits,
  negotiateReplicationLimits,
  negotiateReplicationSession,
  replicationErrorFromRecord,
  replicationErrorRecord,
  replicationSha256,
  receiptChainDigestHex,
  requiredRoles,
  validateLimitsAgainstStorage,
  validateBatchAcknowledgement,
  validateReplicationLimits,
  validateReplicationSessionId,
} from "../../packages/replication/dist/index.js";
import {
  authorization,
  batch,
  batchAcknowledgement,
  branchGenerationFragment,
  branchPlan,
  capabilities,
  checkpointFragment,
  cursor,
  limits,
  mainPlan,
  revisionFragment,
  storage,
  terminalResult,
  unboundReplicaCapabilities,
} from "./protocol-fixtures.mjs";

function hash(bytes) {
  return bytesToLowerHex(replicationSha256(bytes));
}

function roundTrip(envelope, maxBytes = 3 * 1024 * 1024) {
  const encoded = encodeCanonicalEnvelope(envelope);
  const decoded = decodeCanonicalEnvelope(encoded, { maxBytes });
  assert.deepEqual(decoded, envelope);
  assert.deepEqual(encodeCanonicalEnvelope(decoded), encoded);
  return encoded;
}

const GOLDEN = Object.freeze({
  capabilities: "e9920dd70e5f3f2bbc7654e15728ff01cccdec00e174a19792dbe8931147edc5",
  authorization: "bb4c8a84bc18d4a47f6c591b3a231b85c90b1591dd8a7ee6f12a46e18dd5dd08",
  batch: "bbedb4e7c274d1fba9d608253e5fb6ad88a14516140e2906b0fcb858b78305c3",
  batchAcknowledgement:
    "84092a2308dd3c74ab6d70c15ae42c330ebdad4468fb2fe4c86700b3a9911708",
  cursor: "949991cb1e965e6cf5b185c2ad221f3e64f5b80dda3db3659fbee01b1684bb5d",
  revisionFragment: "de66dd9a0b1e790c23b19e6561fd5c80cf3fe7350ac89a3d70d54ac5fa5afd5b",
  checkpointFragment:
    "abca64bd9b379af8e2ba9565108745f464ea0082e5ed518c22b60e3d01f71c97",
  branchGenerationFragment:
    "8fc7c0d226e21a066655416850ad5a7fa5d083f20f2351cbebf6592e1f73c994",
  terminalResult: "c67257e11d93c8ba04e2ba85adfda5d2218db6ec83f9792ee85463a7fa9f00fd",
  error: "76f49d891c3b99a3058b4d0cda5f17a85f5de934f04606d7afa173a790ade7fb",
  capabilityDigest: "3eaeb8228e026edad086e7bbad10e33245530c2796bd2307cfc8d9fb93e3772a",
  authorizationDigest:
    "d8cd3907231f41557774ec354d4ffc26ec7f18b0085bd5ace68063211878f48f",
  batchDigest: "dcf0bdbc12445c02e39799deb7326af9eec2128c5c3850660be3a562d5d3d257",
  batchEnvelopeDigest:
    "cb4d2914e8dbd2edbbffbc35c00e14e01c62c91c5e552ca01a254abb4e3318b1",
  receiptChainDigest:
    "9f01ca484c9e6b850d3fd8be2dde83926d9b08b4cee475aa0a7913cd2ef889ea",
  cursorDigest: "faeeb127c6ae299d38aa2cc79be0fecc8a54c95bf647baba3aeafd5e5460b16e",
});

test("replication SHA-256 is incremental-compatible with standard vectors", () => {
  assert.equal(
    hash(new Uint8Array()),
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
  assert.equal(
    hash(new TextEncoder().encode("abc")),
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
});

test("canonical version 1 envelopes and digests match all golden categories", () => {
  const sourceCapabilities = capabilities();
  const authRecord = {
    authorization: authorization([mainPlan, branchPlan]),
    effectiveLimits: limits,
  };
  const semanticError = {
    code: "BranchDiverged",
    phase: "activation",
    sessionId: "00112233445566778899aabbccddeeff",
    message: "generation digest changed",
    retryable: false,
  };
  const encoded = {
    capabilities: roundTrip({ kind: "capabilities", value: sourceCapabilities }),
    authorization: roundTrip({ kind: "authorization", value: authRecord }),
    batch: roundTrip({ kind: "batch", value: batch }),
    batchAcknowledgement: roundTrip({
      kind: "batch-acknowledgement",
      value: batchAcknowledgement,
    }),
    cursor: roundTrip({ kind: "cursor", value: cursor }),
    revisionFragment: roundTrip({
      kind: "revision-fragment",
      value: revisionFragment,
    }),
    checkpointFragment: roundTrip({
      kind: "checkpoint-fragment",
      value: checkpointFragment,
    }),
    branchGenerationFragment: roundTrip({
      kind: "branch-generation-fragment",
      value: branchGenerationFragment,
    }),
    terminalResult: roundTrip({ kind: "terminal-result", value: terminalResult }),
    error: roundTrip({ kind: "error", value: semanticError }),
  };
  const actual = {
    ...Object.fromEntries(
      Object.entries(encoded).map(([name, bytes]) => [name, hash(bytes)]),
    ),
    capabilityDigest: capabilityDigestHex(
      sourceCapabilities,
      sourceCapabilities.limits,
    ),
    authorizationDigest: authorizationDigestHex(authRecord),
    batchDigest: batchPayloadDigestHex(batch.records),
    batchEnvelopeDigest: batchEnvelopeDigestHex(batch),
    receiptChainDigest: receiptChainDigestHex(
      new Uint8Array(32),
      batch.sequence,
      batchAcknowledgement.batchEnvelopeDigest,
    ),
    cursorDigest: cursorBindingDigestHex(cursor),
  };
  assert.deepEqual(actual, GOLDEN);
});

test("session identifiers are package-generated 128-bit lowercase hex", () => {
  assert.equal(
    generateReplicationSessionId((target) => target.fill(0xab)),
    "abababababababababababababababab",
  );
  assert.equal(
    validateReplicationSessionId("00112233445566778899aabbccddeeff"),
    "00112233445566778899aabbccddeeff",
  );
  for (const invalid of ["session-01", "00112233445566778899AABBCCDDEEFF", "00"])
    assert.throws(
      () => validateReplicationSessionId(invalid),
      (error) => error instanceof ReplicationError && error.code === "ProtocolMismatch",
    );
});

test("batch acknowledgement binds the complete request and committed cursor", () => {
  const encoded = encodeCanonicalBatchAcknowledgement(batchAcknowledgement);
  const decoded = decodeCanonicalBatchAcknowledgement(encoded);
  assert.deepEqual(decoded, batchAcknowledgement);
  assert.doesNotThrow(() => validateBatchAcknowledgement(batch, decoded));
  assert.throws(
    () =>
      validateBatchAcknowledgement(
        { ...batch, priorCursorDigest: new Uint8Array(32).fill(0x99) },
        decoded,
      ),
    (error) =>
      error instanceof ReplicationError && error.code === "BatchReplayMismatch",
  );
});

test("authorization encoding canonicalizes plan order and binds identity, policy, and limits", () => {
  const first = {
    authorization: authorization([branchPlan, mainPlan]),
    effectiveLimits: limits,
  };
  const second = {
    authorization: authorization([mainPlan, branchPlan]),
    effectiveLimits: limits,
  };
  assert.equal(authorizationDigestHex(first), authorizationDigestHex(second));
  assert.notEqual(
    authorizationDigestHex(first),
    authorizationDigestHex({
      ...first,
      authorization: { ...first.authorization, principalId: "principal-02" },
    }),
  );
  assert.notEqual(
    authorizationDigestHex(first),
    authorizationDigestHex({
      ...first,
      effectiveLimits: { ...limits, maxRetryAttempts: limits.maxRetryAttempts - 1 },
    }),
  );
  assert.throws(
    () =>
      authorizationDigestHex({
        ...first,
        authorization: { ...first.authorization, allowedPlans: [mainPlan, mainPlan] },
      }),
    (error) => error instanceof ReplicationError && error.code === "UnauthorizedScope",
  );
});

test("the endpoint returns its own authenticated policy record", async () => {
  const sourceAuthorization = authorization([mainPlan]);
  const destinationAuthorization = {
    ...sourceAuthorization,
    principalId: "destination-principal",
    hostScopeId: "destination-host",
  };
  const endpoint = createReplicationEndpoint({
    bridge: { capabilities: capabilities("replica") },
    authorization: destinationAuthorization,
  });
  try {
    const response = decodeCanonicalEnvelope(
      await endpoint.exchange(
        encodeCanonicalEnvelope({
          kind: "authorization",
          value: {
            authorization: sourceAuthorization,
            effectiveLimits: limits,
          },
        }),
      ),
    );
    assert.equal(response.kind, "authorization");
    assert.equal(response.value.authorization.principalId, "destination-principal");
    assert.equal(response.value.authorization.hostScopeId, "destination-host");
    assert.deepEqual(response.value.effectiveLimits, limits);
  } finally {
    await endpoint.close();
  }
});

test("capability digest binds both the advertised row and effective limits", () => {
  const advertised = capabilities();
  assert.notEqual(
    capabilityDigestHex(advertised, limits),
    capabilityDigestHex(advertised, {
      ...limits,
      maxRetryAttempts: limits.maxRetryAttempts - 1,
    }),
  );
  assert.notEqual(
    capabilityDigestHex(advertised, limits),
    capabilityDigestHex(
      {
        ...advertised,
        storage: {
          ...advertised.storage,
          maxFinalTransactionRows: advertised.storage.maxFinalTransactionRows - 1,
        },
      },
      limits,
    ),
  );
});

test("limit negotiation uses minima for ceilings, maximum retry floor, and rejects cross-field hazards", () => {
  const source = { ...limits, maxBatchEntries: 200, minRetryDelayMs: 150 };
  const destination = { ...limits, maxBatchEntries: 180, minRetryDelayMs: 250 };
  const sourcePolicy = limitPolicyFromLimits({
    ...limits,
    maxBatchEntries: 170,
    minRetryDelayMs: 300,
  });
  const destinationPolicy = limitPolicyFromLimits({
    ...limits,
    maxBatchEntries: 160,
    minRetryDelayMs: 400,
  });
  const effective = negotiateReplicationLimits({
    source,
    destination,
    sourcePolicy,
    destinationPolicy,
  });
  assert.equal(effective.maxBatchEntries, 160);
  assert.equal(effective.minRetryDelayMs, 400);
  assert.equal(effective.maxRequestBytes, 3 * 1024 * 1024);
  assert.throws(
    () => validateReplicationLimits({ ...limits, maxInFlightBatches: 2 }),
    (error) => error instanceof ReplicationError && error.code === "IncompatibleLimit",
  );
  assert.throws(
    () =>
      validateReplicationLimits({
        ...limits,
        maxBufferedBytes: limits.maxRequestBytes + limits.maxResponseBytes,
      }),
    /codec headroom/,
  );
  assert.throws(
    () => validateReplicationLimits({ ...limits, minRetryDelayMs: 10_001 }),
    /exceeds maxRetryDelayMs/,
  );
  assert.throws(
    () =>
      validateLimitsAgainstStorage(limits, {
        ...storage,
        maxStagingPayloadBytes: limits.maxStagingBytesPerSession - 1,
      }),
    (error) => error instanceof ReplicationError && error.code === "IncompatibleLimit",
  );
  assert.throws(
    () =>
      validateLimitsAgainstStorage(limits, {
        ...storage,
        maxMaintenanceBytes: limits.maxReplicationMetadataBytes - 1,
      }),
    (error) => error instanceof ReplicationError && error.code === "IncompatibleLimit",
  );
});

test("the normative global role-flow matrix accepts only its four rows", () => {
  const plans = [
    mainPlan,
    branchPlan,
    { flow: "replica-branch-to-authority", branchId: "branch-1" },
    { flow: "replica-branch-to-replica", branchId: "branch-1" },
  ];
  const roles = ["main-authority", "replica"];
  for (const plan of plans) {
    const expected = requiredRoles(plan);
    for (const sourceRole of roles) {
      for (const destinationRole of roles) {
        const options = {
          sourceRole,
          destinationRole,
          plan,
          sourceAuthorization: authorization(plans),
          destinationAuthorization: authorization(plans),
        };
        if (sourceRole === expected.source && destinationRole === expected.destination)
          assert.doesNotThrow(() => authorizeReplicationFlow(options));
        else
          assert.throws(
            () => authorizeReplicationFlow(options),
            (error) =>
              error instanceof ReplicationError && error.code === "UnauthorizedScope",
          );
      }
    }
  }
  assert.throws(
    () =>
      authorizeReplicationFlow({
        sourceRole: "main-authority",
        destinationRole: "replica",
        plan: { ...branchPlan, branchId: "another" },
        sourceAuthorization: authorization([branchPlan]),
        destinationAuthorization: authorization([branchPlan]),
      }),
    (error) => error instanceof ReplicationError && error.code === "UnauthorizedScope",
  );
});

test("fresh replica negotiation permits only authenticated authority-main provisioning", () => {
  const source = capabilities("main-authority");
  const destination = unboundReplicaCapabilities();
  const negotiated = negotiateReplicationSession({
    source,
    destination,
    sourceAuthorization: authorization([mainPlan]),
    destinationAuthorization: authorization([mainPlan]),
    plan: mainPlan,
  });
  assert.equal(negotiated.provisioning, true);
  assert.deepEqual(negotiated.limits, COMPUTER_EFS_CARRIER_V1_LIMITS);
  assert.throws(
    () =>
      negotiateReplicationSession({
        source,
        destination,
        sourceAuthorization: authorization([branchPlan]),
        destinationAuthorization: authorization([branchPlan]),
        plan: branchPlan,
      }),
    (error) =>
      error instanceof ReplicationError &&
      ["UnauthorizedScope", "ProvisioningRejected"].includes(error.code),
  );
  assert.throws(
    () =>
      negotiateReplicationSession({
        source,
        destination: {
          ...destination,
          applicationId: null,
        },
        sourceAuthorization: authorization([mainPlan]),
        destinationAuthorization: authorization([mainPlan]),
        plan: mainPlan,
      }),
    (error) => error instanceof ReplicationError && error.code === "SchemaMismatch",
  );
  assert.throws(
    () =>
      negotiateReplicationSession({
        source: {
          ...source,
          fastCdc: { minimum: 1024, average: 3072, maximum: 4096 },
        },
        destination,
        sourceAuthorization: authorization([mainPlan]),
        destinationAuthorization: authorization([mainPlan]),
        plan: mainPlan,
      }),
    (error) => error instanceof ReplicationError && error.code === "CapabilityMismatch",
  );
});

test("semantic errors survive canonical response records without thrown-object preservation", () => {
  assert.throws(
    () => new ReplicationError("Busy", "invalid override", { retryable: false }),
    /canonical code policy/,
  );
  const original = new ReplicationError("Busy", "database is busy", {
    phase: "activation",
    sessionId: "00112233445566778899aabbccddeeff",
  });
  const record = replicationErrorRecord(original);
  const decoded = decodeCanonicalEnvelope(
    encodeCanonicalEnvelope({ kind: "error", value: record }),
  ).value;
  const restored = replicationErrorFromRecord(decoded);
  assert.equal(restored.name, "ReplicationError");
  assert.equal(restored.code, "Busy");
  assert.equal(restored.phase, "activation");
  assert.equal(restored.sessionId, "00112233445566778899aabbccddeeff");
  assert.equal(restored.retryable, true);
  assert.throws(
    () =>
      encodeCanonicalEnvelope({
        kind: "error",
        value: { ...record, retryable: false },
      }),
    /retryability does not match/,
  );
});

test("decoded carrier boundary is exact at 3 MiB and rejects one byte over", () => {
  const maximum = 3 * 1024 * 1024;
  const empty = encodeCanonicalEnvelope({
    kind: "revision-fragment",
    value: { ...revisionFragment, fragmentBytes: new Uint8Array() },
  });
  const exact = encodeCanonicalEnvelope({
    kind: "revision-fragment",
    value: {
      ...revisionFragment,
      fragmentBytes: new Uint8Array(maximum - empty.byteLength),
    },
  });
  assert.equal(exact.byteLength, maximum);
  const decoded = decodeCanonicalEnvelope(exact, { maxBytes: maximum });
  assert.equal(decoded.kind, "revision-fragment");
  assert.equal(decoded.value.fragmentBytes.buffer, exact.buffer);
  const over = encodeCanonicalEnvelope({
    kind: "revision-fragment",
    value: {
      ...revisionFragment,
      fragmentBytes: new Uint8Array(maximum - empty.byteLength + 1),
    },
  });
  assert.equal(over.byteLength, maximum + 1);
  assert.throws(
    () => decodeCanonicalEnvelope(over, { maxBytes: maximum }),
    (error) => error instanceof ReplicationError && error.code === "ResourceLimit",
  );
});

test("decoder rejects corrupt, noncanonical, truncated, trailing, and oversized envelopes", () => {
  const encoded = encodeCanonicalEnvelope({ kind: "cursor", value: cursor });
  for (const [name, mutate, code] of [
    ["magic", (bytes) => (bytes[0] ^= 1), "ProtocolMismatch"],
    ["version", (bytes) => (bytes[5] = 2), "ProtocolMismatch"],
    ["kind", (bytes) => (bytes[6] = 0xff), "ProtocolMismatch"],
    ["flags", (bytes) => (bytes[7] = 1), "ProtocolMismatch"],
    ["length", (bytes) => (bytes[11] ^= 1), "ProtocolMismatch"],
  ]) {
    const corrupt = encoded.slice();
    mutate(corrupt);
    assert.throws(
      () => decodeCanonicalEnvelope(corrupt, { maxBytes: 3 * 1024 * 1024 }),
      (error) => error instanceof ReplicationError && error.code === code,
      name,
    );
  }
  assert.throws(() => decodeCanonicalEnvelope(encoded.slice(0, -1)), /length mismatch/);
  const trailing = new Uint8Array(encoded.length + 1);
  trailing.set(encoded);
  assert.throws(() => decodeCanonicalEnvelope(trailing), /length mismatch/);
  assert.throws(
    () => decodeCanonicalEnvelope(new Uint8Array(64 * 1024 + 1)),
    (error) => error instanceof ReplicationError && error.code === "ResourceLimit",
  );
  assert.throws(
    () =>
      encodeCanonicalEnvelope({
        kind: "cursor",
        value: { ...cursor, selectedIdentity: "\ud800" },
      }),
    /unpaired UTF-16 surrogate/,
  );
  const malformedUtf8 = encoded.slice();
  malformedUtf8[16] = 0xff;
  assert.throws(() => decodeCanonicalEnvelope(malformedUtf8), /not valid UTF-8/);
  assert.throws(
    () =>
      encodeCanonicalEnvelope({
        kind: "cursor",
        value: {
          ...cursor,
          plan: {
            flow: "authority-branch-to-replica",
            branchId: "x".repeat(201),
          },
        },
      }),
    /200 UTF-8 bytes/,
  );
  const corruptBatch = encodeCanonicalEnvelope({ kind: "batch", value: batch });
  corruptBatch[corruptBatch.length - 1] ^= 1;
  assert.throws(
    () => decodeCanonicalEnvelope(corruptBatch, { maxBytes: 3 * 1024 * 1024 }),
    (error) =>
      error instanceof ReplicationError &&
      ["IntegrityFailure", "ProtocolMismatch"].includes(error.code),
  );
  assert.equal(EFS_REPLICATION_V1_WIRE.unknownFields, "reject");
});

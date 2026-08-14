import { ReplicationError } from "./errors.js";
import {
  negotiateReplicationLimits,
  validateLimitsAgainstStorage,
  validateReplicationStorageCapabilities,
} from "./limits.js";
import type {
  AuthorizedReplicationPeer,
  CanonicalAuthorizationRecord,
  ReplicationCapabilities,
  ReplicationLimits,
  ReplicationPlan,
  ReplicationRole,
} from "./types.js";
import {
  REPLICATION_APPLICATION_ID,
  REPLICATION_CHUNKER_FORMAT,
  REPLICATION_FILESYSTEM_SCHEMA_VERSION,
  REPLICATION_HOST_PROFILE,
  REPLICATION_MANIFEST_FORMAT,
  REPLICATION_PROTOCOL_VERSION,
  REPLICATION_STORAGE_USER_VERSION,
} from "./types.js";
import { canonicalUtf8 } from "./validation.js";
import { authorizationDigest, capabilityDigest } from "./wire.js";

function samePlan(left: ReplicationPlan, right: ReplicationPlan): boolean {
  return (
    left.flow === right.flow &&
    (left.flow === "authority-main-to-replica" ||
      (right.flow !== "authority-main-to-replica" && left.branchId === right.branchId))
  );
}

export function requiredRoles(plan: ReplicationPlan): Readonly<{
  source: ReplicationRole;
  destination: ReplicationRole;
}> {
  switch (plan.flow) {
    case "authority-main-to-replica":
    case "authority-branch-to-replica":
      return Object.freeze({ source: "main-authority", destination: "replica" });
    case "replica-branch-to-authority":
      return Object.freeze({ source: "replica", destination: "main-authority" });
    case "replica-branch-to-replica":
      return Object.freeze({ source: "replica", destination: "replica" });
  }
}

export function validateAuthorizedPeer(
  authorization: AuthorizedReplicationPeer,
  name = "authorization",
): void {
  canonicalUtf8(authorization.principalId, `${name}.principalId`);
  canonicalUtf8(authorization.hostScopeId, `${name}.hostScopeId`);
  canonicalUtf8(authorization.expectedFilesystemId, `${name}.expectedFilesystemId`);
  canonicalUtf8(authorization.expectedAuthorityId, `${name}.expectedAuthorityId`);
  canonicalUtf8(authorization.policyVersion, `${name}.policyVersion`);
  if (authorization.hostProfile !== REPLICATION_HOST_PROFILE)
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name}.hostProfile is unsupported`,
    );
  if (
    !Array.isArray(authorization.allowedPlans) ||
    authorization.allowedPlans.length === 0
  )
    throw new ReplicationError("UnauthorizedScope", `${name}.allowedPlans is empty`);
  for (const plan of authorization.allowedPlans) {
    if (plan.flow !== "authority-main-to-replica")
      canonicalUtf8(plan.branchId, `${name}.allowedPlans.branchId`, 200);
  }
}

export function authorizeReplicationFlow(options: {
  readonly sourceRole: ReplicationRole;
  readonly destinationRole: ReplicationRole;
  readonly plan: ReplicationPlan;
  readonly sourceAuthorization: AuthorizedReplicationPeer;
  readonly destinationAuthorization: AuthorizedReplicationPeer;
}): void {
  validateAuthorizedPeer(options.sourceAuthorization, "sourceAuthorization");
  validateAuthorizedPeer(options.destinationAuthorization, "destinationAuthorization");
  const roles = requiredRoles(options.plan);
  if (
    options.sourceRole !== roles.source ||
    options.destinationRole !== roles.destination
  )
    throw new ReplicationError(
      "UnauthorizedScope",
      `${options.plan.flow} is not allowed for ${options.sourceRole} to ${options.destinationRole}`,
    );
  for (const [name, authorization] of [
    ["sourceAuthorization", options.sourceAuthorization],
    ["destinationAuthorization", options.destinationAuthorization],
  ] as const) {
    if (!authorization.allowedPlans.some((allowed) => samePlan(allowed, options.plan)))
      throw new ReplicationError(
        "UnauthorizedScope",
        `${name} does not authorize the exact global plan`,
      );
  }
}

function includesFastCdc(
  capabilities: ReplicationCapabilities,
  expected: NonNullable<ReplicationCapabilities["fastCdc"]>,
): boolean {
  return capabilities.supportedFastCdcConfigurations.some(
    (item) =>
      item.minimum === expected.minimum &&
      item.average === expected.average &&
      item.maximum === expected.maximum,
  );
}

function validateFastCdcRow(
  value: NonNullable<ReplicationCapabilities["fastCdc"]>,
  name: string,
): void {
  if (
    !Number.isSafeInteger(value.minimum) ||
    !Number.isSafeInteger(value.average) ||
    !Number.isSafeInteger(value.maximum) ||
    value.minimum <= 0 ||
    value.minimum > value.average ||
    value.average > value.maximum ||
    !Number.isInteger(Math.log2(value.average))
  )
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name} is not a valid FastCDC row`,
    );
}

function validateBoundCapabilities(
  capabilities: ReplicationCapabilities,
  name: string,
): void {
  if (!capabilities.filesystemId)
    throw new ReplicationError("FilesystemMismatch", `${name}.filesystemId is absent`);
  if (!capabilities.authorityId)
    throw new ReplicationError("AuthorityMismatch", `${name}.authorityId is absent`);
  if (
    capabilities.applicationId !== REPLICATION_APPLICATION_ID ||
    capabilities.filesystemSchemaVersion !== REPLICATION_FILESYSTEM_SCHEMA_VERSION
  )
    throw new ReplicationError(
      "SchemaMismatch",
      `${name} is outside the version 13 row`,
    );
  if (
    capabilities.activeManifestFormat !== REPLICATION_MANIFEST_FORMAT ||
    capabilities.activeChunkerFormat !== REPLICATION_CHUNKER_FORMAT ||
    capabilities.fastCdc === null ||
    capabilities.copyOnWritePageBytes === null ||
    !capabilities.supportedManifestFormats.includes(
      capabilities.activeManifestFormat,
    ) ||
    !capabilities.supportedChunkerFormats.includes(capabilities.activeChunkerFormat) ||
    !includesFastCdc(capabilities, capabilities.fastCdc) ||
    !capabilities.supportedCopyOnWritePageBytes.includes(
      capabilities.copyOnWritePageBytes,
    )
  )
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name} has an unsupported format row`,
    );
}

function validateUnboundCapabilities(
  capabilities: ReplicationCapabilities,
  name: string,
): void {
  if (
    capabilities.role !== "replica" ||
    capabilities.filesystemId !== null ||
    capabilities.authorityId !== null ||
    capabilities.activeManifestFormat !== null ||
    capabilities.activeChunkerFormat !== null ||
    capabilities.fastCdc !== null ||
    capabilities.copyOnWritePageBytes !== null
  )
    throw new ReplicationError(
      "ProvisioningRejected",
      `${name} is not the exact durable unbound-replica capability row`,
    );
}

function validateCommonCapabilities(
  capabilities: ReplicationCapabilities,
  name: string,
): void {
  validateReplicationStorageCapabilities(capabilities.storage, `${name}.storage`);
  if (capabilities.fastCdc) validateFastCdcRow(capabilities.fastCdc, `${name}.fastCdc`);
  for (
    let index = 0;
    index < capabilities.supportedFastCdcConfigurations.length;
    index += 1
  )
    validateFastCdcRow(
      capabilities.supportedFastCdcConfigurations[index]!,
      `${name}.supportedFastCdcConfigurations[${index}]`,
    );
  if (!capabilities.protocolVersions.includes(REPLICATION_PROTOCOL_VERSION))
    throw new ReplicationError(
      "ProtocolMismatch",
      `${name} does not support version 1`,
    );
  if (capabilities.hostProfile !== REPLICATION_HOST_PROFILE)
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name} has the wrong host profile`,
    );
  if (
    capabilities.applicationId !== REPLICATION_APPLICATION_ID ||
    capabilities.storageUserVersion !== REPLICATION_STORAGE_USER_VERSION ||
    capabilities.storageMigrationState !== "none" ||
    capabilities.writableFilesystemSchemaVersion !==
      REPLICATION_FILESYSTEM_SCHEMA_VERSION ||
    !capabilities.readableFilesystemSchemaVersions.includes(
      REPLICATION_FILESYSTEM_SCHEMA_VERSION,
    ) ||
    (capabilities.provisioningState === "bound" &&
      capabilities.filesystemSchemaVersion !== REPLICATION_FILESYSTEM_SCHEMA_VERSION) ||
    (capabilities.provisioningState === "unbound-replica" &&
      capabilities.filesystemSchemaVersion !== null)
  )
    throw new ReplicationError(
      "SchemaMismatch",
      `${name} is outside the initial version 13 schema row`,
    );
  if (
    capabilities.hashAlgorithms.length !== 1 ||
    capabilities.hashAlgorithms[0] !== "sha256" ||
    !capabilities.supportedManifestFormats.includes(REPLICATION_MANIFEST_FORMAT) ||
    !capabilities.supportedChunkerFormats.includes(REPLICATION_CHUNKER_FORMAT)
  )
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name} lacks the initial format row`,
    );
  if (capabilities.provisioningState === "bound")
    validateBoundCapabilities(capabilities, name);
  else if (capabilities.provisioningState === "unbound-replica")
    validateUnboundCapabilities(capabilities, name);
  else throw new ReplicationError("CapabilityMismatch", `${name} has an unknown state`);
}

function requireFlowFeature(
  capabilities: ReplicationCapabilities,
  plan: ReplicationPlan,
  name: string,
): void {
  const supported =
    plan.flow === "authority-main-to-replica"
      ? capabilities.features.authorityMainToReplica
      : plan.flow === "authority-branch-to-replica"
        ? capabilities.features.authorityBranchToReplica
        : plan.flow === "replica-branch-to-authority"
          ? capabilities.features.replicaBranchToAuthority
          : capabilities.features.replicaBranchToReplica;
  if (!supported)
    throw new ReplicationError(
      "CapabilityMismatch",
      `${name} does not support the flow`,
    );
}

function validateIdentityBinding(
  capabilities: ReplicationCapabilities,
  authorization: AuthorizedReplicationPeer,
  name: string,
): void {
  if (
    capabilities.filesystemId !== null &&
    capabilities.filesystemId !== authorization.expectedFilesystemId
  )
    throw new ReplicationError(
      "FilesystemMismatch",
      `${name} filesystem differs from authenticated scope`,
    );
  if (
    capabilities.authorityId !== null &&
    capabilities.authorityId !== authorization.expectedAuthorityId
  )
    throw new ReplicationError(
      "AuthorityMismatch",
      `${name} authority differs from authenticated scope`,
    );
}

export interface NegotiatedReplicationSession {
  readonly protocol: typeof REPLICATION_PROTOCOL_VERSION;
  readonly limits: Readonly<ReplicationLimits>;
  readonly sourceCapabilityDigest: Uint8Array;
  readonly destinationCapabilityDigest: Uint8Array;
  readonly sourceAuthorizationDigest: Uint8Array;
  readonly destinationAuthorizationDigest: Uint8Array;
  readonly provisioning: boolean;
}

export function negotiateReplicationSession(options: {
  readonly source: ReplicationCapabilities;
  readonly destination: ReplicationCapabilities;
  readonly sourceAuthorization: AuthorizedReplicationPeer;
  readonly destinationAuthorization: AuthorizedReplicationPeer;
  readonly plan: ReplicationPlan;
}): NegotiatedReplicationSession {
  validateCommonCapabilities(options.source, "sourceCapabilities");
  validateCommonCapabilities(options.destination, "destinationCapabilities");
  if (options.source.provisioningState !== "bound")
    throw new ReplicationError(
      "ProvisioningRejected",
      "an unbound replica cannot be a replication source",
    );
  for (const [name, feature] of Object.entries(options.source.features))
    if (!feature)
      throw new ReplicationError(
        "CapabilityMismatch",
        `sourceCapabilities does not implement required feature ${name}`,
      );
  for (const [name, feature] of Object.entries(options.destination.features))
    if (!feature)
      throw new ReplicationError(
        "CapabilityMismatch",
        `destinationCapabilities does not implement required feature ${name}`,
      );
  authorizeReplicationFlow({
    sourceRole: options.source.role,
    destinationRole: options.destination.role,
    plan: options.plan,
    sourceAuthorization: options.sourceAuthorization,
    destinationAuthorization: options.destinationAuthorization,
  });
  validateIdentityBinding(options.source, options.sourceAuthorization, "source");
  validateIdentityBinding(
    options.destination,
    options.destinationAuthorization,
    "destination",
  );
  if (
    options.sourceAuthorization.expectedFilesystemId !==
      options.destinationAuthorization.expectedFilesystemId ||
    options.sourceAuthorization.expectedAuthorityId !==
      options.destinationAuthorization.expectedAuthorityId
  )
    throw new ReplicationError(
      "UnauthorizedScope",
      "authenticated source and destination scopes do not identify the same filesystem",
    );
  requireFlowFeature(options.source, options.plan, "sourceCapabilities");
  requireFlowFeature(options.destination, options.plan, "destinationCapabilities");
  const provisioning = options.destination.provisioningState === "unbound-replica";
  if (provisioning) {
    if (
      options.plan.flow !== "authority-main-to-replica" ||
      options.source.provisioningState !== "bound" ||
      !options.source.features.freshReplicaProvisioning ||
      !options.destination.features.freshReplicaProvisioning
    )
      throw new ReplicationError(
        "ProvisioningRejected",
        "unbound replicas accept only authenticated authority-main provisioning",
      );
    if (
      !options.source.fastCdc ||
      !includesFastCdc(options.destination, options.source.fastCdc) ||
      !options.source.copyOnWritePageBytes ||
      !options.destination.supportedCopyOnWritePageBytes.includes(
        options.source.copyOnWritePageBytes,
      )
    )
      throw new ReplicationError(
        "CapabilityMismatch",
        "unbound replica cannot adopt the authority format row",
      );
  } else {
    if (
      options.source.filesystemId !== options.destination.filesystemId ||
      options.source.authorityId !== options.destination.authorityId
    )
      throw new ReplicationError(
        options.source.filesystemId !== options.destination.filesystemId
          ? "FilesystemMismatch"
          : "AuthorityMismatch",
        "bound peers identify different filesystems or authorities",
      );
    if (
      options.source.activeManifestFormat !==
        options.destination.activeManifestFormat ||
      options.source.activeChunkerFormat !== options.destination.activeChunkerFormat ||
      options.source.copyOnWritePageBytes !==
        options.destination.copyOnWritePageBytes ||
      !options.source.fastCdc ||
      !options.destination.fastCdc ||
      options.source.fastCdc.minimum !== options.destination.fastCdc.minimum ||
      options.source.fastCdc.average !== options.destination.fastCdc.average ||
      options.source.fastCdc.maximum !== options.destination.fastCdc.maximum
    )
      throw new ReplicationError(
        "CapabilityMismatch",
        "bound peers have different persisted format rows",
      );
  }
  const limits = negotiateReplicationLimits({
    source: options.source.limits,
    destination: options.destination.limits,
    sourcePolicy: options.sourceAuthorization.limitPolicy,
    destinationPolicy: options.destinationAuthorization.limitPolicy,
  });
  validateLimitsAgainstStorage(limits, options.source.storage, "sourceCapabilities");
  validateLimitsAgainstStorage(
    limits,
    options.destination.storage,
    "destinationCapabilities",
  );
  const sourceRecord: CanonicalAuthorizationRecord = {
    authorization: options.sourceAuthorization,
    effectiveLimits: limits,
  };
  const destinationRecord: CanonicalAuthorizationRecord = {
    authorization: options.destinationAuthorization,
    effectiveLimits: limits,
  };
  return Object.freeze({
    protocol: REPLICATION_PROTOCOL_VERSION,
    limits,
    sourceCapabilityDigest: capabilityDigest(options.source, limits),
    destinationCapabilityDigest: capabilityDigest(options.destination, limits),
    sourceAuthorizationDigest: authorizationDigest(sourceRecord),
    destinationAuthorizationDigest: authorizationDigest(destinationRecord),
    provisioning,
  });
}

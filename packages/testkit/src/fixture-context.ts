import type { FilesystemSQLiteDriver } from "@ephemeralai/fs/sqlite-driver";

export const PORTABLE_FIXTURE_CONTEXT_SCHEMA =
  "efs-portable-fixture-context-v1" as const;

export interface PortableFixtureContext {
  readonly schema: typeof PORTABLE_FIXTURE_CONTEXT_SCHEMA;
  readonly label: string;
  readonly seed: number;
  readonly fixtureDigest: string;
  readonly digestBasis: "sha256-utf8-canonical-fixture-descriptor";
}

interface FixtureContextRecorder {
  recordFixtureContext?(context: PortableFixtureContext): void | Promise<void>;
}

function hex(bytes: Uint8Array): string {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Record the exact deterministic descriptor used to create one portable fixture. */
export async function recordPortableFixtureContext(
  recorder: FixtureContextRecorder,
  adapter: FilesystemSQLiteDriver,
  label: string,
  seed: number,
): Promise<void> {
  if (recorder.recordFixtureContext === undefined) return;
  const descriptor = new TextEncoder().encode(
    `${PORTABLE_FIXTURE_CONTEXT_SCHEMA}\n${label}\n${seed}\n`,
  );
  const digest = adapter.hashBytes
    ? adapter.hashBytes(descriptor)
    : await adapter.hashBytesAsync?.(descriptor);
  if (!(digest instanceof Uint8Array) || digest.byteLength !== 32)
    throw new Error("portable fixture context requires SHA-256");
  await recorder.recordFixtureContext(
    Object.freeze({
      schema: PORTABLE_FIXTURE_CONTEXT_SCHEMA,
      label,
      seed,
      fixtureDigest: hex(digest),
      digestBasis: "sha256-utf8-canonical-fixture-descriptor",
    }),
  );
}

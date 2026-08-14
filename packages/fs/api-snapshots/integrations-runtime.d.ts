/* Generated public API declaration snapshot. Update only with: pnpm api:update */
/* package: @ephemeralai/fs; subpath: ./integrations/runtime; entry: packages/fs/dist/integrations/runtime.d.ts */

/* export: EphemeralRuntime; kinds: value,type */
/* source: packages/fs/dist/filesystem/ephemeral-runtime.d.ts */
/** One ownership root for the portable FS, replication, and branch Node VFS. */
export declare class EphemeralRuntime {
    #private;
    readonly provisioningState: "bound" | "unbound-replica";
    readonly identity: ReplicationFilesystemIdentity | null;
    readonly filesystem: PublicEphemeralFS | null;
    readonly replication: ReplicationFilesystemBridge;
    private constructor();
    static open(options: OpenEphemeralRuntimeOptions): Promise<EphemeralRuntime>;
    openNodeVfs(options?: {
        readonly branchId?: string;
    }): NodeVfsFilesystemBridge;
    close(): Promise<void>;
}

/* export: OpenEphemeralRuntimeOptions; kinds: type */
/* source: packages/fs/dist/filesystem/ephemeral-runtime.d.ts */
export interface OpenEphemeralRuntimeOptions extends OpenFilesystemOptions {
    readonly provisioningState?: "bound" | "unbound-replica";
    readonly replicationIdentity?: {
        readonly authorityId: string;
        readonly role: ReplicationRole;
    };
}
